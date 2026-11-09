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
use ptk_core::doctor::Check;
use ptk_core::engine::EngineClient;
use ptk_core::manifest::{Issue, MANIFEST_FILE, Manifest};
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

/// The pages of the app.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum View {
    Home,
    Project,
    Scan,
    Options,
    Share,
    Plugins,
    Targets,
    Doctor,
    About,
}

impl View {
    pub const ALL: [View; 9] = [
        View::Home,
        View::Project,
        View::Scan,
        View::Options,
        View::Share,
        View::Plugins,
        View::Targets,
        View::Doctor,
        View::About,
    ];

    pub fn icon(self) -> &'static str {
        match self {
            View::Home => "🏠",
            View::Project => "🗀",
            View::Scan => "🔍",
            View::Options => "⚙",
            View::Share => "📦",
            View::Plugins => "🔌",
            View::Targets => "🎯",
            View::Doctor => "✚",
            View::About => "ℹ",
        }
    }

    pub fn title(self) -> &'static str {
        match self {
            View::Home => "Home",
            View::Project => "Project",
            View::Scan => "Scan",
            View::Options => "Options",
            View::Share => "Share",
            View::Plugins => "Plugins",
            View::Targets => "Targets",
            View::Doctor => "Doctor",
            View::About => "About",
        }
    }

    pub fn parse(s: &str) -> Option<View> {
        View::ALL.into_iter().find(|v| {
            v.title().eq_ignore_ascii_case(s)
                || format!("{v:?}").eq_ignore_ascii_case(s)
                || (s.eq_ignore_ascii_case("welcome") && *v == View::Home)
        })
    }
}

/// Remembered between runs (`<data>/gui-state.json`).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct GuiState {
    #[serde(default)]
    pub recent_projects: Vec<PathBuf>,
    #[serde(default)]
    pub recent_games: Vec<PathBuf>,
}

const MAX_RECENT: usize = 8;

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
    pub confirm_install: bool,

    pub busy: Option<String>,
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
            view: config.view.unwrap_or(View::Home),
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
            confirm_install: false,
            busy: None,
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
        if let Some(p) = config.project {
            app.open_project(&p);
        }
        app
    }

    // ---------------------------------------------------------------- project

    pub fn open_project(&mut self, dir: &Path) {
        match Manifest::load(dir) {
            Ok(m) => {
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
                self.pack_result = None;
                self.verify_result = None;
                if self.view == View::Home {
                    self.view = View::Project;
                }
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
        let opts = ptk_core::project::InitOptions {
            title,
            ..Default::default()
        };
        match ptk_core::project::init(&dir, opts, &self.registry) {
            Ok(m) => {
                self.notify(format!("Created project {}", m.name), ToastKind::Ok);
                self.new_dir.clear();
                self.new_title.clear();
                self.open_project(&dir);
                self.view = View::Project;
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

    pub fn install_plugin(&mut self, ctx: &egui::Context) {
        let Some(Ok(v)) = &self.plugin_validation else {
            return;
        };
        let v = v.clone();
        let dir = PathBuf::from(&v.path);
        self.confirm_install = false;
        self.spawn(ctx, "Installing plugin", move |_| {
            Msg::Installed(ptk_core::plugins::install(&dir, &v).map_err(|e| e.to_string()))
        });
    }

    /// Applies finished job results.
    pub fn poll(&mut self, ctx: &egui::Context) {
        while let Ok(msg) = self.rx.try_recv() {
            if !matches!(msg, Msg::ScanProgress(_)) {
                self.busy = None;
            }
            match msg {
                Msg::ScanProgress(n) => self.scan_progress = Some(n),
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
                        self.plugin_dir = c.path;
                        self.plugin_new_dir.clear();
                        self.validate_plugin(ctx);
                    }
                    Err(e) => self.notify(e, ToastKind::Error),
                },
                Msg::Installed(r) => match r {
                    Ok((dest, n)) => {
                        self.notify(
                            format!("Installed {n} files in {}", dest.display()),
                            ToastKind::Ok,
                        );
                        self.refresh_plugins(ctx);
                    }
                    Err(e) => self.notify(e, ToastKind::Error),
                },
            }
        }
    }

    /// Opens what was dropped on the window: a project folder, a `.ptk`
    /// file, or a game folder to scan.
    pub fn handle_drop(&mut self, path: &Path) {
        if path.join(MANIFEST_FILE).is_file() {
            self.open_project(path);
            self.view = View::Project;
        } else if path
            .extension()
            .is_some_and(|e| e.eq_ignore_ascii_case("ptk"))
        {
            self.unpack_input = path.display().to_string();
            self.view = View::Share;
        } else if path.is_dir() {
            self.game_input = path.display().to_string();
            self.view = View::Scan;
        }
    }

    fn first_run(&mut self, ctx: &egui::Context) {
        if !std::mem::take(&mut self.auto_run) {
            return;
        }
        match self.view {
            View::Scan if !self.game_input.is_empty() => self.start_scan(ctx),
            View::Doctor => self.start_doctor(ctx, false),
            View::Plugins => self.refresh_plugins(ctx),
            View::Share if !self.verify_game.is_empty() && self.project.is_some() => {
                self.start_verify(ctx)
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
            Command::OpenProject => {
                if let Some(dir) = rfd::FileDialog::new()
                    .set_title("Open a Porting Toolkit project")
                    .pick_folder()
                {
                    self.open_project(&dir);
                    self.view = View::Project;
                }
            }
            Command::OpenRecent(i) => {
                if let Some(p) = self.state.recent_projects.get(i).cloned() {
                    self.open_project(&p);
                    self.view = View::Project;
                }
            }
            Command::Save => self.save_project(),
            Command::Revert => self.revert_project(),
            Command::CloseProject => {
                self.project = None;
                self.view = View::Home;
            }
            Command::UnpackPtk => {
                let pack = rfd::FileDialog::new()
                    .add_filter("Porting Toolkit project", &["ptk"])
                    .set_title("Unpack a .ptk file")
                    .pick_file();
                if let Some(pack) = pack {
                    self.unpack_input = pack.display().to_string();
                    self.view = View::Share;
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
                self.view = View::Share;
                self.start_pack(ctx);
            }
            Command::Undo
            | Command::Redo
            | Command::Cut
            | Command::Copy
            | Command::Paste
            | Command::SelectAll => self.inject_edit(ctx, cmd),
            Command::Go(v) => self.go(v, ctx),
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
                    self.view = View::Share;
                    self.start_verify(ctx);
                }
            }
            Command::RunDoctor => {
                self.view = View::Doctor;
                self.start_doctor(ctx, false);
            }
            Command::SetUpEngine => {
                self.view = View::Doctor;
                self.start_doctor(ctx, true);
            }
            Command::RevealData => reveal(&ptk_core::paths::data_dir()),
            Command::RevealPlugins => {
                let dir = ptk_core::paths::plugins_dir();
                let _ = std::fs::create_dir_all(&dir);
                reveal(&dir);
            }
            Command::About => self.view = View::About,
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
        if view == View::Plugins && self.plugins.is_none() {
            self.refresh_plugins(ctx);
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
                let Some(key) = Key::from_name(&ch.to_string()) else {
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
        if let Some(menu) = &mut self.native_menu {
            let state = MenuState {
                project_open: self.project.is_some(),
                dirty: self.project.as_ref().is_some_and(|p| p.dirty()),
                busy: self.busy.is_some(),
                has_game: !self.game_input.trim().is_empty(),
                has_hashes: self
                    .project
                    .as_ref()
                    .is_some_and(|p| !p.saved.source_files.is_empty()),
            };
            menu.sync(state, &self.state.recent_projects);
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

        egui::TopBottomPanel::top("header")
            .exact_height(58.0)
            .frame(
                Frame::new()
                    .fill(HEADER)
                    .inner_margin(Margin::symmetric(16, 0))
                    .stroke(Stroke::new(1.0_f32, BORDER)),
            )
            .show(ctx, |ui| self.header(ui));
        egui::CentralPanel::default()
            .frame(Frame::new().fill(PAGE))
            .show(ctx, |ui| {
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
        self.install_modal(ctx);
        self.toast_area(ctx);
    }

    fn header(&mut self, ui: &mut egui::Ui) {
        let wide = ui.available_width() >= 1080.0;
        let medium = ui.available_width() >= 760.0;
        ui.horizontal_centered(|ui| {
            #[cfg(not(target_os = "macos"))]
            self.fallback_menu(ui);

            widgets::logo(ui, 30.0);
            if medium {
                ui.label(
                    RichText::new("Porting Toolkit")
                        .size(15.0)
                        .strong()
                        .color(Color32::WHITE),
                );
            }
            ui.add_space(6.0);
            self.project_chip(ui);
            ui.add_space(10.0);

            for view in [
                View::Home,
                View::Project,
                View::Scan,
                View::Options,
                View::Share,
                View::Plugins,
            ] {
                let selected = self.view == view;
                if widgets::nav_pill(ui, view.icon(), view.title(), selected, wide)
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

            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                for view in [View::Doctor, View::Targets] {
                    if widgets::icon_button(ui, view.icon(), self.view == view)
                        .on_hover_text(view.title())
                        .clicked()
                    {
                        self.go(view, ui.ctx());
                    }
                }
                if let Some(label) = &self.busy {
                    let extra = match self.scan_progress {
                        Some(n) if n > 0 => format!(" · {n} files"),
                        _ => String::new(),
                    };
                    widgets::busy_pill(ui, &format!("{label}{extra}"));
                }
            });
        });
    }

    fn project_chip(&mut self, ui: &mut egui::Ui) {
        let (text, color) = match &self.project {
            Some(p) => (
                format!(
                    "🗀  {}{}",
                    p.manifest.name,
                    if p.dirty() { "  ●" } else { "" }
                ),
                Color32::WHITE,
            ),
            None => ("🗀  No project".to_owned(), TEXT_WEAK),
        };
        let mut picked = None;
        ui.menu_button(RichText::new(format!("{text}  ⏷")).color(color), |ui| {
            ui.set_min_width(240.0);
            for c in [Command::NewProject, Command::OpenProject] {
                if ui.button(c.label()).clicked() {
                    picked = Some(c);
                }
            }
            if !self.state.recent_projects.is_empty() {
                ui.separator();
                ui.label(RichText::new("RECENT").size(11.0).color(TEXT_FAINT));
                for (i, p) in self.state.recent_projects.iter().enumerate() {
                    let name = p
                        .file_name()
                        .map(|n| n.to_string_lossy().into_owned())
                        .unwrap_or_default();
                    if ui
                        .button(name)
                        .on_hover_text(p.display().to_string())
                        .clicked()
                    {
                        picked = Some(Command::OpenRecent(i));
                    }
                }
            }
            if self.project.is_some() {
                ui.separator();
                if ui.button("Close Project").clicked() {
                    picked = Some(Command::CloseProject);
                }
            }
        });
        if let Some(c) = picked {
            self.run_command(c, ui.ctx());
        }
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
            if ui.button("About Porting Toolkit").clicked() {
                picked = Some(Command::About);
            }
        });
        if let Some(c) = picked {
            self.run_command(c, ui.ctx());
        }
    }

    fn install_modal(&mut self, ctx: &egui::Context) {
        if !self.confirm_install {
            return;
        }
        let Some(Ok(v)) = self.plugin_validation.clone() else {
            self.confirm_install = false;
            return;
        };
        let mut install = false;
        let mut cancel = false;
        egui::Modal::new(egui::Id::new("install-plugin")).show(ctx, |ui| {
            ui.set_width(460.0);
            let id = v
                .plugin
                .as_ref()
                .and_then(|p| p.id.clone())
                .unwrap_or_default();
            ui.heading(format!("Install {id}?"));
            ui.add_space(6.0);
            ui.label("This plugin declares that it needs:");
            for p in &v.permissions {
                ui.label(format!("  •  {p}"));
            }
            ui.add_space(6.0);
            widgets::message(
                ui,
                WARN,
                "Python plugins can do anything your user account can. The permissions above \
                 are what the author declares, and this plugin has not been reviewed. \
                 Plugins do not run until phase 3.",
            );
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                install = widgets::primary(ui, true, "Install").clicked();
                cancel = ui.button("Cancel").clicked();
            });
        });
        if install {
            self.install_plugin(ctx);
        } else if cancel || ctx.input(|i| i.key_pressed(Key::Escape)) {
            self.confirm_install = false;
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

/// Shows a folder in Finder (or the system file manager).
fn reveal(path: &Path) {
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
fn open_url(url: &str) {
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
        app.handle_drop(&fixtures().join("godot"));
        assert_eq!(app.view, View::Scan);
        assert!(app.game_input.ends_with("godot"));
        let pack = tmp.path().join("x.ptk");
        std::fs::write(&pack, b"").unwrap();
        app.handle_drop(&pack);
        assert_eq!(app.view, View::Share);
        app.handle_drop(&project);
        assert_eq!(app.view, View::Project);
        assert!(app.project.is_some());
    }

    #[test]
    fn recent_lists_are_deduplicated_and_bounded() {
        let mut list = Vec::new();
        for i in 0..12 {
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
