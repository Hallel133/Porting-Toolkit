//! Application state, background jobs, commands and the window chrome (a
//! header with the project switcher and navigation). Pages live in
//! `views.rs`, the menu bar in `menu.rs`.
//!
//! Long work (engine calls, hashing, packing) runs on a thread and reports
//! back through a channel; the thread wakes the UI with `request_repaint`.

use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::time::{Duration, Instant};

use eframe::egui::{
    self, Align, Color32, CornerRadius, Frame, Key, KeyboardShortcut, Layout, Margin, Modifiers,
    RichText, Stroke, vec2,
};
use ptk_core::convert::{self, Event, RunResult};
use ptk_core::doctor::Check;
use ptk_core::engine::EngineClient;
use ptk_core::manifest::{Issue, MANIFEST_FILE, Manifest};
use ptk_core::market::{EntryKind, MarketEntry, MarketIndex, Published};
use ptk_core::pack::{PackOptions, UnpackReport};
use ptk_core::plugins::{CreatedPlugin, ListedPlugin, Validation};
use ptk_core::project::Contents;
use ptk_core::scan::ScanResult;
use ptk_core::targets::TargetRegistry;
use ptk_core::verify::VerifyReport;
use serde::{Deserialize, Serialize};

use crate::menu::{Command, MenuState};
use crate::theme::*;
use crate::widgets;

/// The pages of the app. The first three are always in the tab bar; the
/// workspace pages appear when a project is open.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum View {
    Projects,
    Marketplace,
    Settings,
    Overview,
    Scan,
    Export,
}

impl View {
    pub const ALL: [View; 6] = [
        View::Projects,
        View::Marketplace,
        View::Settings,
        View::Overview,
        View::Scan,
        View::Export,
    ];
    /// The open game's pages. Its advanced options live on the Game page.
    pub const WORKSPACE: [View; 3] = [View::Overview, View::Scan, View::Export];

    pub fn icon(self) -> &'static str {
        match self {
            View::Projects => "🗀",
            View::Marketplace => "🔌",
            View::Settings => "🔧",
            View::Overview => "🏠",
            View::Scan => "🔍",
            View::Export => "📦",
        }
    }

    pub fn title(self) -> &'static str {
        match self {
            View::Projects => "Projects",
            View::Marketplace => "Marketplace",
            View::Settings => "Settings",
            View::Overview => "Game",
            View::Scan => "Scan",
            View::Export => "Export",
        }
    }

    pub fn parse(s: &str) -> Option<View> {
        let s = s.to_ascii_lowercase();
        let alias = match s.as_str() {
            "home" | "welcome" => Some(View::Projects),
            "project" | "overview" | "options" => Some(View::Overview),
            "share" => Some(View::Export),
            "plugins" | "market" => Some(View::Marketplace),
            "doctor" | "targets" | "about" => Some(View::Settings),
            _ => None,
        };
        alias.or_else(|| {
            View::ALL
                .into_iter()
                .find(|v| v.title().eq_ignore_ascii_case(&s))
        })
    }
}

/// Sub-pages of Settings.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SettingsTab {
    Defaults,
    Environment,
    Targets,
    About,
}

/// Sub-pages of the Marketplace.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MarketTab {
    Browse,
    Installed,
    Publish,
}

/// How the project list is sorted.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProjectSort {
    LastOpened,
    Name,
    Engine,
}

/// One row of the project list.
#[derive(Clone, Debug)]
pub struct ProjectRow {
    pub path: PathBuf,
    pub name: String,
    pub title: Option<String>,
    pub engine: Option<String>,
    /// Its icon: chosen art, or the game's own icon on this computer.
    pub icon: Option<PathBuf>,
    pub modified: Option<std::time::SystemTime>,
    pub missing: bool,
}

impl ProjectRow {
    pub fn load(path: &Path) -> Self {
        match Manifest::load(path) {
            Ok(m) => ProjectRow {
                path: path.to_owned(),
                icon: ptk_core::art::resolve(path, &m, ptk_core::art::ArtKind::Icon),
                name: m.name,
                title: m.game.title,
                engine: m.game.engine,
                modified: std::fs::metadata(path.join(MANIFEST_FILE))
                    .and_then(|m| m.modified())
                    .ok(),
                missing: false,
            },
            Err(_) => ProjectRow {
                path: path.to_owned(),
                icon: None,
                name: "Missing project".into(),
                title: None,
                engine: None,
                modified: None,
                missing: true,
            },
        }
    }
}

/// Something waiting for the user's consent before it is installed.
#[derive(Clone, Debug)]
pub enum PendingInstall {
    Plugin(Validation),
    Sdk(ptk_core::sdk::SdkReport),
}

/// Remembered between runs (`<data>/gui-state.json`).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct GuiState {
    #[serde(default)]
    pub recent_projects: Vec<PathBuf>,
    #[serde(default)]
    pub recent_games: Vec<PathBuf>,
    #[serde(default)]
    pub favorites: Vec<PathBuf>,
}

const MAX_RECENT: usize = 24;

impl GuiState {
    pub fn load(path: &Path) -> Self {
        std::fs::read(path)
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default()
    }

    pub fn save(&self, path: &Path) {
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        if let Ok(json) = serde_json::to_vec_pretty(self) {
            let _ = std::fs::write(path, json);
        }
    }

    pub fn remember(list: &mut Vec<PathBuf>, path: &Path) {
        let path = path.canonicalize().unwrap_or_else(|_| path.to_owned());
        list.retain(|p| p != &path);
        list.insert(0, path);
        list.truncate(MAX_RECENT);
    }
}

/// The open project.
pub struct OpenProject {
    pub dir: PathBuf,
    pub manifest: Manifest,
    /// As on disk, to detect unsaved edits.
    pub saved: Manifest,
    pub authors_text: String,
    pub contents: Contents,
}

impl OpenProject {
    pub fn dirty(&self) -> bool {
        self.manifest != self.saved
    }
}

/// Summary of a finished pack.
#[derive(Clone, Debug)]
pub struct PackSummary {
    pub output: PathBuf,
    pub files: Vec<(String, u64)>,
    pub bytes: u64,
    pub licensed_third_party: Vec<String>,
}

/// Results coming back from background jobs.
/// A finished import: project folder, scan, and what the import plugin found.
pub type ImportDone = (PathBuf, ScanResult, Option<Result<RunResult, String>>);

pub enum Msg {
    ScanProgress(u64),
    Scanned(Result<ScanResult, String>),
    Recorded(Result<usize, String>),
    Packed(Result<PackSummary, String>),
    Unpacked(Result<UnpackReport, String>),
    Verified(Result<VerifyReport, String>),
    Doctor(Vec<Check>),
    Plugins(Result<Vec<ListedPlugin>, String>),
    Validated(Result<Validation, String>),
    PluginCreated(Result<CreatedPlugin, String>),
    Installed(Result<(PathBuf, usize), String>),
    Imported(Result<ImportDone, String>),
    Market(Result<(MarketIndex, Vec<String>), String>),
    /// A marketplace download, ready for consent (or a project to unpack).
    Downloaded(Result<Downloaded, String>),
    SdkInstalled(Result<ptk_core::sdk::Installed, String>),
    SdkCreated(Result<PathBuf, String>),
    Published(Result<Published, String>),
    FoundProjects(Vec<PathBuf>),
    /// Progress of a running plugin: fraction (if known) and a status line.
    PluginProgress(Option<f64>, String),
    Exported(Vec<(String, Result<RunResult, String>)>),
    /// A marketplace poster downloaded into the cache (by URL).
    Poster(String, Result<PathBuf, String>),
}

pub enum Downloaded {
    Install(Box<PendingInstall>),
    Project(PathBuf),
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum ToastKind {
    Info,
    Ok,
    Error,
}

/// Startup options (command line).
#[derive(Clone, Debug, Default)]
pub struct Config {
    pub state_file: Option<PathBuf>,
    pub view: Option<View>,
    pub project: Option<PathBuf>,
    pub game: Option<PathBuf>,
    /// Start the job of the first view (scan, doctor, plugin list).
    pub auto_run: bool,
    pub screenshot: Option<PathBuf>,
    pub smoke: bool,
    /// Install the native menu bar (macOS; never in tests).
    pub native_menu: bool,
}

pub struct App {
    pub view: View,
    pub registry: TargetRegistry,
    pub state: GuiState,
    state_file: PathBuf,

    pub project: Option<OpenProject>,
    pub new_dir: String,
    pub new_title: String,

    pub game_input: String,
    pub scan: Option<ScanResult>,
    pub scan_progress: Option<u64>,

    pub pack_output: String,
    pub pack_overwrite: bool,
    pub pack_result: Option<Result<PackSummary, String>>,
    pub unpack_input: String,
    pub unpack_dest: String,
    pub unpack_result: Option<Result<UnpackReport, String>>,
    pub verify_game: String,
    pub verify_result: Option<Result<VerifyReport, String>>,

    pub doctor: Option<Vec<Check>>,
    pub plugins: Option<Result<Vec<ListedPlugin>, String>>,
    pub plugin_dir: String,
    pub plugin_new_dir: String,
    pub plugin_kind_export: bool,
    pub plugin_validation: Option<Result<Validation, String>>,
    pub pending: Option<PendingInstall>,

    pub settings_tab: SettingsTab,
    pub market_tab: MarketTab,
    pub rows: Vec<ProjectRow>,
    pub selected: Option<PathBuf>,
    pub filter: String,
    pub sort: ProjectSort,
    pub import_open: bool,
    pub import_build: String,
    pub import_dest: String,
    pub market: Option<Result<MarketIndex, String>>,
    pub market_search: String,
    pub market_kind: Option<EntryKind>,
    pub publish_path: String,
    pub published: Option<Result<Published, String>>,
    pub sdk_new_dir: String,

    pub busy: Option<String>,
    /// Plugin progress while a job runs plugins.
    pub plugin_progress: Option<(Option<f64>, String)>,
    pub export_output: String,
    /// Plugin settings, one `key=value` per line.
    pub export_settings: String,
    pub export_results: Vec<(String, Result<RunResult, String>)>,
    /// What the import plugin found, for the open project.
    pub import_result: Option<Result<RunResult, String>>,
    /// The local web server behind "Play in browser".
    pub server: Option<ptk_core::serve::Server>,
    /// App-wide defaults (Settings → Defaults) and the saved copy.
    pub app_settings: ptk_core::settings::AppSettings,
    pub app_settings_saved: ptk_core::settings::AppSettings,
    /// Marketplace posters by URL: `None` while downloading or if it failed.
    pub posters: std::collections::HashMap<String, Option<PathBuf>>,
    /// Decoded icons and posters, by file.
    pub images: std::collections::HashMap<PathBuf, Option<egui::TextureHandle>>,
    pub toast: Option<(String, ToastKind, Instant)>,
    tx: Sender<Msg>,
    rx: Receiver<Msg>,
    menu_rx: Receiver<Command>,
    #[cfg(target_os = "macos")]
    native_menu: Option<crate::menu::NativeMenu>,

    created: Instant,
    screenshot: Option<PathBuf>,
    screenshot_requested: bool,
    smoke: bool,
    auto_run: bool,
}

impl App {
    pub fn new(config: Config, ctx: &egui::Context) -> Self {
        let (tx, rx) = channel();
        #[allow(unused_variables)]
        let (menu_tx, menu_rx) = channel();
        #[cfg(target_os = "macos")]
        let native_menu = config.native_menu.then(|| {
            let ctx = ctx.clone();
            crate::menu::NativeMenu::install(menu_tx, move || ctx.request_repaint())
        });
        #[cfg(not(target_os = "macos"))]
        let _ = (ctx, &config.native_menu);
        let state_file = config
            .state_file
            .clone()
            .unwrap_or_else(|| ptk_core::paths::data_dir().join("gui-state.json"));
        let mut app = App {
            view: config.view.unwrap_or(View::Projects),
            registry: TargetRegistry::load_default().0,
            state: GuiState::load(&state_file),
            state_file,
            project: None,
            new_dir: String::new(),
            new_title: String::new(),
            game_input: config
                .game
                .as_ref()
                .map(|g| g.display().to_string())
                .unwrap_or_default(),
            scan: None,
            scan_progress: None,
            pack_output: String::new(),
            pack_overwrite: false,
            pack_result: None,
            unpack_input: String::new(),
            unpack_dest: String::new(),
            unpack_result: None,
            verify_game: config
                .game
                .as_ref()
                .map(|g| g.display().to_string())
                .unwrap_or_default(),
            verify_result: None,
            doctor: None,
            plugins: None,
            plugin_dir: String::new(),
            plugin_new_dir: String::new(),
            plugin_kind_export: false,
            plugin_validation: None,
            pending: None,
            settings_tab: SettingsTab::Defaults,
            market_tab: MarketTab::Browse,
            rows: Vec::new(),
            selected: None,
            filter: String::new(),
            sort: ProjectSort::LastOpened,
            import_open: false,
            import_build: String::new(),
            import_dest: String::new(),
            market: None,
            market_search: String::new(),
            market_kind: None,
            publish_path: String::new(),
            published: None,
            sdk_new_dir: String::new(),
            busy: None,
            plugin_progress: None,
            export_output: String::new(),
            export_settings: String::new(),
            export_results: Vec::new(),
            import_result: None,
            server: None,
            app_settings: ptk_core::settings::AppSettings::load(),
            app_settings_saved: ptk_core::settings::AppSettings::load(),
            images: Default::default(),
            posters: Default::default(),
            toast: None,
            tx,
            rx,
            menu_rx,
            #[cfg(target_os = "macos")]
            native_menu,
            created: Instant::now(),
            screenshot: config.screenshot,
            screenshot_requested: false,
            smoke: config.smoke,
            auto_run: config.auto_run,
        };
        let view = app.view;
        if let Some(p) = config.project {
            app.open_project(&p);
            app.view = view;
        }
        app.refresh_rows();
        app
    }

    /// Reloads the project list from the remembered projects.
    pub fn refresh_rows(&mut self) {
        self.rows = self
            .state
            .recent_projects
            .iter()
            .map(|p| ProjectRow::load(p))
            .collect();
    }

    /// Rows after filtering and sorting (favorites first).
    pub fn visible_rows(&self) -> Vec<ProjectRow> {
        let q = self.filter.trim().to_lowercase();
        let mut rows: Vec<ProjectRow> = self
            .rows
            .iter()
            .filter(|r| {
                q.is_empty()
                    || r.name.to_lowercase().contains(&q)
                    || r.path.display().to_string().to_lowercase().contains(&q)
                    || r.engine.as_deref().unwrap_or("").contains(&q)
            })
            .cloned()
            .collect();
        match self.sort {
            ProjectSort::LastOpened => {}
            ProjectSort::Name => rows.sort_by_key(|r| r.name.to_lowercase()),
            ProjectSort::Engine => {
                rows.sort_by_key(|r| r.engine.clone().unwrap_or_else(|| "~".into()))
            }
        }
        let fav = &self.state.favorites;
        rows.sort_by_key(|r| !fav.contains(&r.path));
        rows
    }

    pub fn toggle_favorite(&mut self, path: &Path) {
        let favs = &mut self.state.favorites;
        if let Some(i) = favs.iter().position(|p| p == path) {
            favs.remove(i);
        } else {
            favs.push(path.to_owned());
        }
        self.state.save(&self.state_file);
    }

    /// Forgets a project (the folder is not touched).
    pub fn forget_project(&mut self, path: &Path) {
        self.state.recent_projects.retain(|p| p != path);
        self.state.favorites.retain(|p| p != path);
        self.state.save(&self.state_file);
        if self.project.as_ref().is_some_and(|p| p.dir == path) {
            self.project = None;
        }
        if self.selected.as_deref() == Some(path) {
            self.selected = None;
        }
        self.refresh_rows();
    }

    pub fn forget_missing(&mut self) {
        let missing: Vec<PathBuf> = self
            .rows
            .iter()
            .filter(|r| r.missing)
            .map(|r| r.path.clone())
            .collect();
        for p in missing {
            self.forget_project(&p);
        }
    }

    // ---------------------------------------------------------------- project

    pub fn open_project(&mut self, dir: &Path) {
        match Manifest::load(dir) {
            Ok(m) => {
                let same = self.project.as_ref().is_some_and(|p| p.dir == dir);
                let contents = ptk_core::project::contents(dir).unwrap_or_default();
                self.pack_output = dir
                    .parent()
                    .unwrap_or(dir)
                    .join(ptk_core::pack::default_pack_name(&m))
                    .display()
                    .to_string();
                self.project = Some(OpenProject {
                    dir: dir.to_owned(),
                    authors_text: m.authors.join(", "),
                    saved: m.clone(),
                    manifest: m,
                    contents,
                });
                GuiState::remember(&mut self.state.recent_projects, dir);
                self.state.save(&self.state_file);
                self.selected = dir.canonicalize().ok().or(Some(dir.to_owned()));
                self.pack_result = None;
                self.verify_result = None;
                if !same {
                    self.export_results.clear();
                    self.import_result = None;
                }
                self.export_output = widgets::display_path(&convert::default_output(dir));
                if !View::WORKSPACE.contains(&self.view) {
                    self.view = View::Overview;
                }
                self.refresh_rows();
            }
            Err(e) => self.notify(e.to_string(), ToastKind::Error),
        }
    }

    pub fn create_project(&mut self) {
        let dir = PathBuf::from(self.new_dir.trim());
        if dir.as_os_str().is_empty() {
            return self.notify(
                "Choose a folder for the new project".into(),
                ToastKind::Error,
            );
        }
        let title = Some(self.new_title.trim().to_owned()).filter(|t| !t.is_empty());
        let opts = ptk_core::project::InitOptions::with_defaults(title);
        match ptk_core::project::init(&dir, opts, &self.registry) {
            Ok(m) => {
                self.notify(format!("Created project {}", m.name), ToastKind::Ok);
                self.new_dir.clear();
                self.new_title.clear();
                self.open_project(&dir);
                self.view = View::Overview;
            }
            Err(e) => self.notify(e.to_string(), ToastKind::Error),
        }
    }

    pub fn save_project(&mut self) {
        let Some(p) = &mut self.project else { return };
        let path = p.dir.join(MANIFEST_FILE);
        let result = p
            .manifest
            .check(&path, &self.registry)
            .map_err(|e| e.to_string())
            .and_then(|_| p.manifest.save(&p.dir).map_err(|e| e.to_string()));
        match result {
            Ok(_) => {
                p.saved = p.manifest.clone();
                self.notify(format!("Saved {}", path.display()), ToastKind::Ok);
            }
            Err(e) => self.notify(e, ToastKind::Error),
        }
    }

    pub fn revert_project(&mut self) {
        if let Some(p) = &mut self.project {
            p.manifest = p.saved.clone();
            p.authors_text = p.manifest.authors.join(", ");
        }
    }

    pub fn issues(&self) -> Vec<Issue> {
        self.project
            .as_ref()
            .map(|p| p.manifest.validate(&self.registry))
            .unwrap_or_default()
    }

    // ------------------------------------------------------------------- jobs

    pub fn notify(&mut self, text: String, kind: ToastKind) {
        self.toast = Some((text, kind, Instant::now()));
    }

    fn spawn(
        &mut self,
        ctx: &egui::Context,
        label: &str,
        job: impl FnOnce(&Sender<Msg>) -> Msg + Send + 'static,
    ) {
        if self.busy.is_some() {
            return;
        }
        self.busy = Some(label.to_owned());
        let tx = self.tx.clone();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let msg = job(&tx);
            let _ = tx.send(msg);
            ctx.request_repaint();
        });
    }

    pub fn start_scan(&mut self, ctx: &egui::Context) {
        let game = PathBuf::from(self.game_input.trim());
        if game.as_os_str().is_empty() {
            return self.notify("Choose a game folder to scan".into(), ToastKind::Error);
        }
        GuiState::remember(&mut self.state.recent_games, &game);
        self.state.save(&self.state_file);
        self.scan = None;
        self.scan_progress = Some(0);
        let repaint = ctx.clone();
        self.spawn(ctx, "Scanning", move |tx| {
            let tx = tx.clone();
            let result = EngineClient::start()
                .map_err(ptk_core::Error::from)
                .and_then(|mut engine| {
                    let r = ptk_core::scan::scan(&mut engine, &game, &mut |n| {
                        let _ = tx.send(Msg::ScanProgress(n));
                        repaint.request_repaint();
                    });
                    let _ = engine.shutdown();
                    r
                });
            Msg::Scanned(result.map(|(s, _)| s).map_err(|e| e.to_string()))
        });
    }

    pub fn start_record(&mut self, ctx: &egui::Context) {
        let (Some(p), Some(scan)) = (&self.project, &self.scan) else {
            return;
        };
        if p.dirty() {
            return self.notify("Save the project first".into(), ToastKind::Error);
        }
        let dir = p.dir.clone();
        let game = PathBuf::from(&scan.path);
        let scan = scan.clone();
        self.spawn(ctx, "Hashing game files", move |_| {
            Msg::Recorded(
                ptk_core::scan::record(&dir, &game, &scan)
                    .map(|r| r.files)
                    .map_err(|e| e.to_string()),
            )
        });
    }

    pub fn start_pack(&mut self, ctx: &egui::Context) {
        let Some(p) = &self.project else { return };
        if p.dirty() {
            return self.notify("Save the project first".into(), ToastKind::Error);
        }
        let dir = p.dir.clone();
        let output = PathBuf::from(self.pack_output.trim());
        let overwrite = self.pack_overwrite;
        let registry = self.registry.clone();
        self.pack_result = None;
        self.spawn(ctx, "Packing", move |_| {
            let r = ptk_core::pack::pack(&dir, &output, &registry, PackOptions { overwrite });
            Msg::Packed(
                r.map(|r| PackSummary {
                    output: r.output,
                    files: r.entries.iter().map(|e| (e.path.clone(), e.size)).collect(),
                    bytes: r.bytes,
                    licensed_third_party: r
                        .licensed_third_party
                        .iter()
                        .map(|f| f.to_string())
                        .collect(),
                })
                .map_err(|e| e.to_string()),
            )
        });
    }

    pub fn start_unpack(&mut self, ctx: &egui::Context) {
        let pack = PathBuf::from(self.unpack_input.trim());
        let dest = PathBuf::from(self.unpack_dest.trim());
        if pack.as_os_str().is_empty() || dest.as_os_str().is_empty() {
            return self.notify(
                "Choose a .ptk file and a destination folder".into(),
                ToastKind::Error,
            );
        }
        self.unpack_result = None;
        self.spawn(ctx, "Unpacking", move |_| {
            Msg::Unpacked(ptk_core::pack::unpack(&pack, &dest).map_err(|e| e.to_string()))
        });
    }

    pub fn start_verify(&mut self, ctx: &egui::Context) {
        let Some(p) = &self.project else { return };
        let manifest = p.saved.clone();
        let game = PathBuf::from(self.verify_game.trim());
        self.verify_result = None;
        self.spawn(ctx, "Verifying", move |_| {
            Msg::Verified(
                ptk_core::verify::verify_game(&game, &manifest, |_, _| {})
                    .map_err(|e| e.to_string()),
            )
        });
    }

    pub fn start_doctor(&mut self, ctx: &egui::Context, install: bool) {
        self.spawn(
            ctx,
            if install {
                "Setting up the engine"
            } else {
                "Running checks"
            },
            move |_| Msg::Doctor(ptk_core::doctor::run(install)),
        );
    }

    pub fn refresh_plugins(&mut self, ctx: &egui::Context) {
        self.spawn(ctx, "Loading plugins", move |_| {
            let r = EngineClient::start()
                .map_err(ptk_core::Error::from)
                .and_then(|mut e| {
                    let r = ptk_core::plugins::list(&mut e, &ptk_core::plugins::default_dirs());
                    let _ = e.shutdown();
                    r
                });
            Msg::Plugins(r.map(|(l, _)| l).map_err(|e| e.to_string()))
        });
    }

    pub fn validate_plugin(&mut self, ctx: &egui::Context) {
        let dir = PathBuf::from(self.plugin_dir.trim());
        self.plugin_validation = None;
        self.spawn(ctx, "Validating plugin", move |_| {
            let r = EngineClient::start()
                .map_err(ptk_core::Error::from)
                .and_then(|mut e| {
                    let r = ptk_core::plugins::validate(&mut e, &dir);
                    let _ = e.shutdown();
                    r
                });
            Msg::Validated(r.map(|(v, _)| v).map_err(|e| e.to_string()))
        });
    }

    pub fn create_plugin(&mut self, ctx: &egui::Context) {
        let dir = PathBuf::from(self.plugin_new_dir.trim());
        let kind = if self.plugin_kind_export {
            "export"
        } else {
            "import"
        };
        self.spawn(ctx, "Creating plugin", move |_| {
            let r = EngineClient::start()
                .map_err(ptk_core::Error::from)
                .and_then(|mut e| {
                    let r = ptk_core::plugins::create(&mut e, &dir, kind, None);
                    let _ = e.shutdown();
                    r
                });
            Msg::PluginCreated(r.map_err(|e| e.to_string()))
        });
    }

    /// Installs what the user just agreed to.
    pub fn confirm_pending(&mut self, ctx: &egui::Context) {
        let Some(p) = self.pending.take() else {
            return;
        };
        match p {
            PendingInstall::Plugin(v) => {
                let dir = PathBuf::from(&v.path);
                self.spawn(ctx, "Installing plugin", move |_| {
                    Msg::Installed(ptk_core::plugins::install(&dir, &v).map_err(|e| e.to_string()))
                });
            }
            PendingInstall::Sdk(r) => {
                self.spawn(ctx, "Installing SDK", move |_| {
                    Msg::SdkInstalled(ptk_core::sdk::install(&r).map_err(|e| e.to_string()))
                });
            }
        }
    }

    /// Asks to install the plugin that was just validated.
    pub fn install_plugin(&mut self) {
        if let Some(Ok(v)) = &self.plugin_validation {
            self.pending = Some(PendingInstall::Plugin(v.clone()));
        }
    }

    /// Import Game: unpack if needed, scan, create the project, record hashes.
    pub fn import_game(&mut self, ctx: &egui::Context) {
        let build = PathBuf::from(self.import_build.trim());
        if build.as_os_str().is_empty() {
            return self.notify("Choose the game build to import".into(), ToastKind::Error);
        }
        let dest =
            Some(PathBuf::from(self.import_dest.trim())).filter(|d| !d.as_os_str().is_empty());
        let registry = self.registry.clone();
        self.import_open = false;
        let repaint = ctx.clone();
        self.spawn(ctx, "Importing game", move |tx| {
            let tx = tx.clone();
            let run = || -> ptk_core::Result<ImportDone> {
                let prepared = ptk_core::import::prepare(&build)?;
                let mut engine = EngineClient::start()?;
                let (scan, _) = ptk_core::scan::scan(&mut engine, &prepared.game, &mut |_| {})?;
                let project = dest.unwrap_or_else(|| default_project_dir(&prepared.title));
                if !project.join(MANIFEST_FILE).exists() {
                    ptk_core::project::init(
                        &project,
                        ptk_core::project::InitOptions::with_defaults(Some(prepared.title.clone())),
                        &registry,
                    )?;
                }
                ptk_core::scan::record(&project, &prepared.game, &scan)?;
                if prepared.extracted.is_some() {
                    convert::LocalState::remember_game(&project, &prepared.game, Some(&build))?;
                }
                let plugin = convert::import(
                    &mut engine,
                    &project,
                    &prepared.game,
                    Some(&prepared.title),
                    &mut |e| {
                        if let Event::Progress { fraction, message } = e {
                            let _ = tx.send(Msg::PluginProgress(fraction, message));
                            repaint.request_repaint();
                        }
                    },
                )
                .map_err(|e| e.to_string())
                .transpose();
                let _ = engine.shutdown();
                Ok((project, scan, plugin))
            };
            Msg::Imported(run().map_err(|e| e.to_string()))
        });
    }

    /// Exports the open project for `targets`, one after the other.
    pub fn export_targets(&mut self, ctx: &egui::Context, targets: Vec<String>) {
        let Some(p) = &self.project else { return };
        if targets.is_empty() {
            return self.notify("Choose a target in Overview first".into(), ToastKind::Error);
        }
        let lines: Vec<String> = self
            .export_settings
            .lines()
            .map(str::trim)
            .filter(|l| !l.is_empty() && !l.starts_with('#'))
            .map(str::to_owned)
            .collect();
        let settings = match convert::parse_settings(&lines) {
            Ok(s) => s,
            Err(e) => return self.notify(e.to_string(), ToastKind::Error),
        };
        let opts = convert::ExportOptions {
            output: Some(expand_tilde(self.export_output.trim()))
                .filter(|o| !o.as_os_str().is_empty()),
            settings,
            ..Default::default()
        };
        let dir = p.dir.clone();
        self.export_results.clear();
        self.plugin_progress = Some((Some(0.0), "Starting".into()));
        let repaint = ctx.clone();
        self.spawn(ctx, "Exporting", move |tx| {
            let tx = tx.clone();
            let mut out = Vec::new();
            match EngineClient::start() {
                Err(e) => {
                    for t in targets {
                        out.push((t, Err(e.to_string())));
                    }
                }
                Ok(mut engine) => {
                    for t in &targets {
                        let r = convert::export(&mut engine, &dir, t, &opts, &mut |e| {
                            if let Event::Progress { fraction, message } = e {
                                let _ = tx
                                    .send(Msg::PluginProgress(fraction, format!("{t}: {message}")));
                                repaint.request_repaint();
                            }
                        });
                        out.push((t.clone(), r.map_err(|e| e.to_string())));
                    }
                    let _ = engine.shutdown();
                }
            }
            Msg::Exported(out)
        });
    }

    /// An icon or poster as a texture (decoded once, scaled to 512 px).
    pub fn image(&mut self, ctx: &egui::Context, path: &Path) -> Option<egui::TextureHandle> {
        if let Some(t) = self.images.get(path) {
            return t.clone();
        }
        let t = std::fs::read(path)
            .ok()
            .and_then(|b| image::load_from_memory(&b).ok())
            .map(|img| {
                let img = img.thumbnail(512, 512).to_rgba8();
                let size = [img.width() as usize, img.height() as usize];
                ctx.load_texture(
                    path.display().to_string(),
                    egui::ColorImage::from_rgba_unmultiplied(size, img.as_raw()),
                    egui::TextureOptions::LINEAR,
                )
            });
        self.images.insert(path.to_owned(), t.clone());
        t
    }

    /// A marketplace poster, downloaded in the background the first time.
    pub fn poster(&mut self, ctx: &egui::Context, url: &str) -> Option<egui::TextureHandle> {
        match self.posters.get(url) {
            Some(Some(path)) => {
                let path = path.clone();
                self.image(ctx, &path)
            }
            Some(None) => None,
            None => {
                self.posters.insert(url.to_owned(), None);
                let (tx, url, ctx) = (self.tx.clone(), url.to_owned(), ctx.clone());
                std::thread::spawn(move || {
                    let r = ptk_core::market::fetch_poster(&url).map_err(|e| e.to_string());
                    let _ = tx.send(Msg::Poster(url, r));
                    ctx.request_repaint();
                });
                None
            }
        }
    }

    /// Lets you pick an image as the open project's icon or poster.
    pub fn choose_art(&mut self, kind: ptk_core::art::ArtKind) {
        let Some(dir) = self.project.as_ref().map(|p| p.dir.clone()) else {
            return;
        };
        let Some(file) = rfd::FileDialog::new()
            .set_title(format!("Choose the {}", kind.name()))
            .add_filter("Image", ptk_core::art::EXTENSIONS)
            .pick_file()
        else {
            return;
        };
        self.set_art(&dir, kind, Some(&file));
    }

    /// Sets (or with `None`, removes) a project's icon or poster.
    pub fn set_art(&mut self, dir: &Path, kind: ptk_core::art::ArtKind, file: Option<&Path>) {
        let r = match file {
            Some(f) => ptk_core::art::set(dir, kind, f).map(|_| ()),
            None => ptk_core::art::clear(dir, kind),
        };
        match r {
            Ok(()) => {
                self.images.clear();
                // Keep unsaved edits: only the art fields come from disk.
                let saved = Manifest::load(dir).ok();
                if let (Some(p), Some(m)) = (self.project.as_mut(), saved) {
                    p.manifest.game.icon = m.game.icon.clone();
                    p.manifest.game.poster = m.game.poster.clone();
                    p.saved.game.icon = m.game.icon;
                    p.saved.game.poster = m.game.poster;
                }
                self.refresh_rows();
            }
            Err(e) => self.notify(e.to_string(), ToastKind::Error),
        }
    }

    pub fn save_app_settings(&mut self) {
        match self.app_settings.save() {
            Ok(()) => {
                self.app_settings_saved = self.app_settings.clone();
                self.notify("Defaults saved".into(), ToastKind::Ok);
            }
            Err(e) => self.notify(e.to_string(), ToastKind::Error),
        }
    }

    /// Serves a web build on this computer and opens it in the browser.
    pub fn play_in_browser(&mut self, dir: &Path) {
        self.server = None;
        match ptk_core::serve::start(dir, 0) {
            Ok(s) => {
                open_url(&s.url());
                self.notify(format!("Serving at {}", s.url()), ToastKind::Info);
                self.server = Some(s);
            }
            Err(e) => self.notify(e.to_string(), ToastKind::Error),
        }
    }

    pub fn refresh_market(&mut self, ctx: &egui::Context) {
        let url = ptk_core::market::index_url();
        self.market = None;
        self.spawn(ctx, "Loading the marketplace", move |_| {
            Msg::Market(ptk_core::market::fetch_index(&url).map_err(|e| e.to_string()))
        });
    }

    /// Downloads a marketplace entry, checks it, and asks before installing.
    pub fn market_install(&mut self, ctx: &egui::Context, entry: MarketEntry) {
        self.spawn(ctx, &format!("Downloading {}", entry.id), move |_| {
            let run = || -> ptk_core::Result<Downloaded> {
                let path = ptk_core::market::download(&entry)?;
                if entry.kind == EntryKind::Project {
                    // A port is a project: unpack it next to your others.
                    let root = ptk_core::settings::AppSettings::load().projects_dir();
                    let mut dir = root.join(&entry.id);
                    let mut n = 2;
                    while dir.exists() {
                        dir = root.join(format!("{}-{n}", entry.id));
                        n += 1;
                    }
                    ptk_core::pack::unpack(&path, &dir)?;
                    return Ok(Downloaded::Project(dir));
                }
                let mut e = EngineClient::start()?;
                let pending = if entry.kind == EntryKind::Sdk {
                    PendingInstall::Sdk(ptk_core::sdk::validate(&path, &mut e)?)
                } else {
                    PendingInstall::Plugin(ptk_core::plugins::validate(&mut e, &path)?.0)
                };
                let _ = e.shutdown();
                Ok(Downloaded::Install(Box::new(pending)))
            };
            Msg::Downloaded(run().map_err(|e| e.to_string()))
        });
    }

    pub fn create_sdk(&mut self, ctx: &egui::Context) {
        let dir = PathBuf::from(self.sdk_new_dir.trim());
        if dir.as_os_str().is_empty() {
            return self.notify("Choose a folder for the new SDK".into(), ToastKind::Error);
        }
        let name = dir
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        let id = ptk_core::manifest::slugify(&name)
            .trim_end_matches("-sdk")
            .to_owned();
        self.spawn(ctx, "Creating SDK", move |_| {
            let run = || -> ptk_core::Result<PathBuf> {
                let mut e = EngineClient::start()?;
                ptk_core::sdk::create(&dir, &id, &mut e)?;
                let _ = e.shutdown();
                Ok(dir)
            };
            Msg::SdkCreated(run().map_err(|e| e.to_string()))
        });
    }

    /// Validates and packages a plugin, SDK or .ptk for the marketplace.
    pub fn publish(&mut self, ctx: &egui::Context) {
        let src = PathBuf::from(self.publish_path.trim());
        if src.as_os_str().is_empty() {
            return self.notify("Choose what to publish".into(), ToastKind::Error);
        }
        let out = src.parent().unwrap_or(Path::new(".")).join("dist");
        self.published = None;
        self.spawn(ctx, "Packaging", move |_| {
            let run = || -> ptk_core::Result<Published> {
                let mut e = EngineClient::start()?;
                let p = ptk_core::market::package(&src, &out, &mut e)?;
                let _ = e.shutdown();
                Ok(p)
            };
            Msg::Published(run().map_err(|e| e.to_string()))
        });
    }

    /// Finds `ptk.json` projects under a folder (a few levels deep).
    pub fn find_projects(&mut self, ctx: &egui::Context, root: PathBuf) {
        self.spawn(ctx, "Looking for projects", move |_| {
            fn walk(dir: &Path, depth: usize, out: &mut Vec<PathBuf>) {
                if dir.join(MANIFEST_FILE).is_file() {
                    out.push(dir.to_owned());
                    return;
                }
                if depth == 0 {
                    return;
                }
                if let Ok(rd) = std::fs::read_dir(dir) {
                    for e in rd.flatten() {
                        let name = e.file_name().to_string_lossy().into_owned();
                        let skip =
                            name.starts_with('.') || name == "node_modules" || name == "target";
                        if !skip && e.file_type().is_ok_and(|t| t.is_dir()) {
                            walk(&e.path(), depth - 1, out);
                        }
                    }
                }
            }
            let mut out = Vec::new();
            walk(&root, 4, &mut out);
            Msg::FoundProjects(out)
        });
    }

    /// Applies finished job results.
    pub fn poll(&mut self, ctx: &egui::Context) {
        while let Ok(msg) = self.rx.try_recv() {
            if !matches!(
                msg,
                Msg::ScanProgress(_) | Msg::PluginProgress(..) | Msg::Poster(..)
            ) {
                self.busy = None;
            }
            match msg {
                Msg::ScanProgress(n) => self.scan_progress = Some(n),
                Msg::PluginProgress(f, text) => self.plugin_progress = Some((f, text)),
                Msg::Poster(url, r) => {
                    self.posters.insert(url, r.ok());
                }
                Msg::Exported(results) => {
                    self.plugin_progress = None;
                    let ok = results.iter().filter(|(_, r)| r.is_ok()).count();
                    let (text, kind) = if ok == results.len() {
                        (format!("Exported {ok} build(s)"), ToastKind::Ok)
                    } else {
                        (
                            format!("{} of {} exports failed", results.len() - ok, results.len()),
                            ToastKind::Error,
                        )
                    };
                    self.export_results = results;
                    self.notify(text, kind);
                }
                Msg::Scanned(r) => {
                    self.scan_progress = None;
                    match r {
                        Ok(s) => self.scan = Some(s),
                        Err(e) => self.notify(e, ToastKind::Error),
                    }
                }
                Msg::Recorded(r) => match r {
                    Ok(n) => {
                        self.notify(
                            format!("Recorded {n} file hashes into the project"),
                            ToastKind::Ok,
                        );
                        if let Some(dir) = self.project.as_ref().map(|p| p.dir.clone()) {
                            self.open_project(&dir);
                        }
                    }
                    Err(e) => self.notify(e, ToastKind::Error),
                },
                Msg::Packed(r) => {
                    if let Ok(s) = &r {
                        self.notify(format!("Packed {}", s.output.display()), ToastKind::Ok);
                    }
                    self.pack_result = Some(r);
                }
                Msg::Unpacked(r) => self.unpack_result = Some(r),
                Msg::Verified(r) => self.verify_result = Some(r),
                Msg::Doctor(checks) => self.doctor = Some(checks),
                Msg::Plugins(r) => self.plugins = Some(r),
                Msg::Validated(r) => self.plugin_validation = Some(r),
                Msg::PluginCreated(r) => match r {
                    Ok(c) => {
                        self.notify(format!("Created plugin {}", c.id), ToastKind::Ok);
                        self.plugin_dir = c.path.clone();
                        self.publish_path = c.path;
                        self.plugin_new_dir.clear();
                    }
                    Err(e) => self.notify(e, ToastKind::Error),
                },
                Msg::Installed(r) => match r {
                    Ok((dest, n)) => {
                        self.notify(
                            format!("Installed {n} files in {}", dest.display()),
                            ToastKind::Ok,
                        );
                        self.registry = TargetRegistry::load_default().0;
                        self.refresh_plugins(ctx);
                    }
                    Err(e) => self.notify(e, ToastKind::Error),
                },
                Msg::Imported(r) => match r {
                    Ok((dir, scan, plugin)) => {
                        self.plugin_progress = None;
                        self.game_input = scan.path.clone();
                        self.scan = Some(scan);
                        self.open_project(&dir);
                        self.view = View::Overview;
                        let text = match &plugin {
                            Some(Ok(r)) => r.summary.clone(),
                            _ => format!("Imported into {}", dir.display()),
                        };
                        self.import_result = plugin;
                        self.notify(text, ToastKind::Ok);
                    }
                    Err(e) => self.notify(e, ToastKind::Error),
                },
                Msg::Market(r) => {
                    self.market = Some(r.map(|(index, _)| index));
                    if self.plugins.is_none() {
                        self.refresh_plugins(ctx);
                    }
                }
                Msg::Downloaded(r) => match r {
                    Ok(Downloaded::Install(p)) => self.pending = Some(*p),
                    Ok(Downloaded::Project(dir)) => {
                        self.open_project(&dir);
                        self.view = View::Scan;
                        self.notify(
                            "Got the port. Now choose your own copy of the game and scan it: the port checks it against its hashes."
                                .into(),
                            ToastKind::Ok,
                        );
                    }
                    Err(e) => self.notify(e, ToastKind::Error),
                },
                Msg::SdkInstalled(r) => match r {
                    Ok(i) => {
                        self.notify(
                            format!(
                                "Installed {} targets and {} plugins",
                                i.targets.len(),
                                i.plugins.len()
                            ),
                            ToastKind::Ok,
                        );
                        self.registry = TargetRegistry::load_default().0;
                        self.refresh_plugins(ctx);
                    }
                    Err(e) => self.notify(e, ToastKind::Error),
                },
                Msg::SdkCreated(r) => match r {
                    Ok(dir) => {
                        self.notify(format!("Created SDK in {}", dir.display()), ToastKind::Ok);
                        self.publish_path = dir.display().to_string();
                        self.sdk_new_dir.clear();
                    }
                    Err(e) => self.notify(e, ToastKind::Error),
                },
                Msg::Published(r) => self.published = Some(r),
                Msg::FoundProjects(list) => {
                    let n = list.len();
                    for p in &list {
                        GuiState::remember(&mut self.state.recent_projects, p);
                    }
                    self.state.save(&self.state_file);
                    self.refresh_rows();
                    self.notify(format!("Found {n} projects"), ToastKind::Info);
                }
            }
        }
    }

    /// Opens what was dropped on the window: a project folder, a `.ptk`
    /// file, or a game build to import.
    pub fn handle_drop(&mut self, path: &Path) {
        if path.join(MANIFEST_FILE).is_file() {
            self.open_project(path);
            self.view = View::Overview;
        } else if path
            .extension()
            .is_some_and(|e| e.eq_ignore_ascii_case("ptk"))
        {
            self.unpack_input = path.display().to_string();
            self.view = View::Export;
        } else if path.exists() {
            self.import_build = path.display().to_string();
            self.import_open = true;
            self.view = View::Projects;
        }
    }

    fn first_run(&mut self, ctx: &egui::Context) {
        if !std::mem::take(&mut self.auto_run) {
            return;
        }
        match self.view {
            View::Scan if !self.game_input.is_empty() => self.start_scan(ctx),
            View::Settings => self.start_doctor(ctx, false),
            View::Marketplace => self.refresh_market(ctx),
            View::Export if !self.verify_game.is_empty() && self.project.is_some() => {
                self.start_verify(ctx)
            }
            View::Export if self.project.is_some() => {
                let targets = self
                    .project
                    .as_ref()
                    .map(|p| p.saved.targets.clone())
                    .unwrap_or_default();
                self.export_targets(ctx, targets)
            }
            _ => {}
        }
    }

    // ----------------------------------------------------------------- layout

    /// What the menus need to know to enable items.
    pub fn menu_state(&self) -> MenuState {
        MenuState {
            project_open: self.project.is_some(),
            dirty: self.project.as_ref().is_some_and(|p| p.dirty()),
            busy: self.busy.is_some(),
            has_game: !self.game_input.trim().is_empty(),
            has_hashes: self
                .project
                .as_ref()
                .is_some_and(|p| !p.saved.source_files.is_empty()),
        }
    }

    /// Runs a menu or shortcut command (ignored while it is disabled).
    pub fn run_command(&mut self, cmd: Command, ctx: &egui::Context) {
        if !self.menu_state().enabled(cmd) {
            return;
        }
        match cmd {
            Command::NewProject => {
                if let Some(dir) = rfd::FileDialog::new()
                    .set_title("Choose a folder for the new project")
                    .pick_folder()
                {
                    if dir.join(MANIFEST_FILE).is_file() {
                        self.open_project(&dir);
                    } else {
                        self.new_dir = dir.display().to_string();
                        self.create_project();
                    }
                }
            }
            Command::ImportGame => {
                self.view = View::Projects;
                self.import_open = true;
            }
            Command::OpenProject => {
                if let Some(dir) = rfd::FileDialog::new()
                    .set_title("Open a Porting Toolkit project")
                    .pick_folder()
                {
                    self.open_project(&dir);
                    self.view = View::Overview;
                }
            }
            Command::OpenRecent(i) => {
                if let Some(p) = self.state.recent_projects.get(i).cloned() {
                    self.open_project(&p);
                    self.view = View::Overview;
                }
            }
            Command::Save => self.save_project(),
            Command::Revert => self.revert_project(),
            Command::CloseProject => {
                self.project = None;
                self.view = View::Projects;
            }
            Command::UnpackPtk => {
                let pack = rfd::FileDialog::new()
                    .add_filter("Porting Toolkit project", &["ptk"])
                    .set_title("Unpack a .ptk file")
                    .pick_file();
                if let Some(pack) = pack {
                    self.unpack_input = pack.display().to_string();
                    self.view = View::Export;
                    if let Some(dest) = rfd::FileDialog::new()
                        .set_title("Choose a new or empty folder to unpack into")
                        .pick_folder()
                    {
                        self.unpack_dest = dest.display().to_string();
                        self.start_unpack(ctx);
                    }
                }
            }
            Command::PackProject => {
                self.view = View::Export;
                self.start_pack(ctx);
            }
            Command::Undo
            | Command::Redo
            | Command::Cut
            | Command::Copy
            | Command::Paste
            | Command::SelectAll => self.inject_edit(ctx, cmd),
            Command::Go(v) => self.go(v, ctx),
            Command::Settings => self.go(View::Settings, ctx),
            Command::ScanGame => {
                if let Some(dir) = rfd::FileDialog::new()
                    .set_title("Choose your game folder")
                    .pick_folder()
                {
                    self.game_input = dir.display().to_string();
                    self.view = View::Scan;
                    self.start_scan(ctx);
                }
            }
            Command::RescanGame => {
                self.view = View::Scan;
                self.start_scan(ctx);
            }
            Command::VerifyGame => {
                if let Some(dir) = rfd::FileDialog::new()
                    .set_title("Choose your copy of the game")
                    .pick_folder()
                {
                    self.verify_game = dir.display().to_string();
                    self.view = View::Export;
                    self.start_verify(ctx);
                }
            }
            Command::FindProjects => {
                if let Some(dir) = rfd::FileDialog::new()
                    .set_title("Find Porting Toolkit projects in a folder")
                    .pick_folder()
                {
                    self.view = View::Projects;
                    self.find_projects(ctx, dir);
                }
            }
            Command::Publish => {
                self.go(View::Marketplace, ctx);
                self.market_tab = MarketTab::Publish;
            }
            Command::NewSdk => {
                self.go(View::Marketplace, ctx);
                self.market_tab = MarketTab::Publish;
                if let Some(dir) = rfd::FileDialog::new()
                    .set_title("Choose a new folder for the SDK")
                    .pick_folder()
                {
                    self.sdk_new_dir = dir.display().to_string();
                    self.create_sdk(ctx);
                }
            }
            Command::RunDoctor => {
                self.view = View::Settings;
                self.settings_tab = SettingsTab::Environment;
                self.start_doctor(ctx, false);
            }
            Command::SetUpEngine => {
                self.view = View::Settings;
                self.settings_tab = SettingsTab::Environment;
                self.start_doctor(ctx, true);
            }
            Command::RevealData => reveal(&ptk_core::paths::data_dir()),
            Command::RevealPlugins => {
                let dir = ptk_core::paths::plugins_dir();
                let _ = std::fs::create_dir_all(&dir);
                reveal(&dir);
            }
            Command::About => {
                self.view = View::Settings;
                self.settings_tab = SettingsTab::About;
            }
            Command::Help(i) => {
                if let Some((_, url)) = crate::menu::HELP_LINKS.get(i) {
                    open_url(url);
                }
            }
        }
    }

    /// Switches view, loading what the view needs.
    pub fn go(&mut self, view: View, ctx: &egui::Context) {
        self.view = view;
        if view == View::Marketplace && self.market.is_none() && self.busy.is_none() {
            self.refresh_market(ctx);
        }
    }

    /// Edit menu items act on the focused text field, as the keys would.
    fn inject_edit(&mut self, ctx: &egui::Context, cmd: Command) {
        let key = |key: Key, shift: bool| egui::Event::Key {
            key,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: if shift {
                Modifiers::COMMAND | Modifiers::SHIFT
            } else {
                Modifiers::COMMAND
            },
        };
        let event = match cmd {
            Command::Copy => egui::Event::Copy,
            Command::Cut => egui::Event::Cut,
            Command::Undo => key(Key::Z, false),
            Command::Redo => key(Key::Z, true),
            Command::SelectAll => key(Key::A, false),
            Command::Paste => match clipboard_text() {
                Some(text) => egui::Event::Paste(text),
                None => return,
            },
            _ => return,
        };
        ctx.input_mut(|i| i.events.push(event));
    }

    /// Keyboard shortcuts for commands, when no native menu handles them.
    fn shortcut_commands(&mut self, ctx: &egui::Context) -> Vec<Command> {
        let mut out = Vec::new();
        if ctx.input_mut(|i| {
            i.consume_shortcut(&KeyboardShortcut::new(Modifiers::COMMAND, Key::Comma))
        }) {
            out.push(Command::Settings);
        }
        for spec in crate::menu::menus() {
            for cmd in spec.items.into_iter().flatten() {
                if matches!(
                    cmd,
                    Command::Undo
                        | Command::Redo
                        | Command::Cut
                        | Command::Copy
                        | Command::Paste
                        | Command::SelectAll
                ) {
                    continue; // egui handles these keys itself
                }
                let Some((shift, ch)) = cmd.shortcut() else {
                    continue;
                };
                let key = Key::from_name(&ch.to_string());
                let Some(key) = key else {
                    continue;
                };
                let mods = if shift {
                    Modifiers::COMMAND | Modifiers::SHIFT
                } else {
                    Modifiers::COMMAND
                };
                if ctx.input_mut(|i| i.consume_shortcut(&KeyboardShortcut::new(mods, key))) {
                    out.push(cmd);
                }
            }
        }
        out
    }

    pub fn layout(&mut self, ctx: &egui::Context) {
        let mut commands: Vec<Command> = self.menu_rx.try_iter().collect();
        commands.extend(self.shortcut_commands(ctx));
        for c in commands {
            self.run_command(c, ctx);
        }
        self.poll(ctx);
        self.first_run(ctx);
        #[cfg(target_os = "macos")]
        {
            let state = self.menu_state();
            if let Some(menu) = &mut self.native_menu {
                menu.sync(state, &self.state.recent_projects);
            }
        }
        let dropped: Vec<PathBuf> = ctx.input(|i| {
            i.raw
                .dropped_files
                .iter()
                .filter_map(|f| f.path.clone())
                .collect()
        });
        for p in dropped {
            self.handle_drop(&p);
        }

        egui::TopBottomPanel::top("tabs")
            .exact_height(54.0)
            .frame(
                Frame::new()
                    .fill(HEADER)
                    .inner_margin(Margin::symmetric(14, 0))
                    .stroke(Stroke::new(1.0_f32, BORDER)),
            )
            .show(ctx, |ui| self.tab_bar(ui));
        egui::TopBottomPanel::bottom("status")
            .exact_height(26.0)
            .frame(
                Frame::new()
                    .fill(HEADER)
                    .inner_margin(Margin::symmetric(14, 0))
                    .stroke(Stroke::new(1.0_f32, BORDER)),
            )
            .show(ctx, |ui| self.status_bar(ui));
        egui::CentralPanel::default()
            .frame(Frame::new().fill(PAGE))
            .show(ctx, |ui| {
                if self.view == View::Projects {
                    Frame::new()
                        .inner_margin(Margin::same(14))
                        .show(ui, |ui| self.projects_page(ui));
                    return;
                }
                egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        let width = ui.available_width();
                        let side = ((width - 1120.0) / 2.0).max(0.0)
                            + if width < 700.0 { 16.0 } else { 32.0 };
                        Frame::new()
                            .inner_margin(Margin {
                                left: side as i8,
                                right: side as i8,
                                top: 24,
                                bottom: 32,
                            })
                            .show(ui, |ui| {
                                ui.set_max_width(ui.available_width());
                                self.page(ui);
                            });
                    });
            });
        self.import_modal(ctx);
        self.install_modal(ctx);
        self.toast_area(ctx);
    }

    /// The top bar: brand, main tabs, the open project's tabs, Settings.
    fn tab_bar(&mut self, ui: &mut egui::Ui) {
        let width = ui.available_width();
        let labels = width >= 980.0;
        ui.horizontal_centered(|ui| {
            #[cfg(not(target_os = "macos"))]
            self.fallback_menu(ui);

            widgets::logo(ui, 28.0);
            if width >= 760.0 {
                ui.label(
                    RichText::new("PORTING TOOLKIT")
                        .size(14.0)
                        .strong()
                        .color(Color32::WHITE),
                );
            }
            ui.add_space(14.0);
            for view in [View::Projects, View::Marketplace] {
                if widgets::tab(ui, view.icon(), view.title(), self.view == view, true)
                    .on_hover_text(Command::Go(view).shortcut_text())
                    .clicked()
                {
                    self.go(view, ui.ctx());
                }
            }
            if let Some(p) = &self.project {
                let name = format!("{}{}", p.manifest.name, if p.dirty() { " ●" } else { "" });
                ui.add_space(6.0);
                ui.label(RichText::new("│").color(BORDER).size(20.0));
                ui.label(RichText::new(name).strong().color(LIGHT_BLUE));
                for view in View::WORKSPACE {
                    if widgets::tab(ui, view.icon(), view.title(), self.view == view, labels)
                        .on_hover_text(format!(
                            "{}  {}",
                            view.title(),
                            Command::Go(view).shortcut_text()
                        ))
                        .clicked()
                    {
                        self.go(view, ui.ctx());
                    }
                }
            }
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                let s = View::Settings;
                if widgets::tab(ui, s.icon(), s.title(), self.view == s, width >= 640.0)
                    .on_hover_text("⌘,")
                    .clicked()
                {
                    self.go(s, ui.ctx());
                }
            });
        });
    }

    fn status_bar(&mut self, ui: &mut egui::Ui) {
        ui.horizontal_centered(|ui| {
            let small = |t: String, c: Color32| RichText::new(t).size(12.0).color(c);
            match &self.busy {
                Some(label) => {
                    ui.add(egui::Spinner::new().size(12.0).color(FOCUS));
                    let extra = match self.scan_progress {
                        Some(n) if n > 0 => format!(" · {n} files"),
                        _ => String::new(),
                    };
                    ui.label(small(format!("{label}…{extra}"), TEXT));
                    if let Some((fraction, text)) = &self.plugin_progress {
                        if let Some(f) = fraction {
                            ui.add(
                                egui::ProgressBar::new(*f as f32)
                                    .desired_width(140.0)
                                    .desired_height(8.0)
                                    .fill(FOCUS),
                            );
                        }
                        ui.label(small(widgets::elide_middle(text, 70), TEXT_WEAK));
                    }
                }
                None => {
                    let text = match &self.project {
                        Some(p) => widgets::display_path(&p.dir),
                        None => format!("{} projects", self.rows.len()),
                    };
                    ui.label(small(text, TEXT_FAINT));
                }
            }
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                let host = self
                    .registry
                    .host()
                    .map(|t| t.id.clone())
                    .unwrap_or_default();
                ui.label(small(
                    format!("v{}  ·  {host}", ptk_core::VERSION),
                    TEXT_WEAK,
                ));
            });
        });
    }

    /// Menu button with every command, for systems without the native bar.
    #[cfg(not(target_os = "macos"))]
    fn fallback_menu(&mut self, ui: &mut egui::Ui) {
        let state = self.menu_state();
        let mut picked = None;
        ui.menu_button(RichText::new("☰").size(18.0), |ui| {
            for spec in crate::menu::menus() {
                ui.menu_button(spec.title, |ui| {
                    ui.set_min_width(220.0);
                    for entry in spec.items {
                        match entry {
                            None => {
                                ui.separator();
                            }
                            Some(c) => {
                                let b =
                                    egui::Button::new(c.label()).shortcut_text(c.shortcut_text());
                                if ui.add_enabled(state.enabled(c), b).clicked() {
                                    picked = Some(c);
                                }
                            }
                        }
                    }
                });
            }
            if ui.button("Settings").clicked() {
                picked = Some(Command::Settings);
            }
        });
        if let Some(c) = picked {
            self.run_command(c, ui.ctx());
        }
    }

    /// Import Game: start a project from a build the user owns.
    fn import_modal(&mut self, ctx: &egui::Context) {
        if !self.import_open {
            return;
        }
        let mut import = false;
        let mut cancel = false;
        egui::Modal::new(egui::Id::new("import-game")).show(ctx, |ui| {
            ui.set_width(520.0);
            ui.heading("Import a game");
            widgets::hint(
                ui,
                "Start a project from a build you own. No source code needed: Porting Toolkit reads file names and headers, records SHA-256 hashes, and never copies the game into the project.",
            );
            ui.add_space(8.0);
            ui.label(RichText::new("Game build").color(TEXT_WEAK));
            ui.add(
                egui::TextEdit::singleline(&mut self.import_build)
                    .hint_text("folder, .app, .exe, .zip, .deb or .dmg")
                    .desired_width(f32::INFINITY),
            );
            ui.horizontal(|ui| {
                if ui.button("Choose File…").clicked()
                    && let Some(p) = rfd::FileDialog::new().set_title("Choose the game").pick_file()
                {
                    self.import_build = p.display().to_string();
                }
                if ui.button("Choose Folder…").clicked()
                    && let Some(p) = rfd::FileDialog::new().set_title("Choose the game folder").pick_folder()
                {
                    self.import_build = p.display().to_string();
                }
            });
            ui.add_space(6.0);
            ui.label(RichText::new("Project folder").color(TEXT_WEAK));
            let default = if self.import_build.trim().is_empty() {
                default_project_dir("my-game")
            } else {
                default_project_dir(&ptk_core::import::title_of(Path::new(self.import_build.trim())))
            };
            widgets::path_field(
                ui,
                &mut self.import_dest,
                &default.display().to_string(),
                widgets::Pick::Folder,
            );
            ui.add_space(10.0);
            ui.horizontal(|ui| {
                import = widgets::primary(ui, !self.import_build.trim().is_empty(), "Import").clicked();
                cancel = ui.button("Cancel").clicked();
            });
        });
        if import {
            self.import_game(ctx);
        } else if cancel || ctx.input(|i| i.key_pressed(Key::Escape)) {
            self.import_open = false;
        }
    }

    /// Consent before installing a plugin or an SDK.
    fn install_modal(&mut self, ctx: &egui::Context) {
        let Some(pending) = self.pending.clone() else {
            return;
        };
        let mut install = false;
        let mut cancel = false;
        egui::Modal::new(egui::Id::new("install")).show(ctx, |ui| {
            ui.set_width(500.0);
            let (title, valid, plugins): (String, bool, Vec<Validation>) = match &pending {
                PendingInstall::Plugin(v) => (
                    format!(
                        "Install plugin {}?",
                        v.plugin.as_ref().and_then(|p| p.id.clone()).unwrap_or_default()
                    ),
                    v.valid,
                    vec![v.clone()],
                ),
                PendingInstall::Sdk(r) => (
                    format!(
                        "Install SDK {}?",
                        r.sdk.as_ref().map(|s| s.name.clone()).unwrap_or_default()
                    ),
                    r.valid,
                    r.plugins.clone(),
                ),
            };
            ui.heading(title);
            ui.add_space(6.0);
            if let PendingInstall::Sdk(r) = &pending {
                ui.label(RichText::new("Adds these platforms").color(TEXT_WEAK));
                for t in &r.targets {
                    ui.label(format!("  •  {} ({} / {})", t.id, t.os, t.arch));
                }
                for e in &r.errors {
                    ui.label(RichText::new(format!("✖ {e}")).color(ERROR));
                }
            }
            for v in &plugins {
                let id = v.plugin.as_ref().and_then(|p| p.id.clone()).unwrap_or_default();
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    ui.label(RichText::new(format!("🔌 {id}")).strong());
                    widgets::badge(ui, &format!("{} risk", v.risk), match v.risk.as_str() {
                        "low" => GREEN,
                        "medium" => WARN,
                        _ => ERROR,
                    });
                });
                for p in &v.permissions {
                    ui.label(format!("  •  {p}"));
                }
            }
            ui.add_space(6.0);
            widgets::message(
                ui,
                WARN,
                "Python plugins can do anything your user account can. The permissions above are what the author declares.",
            );
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                install = widgets::primary(ui, valid, "Install").clicked();
                cancel = ui.button("Cancel").clicked();
            });
        });
        if install {
            self.confirm_pending(ctx);
        } else if cancel || ctx.input(|i| i.key_pressed(Key::Escape)) {
            self.pending = None;
        }
    }

    fn toast_area(&mut self, ctx: &egui::Context) {
        let Some((text, kind, at)) = &self.toast else {
            return;
        };
        if at.elapsed() > Duration::from_secs(6) {
            self.toast = None;
            return;
        }
        let (icon, color) = match kind {
            ToastKind::Info => ("ℹ", FOCUS),
            ToastKind::Ok => ("✔", GREEN),
            ToastKind::Error => ("✖", ERROR),
        };
        let text = text.clone();
        egui::Area::new(egui::Id::new("toast"))
            .anchor(egui::Align2::RIGHT_BOTTOM, vec2(-14.0, -36.0))
            .show(ctx, |ui| {
                Frame::new()
                    .fill(CARD)
                    .stroke(Stroke::new(1.0_f32, BORDER))
                    .corner_radius(CornerRadius::same(4))
                    .inner_margin(Margin::symmetric(12, 10))
                    .shadow(ctx.style().visuals.popup_shadow)
                    .show(ui, |ui| {
                        ui.set_max_width(420.0);
                        ui.horizontal_wrapped(|ui| {
                            ui.label(RichText::new(icon).color(color).strong());
                            ui.label(text);
                        });
                    });
            });
        ctx.request_repaint_after(Duration::from_millis(500));
    }

    // ------------------------------------------------------------- screenshot

    /// Saves a screenshot once background work is done, then closes the
    /// window (used by `--screenshot` and `--smoke`).
    fn screenshot_and_smoke(&mut self, ctx: &egui::Context) {
        if self.screenshot.is_some() || self.smoke {
            ctx.request_repaint_after(Duration::from_millis(100));
        }
        if let Some(path) = self.screenshot.clone() {
            let ready = self.busy.is_none() && self.created.elapsed() > Duration::from_millis(1500);
            if ready && !self.screenshot_requested {
                ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(Default::default()));
                self.screenshot_requested = true;
            }
            let saved = ctx.input(|i| {
                i.events.iter().any(|event| {
                    if let egui::Event::Screenshot { image, .. } = event {
                        let pixels: Vec<u8> =
                            image.pixels.iter().flat_map(|c| c.to_array()).collect();
                        image::save_buffer(
                            &path,
                            &pixels,
                            image.size[0] as u32,
                            image.size[1] as u32,
                            image::ColorType::Rgba8,
                        )
                        .is_ok()
                    } else {
                        false
                    }
                })
            });
            if saved {
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
            if self.created.elapsed() > Duration::from_secs(60) {
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
        } else if self.smoke && self.created.elapsed() > Duration::from_secs(3) {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
    }
}

/// Default folder for a project imported from a game build.
pub fn default_project_dir(title: &str) -> PathBuf {
    ptk_core::settings::AppSettings::load()
        .projects_dir()
        .join(format!("{}-port", ptk_core::manifest::slugify(title)))
}

/// `~/x` → `$HOME/x` (fields show home paths with a tilde).
pub(crate) fn expand_tilde(text: &str) -> PathBuf {
    match (text.strip_prefix("~/"), std::env::var_os("HOME")) {
        (Some(rest), Some(home)) => PathBuf::from(home).join(rest),
        _ if text == "~" => std::env::var_os("HOME")
            .map(PathBuf::from)
            .unwrap_or_default(),
        _ => PathBuf::from(text),
    }
}

/// Selects a file or app in Finder without opening it.
pub(crate) fn show_in_finder(path: &Path) {
    if cfg!(target_os = "macos") {
        let _ = std::process::Command::new("open")
            .arg("-R")
            .arg(path)
            .spawn();
    } else {
        reveal(path.parent().unwrap_or(path));
    }
}

/// Shows a folder in Finder (or the system file manager); opens files and apps.
pub(crate) fn reveal(path: &Path) {
    let opener = if cfg!(target_os = "macos") {
        "open"
    } else if cfg!(windows) {
        "explorer"
    } else {
        "xdg-open"
    };
    let _ = std::process::Command::new(opener).arg(path).spawn();
}

/// Opens a web page in the default browser.
pub(crate) fn open_url(url: &str) {
    let (prog, args): (&str, Vec<&str>) = if cfg!(target_os = "macos") {
        ("open", vec![url])
    } else if cfg!(windows) {
        ("cmd", vec!["/C", "start", "", url])
    } else {
        ("xdg-open", vec![url])
    };
    let _ = std::process::Command::new(prog).args(args).spawn();
}

/// Text on the system clipboard, for the Edit > Paste menu item.
#[cfg(target_os = "macos")]
fn clipboard_text() -> Option<String> {
    arboard::Clipboard::new().ok()?.get_text().ok()
}

#[cfg(not(target_os = "macos"))]
fn clipboard_text() -> Option<String> {
    None
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.layout(ctx);
        if self.busy.is_some() {
            ctx.request_repaint_after(Duration::from_millis(150));
        }
        self.screenshot_and_smoke(ctx);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ptk_core::manifest::AssetAction;

    fn fixtures() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/games")
    }

    fn app(dir: &Path) -> App {
        App::new(
            Config {
                state_file: Some(dir.join("gui-state.json")),
                ..Default::default()
            },
            &egui::Context::default(),
        )
    }

    fn frame(app: &mut App, ctx: &egui::Context) {
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                vec2(1280.0, 820.0),
            )),
            ..Default::default()
        };
        let _ = ctx.run(input, |ctx| app.layout(ctx));
    }

    fn new_project(dir: &Path) -> PathBuf {
        let project = dir.join("demo");
        ptk_core::project::init(&project, Default::default(), &TargetRegistry::builtin()).unwrap();
        project
    }

    #[test]
    fn every_view_renders_with_and_without_a_project() {
        let tmp = tempfile::tempdir().unwrap();
        let ctx = egui::Context::default();
        crate::theme::install(&ctx);
        let mut app = app(tmp.path());
        let views = View::ALL;
        for v in &views {
            app.view = *v;
            frame(&mut app, &ctx);
            frame(&mut app, &ctx);
        }
        let project = new_project(tmp.path());
        app.open_project(&project);
        assert!(app.project.is_some());
        for v in &views {
            app.view = *v;
            frame(&mut app, &ctx);
            frame(&mut app, &ctx);
        }
        // Rendering the options view must not count as an edit.
        assert!(!app.project.as_ref().unwrap().dirty());
    }

    #[test]
    fn edit_save_and_revert_options() {
        let tmp = tempfile::tempdir().unwrap();
        let project = new_project(tmp.path());
        let mut app = app(tmp.path());
        app.open_project(&project);
        let p = app.project.as_mut().unwrap();
        p.manifest.options.get_mut("textures").unwrap().action = AssetAction::Recompress;
        p.manifest.options.get_mut("textures").unwrap().quality = Some(70);
        assert!(p.dirty());
        app.revert_project();
        assert!(!app.project.as_ref().unwrap().dirty());

        let p = app.project.as_mut().unwrap();
        p.manifest.options.get_mut("video").unwrap().action = AssetAction::Drop;
        app.save_project();
        assert!(!app.project.as_ref().unwrap().dirty());
        let on_disk = Manifest::load(&project).unwrap();
        assert_eq!(on_disk.options["video"].action, AssetAction::Drop);

        // Invalid values are refused, not written.
        let p = app.project.as_mut().unwrap();
        p.manifest.options.get_mut("audio").unwrap().quality = Some(0);
        app.save_project();
        assert!(app.project.as_ref().unwrap().dirty());
        assert!(matches!(app.toast, Some((_, ToastKind::Error, _))));
        assert_eq!(
            Manifest::load(&project).unwrap().options["audio"].quality,
            None
        );
    }

    #[test]
    fn create_project_and_remember_it() {
        let tmp = tempfile::tempdir().unwrap();
        let mut app = app(tmp.path());
        app.new_dir = tmp.path().join("New Port").display().to_string();
        app.new_title = "New Game".into();
        app.create_project();
        let p = app.project.as_ref().expect("opened");
        assert_eq!(p.manifest.name, "new-port");
        assert_eq!(p.manifest.game.title.as_deref(), Some("New Game"));
        let state = GuiState::load(&tmp.path().join("gui-state.json"));
        assert_eq!(state.recent_projects.len(), 1);
    }

    #[test]
    fn drops_route_to_the_right_view() {
        let tmp = tempfile::tempdir().unwrap();
        let project = new_project(tmp.path());
        let mut app = app(tmp.path());
        // A game build opens Import Game, prefilled.
        app.handle_drop(&fixtures().join("godot"));
        assert_eq!(app.view, View::Projects);
        assert!(app.import_open);
        assert!(app.import_build.ends_with("godot"));
        let pack = tmp.path().join("x.ptk");
        std::fs::write(&pack, b"").unwrap();
        app.handle_drop(&pack);
        assert_eq!(app.view, View::Export);
        app.handle_drop(&project);
        assert_eq!(app.view, View::Overview);
        assert!(app.project.is_some());
    }

    #[test]
    fn recent_lists_are_deduplicated_and_bounded() {
        let mut list = Vec::new();
        for i in 0..12 {
            GuiState::remember(&mut list, Path::new(&format!("/p{i}")));
        }
        for i in 12..30 {
            GuiState::remember(&mut list, Path::new(&format!("/p{i}")));
        }
        GuiState::remember(&mut list, Path::new("/p5"));
        assert_eq!(list.len(), MAX_RECENT);
        assert_eq!(list[0], PathBuf::from("/p5"));
        assert_eq!(
            list.iter()
                .filter(|p| p.as_path() == Path::new("/p5"))
                .count(),
            1
        );
    }

    #[test]
    fn project_list_filters_sorts_and_forgets() {
        let tmp = tempfile::tempdir().unwrap();
        let mut app = app(tmp.path());
        for name in ["beta", "alpha"] {
            let p = tmp.path().join(name);
            ptk_core::project::init(&p, Default::default(), &TargetRegistry::builtin()).unwrap();
            app.open_project(&p);
        }
        let gone = tmp.path().join("gone");
        ptk_core::project::init(&gone, Default::default(), &TargetRegistry::builtin()).unwrap();
        app.open_project(&gone);
        std::fs::remove_dir_all(&gone).unwrap();
        app.refresh_rows();
        assert_eq!(app.rows.len(), 3);
        assert!(app.rows.iter().any(|r| r.missing));

        app.sort = ProjectSort::Name;
        let names: Vec<_> = app.visible_rows().iter().map(|r| r.name.clone()).collect();
        assert_eq!(names, ["alpha", "beta", "Missing project"]);
        let beta = app
            .rows
            .iter()
            .find(|r| r.name == "beta")
            .unwrap()
            .path
            .clone();
        app.toggle_favorite(&beta);
        assert_eq!(app.visible_rows()[0].name, "beta");
        app.filter = "alp".into();
        assert_eq!(app.visible_rows().len(), 1);
        app.filter.clear();
        app.forget_missing();
        assert_eq!(app.rows.len(), 2);
        app.forget_project(&beta);
        assert_eq!(app.rows.len(), 1);
    }

    fn wait(app: &mut App, ctx: &egui::Context) {
        let start = Instant::now();
        while app.busy.is_some() {
            assert!(start.elapsed() < Duration::from_secs(60), "job timed out");
            std::thread::sleep(Duration::from_millis(20));
            app.poll(ctx);
        }
    }

    #[test]
    fn import_runs_the_plugin_and_export_builds_for_the_web() {
        let tmp = tempfile::tempdir().unwrap();
        let ctx = egui::Context::default();
        let mut app = app(tmp.path());
        let game = tmp.path().join("NwGame");
        std::fs::create_dir_all(game.join("data")).unwrap();
        std::fs::write(game.join("index.html"), "<title>hi</title>").unwrap();
        std::fs::write(game.join("data/info.json"), "{}").unwrap();
        std::fs::write(game.join("package.json"), r#"{"main": "index.html"}"#).unwrap();
        std::fs::write(game.join("package.nw"), b"").unwrap();
        std::fs::write(game.join("nw.dll"), b"MZ").unwrap();
        app.import_build = game.display().to_string();
        app.import_dest = tmp.path().join("nw-port").display().to_string();
        app.import_game(&ctx);
        wait(&mut app, &ctx);
        let summary = match &app.import_result {
            Some(Ok(r)) => r.summary.clone(),
            other => panic!(
                "import plugin did not run: {:?}",
                other.as_ref().map(|r| r.is_ok())
            ),
        };
        assert!(summary.contains("NW.js"), "{summary}");
        let dir = app.project.as_ref().unwrap().dir.clone();
        assert!(dir.join("recipes/html5.json").is_file());
        assert!(app.export_output.ends_with("build"));
        assert_eq!(
            expand_tilde("~/x"),
            PathBuf::from(std::env::var("HOME").unwrap()).join("x")
        );
        app.export_targets(&ctx, vec!["web".into()]);
        wait(&mut app, &ctx);
        let (target, r) = &app.export_results[0];
        assert_eq!(target, "web");
        let r = r.as_ref().expect("export works");
        assert_eq!(r.plugin_id, "html5-export");
        assert!(dir.join("build/web/index.html").is_file());
        assert!(dir.join("build/web/data/info.json").is_file());
        assert!(r.serve.is_some());
        app.view = View::Export;
        crate::theme::install(&ctx);
        for _ in 0..3 {
            frame(&mut app, &ctx);
        }
    }

    #[test]
    fn import_game_creates_a_project() {
        let tmp = tempfile::tempdir().unwrap();
        let ctx = egui::Context::default();
        let mut app = app(tmp.path());
        let exe = tmp.path().join("MyGame.exe");
        let mut bytes = b"MZ".to_vec();
        bytes.extend([0u8; 40]);
        bytes.extend(8u64.to_le_bytes());
        bytes.extend(b"GDPC");
        std::fs::write(&exe, bytes).unwrap();
        app.import_build = exe.display().to_string();
        app.import_dest = tmp.path().join("my-port").display().to_string();
        app.import_open = true;
        app.import_game(&ctx);
        let start = Instant::now();
        while app.busy.is_some() {
            assert!(start.elapsed() < Duration::from_secs(30));
            std::thread::sleep(Duration::from_millis(20));
            app.poll(&ctx);
        }
        let p = app.project.as_ref().expect("project opened");
        assert_eq!(p.manifest.game.engine.as_deref(), Some("godot"));
        assert_eq!(p.manifest.game.title.as_deref(), Some("MyGame"));
        assert_eq!(p.manifest.source_files.len(), 1);
        assert_eq!(app.view, View::Overview);
        assert!(!app.import_open);
    }

    /// Scan through the real engine, record into a project, pack and
    /// verify, all as background jobs.
    #[test]
    fn background_jobs_end_to_end() {
        let tmp = tempfile::tempdir().unwrap();
        let project = new_project(tmp.path());
        let ctx = egui::Context::default();
        let mut app = app(tmp.path());
        app.open_project(&project);

        let wait = |app: &mut App| {
            let start = Instant::now();
            while app.busy.is_some() {
                assert!(start.elapsed() < Duration::from_secs(30), "job timed out");
                std::thread::sleep(Duration::from_millis(20));
                app.poll(&ctx);
            }
        };

        app.game_input = fixtures().join("rage").display().to_string();
        app.start_scan(&ctx);
        wait(&mut app);
        let scan = app.scan.as_ref().expect("scan result");
        assert_eq!(scan.primary.as_ref().unwrap().id, "rage");

        app.start_record(&ctx);
        wait(&mut app);
        let p = app.project.as_ref().unwrap();
        assert_eq!(p.manifest.game.engine.as_deref(), Some("rage"));
        assert_eq!(p.manifest.source_files.len(), 4);

        app.pack_output = tmp.path().join("demo.ptk").display().to_string();
        app.start_pack(&ctx);
        wait(&mut app);
        assert!(
            matches!(app.pack_result, Some(Ok(_))),
            "{:?}",
            app.pack_result.as_ref().map(|r| r.as_ref().err())
        );

        app.verify_game = fixtures().join("rage").display().to_string();
        app.start_verify(&ctx);
        wait(&mut app);
        assert!(matches!(&app.verify_result, Some(Ok(r)) if r.is_ok()));

        app.unpack_input = tmp.path().join("demo.ptk").display().to_string();
        app.unpack_dest = tmp.path().join("copy").display().to_string();
        app.start_unpack(&ctx);
        wait(&mut app);
        assert!(matches!(app.unpack_result, Some(Ok(_))));
    }
}
