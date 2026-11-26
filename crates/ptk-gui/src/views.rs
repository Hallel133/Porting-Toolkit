//! The pages. Each one is a column of rounded cards that reflows to the
//! window width (two or three columns when there is room, one when narrow).

use std::path::{Path, PathBuf};

use eframe::egui::{self, Align, Color32, Layout, RichText, Ui, vec2};
use ptk_core::category::CATEGORIES;
use ptk_core::doctor::Status;
use ptk_core::manifest::{AssetAction, CategoryOptions, Level, OWN_WORK_LICENSE};
use ptk_core::market::EntryKind;
use ptk_core::targets::{TargetSource, TargetStatus};

use crate::app::{App, MarketTab, ProjectSort, SettingsTab, ToastKind, View};
use crate::menu::Command;
use crate::theme::*;
use crate::widgets::{
    self, Pick, badge, card, card_title, category_color, columns_for, hint, human_size, kv,
    page_header, primary,
};

const CONTENT_LICENSES: &[(&str, &str)] = &[
    ("", "Not declared (proprietary)"),
    (OWN_WORK_LICENSE, "My own game"),
    ("MIT", "MIT"),
    ("Apache-2.0", "Apache-2.0"),
    ("GPL-3.0-or-later", "GPL-3.0-or-later"),
    ("GPL-2.0-or-later", "GPL-2.0-or-later"),
    ("CC0-1.0", "CC0-1.0"),
    ("CC-BY-4.0", "CC-BY-4.0"),
];

const RESOLUTIONS: &[Option<u32>] = &[
    None,
    Some(8192),
    Some(4096),
    Some(2048),
    Some(1024),
    Some(512),
];

fn risk_color(r: &str) -> Color32 {
    match r {
        "low" => GREEN,
        "medium" => WARN,
        "high" => ERROR,
        _ => TEXT_WEAK,
    }
}

/// Two columns side by side (`ratio` for the left one), or stacked when
/// narrower than `min_total`. `f(ui, column)` draws column 0 then 1.
fn split(ui: &mut Ui, ratio: f32, min_total: f32, mut f: impl FnMut(&mut Ui, usize)) {
    let w = ui.available_width();
    if w < min_total {
        f(ui, 0);
        ui.add_space(14.0);
        f(ui, 1);
        return;
    }
    let gap = 18.0;
    let left = (w - gap) * ratio;
    let right = w - gap - left;
    ui.horizontal_top(|ui| {
        ui.spacing_mut().item_spacing.x = gap;
        ui.vertical(|ui| {
            ui.spacing_mut().item_spacing.x = 8.0;
            ui.set_width(left);
            f(ui, 0);
        });
        ui.vertical(|ui| {
            ui.spacing_mut().item_spacing.x = 8.0;
            ui.set_width(right);
            f(ui, 1);
        });
    });
}

/// One entry per plugin id: the copy that runs (the pinned one, else the
/// newest version, and on a tie the installed copy; see
/// `runtime.find_plugins`).
fn newest_copies(
    list: &[ptk_core::plugins::ListedPlugin],
    pins: &std::collections::BTreeMap<String, PathBuf>,
) -> Vec<ptk_core::plugins::ListedPlugin> {
    let pinned = |p: &ptk_core::plugins::ListedPlugin| {
        pins.get(&p.id)
            .is_some_and(|d| same_dir(d, Path::new(&p.path)))
    };
    let mut out: Vec<ptk_core::plugins::ListedPlugin> = Vec::new();
    for p in list {
        match out.iter_mut().find(|q| q.id == p.id) {
            Some(q) => {
                if pinned(p)
                    || (!pinned(q)
                        && (ptk_core::market::version_newer(&p.version, &q.version)
                            || (p.version == q.version && q.builtin && !p.builtin)))
                {
                    *q = p.clone();
                }
            }
            None => out.push(p.clone()),
        }
    }
    out
}

/// What the Installed tab may do with one copy of a plugin.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum PluginState {
    /// Ships with Porting Toolkit: nothing to install or delete.
    BuiltIn,
    /// In the installed-plugins folder: it can be uninstalled.
    Installed,
    /// A folder elsewhere whose id is already here: installing it replaces
    /// (or adds a version next to) the copy that runs now.
    Replaces { running: String, newer: bool },
    /// Not here yet.
    New,
}

pub(crate) fn plugin_state(
    path: &str,
    id: Option<&str>,
    version: &str,
    list: &[ptk_core::plugins::ListedPlugin],
    pins: &std::collections::BTreeMap<String, PathBuf>,
    plugins_dir: &Path,
) -> PluginState {
    let p = Path::new(path);
    if list
        .iter()
        .any(|q| q.builtin && same_dir(Path::new(&q.path), p))
    {
        return PluginState::BuiltIn;
    }
    if p.parent().is_some_and(|d| same_dir(d, plugins_dir)) {
        return PluginState::Installed;
    }
    match id.and_then(|id| newest_copies(list, pins).into_iter().find(|q| q.id == id)) {
        Some(q) => PluginState::Replaces {
            newer: ptk_core::market::version_newer(version, &q.version),
            running: q.version,
        },
        None => PluginState::New,
    }
}

/// The marketplace entry that updates a plugin, when the index has a newer version.
fn update_for(
    market: &Option<Result<ptk_core::market::MarketIndex, String>>,
    id: &str,
    version: &str,
) -> Option<ptk_core::market::MarketEntry> {
    let Some(Ok(index)) = market else { return None };
    index
        .entries
        .iter()
        .filter(|e| e.kind == EntryKind::Plugin && e.id == id)
        .find(|e| ptk_core::market::version_newer(&e.version, version))
        .cloned()
}

fn same_dir(a: &Path, b: &Path) -> bool {
    a == b
        || a.canonicalize()
            .ok()
            .is_some_and(|x| b.canonicalize().ok() == Some(x))
}

/// `count` cards in rows of `cols`.
fn card_grid(ui: &mut Ui, count: usize, cols: usize, mut f: impl FnMut(&mut Ui, usize)) {
    for row in (0..count).step_by(cols) {
        ui.columns(cols, |c| {
            for (k, col) in c.iter_mut().enumerate() {
                let i = row + k;
                if i < count {
                    col.spacing_mut().item_spacing = vec2(8.0, 6.0);
                    f(col, i);
                }
            }
        });
        ui.add_space(14.0);
    }
}

fn empty_state(ui: &mut Ui, icon: &str, title: &str, text: &str) {
    card(ui, |ui| {
        ui.vertical_centered(|ui| {
            ui.add_space(18.0);
            ui.label(RichText::new(icon).size(40.0).color(TEXT_FAINT));
            ui.label(RichText::new(title).size(19.0).color(TEXT));
            hint(ui, text);
            ui.add_space(14.0);
        });
    });
}

impl App {
    pub fn page(&mut self, ui: &mut Ui) {
        match self.view {
            View::Projects => self.projects_page(ui),
            View::Marketplace => self.marketplace_page(ui),
            View::Settings => self.settings_page(ui),
            View::Overview => self.project_page(ui),
            View::Scan => self.scan_page(ui),
            View::Export => self.export_page(ui),
        }
    }

    fn command_button(&mut self, ui: &mut Ui, cmd: Command, text: &str, main: bool) {
        let enabled = self.menu_state().enabled(cmd);
        let clicked = if main {
            primary(ui, enabled, text).clicked()
        } else {
            ui.add_enabled(enabled, egui::Button::new(text)).clicked()
        };
        if clicked {
            self.run_command(cmd, ui.ctx());
        }
    }

    /// Edits save on their own; this bar shows only when ptk.json could not
    /// be saved (an invalid value), with Revert to go back to the file.
    fn unsaved_bar(&mut self, ui: &mut Ui) {
        let Some(err) = self.save_error.clone() else {
            return;
        };
        if !self.project.as_ref().is_some_and(|p| p.dirty()) {
            return;
        }
        egui::Frame::new()
            .fill(ERROR.gamma_multiply(0.12))
            .stroke(egui::Stroke::new(1.0_f32, ERROR.gamma_multiply(0.5)))
            .corner_radius(egui::CornerRadius::same(10))
            .inner_margin(egui::Margin::symmetric(14, 8))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal(|ui| {
                    ui.label(RichText::new("●").color(ERROR));
                    ui.label(format!("Not saved: {err}"));
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        self.command_button(ui, Command::Revert, "Revert", false);
                    });
                });
            });
        ui.add_space(12.0);
    }

    // ============================================================ projects

    /// The project manager: toolbar, project list and an action column.
    pub fn projects_page(&mut self, ui: &mut Ui) {
        ui.horizontal(|ui| {
            self.command_button(ui, Command::NewProject, "+  Create", false);
            self.command_button(ui, Command::ImportGame, "⬇  Import Game", true);
            self.command_button(ui, Command::OpenProject, "🗀  Open", false);
            self.command_button(ui, Command::FindProjects, "🔍  Find", false);
            ui.add_space(6.0);
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                egui::ComboBox::from_id_salt("sort")
                    .width(130.0)
                    .selected_text(match self.sort {
                        ProjectSort::LastOpened => "Last opened",
                        ProjectSort::Name => "Name",
                        ProjectSort::Engine => "Engine",
                    })
                    .show_ui(ui, |ui| {
                        ui.selectable_value(&mut self.sort, ProjectSort::LastOpened, "Last opened");
                        ui.selectable_value(&mut self.sort, ProjectSort::Name, "Name");
                        ui.selectable_value(&mut self.sort, ProjectSort::Engine, "Engine");
                    });
                ui.label(RichText::new("Sort:").color(TEXT_WEAK));
                ui.add(
                    egui::TextEdit::singleline(&mut self.filter)
                        .hint_text("🔍  Filter projects")
                        .desired_width(ui.available_width()),
                );
            });
        });
        ui.add_space(10.0);

        let rows = self.visible_rows();
        let narrow = ui.available_width() < 720.0;
        let actions_w = 200.0;
        let height = ui.available_height();
        if narrow {
            ui.horizontal_wrapped(|ui| self.project_actions(ui, true));
            ui.add_space(8.0);
        }
        ui.horizontal_top(|ui| {
            let list_w = if narrow {
                ui.available_width()
            } else {
                ui.available_width() - actions_w - 12.0
            };
            egui::Frame::new()
                .fill(rgb(12, 23, 41))
                .stroke(egui::Stroke::new(1.0_f32, BORDER))
                .corner_radius(egui::CornerRadius::same(8))
                .show(ui, |ui| {
                    // The list stacks top to bottom inside the horizontal row.
                    ui.with_layout(Layout::top_down(Align::Min), |ui| {
                        ui.set_width(list_w);
                        ui.set_height(height - if narrow { 60.0 } else { 0.0 });
                        if rows.is_empty() {
                            self.no_projects(ui);
                        } else {
                            egui::ScrollArea::vertical()
                                .auto_shrink([false, false])
                                .show(ui, |ui| {
                                    for row in &rows {
                                        self.project_row(ui, row);
                                    }
                                });
                        }
                    });
                });
            if !narrow {
                ui.vertical(|ui| {
                    ui.set_width(actions_w);
                    self.project_actions(ui, false);
                });
            }
        });
    }

    fn no_projects(&mut self, ui: &mut Ui) {
        ui.vertical_centered(|ui| {
            ui.add_space((ui.available_height() / 2.0 - 120.0).max(20.0));
            let cell = ((ui.available_width() - 40.0) / 72.0).clamp(2.0, 4.0);
            widgets::wordmark(ui, cell);
            ui.add_space(16.0);
            ui.label(RichText::new("You don't have any projects yet.").size(17.0).color(TEXT));
            hint(ui, "Import a game you own (folder, .app, .exe, .zip, .deb or .dmg) to start porting it.");
            ui.add_space(10.0);
            ui.horizontal(|ui| {
                let w = 320.0;
                ui.add_space(((ui.available_width() - w) / 2.0).max(0.0));
                self.command_button(ui, Command::ImportGame, "⬇  Import Game", true);
                self.command_button(ui, Command::NewProject, "+  Create Empty Project", false);
            });
        });
    }

    fn project_row(&mut self, ui: &mut Ui, row: &crate::app::ProjectRow) {
        let ctx = ui.ctx().clone();
        let art = row.icon.as_deref().and_then(|p| self.image(&ctx, p));
        let width = ui.available_width();
        let (rect, response) = ui.allocate_exact_size(vec2(width, 70.0), egui::Sense::click());
        let selected = self.selected.as_deref() == Some(row.path.as_path());
        let p = ui.painter();
        if selected {
            p.rect_filled(rect.shrink(3.0), egui::CornerRadius::same(6), SELECTION);
        } else if response.hovered() {
            p.rect_filled(
                rect.shrink(3.0),
                egui::CornerRadius::same(6),
                HOVER.gamma_multiply(0.6),
            );
        }
        // Favorite star.
        let star =
            egui::Rect::from_center_size(rect.left_center() + vec2(24.0, 0.0), vec2(26.0, 26.0));
        let star_resp = ui.interact(
            star,
            ui.id().with(("star", &row.path)),
            egui::Sense::click(),
        );
        let fav = self.state.favorites.contains(&row.path);
        widgets::centered_text(
            ui,
            star,
            if fav { "★" } else { "☆" },
            17.0,
            if fav {
                YELLOW
            } else if star_resp.hovered() {
                TEXT
            } else {
                TEXT_FAINT
            },
        );
        // Engine icon.
        let icon = egui::Rect::from_min_size(rect.min + vec2(46.0, 11.0), vec2(48.0, 48.0));
        match &art {
            Some(tex) => widgets::image_in(ui, icon, tex, 10),
            None => widgets::project_icon(ui, icon, row.engine.as_deref(), row.missing),
        }
        let text_x = icon.max.x + 14.0;
        let name_color = if row.missing {
            TEXT_FAINT
        } else {
            Color32::WHITE
        };
        p.text(
            egui::pos2(text_x, rect.min.y + 22.0),
            egui::Align2::LEFT_CENTER,
            &row.name,
            egui::FontId::proportional(15.5),
            name_color,
        );
        // Leave room for the date on the right; elide long paths in the middle.
        let room = ((rect.max.x - 120.0 - text_x) / 7.2).max(12.0) as usize;
        p.text(
            egui::pos2(text_x, rect.min.y + 48.0),
            egui::Align2::LEFT_CENTER,
            format!(
                "{}  {}",
                if row.missing { "⚠" } else { "🗀" },
                widgets::elide_middle(&widgets::display_path(&row.path), room)
            ),
            egui::FontId::proportional(12.5),
            if row.missing {
                ERROR.gamma_multiply(0.8)
            } else {
                TEXT_WEAK
            },
        );
        // Right side: engine and when it changed.
        let right = rect.max.x - 16.0;
        let when = row
            .modified
            .map(widgets::relative_time)
            .unwrap_or_else(|| "missing".into());
        p.text(
            egui::pos2(right, rect.min.y + 48.0),
            egui::Align2::RIGHT_CENTER,
            when,
            egui::FontId::proportional(12.5),
            TEXT_FAINT,
        );
        if let Some(title) = &row.title {
            let engine = row.engine.clone().unwrap_or_else(|| "not scanned".into());
            p.text(
                egui::pos2(right, rect.min.y + 22.0),
                egui::Align2::RIGHT_CENTER,
                format!("{title} · {engine}"),
                egui::FontId::proportional(12.5),
                LIGHT_BLUE,
            );
        }
        if star_resp.clicked() {
            self.toggle_favorite(&row.path);
        } else if response.double_clicked() && !row.missing {
            self.open_project(&row.path);
            self.view = View::Overview;
        } else if response.clicked() {
            self.selected = Some(row.path.clone());
        }
    }

    /// The action column (or row, when narrow) for the selected project.
    fn project_actions(&mut self, ui: &mut Ui, compact: bool) {
        let sel = self.selected.clone();
        let row = sel
            .as_ref()
            .and_then(|p| self.rows.iter().find(|r| &r.path == p))
            .cloned();
        let ok = row.as_ref().is_some_and(|r| !r.missing);
        let any_missing = self.rows.iter().any(|r| r.missing);
        let busy = self.busy.is_some();
        let actions: [(&str, &str, bool, u8); 7] = [
            ("✏", "Open", ok, 0),
            ("📦", "Export", ok, 1),
            ("🔍", "Scan Game", ok && !busy, 2),
            ("✔", "Verify Game", ok && !busy, 3),
            ("🗀", "Show in Finder", row.is_some(), 4),
            ("✖", "Remove", row.is_some(), 5),
            ("⚠", "Remove Missing", any_missing, 6),
        ];
        let mut picked = None;
        for (icon, label, enabled, id) in actions {
            let clicked = if compact {
                ui.add_enabled(enabled, egui::Button::new(format!("{icon} {label}")))
                    .clicked()
            } else {
                let r = widgets::action_button(ui, icon, label, enabled).clicked();
                ui.add_space(4.0);
                r
            };
            if clicked && enabled {
                picked = Some(id);
            }
        }
        if !compact {
            ui.with_layout(Layout::bottom_up(Align::Min), |ui| {
                if widgets::action_button(ui, "🔌", "Marketplace", true).clicked() {
                    picked = Some(7);
                }
            });
        }
        let Some(id) = picked else { return };
        let ctx = ui.ctx().clone();
        match (id, row) {
            (7, _) => self.go(View::Marketplace, &ctx),
            (6, _) => self.forget_missing(),
            (5, Some(r)) => self.forget_project(&r.path),
            (4, Some(r)) => {
                let _ = std::process::Command::new(if cfg!(target_os = "macos") {
                    "open"
                } else {
                    "xdg-open"
                })
                .arg(&r.path)
                .spawn();
            }
            (n, Some(r)) => {
                self.open_project(&r.path);
                match n {
                    0 => self.view = View::Overview,
                    1 => self.view = View::Export,
                    2 => self.run_command(Command::ScanGame, &ctx),
                    3 => self.run_command(Command::VerifyGame, &ctx),
                    _ => {}
                }
            }
            _ => {}
        }
    }

    // =========================================================== marketplace

    fn marketplace_page(&mut self, ui: &mut Ui) {
        page_header(
            ui,
            "Marketplace",
            "Community game ports, plugins and SDKs. Everything is checked by SHA-256 and asks before installing.",
        );
        widgets::segmented(
            ui,
            &mut self.market_tab,
            &[
                (MarketTab::Browse, "Browse", FOCUS),
                (MarketTab::Installed, "Installed", FOCUS),
                (MarketTab::Publish, "Publish", FOCUS),
            ],
        );
        ui.add_space(14.0);
        match self.market_tab {
            MarketTab::Browse => self.market_browse(ui),
            MarketTab::Installed => {
                split(ui, 0.5, 880.0, |ui, col| {
                    if col == 0 {
                        self.installed_plugins(ui);
                        ui.add_space(14.0);
                        self.installed_sdks(ui);
                    } else {
                        self.plugin_tools(ui);
                    }
                });
            }
            MarketTab::Publish => self.market_publish(ui),
        }
    }

    fn market_browse(&mut self, ui: &mut Ui) {
        let mut go_publish = false;
        let mut refresh = false;
        let mut install = None;
        card(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.label(RichText::new("🔌").size(30.0).color(TEAL));
                ui.vertical(|ui| {
                    ui.label(RichText::new("Built by the community").size(18.0).strong().color(Color32::WHITE));
                    hint(ui, "Game ports are ready-made porting projects: settings, recipes and patches for a game. Import plugins read engines, export plugins build for platforms, and SDKs add new platforms.");
                    hint(ui, "A port never contains the game. You play it with your own copy, which the port checks against its SHA-256 hashes.");
                });
            });
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                go_publish = primary(ui, true, "Publish yours").clicked();
                if ui.button("Open the marketplace repository").clicked() {
                    let _ = std::process::Command::new(if cfg!(target_os = "macos") {
                        "open"
                    } else {
                        "xdg-open"
                    })
                    .arg(ptk_core::market::MARKETPLACE_REPO)
                    .spawn();
                }
            });
        });
        ui.add_space(14.0);
        ui.horizontal(|ui| {
            ui.add(
                egui::TextEdit::singleline(&mut self.market_search)
                    .hint_text("🔍  Search games, plugins and SDKs")
                    .desired_width((ui.available_width() - 260.0).max(160.0)),
            );
            egui::ComboBox::from_id_salt("market-kind")
                .width(120.0)
                .selected_text(self.market_kind.map(|k| k.label()).unwrap_or("Everything"))
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut self.market_kind, None, "Everything");
                    for k in [EntryKind::Project, EntryKind::Plugin, EntryKind::Sdk] {
                        ui.selectable_value(&mut self.market_kind, Some(k), k.label());
                    }
                });
            refresh = ui
                .add_enabled(self.busy.is_none(), egui::Button::new("⟳ Refresh"))
                .clicked();
        });
        ui.add_space(12.0);
        match &self.market {
            None => {
                card(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.spinner();
                        ui.label("Loading the marketplace…");
                    });
                });
            }
            Some(Err(e)) => {
                let e = e.clone();
                card(ui, |ui| {
                    card_title(ui, "The marketplace could not be loaded");
                    widgets::message(ui, WARN, &e);
                    hint(
                        ui,
                        &format!(
                            "The index is read from {}. Add another source in Settings → Marketplace, install plugins and SDKs from a folder (Installed tab), or prepare your own (Publish tab).",
                            ptk_core::market::index_urls_from(&self.app_settings).join(", ")
                        ),
                    );
                });
            }
            Some(Ok(index)) => {
                let mut entries: Vec<_> = index
                    .entries
                    .iter()
                    .filter(|e| e.matches(&self.market_search))
                    .filter(|e| self.market_kind.is_none_or(|k| e.kind == k))
                    .filter(|e| {
                        self.market_for.as_ref().is_none_or(|(engine, target)| {
                            e.kind == EntryKind::Plugin
                                && e.engines.iter().any(|x| x == engine)
                                && (e.targets.is_empty() || e.targets.iter().any(|x| x == target))
                        })
                    })
                    .cloned()
                    .collect();
                if let Some((engine, target)) = self.market_for.clone() {
                    let route = ptk_core::routes::route_for(Some(&engine));
                    let name = route
                        .map(|r| r.name.to_string())
                        .unwrap_or_else(|| engine.clone());
                    let mut clear = false;
                    ui.horizontal_wrapped(|ui| {
                        ui.label(
                            RichText::new(format!(
                                "Export plugins for {name} that build for {target}"
                            ))
                            .color(TEAL),
                        );
                        if ui.small_button("Show everything").clicked() {
                            clear = true;
                        }
                    });
                    if entries.is_empty() {
                        let how = route.map(|r| r.how).unwrap_or("");
                        widgets::message(
                            ui,
                            WARN,
                            &format!(
                                "No plugin in the Marketplace exports {name} to {target}. {how}"
                            ),
                        );
                    }
                    if clear {
                        self.market_for = None;
                    }
                    ui.add_space(8.0);
                }
                // Game ports first.
                entries.sort_by_key(|e| e.kind != EntryKind::Project);
                let ctx = ui.ctx().clone();
                let posters: Vec<Option<egui::TextureHandle>> = entries
                    .iter()
                    .map(|e| e.poster.as_deref().and_then(|u| self.poster(&ctx, u)))
                    .collect();
                let idle = self.busy.is_none();
                if !self.market_updates.is_empty() {
                    let names: Vec<String> = self
                        .market_updates
                        .iter()
                        .map(|u| format!("{} {} → {}", u.id, u.installed, u.available))
                        .collect();
                    widgets::message(
                        ui,
                        FOCUS,
                        &format!(
                            "⬆ {} update(s) available: {}",
                            names.len(),
                            names.join(", ")
                        ),
                    );
                    ui.add_space(10.0);
                }
                let installed: std::collections::HashMap<String, (String, bool)> = self
                    .plugins
                    .as_ref()
                    .and_then(|r| r.as_ref().ok())
                    .map(|v| {
                        v.iter()
                            .map(|p| (p.id.clone(), (p.version.clone(), p.builtin)))
                            .collect()
                    })
                    .unwrap_or_default();
                let mut manage = false;
                let updates = self.market_updates.clone();
                let update_of = |e: &ptk_core::market::MarketEntry| {
                    updates
                        .iter()
                        .find(|u| u.kind == e.kind && u.id == e.id)
                        .cloned()
                };
                let mut check = None;
                if entries.is_empty() {
                    empty_state(
                        ui,
                        "🔌",
                        "Nothing here yet",
                        "Be the first: publish a game port, a plugin or an SDK from the Publish tab.",
                    );
                } else {
                    let cols = columns_for(ui, 300.0, 3);
                    card_grid(ui, entries.len(), cols, |ui, i| {
                        let e = &entries[i];
                        if e.kind == EntryKind::Project {
                            match port_card(
                                ui,
                                e,
                                posters[i].as_ref(),
                                idle,
                                self.copy_checks.get(&e.id),
                                update_of(e).as_ref(),
                            ) {
                                Some(PortAction::Get) => install = Some(e.clone()),
                                Some(PortAction::Check) => check = Some(e.clone()),
                                None => {}
                            }
                            return;
                        }
                        let update = update_of(e);
                        card(ui, |ui| {
                            ui.horizontal(|ui| {
                                let icon = match e.kind {
                                    EntryKind::Plugin => "🔌",
                                    EntryKind::Sdk => "🧩",
                                    EntryKind::Project => "📦",
                                };
                                ui.label(RichText::new(icon).size(22.0).color(TEAL));
                                ui.vertical(|ui| {
                                    ui.label(
                                        RichText::new(&e.name)
                                            .size(15.0)
                                            .strong()
                                            .color(Color32::WHITE),
                                    );
                                    ui.label(
                                        RichText::new(format!(
                                            "{} · v{}",
                                            if e.author.is_empty() {
                                                "community"
                                            } else {
                                                &e.author
                                            },
                                            e.version
                                        ))
                                        .size(11.5)
                                        .color(TEXT_WEAK),
                                    );
                                });
                            });
                            if !e.description.is_empty() {
                                hint(ui, &e.description);
                            }
                            ui.horizontal_wrapped(|ui| {
                                badge(ui, e.kind.label(), FOCUS);
                                if e.reviewed {
                                    badge(ui, "✔ reviewed", GREEN);
                                } else {
                                    badge(ui, "not reviewed", TEXT_FAINT);
                                }
                                for t in e.engines.iter().chain(&e.targets).take(4) {
                                    badge(ui, t, LIGHT_BLUE);
                                }
                                if let Some(u) = &update {
                                    badge(
                                        ui,
                                        &format!("⬆ v{} → v{}", u.installed, u.available),
                                        FOCUS,
                                    );
                                }
                            });
                            ui.add_space(4.0);
                            let have = (e.kind == EntryKind::Plugin)
                                .then(|| installed.get(&e.id))
                                .flatten();
                            match (&update, have) {
                                (None, Some((version, builtin))) => {
                                    ui.horizontal_wrapped(|ui| {
                                        let text = if *builtin {
                                            format!("✔ Built in (v{version})")
                                        } else {
                                            format!("✔ Installed (v{version})")
                                        };
                                        ui.label(RichText::new(text).color(GREEN));
                                        if ui.small_button("Manage").clicked() {
                                            manage = true;
                                        }
                                    });
                                }
                                _ => {
                                    let label = match (&update, e.kind) {
                                        (Some(_), _) => "Update…",
                                        (None, EntryKind::Project) => "Download",
                                        (None, _) => "Install…",
                                    };
                                    if primary(ui, idle, label).clicked() {
                                        install = Some(e.clone());
                                    }
                                }
                            }
                        });
                    });
                }
                if let Some(e) = check {
                    self.check_copy(ui.ctx(), e);
                }
                if manage {
                    self.market_tab = MarketTab::Installed;
                }
            }
        }
        if go_publish {
            self.market_tab = MarketTab::Publish;
        }
        if refresh {
            self.refresh_market(ui.ctx());
        }
        if let Some(e) = install {
            self.market_install(ui.ctx(), e);
        }
    }

    fn installed_sdks(&mut self, ui: &mut Ui) {
        let sdks = ptk_core::sdk::installed();
        card(ui, |ui| {
            card_title(ui, "SDKs");
            if sdks.is_empty() {
                hint(
                    ui,
                    "No SDKs installed. An SDK adds platforms (targets) and their exporters.",
                );
            }
            for s in &sdks {
                ui.horizontal(|ui| {
                    ui.label(RichText::new("🧩").color(TEAL));
                    ui.label(RichText::new(&s.name).strong().color(Color32::WHITE));
                    ui.label(RichText::new(format!("v{}", s.version)).color(TEXT_WEAK));
                });
            }
        });
    }

    fn market_publish(&mut self, ui: &mut Ui) {
        let busy = self.busy.is_some();
        let cols = columns_for(ui, 320.0, 3);
        card_grid(ui, 3, cols, |ui, i| match i {
            0 => {
                card(ui, |ui| {
                    ui.horizontal(|ui| {
                        badge(ui, "1", FOCUS);
                        card_title(ui, "Create");
                    });
                    hint(
                        ui,
                        "Start from a template. An SDK holds ptk-sdk.toml, targets/*.toml (your platforms) and plugins/ (their exporters).",
                    );
                    ui.label(RichText::new("New SDK").color(TEXT_WEAK));
                    widgets::path_field(
                        ui,
                        &mut self.sdk_new_dir,
                        "new folder, e.g. nova-sdk",
                        Pick::Folder,
                    );
                    if ui
                        .add_enabled(!busy, egui::Button::new("Create SDK"))
                        .clicked()
                    {
                        self.create_sdk(ui.ctx());
                    }
                    ui.add_space(6.0);
                    ui.label(RichText::new("New plugin").color(TEXT_WEAK));
                    widgets::path_field(ui, &mut self.plugin_new_dir, "new folder", Pick::Folder);
                    ui.horizontal(|ui| {
                        ui.radio_value(&mut self.plugin_kind_export, false, "Import");
                        ui.radio_value(&mut self.plugin_kind_export, true, "Export");
                        if ui
                            .add_enabled(!busy, egui::Button::new("Create plugin"))
                            .clicked()
                        {
                            self.create_plugin(ui.ctx());
                        }
                    });
                });
            }
            1 => {
                card(ui, |ui| {
                    ui.horizontal(|ui| {
                        badge(ui, "2", FOCUS);
                        card_title(ui, "Validate & package");
                    });
                    hint(
                        ui,
                        "Checks the folder, zips it with fixed timestamps and computes its SHA-256. Also accepts a .ptk project.",
                    );
                    widgets::path_field(
                        ui,
                        &mut self.publish_path,
                        "SDK or plugin folder, or .ptk",
                        Pick::Folder,
                    );
                    if primary(ui, !busy, "Validate & package").clicked() {
                        self.publish(ui.ctx());
                    }
                });
            }
            _ => {
                card(ui, |ui| {
                    ui.horizontal(|ui| {
                        badge(ui, "3", FOCUS);
                        card_title(ui, "Submit");
                    });
                    hint(
                        ui,
                        "Publishes what you packaged in one step with the GitHub CLI (gh): a GitHub release with the file, and a pull request that adds the entry to the marketplace. A maintainer reviews and merges it.",
                    );
                    let is_port =
                        matches!(&self.published, Some(Ok(p)) if p.kind == EntryKind::Project);
                    if is_port {
                        ui.label(RichText::new("Where players get the game").color(TEXT_WEAK));
                        ui.add(
                            egui::TextEdit::singleline(&mut self.submit_store)
                                .hint_text("https://store.steampowered.com/app/…")
                                .desired_width(f32::INFINITY),
                        );
                    }
                    let ready = matches!(&self.published, Some(Ok(_)));
                    if ui
                        .add_enabled(
                            !busy && ready,
                            egui::Button::new("Publish to the marketplace…"),
                        )
                        .on_disabled_hover_text("Validate & package something first")
                        .clicked()
                    {
                        self.plan_submit(ui.ctx());
                    }
                    if ui.button("Open the marketplace repository").clicked() {
                        let _ = std::process::Command::new(if cfg!(target_os = "macos") {
                            "open"
                        } else {
                            "xdg-open"
                        })
                        .arg(ptk_core::market::MARKETPLACE_REPO)
                        .spawn();
                    }
                });
            }
        });
        match self.published.clone() {
            None => {}
            Some(Err(e)) => widgets::message(ui, ERROR, &e),
            Some(Ok(p)) => {
                card(ui, |ui| {
                    card_title(ui, &format!("✔ {} {} is ready", p.kind.label(), p.entry.id));
                    egui::Grid::new("published")
                        .num_columns(2)
                        .spacing([14.0, 4.0])
                        .show(ui, |ui| {
                            kv(ui, "File", p.archive.display().to_string());
                            kv(
                                ui,
                                "SHA-256",
                                RichText::new(&p.entry.sha256).monospace().size(11.5),
                            );
                        });
                    for w in &p.warnings {
                        ui.label(RichText::new(format!("▲ {w}")).color(WARN));
                    }
                    let mut json = serde_json::to_string_pretty(&p.entry).unwrap_or_default();
                    ui.add_space(4.0);
                    ui.label(RichText::new("Entry for index.json").color(TEXT_WEAK));
                    ui.add(
                        egui::TextEdit::multiline(&mut json)
                            .code_editor()
                            .desired_width(f32::INFINITY)
                            .interactive(false),
                    );
                    if ui.button("Copy entry").clicked() {
                        ui.ctx().copy_text(json);
                    }
                });
            }
        }
        let mut publish_now = false;
        let mut cancel = false;
        match &self.submit_plan {
            None => {}
            Some(Err(e)) => widgets::message(ui, ERROR, e),
            Some(Ok(plan)) => {
                ui.add_space(12.0);
                card(ui, |ui| {
                    card_title(ui, "Publish to the marketplace?");
                    egui::Grid::new("submit-plan")
                        .num_columns(2)
                        .spacing([14.0, 4.0])
                        .show(ui, |ui| {
                            kv(
                                ui,
                                "Entry",
                                format!(
                                    "{} {} ({})",
                                    plan.entry.kind.label(),
                                    plan.entry.id,
                                    plan.entry.version
                                ),
                            );
                            kv(ui, "As", plan.login.clone());
                            kv(
                                ui,
                                "Release",
                                format!(
                                    "{} in {}{}",
                                    plan.tag,
                                    plan.work_repo,
                                    if plan.fork { " (your fork)" } else { "" }
                                ),
                            );
                            for a in &plan.assets {
                                kv(ui, "Upload", widgets::display_path(a));
                            }
                            kv(
                                ui,
                                "Pull request",
                                format!("{} → {}", plan.branch, plan.upstream),
                            );
                        });
                    widgets::message(ui, WARN, "The release and the pull request are public.");
                    ui.horizontal(|ui| {
                        publish_now = primary(ui, self.busy.is_none(), "Publish now").clicked();
                        cancel = ui.button("Cancel").clicked();
                    });
                });
            }
        }
        match &self.submit_result {
            Some(Ok(url)) => {
                let url = url.clone();
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    widgets::message(ui, GREEN, "✔ Pull request opened");
                    if ui.link(&url).clicked() {
                        crate::app::open_url(&url);
                    }
                });
            }
            Some(Err(e)) => widgets::message(ui, ERROR, e),
            None => {}
        }
        if publish_now {
            self.run_submit(ui.ctx());
        }
        if cancel {
            self.submit_plan = None;
        }
    }

    // ================================================================ export

    fn export_page(&mut self, ui: &mut Ui) {
        page_header(
            ui,
            "Export & share",
            "Build for another platform with an export plugin, or share the project as a .ptk file.",
        );
        let engine = self
            .project
            .as_ref()
            .and_then(|p| p.saved.game.engine.clone());
        let targets = self
            .project
            .as_ref()
            .map(|p| p.saved.targets.clone())
            .unwrap_or_default();
        if self.plugins.is_none() && self.busy.is_none() {
            self.refresh_plugins(ui.ctx());
        }
        if self.plugins.is_some() {
            self.refresh_export_checks(ui.ctx());
        }
        let checks = self.export_checks.clone();
        let mut fix: Option<(String, String)> = None;
        // Export plugins that take this engine, newest copy of each id first
        // (an old installed copy never hides a newer built-in one).
        let for_engine: Vec<ptk_core::plugins::ListedPlugin> = match &self.plugins {
            Some(Ok(list)) => newest_copies(list, &self.app_settings.plugin_versions)
                .into_iter()
                .filter(|p| p.kind == "export" && p.valid)
                .filter(|p| {
                    p.inputs.is_empty()
                        || engine
                            .as_deref()
                            .is_some_and(|e| p.inputs.iter().any(|i| i == e))
                })
                .collect(),
            _ => Vec::new(),
        };
        let exporter_for = |t: &str| -> Option<(String, bool)> {
            for_engine
                .iter()
                .find(|p| p.targets.iter().any(|x| x == t))
                .map(|p| (p.name.clone(), p.builtin))
        };
        // Targets the installed exporters can build this game for.
        let possible: Vec<String> = self
            .registry
            .ids()
            .into_iter()
            .filter(|t| for_engine.iter().any(|p| p.targets.iter().any(|x| x == t)))
            .collect();
        type Row = (String, String, Option<(String, bool)>);
        let rows: Vec<Row> = targets
            .iter()
            .map(|t| {
                let name = self
                    .registry
                    .get(t)
                    .map(|x| x.name.clone())
                    .unwrap_or_else(|| t.clone());
                (t.clone(), name, exporter_for(t))
            })
            .collect();
        let engine_name = ptk_core::routes::route_for(engine.as_deref())
            .map(|r| r.name.to_string())
            .unwrap_or_else(|| "This game".into());
        let possible_names: Vec<String> = possible
            .iter()
            .map(|t| {
                self.registry
                    .get(t)
                    .map(|x| x.name.clone())
                    .unwrap_or_else(|| t.clone())
            })
            .collect();
        let idle = self.busy.is_none();
        let mut find = None;
        let mut find_plugin: Option<String> = None;
        let mut add: Option<String> = None;
        let mut remove: Option<String> = None;
        let mut export: Option<Vec<String>> = None;
        let mut estimate: Option<Vec<String>> = None;
        split(ui, 0.55, 900.0, |ui, col| {
            if col == 0 {
                card(ui, |ui| {
                    card_title(ui, "Porting route");
                    match ptk_core::routes::route_for(engine.as_deref()) {
                        Some(r) => {
                            ui.horizontal_wrapped(|ui| {
                                ui.label(RichText::new(r.name).size(18.0).strong().color(TEAL));
                                badge(ui, r.kind.label(), route_color(r));
                            });
                            ui.label(r.how);
                            if let (Some(name), Some(url)) = (r.runtime, r.runtime_url) {
                                ui.horizontal_wrapped(|ui| {
                                    ui.label(RichText::new("Runtime:").color(TEXT_WEAK));
                                    ui.hyperlink_to(name, url);
                                });
                            }
                            if let Some(plugin) = r.plugin {
                                let installed = exporter_for_engine_installed(
                                    &self.installed_plugin_ids(),
                                    plugin,
                                );
                                if installed {
                                    ui.label(
                                        RichText::new(format!(
                                            "✔ The {plugin} plugin is installed"
                                        ))
                                        .color(GREEN),
                                    );
                                } else if ui
                                    .button(format!("Get the {plugin} plugin in the Marketplace"))
                                    .clicked()
                                {
                                    find_plugin = Some(plugin.to_owned());
                                }
                            }
                        }
                        None => hint(
                            ui,
                            "Scan your game first (Scan tab) to see what exporting it takes.",
                        ),
                    }
                    match &self.import_result {
                        Some(Ok(r)) => {
                            ui.add_space(6.0);
                            ui.label(RichText::new(format!("✔ {}", r.summary)).color(GREEN));
                            for w in &r.warnings {
                                ui.label(RichText::new(format!("⚠ {w}")).color(WARN));
                            }
                        }
                        Some(Err(e)) => {
                            ui.add_space(6.0);
                            ui.label(RichText::new(format!("⚠ Import plugin: {e}")).color(WARN));
                        }
                        None => {}
                    }
                });
                ui.add_space(14.0);
                card(ui, |ui| {
                    card_title(ui, "Export to");
                    if rows.is_empty() {
                        hint(ui, "Choose targets in Overview.");
                    }
                    egui::Grid::new("export-targets")
                        .num_columns(3)
                        .spacing([14.0, 8.0])
                        .show(ui, |ui| {
                            for (id, name, plugin) in &rows {
                                ui.label(RichText::new(name).color(Color32::WHITE))
                                    .on_hover_text(id);
                                match plugin {
                                    Some((pname, builtin)) => {
                                        let tag = if *builtin { "built-in" } else { "installed" };
                                        let problems = match checks.get(id) {
                                            Some(Ok(c)) => c.problems.clone(),
                                            _ => Vec::new(),
                                        };
                                        ui.vertical(|ui| {
                                            let blocked =
                                                problems.iter().any(|p| p.severity == "error");
                                            badge(
                                                ui,
                                                &format!("{pname} · {tag}"),
                                                if blocked { WARN } else { GREEN },
                                            );
                                            let warnings: Vec<_> = problems
                                                .iter()
                                                .filter(|p| p.severity != "error")
                                                .collect();
                                            for p in
                                                problems.iter().filter(|p| p.severity == "error")
                                            {
                                                ui.label(
                                                    RichText::new(format!("✖ {}", p.message))
                                                        .size(12.0)
                                                        .color(ERROR),
                                                );
                                                if let Some(a) = &p.action {
                                                    ui.horizontal(|ui| {
                                                        if ui.small_button(&a.label).clicked() {
                                                            fix = Some((
                                                                a.setting.clone(),
                                                                a.value.clone(),
                                                            ));
                                                        }
                                                        if let Some(url) = &a.url {
                                                            ui.hyperlink_to("Read first", url);
                                                        }
                                                    });
                                                }
                                            }
                                            if !warnings.is_empty() {
                                                egui::CollapsingHeader::new(
                                                    RichText::new(format!(
                                                        "▲ {} {}",
                                                        warnings.len(),
                                                        if warnings.len() == 1 {
                                                            "warning"
                                                        } else {
                                                            "warnings"
                                                        }
                                                    ))
                                                    .size(12.0)
                                                    .color(WARN),
                                                )
                                                .id_salt(("check-warnings", id))
                                                .show(ui, |ui| {
                                                    for p in warnings {
                                                        ui.label(
                                                            RichText::new(&p.message)
                                                                .size(12.0)
                                                                .color(WARN),
                                                        );
                                                    }
                                                });
                                            }
                                        });
                                    }
                                    None if !possible.is_empty() => {
                                        // The installed exporters cover this engine,
                                        // just not this platform.
                                        ui.horizontal(|ui| {
                                            badge(ui, "not possible for this engine", TEXT_FAINT)
                                                .on_hover_text(format!(
                                                    "{} can be exported to: {}",
                                                    engine_name,
                                                    possible_names.join(", ")
                                                ));
                                            if ui
                                                .small_button("Remove")
                                                .on_hover_text(
                                                    "Remove this target from the project",
                                                )
                                                .clicked()
                                            {
                                                remove = Some(id.clone());
                                            }
                                        });
                                    }
                                    None => {
                                        let route =
                                            ptk_core::routes::route_for(engine.as_deref());
                                        let portable = route.is_none_or(|r| {
                                            matches!(
                                                r.kind,
                                                ptk_core::routes::RouteKind::Retarget
                                                    | ptk_core::routes::RouteKind::Partial
                                                    | ptk_core::routes::RouteKind::Runtime
                                            )
                                        });
                                        ui.horizontal(|ui| {
                                            if let (false, Some(r)) = (portable, route) {
                                                // No plugin can help: say why instead of
                                                // sending the user shopping.
                                                badge(ui, r.kind.label(), ERROR)
                                                    .on_hover_text(r.how);
                                                return;
                                            }
                                            badge(ui, "no exporter", TEXT_FAINT);
                                            if ui
                                                .small_button("Find one")
                                                .on_hover_text(format!(
                                                    "Export plugins for {engine_name} in the Marketplace"
                                                ))
                                                .clicked()
                                            {
                                                find = Some(id.clone());
                                            }
                                        });
                                    }
                                }
                                if ui
                                    .add_enabled(
                                        idle && plugin.is_some(),
                                        egui::Button::new("Export"),
                                    )
                                    .clicked()
                                {
                                    export = Some(vec![id.clone()]);
                                }
                                ui.end_row();
                            }
                        });
                    let missing: Vec<&String> =
                        possible.iter().filter(|t| !targets.contains(t)).collect();
                    if !missing.is_empty() {
                        ui.add_space(6.0);
                        ui.horizontal_wrapped(|ui| {
                            ui.label(RichText::new("Also possible:").color(TEXT_WEAK));
                            for t in missing {
                                let name = self
                                    .registry
                                    .get(t)
                                    .map(|x| x.name.clone())
                                    .unwrap_or_else(|| t.clone());
                                if ui
                                    .small_button(format!("+ {name}"))
                                    .on_hover_text("Add this target to the project")
                                    .clicked()
                                {
                                    add = Some(t.clone());
                                }
                            }
                        });
                    }
                    ui.add_space(10.0);
                    ui.label(RichText::new("Output folder").color(TEXT_WEAK));
                    widgets::path_field(
                        ui,
                        &mut self.export_output,
                        "each target gets a subfolder",
                        Pick::Folder,
                    );
                    egui::CollapsingHeader::new(
                        RichText::new("Plugin settings").color(TEXT_WEAK),
                    )
                    .id_salt("export-settings")
                    .show(ui, |ui| {
                        ui.add(
                            egui::TextEdit::multiline(&mut self.export_settings)
                                .hint_text("templates=/path/to/godot/templates\nthreads=true\nbundle_id=com.you.game")
                                .desired_rows(3)
                                .desired_width(f32::INFINITY)
                                .font(egui::TextStyle::Monospace),
                        );
                        hint(ui, "One key=value per line, passed to the export plugin.");
                    });
                    ui.add_space(6.0);
                    let ready: Vec<String> = rows
                        .iter()
                        .filter(|r| r.2.is_some())
                        .map(|r| r.0.clone())
                        .collect();
                    let label = if ready.len() > 1 {
                        format!("Export all ({})", ready.len())
                    } else {
                        "Export".to_owned()
                    };
                    let options_ignored = self.project.as_ref().and_then(|p| {
                        ptk_core::category::options_ignored(
                            p.manifest.game.engine.as_deref().unwrap_or(""),
                        )
                    });
                    let options_set = options_ignored.is_none()
                        && self.project.as_ref().is_some_and(|p| {
                            p.saved
                                .options
                                .values()
                                .any(|o| o.action != ptk_core::manifest::AssetAction::Keep)
                        });
                    ui.horizontal(|ui| {
                        if primary(ui, idle && !ready.is_empty(), &label).clicked() {
                            export = Some(ready.clone());
                        }
                        if ui
                            .add_enabled(
                                idle && !ready.is_empty() && options_set,
                                egui::Button::new("Estimate savings"),
                            )
                            .on_hover_text(
                                "Re-encodes the largest files of each kind to estimate what the advanced options save. Builds nothing.",
                            )
                            .on_disabled_hover_text(options_ignored.unwrap_or(
                                "Set some advanced options to Recompress first (Game page).",
                            ))
                            .clicked()
                        {
                            estimate = Some(ready.clone());
                        }
                    });
                });
            } else {
                card(ui, |ui| {
                    card_title(ui, "Without source code?");
                    egui::Grid::new("routes")
                        .num_columns(2)
                        .spacing([12.0, 6.0])
                        .show(ui, |ui| {
                            for r in ptk_core::routes::exportable().filter(|r| {
                                r.plugin.is_none()
                                    && (r.kind == ptk_core::routes::RouteKind::Retarget
                                        || matches!(r.engine, "unity" | "gamemaker"))
                            }) {
                                ui.label(RichText::new(r.name).color(Color32::WHITE));
                                let text = match r.kind {
                                    ptk_core::routes::RouteKind::Retarget => "yes",
                                    _ => "depends",
                                };
                                ui.label(RichText::new(text).color(route_color(r)))
                                    .on_hover_text(r.how);
                                ui.end_row();
                            }
                        });
                    let depends: Vec<_> = ptk_core::routes::exportable()
                        .filter(|r| {
                            r.kind == ptk_core::routes::RouteKind::Partial
                                && !matches!(r.engine, "unity" | "gamemaker")
                        })
                        .collect();
                    egui::CollapsingHeader::new(
                        RichText::new(format!("Depends on the build ({} engines)", depends.len()))
                            .color(WARN),
                    )
                    .id_salt("routes-depends")
                    .show(ui, |ui| {
                        for r in &depends {
                            ui.label(RichText::new(r.name).color(Color32::WHITE))
                                .on_hover_text(r.how);
                        }
                    });
                    let with_plugin: Vec<_> = ptk_core::routes::exportable()
                        .filter(|r| r.plugin.is_some())
                        .collect();
                    egui::CollapsingHeader::new(
                        RichText::new(format!(
                            "With a Marketplace plugin ({} engines)",
                            with_plugin.len()
                        ))
                        .color(TEAL),
                    )
                    .id_salt("routes-plugins")
                    .show(ui, |ui| {
                        egui::Grid::new("routes-plugins-grid")
                            .num_columns(2)
                            .spacing([12.0, 4.0])
                            .show(ui, |ui| {
                                for r in &with_plugin {
                                    ui.label(RichText::new(r.name).color(Color32::WHITE))
                                        .on_hover_text(r.how);
                                    ui.label(
                                        RichText::new(r.plugin.unwrap_or_default())
                                            .monospace()
                                            .color(TEXT_WEAK),
                                    );
                                    ui.end_row();
                                }
                            });
                    });
                    let total = ptk_core::routes::routes().len();
                    hint(
                        ui,
                        &format!(
                            "Hover a row for details. Porting Toolkit recognises {total} engines in all: the ptk engines command lists them with what each needs."
                        ),
                    );
                });
            }
        });
        if let Some((k, v)) = fix {
            self.apply_setting(&k, &v);
        }
        if let Some(p) = &mut self.project {
            if let Some(t) = add {
                if !p.manifest.targets.contains(&t) {
                    p.manifest.targets.push(t);
                }
            }
            if let Some(t) = remove {
                p.manifest.targets.retain(|x| *x != t);
            }
        }
        if let Some(p) = find_plugin {
            self.market_for = None;
            self.market_search = p;
            self.market_tab = MarketTab::Browse;
            self.go(View::Marketplace, ui.ctx());
        }
        if let Some(t) = find {
            if let Some(e) = engine.clone() {
                // A target id: plugins that export this engine to it.
                self.market_for = Some((e, t));
                self.market_search.clear();
            } else {
                self.market_search = t;
            }
            self.market_tab = MarketTab::Browse;
            self.go(View::Marketplace, ui.ctx());
        }
        if let Some(t) = export {
            self.export_targets(ui.ctx(), t, ptk_core::convert::AssetsMode::Apply);
        }
        if let Some(t) = estimate {
            self.export_targets(ui.ctx(), t, ptk_core::convert::AssetsMode::Plan);
        }
        self.export_results_card(ui);
        ui.add_space(14.0);
        ui.label(
            RichText::new("Share")
                .size(17.0)
                .strong()
                .color(Color32::WHITE),
        );
        ui.add_space(6.0);
        let cols = columns_for(ui, 330.0, 3);
        card_grid(ui, 3, cols, |ui, i| match i {
            0 => self.pack_card(ui),
            1 => self.unpack_card(ui),
            _ => self.verify_card(ui),
        });
    }

    fn export_results_card(&mut self, ui: &mut Ui) {
        if self.export_results.is_empty() {
            return;
        }
        ui.add_space(14.0);
        let mut play = None;
        let mut accept: Option<String> = None;
        card(ui, |ui| {
            card_title(ui, "Builds");
            for (i, (target, r)) in self.export_results.iter().enumerate() {
                if i > 0 {
                    ui.separator();
                }
                match r {
                    Ok(r) => {
                        ui.horizontal_wrapped(|ui| {
                            ui.label(RichText::new("✔").color(GREEN));
                            ui.label(RichText::new(&r.summary).strong().color(Color32::WHITE));
                            badge(ui, target, LIGHT_BLUE);
                        });
                        ui.label(
                            RichText::new(match &r.build {
                                Some(b) => format!(
                                    "{} {} · {}",
                                    r.plugin_name,
                                    r.plugin_version,
                                    b.describe()
                                ),
                                None => format!("{} {}", r.plugin_name, r.plugin_version),
                            })
                            .size(12.0)
                            .color(TEXT_FAINT),
                        );
                        for w in &r.warnings {
                            ui.label(RichText::new(format!("⚠ {w}")).color(WARN));
                        }
                        for n in &r.notes {
                            ui.label(RichText::new(n).color(TEXT_WEAK));
                        }
                        if let Some(a) = &r.assets {
                            assets_report(ui, a, &format!("assets-{i}"));
                        }
                        ui.horizontal(|ui| {
                            if let Some(dir) = &r.serve {
                                if ui.button("▶  Play in browser").clicked() {
                                    play = Some(dir.clone());
                                }
                            } else if let Some(app) = &r.open
                                && ui.button("▶  Open").clicked()
                            {
                                crate::app::reveal(app);
                            }
                            if let Some(p) = r.open.as_ref().or(r.outputs.first())
                                && ui.button("Show in Finder").clicked()
                            {
                                crate::app::show_in_finder(p);
                            }
                        });
                    }
                    Err(e) => {
                        ui.horizontal_wrapped(|ui| {
                            ui.label(RichText::new("✖").color(ERROR));
                            badge(ui, target, LIGHT_BLUE);
                        });
                        ui.label(RichText::new(e).color(ERROR));
                        // A third party's terms (Unity's player): the user accepts
                        // them here once, and the plugin remembers it.
                        if e.contains("accept_unity_terms=true") {
                            ui.horizontal(|ui| {
                                if primary(
                                    ui,
                                    self.busy.is_none(),
                                    "Accept Unity's terms and export",
                                )
                                .clicked()
                                {
                                    accept = Some(target.clone());
                                }
                                ui.hyperlink_to(
                                    "Read the terms",
                                    "https://unity.com/legal/terms-of-service",
                                );
                            });
                        }
                    }
                }
            }
            if let Some(s) = &self.server {
                ui.add_space(6.0);
                hint(
                    ui,
                    &format!("Serving {} at {}", widgets::display_path(&s.root), s.url()),
                );
            }
        });
        if let Some(dir) = play {
            self.play_in_browser(&dir);
        }
        if let Some(t) = accept {
            if !self.export_settings.contains("accept_unity_terms") {
                if !self.export_settings.is_empty() && !self.export_settings.ends_with('\n') {
                    self.export_settings.push('\n');
                }
                self.export_settings.push_str("accept_unity_terms=true\n");
            }
            self.export_targets(ui.ctx(), vec![t], ptk_core::convert::AssetsMode::Apply);
        }
    }

    // ============================================================== settings

    fn settings_page(&mut self, ui: &mut Ui) {
        page_header(
            ui,
            "Settings",
            "This Mac, the platforms Porting Toolkit knows, and about the app.",
        );
        widgets::segmented(
            ui,
            &mut self.settings_tab,
            &[
                (SettingsTab::Defaults, "Defaults", FOCUS),
                (SettingsTab::Environment, "Environment", FOCUS),
                (SettingsTab::Marketplace, "Marketplace", FOCUS),
                (SettingsTab::Targets, "Targets", FOCUS),
                (SettingsTab::About, "About", FOCUS),
            ],
        );
        ui.add_space(14.0);
        match self.settings_tab {
            SettingsTab::Defaults => self.defaults_page(ui),
            SettingsTab::Environment => self.doctor_page(ui),
            SettingsTab::Marketplace => self.market_sources_page(ui),
            SettingsTab::Targets => self.targets_page(ui),
            SettingsTab::About => self.about_page(ui),
        }
    }

    /// App-wide defaults for new projects.
    fn defaults_page(&mut self, ui: &mut Ui) {
        // Missing categories mean "keep"; filling them in is not an edit.
        for c in CATEGORIES {
            for s in [&mut self.app_settings, &mut self.app_settings_saved] {
                s.default_options.entry(c.id.to_owned()).or_default();
            }
        }
        hint(
            ui,
            "New projects start with these. Each project keeps its own copy, so changing them never changes existing projects. Changes are saved automatically.",
        );
        ui.add_space(10.0);
        let registry = self.registry.clone();
        let s = &mut self.app_settings;
        split(ui, 0.5, 880.0, |ui, col| {
            if col == 0 {
                card(ui, |ui| {
                    card_title(ui, "Targets of new projects");
                    ui.horizontal_wrapped(|ui| {
                        for t in registry.iter() {
                            let mut on = s.default_targets.contains(&t.id);
                            let text =
                                RichText::new(format!("{}  {}", if on { "✔" } else { "+" }, t.id));
                            if ui
                                .toggle_value(&mut on, text)
                                .on_hover_text(&t.name)
                                .changed()
                            {
                                if on {
                                    s.default_targets.push(t.id.clone());
                                } else {
                                    s.default_targets.retain(|x| x != &t.id);
                                }
                            }
                        }
                    });
                    hint(ui, "None selected: this computer's platform.");
                });
            } else {
                card(ui, |ui| {
                    card_title(ui, "Projects folder");
                    let mut text = s
                        .projects_dir
                        .as_ref()
                        .map(|p| widgets::display_path(p))
                        .unwrap_or_default();
                    let shown = widgets::display_path(&s.projects_dir());
                    if widgets::path_field(ui, &mut text, &shown, Pick::Folder) {
                        s.projects_dir = Some(text.trim())
                            .filter(|t| !t.is_empty())
                            .map(crate::app::expand_tilde);
                    }
                    hint(ui, "Where Import Game creates new projects.");
                });
            }
        });
        ui.add_space(14.0);
        card(ui, |ui| {
            card_title(ui, "Default advanced options");
            hint(
                ui,
                "What new projects keep, drop or recompress, and the default format (algorithm) for each kind of asset.",
            );
            ui.add_space(6.0);
            options_editor(ui, &mut s.default_options, "defaults", &AutoInfo::default());
        });
    }

    // ============================================================= project

    fn project_page(&mut self, ui: &mut Ui) {
        let Some(p) = &self.project else {
            page_header(
                ui,
                "Project",
                "A project holds recipes, settings, patches and the hashes of your game.",
            );
            empty_state(
                ui,
                "🗀",
                "No project open",
                "Create a new project or open an existing one.",
            );
            ui.add_space(10.0);
            ui.horizontal(|ui| {
                self.command_button(ui, Command::ImportGame, "⬇  Import Game…", true);
                self.command_button(ui, Command::NewProject, "+  New Project…", false);
                self.command_button(ui, Command::OpenProject, "🗀  Open Project…", false);
            });
            return;
        };
        let title = p
            .manifest
            .game
            .title
            .clone()
            .unwrap_or_else(|| p.manifest.name.clone());
        let path = widgets::display_path(&p.dir);
        let dir = p.dir.clone();
        let icon = ptk_core::art::resolve(&dir, &p.saved, ptk_core::art::ArtKind::Icon);
        let tex = icon.and_then(|i| self.image(&ui.ctx().clone(), &i));
        ui.horizontal(|ui| {
            if let Some(tex) = &tex {
                let (rect, _) = ui.allocate_exact_size(vec2(56.0, 56.0), egui::Sense::hover());
                widgets::image_in(ui, rect, tex, 12);
                ui.add_space(6.0);
            }
            ui.vertical(|ui| page_header(ui, &title, &path));
        });
        self.unsaved_bar(ui);
        let issues = self.issues();
        split(ui, 0.6, 880.0, |ui, col| {
            if col == 0 {
                self.project_forms(ui);
                ui.add_space(14.0);
                self.project_details(ui);
            } else {
                self.art_card(ui);
                ui.add_space(14.0);
                self.project_side(ui, &issues);
            }
        });
        ui.add_space(14.0);
        self.options_section(ui);
    }

    /// The port's icon and poster: your own images, shared in the .ptk.
    fn art_card(&mut self, ui: &mut Ui) {
        use ptk_core::art::ArtKind;
        let Some(p) = &self.project else { return };
        let dir = p.dir.clone();
        let m = p.saved.clone();
        let ctx = ui.ctx().clone();
        let icon_path = ptk_core::art::resolve(&dir, &m, ArtKind::Icon);
        let poster_path = ptk_core::art::resolve(&dir, &m, ArtKind::Poster);
        let icon = icon_path.as_deref().and_then(|i| self.image(&ctx, i));
        let poster = poster_path.as_deref().and_then(|i| self.image(&ctx, i));
        let mut action: Option<(ArtKind, bool)> = None;
        card(ui, |ui| {
            card_title(ui, "Icon & poster");
            ui.horizontal_top(|ui| {
                for (kind, tex, size, chosen) in [
                    (
                        ArtKind::Icon,
                        &icon,
                        vec2(96.0, 96.0),
                        m.game.icon.is_some(),
                    ),
                    (
                        ArtKind::Poster,
                        &poster,
                        vec2(96.0, 136.0),
                        m.game.poster.is_some(),
                    ),
                ] {
                    ui.vertical(|ui| {
                        let (rect, resp) = ui.allocate_exact_size(size, egui::Sense::click());
                        match tex {
                            Some(t) => widgets::image_in(ui, rect, t, 10),
                            None => {
                                ui.painter().rect(
                                    rect,
                                    egui::CornerRadius::same(10),
                                    INPUT,
                                    egui::Stroke::new(1.0_f32, BORDER),
                                    egui::StrokeKind::Inside,
                                );
                                widgets::centered_text(ui, rect, "+", 26.0, TEXT_FAINT);
                            }
                        }
                        if resp
                            .on_hover_text(format!("Choose the {}", kind.name()))
                            .clicked()
                        {
                            action = Some((kind, true));
                        }
                        ui.horizontal(|ui| {
                            ui.label(
                                RichText::new(match kind {
                                    ArtKind::Icon => "Icon",
                                    ArtKind::Poster => "Poster",
                                })
                                .color(TEXT_WEAK),
                            );
                            if chosen && ui.small_button("✖").on_hover_text("Remove").clicked() {
                                action = Some((kind, false));
                            }
                        });
                    });
                    ui.add_space(10.0);
                }
            });
            let note = if m.game.icon.is_none() && icon.is_some() {
                "Showing the game's own icon (only on this computer). Choose your own to use it in exports and share it."
            } else {
                "Used for the exported app and web page, and shown in the marketplace. Use images you made or may share."
            };
            hint(ui, note);
        });
        match action {
            Some((kind, true)) => self.choose_art(kind),
            Some((kind, false)) => self.set_art(&dir, kind, None),
            None => {}
        }
    }

    fn project_forms(&mut self, ui: &mut Ui) {
        let registry = self.registry.clone();
        let Some(p) = self.project.as_mut() else {
            return;
        };
        let m = &mut p.manifest;
        card(ui, |ui| {
            card_title(ui, "Game");
            egui::Grid::new("game-form")
                .num_columns(2)
                .spacing([18.0, 8.0])
                .show(ui, |ui| {
                    ui.label(RichText::new("Title").color(TEXT_WEAK));
                    let mut title = m.game.title.clone().unwrap_or_default();
                    let w = (ui.available_width() - 130.0).max(160.0);
                    if ui
                        .add(egui::TextEdit::singleline(&mut title).desired_width(w))
                        .changed()
                    {
                        m.game.title = Some(title).filter(|t| !t.trim().is_empty());
                    }
                    ui.end_row();
                    kv(
                        ui,
                        "Engine",
                        RichText::new(
                            m.game
                                .engine_label()
                                .unwrap_or_else(|| "not scanned yet".into()),
                        )
                        .color(TEAL),
                    );
                    kv(
                        ui,
                        "Platform",
                        m.game.source_platform.clone().unwrap_or_else(|| "–".into()),
                    );
                    let total: u64 = m.source_files.iter().map(|f| f.size).sum();
                    kv(
                        ui,
                        "Hashes",
                        if m.source_files.is_empty() {
                            "none recorded".to_owned()
                        } else {
                            format!("{} files, {}", m.source_files.len(), human_size(total))
                        },
                    );
                    ui.label(RichText::new("Game license").color(TEXT_WEAK));
                    let current = m.game.content_license.clone().unwrap_or_default();
                    let shown = CONTENT_LICENSES
                        .iter()
                        .find(|(id, _)| *id == current)
                        .map(|(_, l)| l.to_string())
                        .unwrap_or_else(|| current.clone());
                    egui::ComboBox::from_id_salt("content-license")
                        .width(230.0)
                        .selected_text(shown)
                        .show_ui(ui, |ui| {
                            for (id, label) in CONTENT_LICENSES {
                                if ui.selectable_label(current == *id, *label).clicked() {
                                    m.game.content_license =
                                        Some(id.to_string()).filter(|s| !s.is_empty());
                                }
                            }
                        });
                    ui.end_row();
                });
            ui.add_space(4.0);
            if m.allows_redistribution() {
                widgets::message(
                    ui,
                    GREEN,
                    "The game's own license allows redistribution, so its files may be packed.",
                );
            } else {
                hint(
                    ui,
                    "Original game files are never packed. Only your own or an open-source game changes that.",
                );
            }
        });
        ui.add_space(14.0);

        card(ui, |ui| {
            card_title(ui, "Targets");
            ui.horizontal_wrapped(|ui| {
                for t in registry.iter() {
                    let mut on = m.targets.contains(&t.id);
                    let text = RichText::new(format!("{}  {}", if on { "✔" } else { "+" }, t.id));
                    if ui
                        .toggle_value(&mut on, text)
                        .on_hover_text(&t.name)
                        .changed()
                    {
                        if on {
                            m.targets.push(t.id.clone());
                        } else {
                            m.targets.retain(|x| x != &t.id);
                        }
                    }
                }
            });
        });
    }

    /// Name, version, authors and license of the port (rarely changed).
    fn project_details(&mut self, ui: &mut Ui) {
        let Some(p) = self.project.as_mut() else {
            return;
        };
        let m = &mut p.manifest;
        card(ui, |ui| {
            egui::CollapsingHeader::new(
                RichText::new("Project details")
                    .size(15.0)
                    .color(Color32::WHITE),
            )
            .id_salt("project-details")
            .show(ui, |ui| {
                egui::Grid::new("project-form")
                    .num_columns(2)
                    .spacing([18.0, 8.0])
                    .show(ui, |ui| {
                        let w = (ui.available_width() - 110.0).max(160.0);
                        let field = |ui: &mut Ui, label: &str, value: &mut String| {
                            ui.label(RichText::new(label).color(TEXT_WEAK));
                            let r = ui.add(egui::TextEdit::singleline(value).desired_width(w));
                            ui.end_row();
                            r.changed()
                        };
                        field(ui, "Name", &mut m.name);
                        field(ui, "Version", &mut m.version);
                        field(ui, "Description", &mut m.description);
                        if field(ui, "Authors", &mut p.authors_text) {
                            m.authors = p
                                .authors_text
                                .split(',')
                                .map(|a| a.trim().to_owned())
                                .filter(|a| !a.is_empty())
                                .collect();
                        }
                        field(ui, "License", &mut m.license);
                    });
                hint(
                    ui,
                    "The license covers the community work: recipes and patches.",
                );
            });
        });
    }

    fn project_side(&mut self, ui: &mut Ui, issues: &[ptk_core::manifest::Issue]) {
        let Some(p) = &self.project else { return };
        let hashes = p.saved.source_files.len();
        let changed = CATEGORIES
            .iter()
            .filter(|c| p.saved.options_for(c.id) != CategoryOptions::default())
            .count();
        let packed = matches!(self.pack_result, Some(Ok(_)));
        let ignored =
            ptk_core::category::options_ignored(p.manifest.game.engine.as_deref().unwrap_or(""))
                .is_some();
        let dir = p.dir.clone();
        let contents = p.contents;
        let mut go = None;
        card(ui, |ui| {
            card_title(ui, "Workflow");
            let steps = [
                (
                    hashes > 0,
                    "Scan your game",
                    if hashes > 0 {
                        format!("{hashes} file hashes recorded")
                    } else {
                        "Detect the engine, record hashes".into()
                    },
                    View::Scan,
                ),
                (
                    changed > 0 && !ignored,
                    "Choose what is lost",
                    if ignored {
                        "Not available for Unity games: everything is kept".into()
                    } else if changed > 0 {
                        format!("{changed} categories changed")
                    } else {
                        "Everything is kept for now".into()
                    },
                    View::Overview,
                ),
                (
                    packed,
                    "Share",
                    if packed {
                        "Packed this session".into()
                    } else {
                        "Pack a .ptk for others".into()
                    },
                    View::Export,
                ),
            ];
            for (i, (done, title, detail, view)) in steps.into_iter().enumerate() {
                ui.horizontal(|ui| {
                    if done {
                        badge(ui, "✔", GREEN);
                    } else {
                        badge(ui, &(i + 1).to_string(), FOCUS);
                    }
                    ui.vertical(|ui| {
                        if ui
                            .link(RichText::new(title).strong().color(Color32::WHITE))
                            .clicked()
                        {
                            go = Some(view);
                        }
                        ui.label(RichText::new(detail).size(12.0).color(TEXT_WEAK));
                    });
                });
                ui.add_space(4.0);
            }
        });
        ui.add_space(14.0);
        card(ui, |ui| {
            let errors = issues.iter().filter(|i| i.level == Level::Error).count();
            ui.horizontal(|ui| {
                card_title(ui, "Problems");
                badge(
                    ui,
                    &issues.len().to_string(),
                    if errors > 0 {
                        ERROR
                    } else if issues.is_empty() {
                        GREEN
                    } else {
                        WARN
                    },
                );
            });
            if issues.is_empty() {
                hint(ui, "No problems in ptk.json.");
            }
            for i in issues {
                ui.horizontal_wrapped(|ui| {
                    let (icon, color) = match i.level {
                        Level::Error => ("✖", ERROR),
                        Level::Warning => ("▲", WARN),
                    };
                    ui.label(RichText::new(icon).color(color));
                    ui.label(&i.message);
                });
            }
        });
        ui.add_space(14.0);
        card(ui, |ui| {
            card_title(ui, "Folder");
            egui::Grid::new("folder")
                .num_columns(2)
                .spacing([18.0, 4.0])
                .show(ui, |ui| {
                    kv(ui, "recipes/", contents.recipes.to_string());
                    kv(ui, "patches/", contents.patches.to_string());
                    kv(
                        ui,
                        "plugins.lock",
                        format!("{} plugins", contents.locked_plugins),
                    );
                });
            if ui.button("Show in Finder").clicked() {
                let _ = std::process::Command::new(if cfg!(target_os = "macos") {
                    "open"
                } else {
                    "xdg-open"
                })
                .arg(&dir)
                .spawn();
            }
        });
        if let Some(v) = go {
            self.go(v, ui.ctx());
        }
    }

    // ================================================================ scan

    fn scan_page(&mut self, ui: &mut Ui) {
        page_header(
            ui,
            "Scan a game",
            "Detects the engine from file names and executable headers only. Game archives are never opened.",
        );
        card(ui, |ui| {
            ui.label(RichText::new("Game folder").color(TEXT_WEAK));
            widgets::path_field(
                ui,
                &mut self.game_input,
                "the folder of your game",
                Pick::Folder,
            );
            ui.horizontal_wrapped(|ui| {
                let busy = self.busy.is_some();
                if primary(ui, !busy && !self.game_input.trim().is_empty(), "🔍  Scan").clicked()
                {
                    self.start_scan(ui.ctx());
                }
                let recent = self.state.recent_games.clone();
                if !recent.is_empty() {
                    ui.label(RichText::new("Recent:").color(TEXT_FAINT));
                    for g in recent.iter().take(4) {
                        let name = g
                            .file_name()
                            .map(|n| n.to_string_lossy().into_owned())
                            .unwrap_or_default();
                        if ui
                            .small_button(name)
                            .on_hover_text(g.display().to_string())
                            .clicked()
                        {
                            self.game_input = g.display().to_string();
                        }
                    }
                }
            });
        });
        ui.add_space(14.0);

        if let Some(n) = self.scan_progress {
            card(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.spinner();
                    ui.label(format!("Scanning… {n} files"));
                });
            });
            return;
        }
        let Some(scan) = self.scan.clone() else {
            empty_state(
                ui,
                "🔍",
                "No scan yet",
                "Choose your game folder above, or drop it on this window.",
            );
            return;
        };

        let can_record = self.project.is_some() && self.busy.is_none();
        card(ui, |ui| {
            match &scan.primary {
                Some(p) => {
                    ui.horizontal_wrapped(|ui| {
                        ui.label(RichText::new(&p.name).size(26.0).strong().color(TEAL));
                        if let Some(v) = &p.variant {
                            ui.label(RichText::new(v).size(17.0).color(LIGHT_BLUE));
                        }
                        let c = match p.label.as_str() {
                            "high" => GREEN,
                            "medium" => WARN,
                            _ => TEXT_WEAK,
                        };
                        badge(
                            ui,
                            &format!("{} confidence · {:.2}", p.label, p.confidence),
                            c,
                        );
                        if p.kind != "engine" {
                            badge(ui, "format", PURPLE);
                        }
                    });
                    ui.add_space(6.0);
                    egui::Grid::new("scan-summary")
                        .num_columns(2)
                        .spacing([18.0, 5.0])
                        .show(ui, |ui| {
                            let mut e = p.evidence[..p.evidence.len().min(3)].join(",  ");
                            if p.evidence.len() > 3 {
                                e.push_str(&format!("  +{} more", p.evidence.len() - 3));
                            }
                            kv(
                                ui,
                                "Evidence",
                                RichText::new(e).monospace().size(12.0).color(ORANGE),
                            );
                            let also: Vec<_> = scan
                                .engines
                                .iter()
                                .skip(1)
                                .map(|e| e.name.clone())
                                .collect();
                            if !also.is_empty() {
                                kv(ui, "Also", also.join(", "));
                            }
                            kv(
                                ui,
                                "Platforms",
                                if scan.platforms.is_empty() {
                                    "unknown".into()
                                } else {
                                    scan.platforms.join(", ")
                                },
                            );
                            kv(
                                ui,
                                "Contents",
                                format!(
                                    "{} files in {} folders, {}",
                                    scan.file_count,
                                    scan.dir_count,
                                    human_size(scan.total_size)
                                ),
                            );
                        });
                }
                None => {
                    ui.label(
                        RichText::new("Engine not recognised")
                            .size(22.0)
                            .color(WARN),
                    );
                    hint(
                        ui,
                        &format!("{} files, {}", scan.file_count, human_size(scan.total_size)),
                    );
                }
            }
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                if primary(ui, can_record, "Record into project")
                    .on_hover_text(
                        "Saves the engine and the SHA-256 hashes of every file (never the files)",
                    )
                    .on_disabled_hover_text("Open a project first")
                    .clicked()
                {
                    self.start_record(ui.ctx());
                }
                if ui.button("Copy JSON").clicked()
                    && let Ok(json) = serde_json::to_string_pretty(&scan)
                {
                    ui.ctx().copy_text(json);
                    self.notify(
                        "Copied the scan as JSON".into(),
                        crate::app::ToastKind::Info,
                    );
                }
            });
        });
        ui.add_space(14.0);

        split(ui, 0.58, 900.0, |ui, col| {
            if col == 0 {
                card(ui, |ui| {
                    card_title(
                        ui,
                        if scan.total_size > 0 {
                            "Assets by size"
                        } else {
                            "Assets by file count"
                        },
                    );
                    let cats = scan.categories_by_size();
                    let parts: Vec<_> = cats
                        .iter()
                        .map(|c| (scan.share(c), category_color(&c.id)))
                        .collect();
                    widgets::stacked_bar(ui, &parts);
                    ui.add_space(10.0);
                    egui::Grid::new("scan-cats")
                        .num_columns(4)
                        .spacing([16.0, 6.0])
                        .show(ui, |ui| {
                            for c in cats {
                                ui.horizontal(|ui| {
                                    ui.label(RichText::new("■").color(category_color(&c.id)));
                                    ui.label(&c.id);
                                });
                                ui.label(
                                    RichText::new(format!("{} files", c.files)).color(TEXT_WEAK),
                                );
                                ui.label(human_size(c.size));
                                ui.label(
                                    RichText::new(format!("{:.0}%", scan.share(c) * 100.0))
                                        .color(TEXT_WEAK),
                                );
                                ui.end_row();
                            }
                        });
                });
            } else if !scan.largest.is_empty() {
                card(ui, |ui| {
                    card_title(ui, "Largest files");
                    for f in &scan.largest {
                        ui.horizontal(|ui| {
                            ui.label(RichText::new("■").color(category_color(&f.category)));
                            ui.label(RichText::new(human_size(f.size)).strong());
                        });
                        ui.label(
                            RichText::new(&f.path)
                                .monospace()
                                .size(11.5)
                                .color(TEXT_WEAK),
                        );
                        ui.add_space(4.0);
                    }
                });
            }
        });
    }

    // ============================================================= options

    /// The open game's advanced options, on its Game page.
    fn options_section(&mut self, ui: &mut Ui) {
        let Some(p) = self.project.as_mut() else {
            return;
        };
        // Missing categories mean "keep"; filling them in is not an edit.
        for c in CATEGORIES {
            p.manifest.options.entry(c.id.to_owned()).or_default();
            p.saved.options.entry(c.id.to_owned()).or_default();
        }
        card(ui, |ui| {
            ui.horizontal(|ui| {
                card_title(ui, "Advanced options");
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if ui
                        .small_button("Use my defaults")
                        .on_hover_text("Settings → Defaults")
                        .clicked()
                    {
                        for c in CATEGORIES {
                            let o = self
                                .app_settings
                                .default_options
                                .get(c.id)
                                .cloned()
                                .unwrap_or_default();
                            p.manifest.options.insert(c.id.to_owned(), o);
                        }
                    }
                });
            });
            hint(
                ui,
                "Before porting, decide per asset category what is kept, dropped or recompressed, so you control what is lost.",
            );
            ui.add_space(6.0);
            let ignored = ptk_core::category::options_ignored(
                p.manifest.game.engine.as_deref().unwrap_or(""),
            );
            if let Some(why) = ignored {
                ui.label(RichText::new(format!("⚠ {why}")).color(WARN));
                let set = p
                    .manifest
                    .options
                    .values()
                    .any(|o| o.action != AssetAction::Keep);
                if set && ui.button("Set every category to Keep").clicked() {
                    for o in p.manifest.options.values_mut() {
                        *o = CategoryOptions::default();
                    }
                }
                ui.add_space(6.0);
            }
            let auto = AutoInfo {
                engine: p.manifest.game.engine.clone(),
                targets: p
                    .manifest
                    .targets
                    .iter()
                    .filter_map(|t| self.registry.get(t).cloned())
                    .collect(),
            };
            ui.add_enabled_ui(ignored.is_none(), |ui| {
                options_editor(ui, &mut p.manifest.options, "project", &auto);
            });
        });
    }

    // =============================================================== share

    fn pack_card(&mut self, ui: &mut Ui) {
        let mut pack = false;
        card(ui, |ui| {
            card_title(ui, "📦  Pack");
            if self.project.is_none() {
                hint(ui, "Open a project to pack it.");
                return;
            }
            hint(
                ui,
                "ptk.json, README, LICENSE, plugins.lock, recipes/, patches/ and extra/. Third-party files are refused.",
            );
            widgets::path_field(
                ui,
                &mut self.pack_output,
                "output .ptk file",
                Pick::SavePack,
            );
            ui.checkbox(&mut self.pack_overwrite, "Replace an existing file");
            pack = primary(
                ui,
                self.menu_state().enabled(Command::PackProject),
                "Pack  ⇧⌘E",
            )
            .clicked();
            match &self.pack_result {
                Some(Ok(s)) => {
                    widgets::message(
                        ui,
                        GREEN,
                        &format!(
                            "✔ {} files, {}. No game files inside.",
                            s.files.len(),
                            human_size(s.bytes)
                        ),
                    );
                    ui.label(
                        RichText::new(s.output.display().to_string())
                            .size(11.5)
                            .color(TEXT_WEAK),
                    );
                    for f in &s.licensed_third_party {
                        ui.label(
                            RichText::new(format!("▲ included under the game's license: {f}"))
                                .color(WARN),
                        );
                    }
                }
                Some(Err(e)) => widgets::message(ui, ERROR, e),
                None => {}
            }
        });
        if pack {
            self.run_command(Command::PackProject, ui.ctx());
        }
    }

    fn unpack_card(&mut self, ui: &mut Ui) {
        let busy = self.busy.is_some();
        let mut open = None;
        let mut start = false;
        card(ui, |ui| {
            card_title(ui, "📥  Unpack");
            hint(
                ui,
                "Every file is checked against SHA256SUMS; unexpected paths and links are refused.",
            );
            widgets::path_field(ui, &mut self.unpack_input, ".ptk file", Pick::PackFile);
            widgets::path_field(
                ui,
                &mut self.unpack_dest,
                "new or empty folder",
                Pick::Folder,
            );
            start = primary(ui, !busy, "Unpack").clicked();
            match &self.unpack_result {
                Some(Ok(r)) => {
                    widgets::message(
                        ui,
                        GREEN,
                        &format!(
                            "✔ {} {}: {} files, all checksums match",
                            r.manifest.name,
                            r.manifest.version,
                            r.entries.len()
                        ),
                    );
                    if ui.button("Open unpacked project").clicked() {
                        open = Some(r.dest.clone());
                    }
                }
                Some(Err(e)) => widgets::message(ui, ERROR, e),
                None => {}
            }
        });
        if start {
            self.start_unpack(ui.ctx());
        }
        if let Some(d) = open {
            self.open_project(&d);
            self.view = View::Overview;
        }
    }

    fn verify_card(&mut self, ui: &mut Ui) {
        let hashes = self
            .project
            .as_ref()
            .map(|p| p.saved.source_files.len())
            .unwrap_or(0);
        let busy = self.busy.is_some();
        let mut start = false;
        card(ui, |ui| {
            card_title(ui, "✔  Verify your game");
            if hashes == 0 {
                hint(
                    ui,
                    "Open a project with recorded game hashes to check your own copy against them.",
                );
                return;
            }
            hint(
                ui,
                &format!("Compares your game folder with the {hashes} recorded hashes."),
            );
            widgets::path_field(ui, &mut self.verify_game, "your game folder", Pick::Folder);
            start = primary(ui, !busy, "Verify").clicked();
            match &self.verify_result {
                Some(Ok(r)) if r.is_ok() => widgets::message(
                    ui,
                    GREEN,
                    &format!(
                        "✔ All {} files match the version this project was made for.",
                        r.ok
                    ),
                ),
                Some(Ok(r)) => {
                    widgets::message(
                        ui,
                        ERROR,
                        &format!(
                            "✖ {} of {} match · {} different · {} missing",
                            r.ok,
                            r.checked,
                            r.mismatched.len(),
                            r.missing.len()
                        ),
                    );
                    for f in r.missing.iter().take(12) {
                        ui.label(
                            RichText::new(format!("missing    {f}"))
                                .monospace()
                                .size(11.5)
                                .color(ERROR),
                        );
                    }
                    for f in r.mismatched.iter().take(12) {
                        ui.label(
                            RichText::new(format!("different  {}", f.path))
                                .monospace()
                                .size(11.5)
                                .color(WARN),
                        );
                    }
                }
                Some(Err(e)) => widgets::message(ui, ERROR, e),
                None => {}
            }
        });
        if start {
            self.start_verify(ui.ctx());
        }
    }

    // ============================================================= plugins

    fn installed_plugins(&mut self, ui: &mut Ui) {
        let mut pick = None;
        let mut refresh = false;
        let mut use_copy: Option<(String, Option<PathBuf>)> = None;
        let mut remove: Option<String> = None;
        let mut arm: Option<String> = None;
        let mut update: Option<ptk_core::market::MarketEntry> = None;
        let idle = self.busy.is_none();
        let pins = self.app_settings.plugin_versions.clone();
        card(ui, |ui| {
            ui.horizontal(|ui| {
                card_title(ui, "Installed");
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    refresh = ui.small_button("⟳ Refresh").clicked();
                });
            });
            match &self.plugins {
                None => hint(ui, "Loading…"),
                Some(Err(e)) => widgets::message(ui, ERROR, e),
                Some(Ok(list)) if list.is_empty() => hint(ui, "No plugins installed yet."),
                Some(Ok(list)) => {
                    // One row per plugin: the copy that runs, with its other
                    // versions folded under it.
                    for p in newest_copies(list, &pins) {
                        let copies: Vec<&ptk_core::plugins::ListedPlugin> =
                            list.iter().filter(|q| q.id == p.id).collect();
                        let is_pinned = pins.contains_key(&p.id);
                        ui.horizontal(|ui| {
                            ui.label(RichText::new("🔌").size(18.0).color(TEAL));
                            ui.vertical(|ui| {
                                if ui
                                    .link(RichText::new(&p.id).strong().color(Color32::WHITE))
                                    .clicked()
                                {
                                    pick = Some(p.path.clone());
                                }
                                ui.horizontal(|ui| {
                                    ui.label(
                                        RichText::new(format!("{} · {}", p.version, p.kind))
                                            .size(11.5)
                                            .color(TEXT_WEAK),
                                    );
                                    badge(ui, &format!("{} risk", p.risk), risk_color(&p.risk));
                                    if p.builtin {
                                        badge(ui, "built-in", TEAL);
                                    } else if !p.reviewed {
                                        badge(ui, "not reviewed", TEXT_FAINT);
                                    }
                                    if is_pinned {
                                        badge(ui, "chosen version", LIGHT_BLUE).on_hover_text(
                                            "You chose this version; newer ones are not used",
                                        );
                                    }
                                });
                                let row_update = update_for(&self.market, &p.id, &p.version);
                                if row_update.is_some() || !p.builtin {
                                    ui.horizontal(|ui| {
                                        if let Some(e) = row_update {
                                            if ui
                                                .add_enabled(
                                                    idle,
                                                    egui::Button::new(format!(
                                                        "⬆ Update to {}",
                                                        e.version
                                                    )),
                                                )
                                                .clicked()
                                            {
                                                update = Some(e);
                                            }
                                        }
                                        if !p.builtin {
                                            let armed = self.confirm_remove.as_deref()
                                                == Some(p.path.as_str());
                                            let label = if armed {
                                                "Click again to uninstall"
                                            } else {
                                                "Uninstall"
                                            };
                                            if ui
                                                .add_enabled(
                                                    idle,
                                                    egui::Button::new(
                                                        RichText::new(label).color(if armed {
                                                            ERROR
                                                        } else {
                                                            TEXT
                                                        }),
                                                    ),
                                                )
                                                .on_hover_text("Moves this plugin to the Trash")
                                                .clicked()
                                            {
                                                if armed {
                                                    remove = Some(p.path.clone());
                                                } else {
                                                    arm = Some(p.path.clone());
                                                }
                                            }
                                        }
                                    });
                                }
                                if copies.len() > 1 {
                                    egui::CollapsingHeader::new(
                                        RichText::new(format!("{} versions", copies.len()))
                                            .size(12.0)
                                            .color(TEXT_WEAK),
                                    )
                                    .id_salt(("versions", &p.id))
                                    .show(ui, |ui| {
                                        let mut sorted = copies.clone();
                                        sorted.sort_by(|a, b| {
                                            if ptk_core::market::version_newer(
                                                &a.version, &b.version,
                                            ) {
                                                std::cmp::Ordering::Less
                                            } else if a.version == b.version {
                                                std::cmp::Ordering::Equal
                                            } else {
                                                std::cmp::Ordering::Greater
                                            }
                                        });
                                        for c in sorted {
                                            ui.horizontal(|ui| {
                                                ui.label(
                                                    RichText::new(&c.version).color(Color32::WHITE),
                                                )
                                                .on_hover_text(&c.path);
                                                ui.label(
                                                    RichText::new(if c.builtin {
                                                        "built-in"
                                                    } else {
                                                        "installed"
                                                    })
                                                    .size(11.5)
                                                    .color(TEXT_WEAK),
                                                );
                                                if c.path == p.path {
                                                    badge(ui, "in use", GREEN);
                                                } else if ui
                                                    .small_button("Use this version")
                                                    .clicked()
                                                {
                                                    use_copy = Some((
                                                        p.id.clone(),
                                                        Some(PathBuf::from(&c.path)),
                                                    ));
                                                }
                                                if !c.builtin
                                                    && c.path != p.path
                                                    && ui
                                                        .small_button("Remove")
                                                        .on_hover_text(
                                                            "Moves this copy to the Trash",
                                                        )
                                                        .clicked()
                                                {
                                                    remove = Some(c.path.clone());
                                                }
                                            });
                                        }
                                        if is_pinned
                                            && ui.small_button("Always use the newest").clicked()
                                        {
                                            use_copy = Some((p.id.clone(), None));
                                        }
                                    });
                                }
                            });
                        });
                        ui.add_space(6.0);
                    }
                    hint(
                        ui,
                        "To add another version of a plugin, install it from the Marketplace or from a folder (Check a plugin).",
                    );
                    if matches!(self.market, Some(Err(_))) {
                        hint(
                            ui,
                            "Couldn't check for updates: the marketplace did not load.",
                        );
                    }
                }
            }
        });
        if let Some((id, dir)) = use_copy {
            match dir {
                Some(d) => {
                    self.app_settings.plugin_versions.insert(id, d);
                }
                None => {
                    self.app_settings.plugin_versions.remove(&id);
                }
            }
        }
        if arm.is_some() {
            self.confirm_remove = arm;
        }
        if let Some(e) = update {
            self.market_install(ui.ctx(), e);
        }
        if let Some(path) = remove {
            self.confirm_remove = None;
            if self
                .plugin_validation
                .as_ref()
                .is_some_and(|v| v.as_ref().is_ok_and(|v| v.path == path))
            {
                self.plugin_validation = None;
            }
            match ptk_core::plugins::uninstall(Path::new(&path)) {
                Ok(()) => {
                    self.notify(format!("Moved {path} to the Trash"), ToastKind::Ok);
                    refresh = true;
                }
                Err(e) => self.notify(e.to_string(), ToastKind::Error),
            }
        }
        if refresh {
            self.refresh_plugins(ui.ctx());
        }
        if let Some(path) = pick {
            self.plugin_dir = path;
            self.validate_plugin(ui.ctx());
        }
    }

    fn plugin_tools(&mut self, ui: &mut Ui) {
        enum CheckAction {
            Install,
            Update(Box<ptk_core::market::MarketEntry>),
            Arm,
            Uninstall,
        }
        let busy = self.busy.is_some();
        let mut validate = false;
        card(ui, |ui| {
            card_title(ui, "Check a plugin");
            hint(
                ui,
                "Reads plugin.toml and shows the permissions the plugin declares. No plugin code runs.",
            );
            widgets::path_field(ui, &mut self.plugin_dir, "plugin folder", Pick::Folder);
            validate = primary(ui, !busy, "Validate").clicked();
        });
        if validate {
            self.validate_plugin(ui.ctx());
        }
        ui.add_space(14.0);
        if let Some(result) = self.plugin_validation.clone() {
            match result {
                Err(e) => widgets::message(ui, ERROR, &e),
                Ok(v) => {
                    let mut action: Option<CheckAction> = None;
                    let version = v
                        .plugin
                        .as_ref()
                        .and_then(|p| p.version.clone())
                        .unwrap_or_default();
                    let pid = v.plugin.as_ref().and_then(|p| p.id.clone());
                    let list = match &self.plugins {
                        Some(Ok(l)) => l.clone(),
                        _ => Vec::new(),
                    };
                    let state = plugin_state(
                        &v.path,
                        pid.as_deref(),
                        &version,
                        &list,
                        &self.app_settings.plugin_versions,
                        &ptk_core::paths::plugins_dir(),
                    );
                    let update = match (&state, pid.as_deref()) {
                        (PluginState::BuiltIn | PluginState::Installed, Some(id)) => {
                            update_for(&self.market, id, &version)
                        }
                        _ => None,
                    };
                    card(ui, |ui| {
                        let info = v.plugin.clone();
                        let id = info
                            .as_ref()
                            .and_then(|p| p.id.clone())
                            .unwrap_or_else(|| v.path.clone());
                        ui.label(
                            RichText::new(format!("🔌  {id}"))
                                .size(18.0)
                                .strong()
                                .color(Color32::WHITE),
                        );
                        if let Some(p) = &info {
                            ui.label(
                                RichText::new(format!(
                                    "{} · {} plugin · {}",
                                    p.version.clone().unwrap_or_default(),
                                    p.kind.clone().unwrap_or_default(),
                                    p.license.clone().unwrap_or_default()
                                ))
                                .color(TEXT_WEAK),
                            );
                            if let Some(d) = p.description.clone().filter(|d| !d.is_empty()) {
                                ui.label(d);
                            }
                        }
                        ui.horizontal_wrapped(|ui| {
                            badge(
                                ui,
                                if v.valid { "valid" } else { "invalid" },
                                if v.valid { GREEN } else { ERROR },
                            );
                            badge(ui, &format!("{} risk", v.risk), risk_color(&v.risk));
                            match &state {
                                PluginState::BuiltIn => badge(ui, "built-in · reviewed", TEAL),
                                PluginState::Installed => {
                                    badge(ui, "installed · not reviewed", TEXT_FAINT)
                                }
                                _ => badge(ui, "not reviewed", TEXT_FAINT),
                            };
                        });
                        if !v.permissions.is_empty() {
                            ui.add_space(6.0);
                            ui.label(RichText::new("Declared permissions").color(TEXT_WEAK));
                            for p in &v.permissions {
                                ui.label(format!("•  {p}"));
                            }
                        }
                        for e in &v.errors {
                            ui.label(RichText::new(format!("✖ {e}")).color(ERROR));
                        }
                        for w in &v.warnings {
                            ui.label(RichText::new(format!("▲ {w}")).color(WARN));
                        }
                        ui.add_space(6.0);
                        ui.horizontal_wrapped(|ui| {
                            if let Some(e) = &update {
                                if primary(ui, !busy, &format!("⬆ Update to {}", e.version))
                                    .clicked()
                                {
                                    action = Some(CheckAction::Update(Box::new(e.clone())));
                                }
                            }
                            match &state {
                                PluginState::BuiltIn => {
                                    hint(ui, "Ships with Porting Toolkit; it is always installed.");
                                }
                                PluginState::Installed => {
                                    let armed =
                                        self.confirm_remove.as_deref() == Some(v.path.as_str());
                                    let label = if armed {
                                        "Click again to uninstall"
                                    } else {
                                        "Uninstall"
                                    };
                                    if ui
                                        .add_enabled(
                                            !busy,
                                            egui::Button::new(
                                                RichText::new(label).color(if armed {
                                                    ERROR
                                                } else {
                                                    TEXT
                                                }),
                                            ),
                                        )
                                        .on_hover_text("Moves this plugin to the Trash")
                                        .clicked()
                                    {
                                        action = Some(if armed {
                                            CheckAction::Uninstall
                                        } else {
                                            CheckAction::Arm
                                        });
                                    }
                                }
                                PluginState::Replaces { running, newer } => {
                                    let older = !*newer && *running != version;
                                    let armed =
                                        self.confirm_remove.as_deref() == Some(v.path.as_str());
                                    let label = if *newer {
                                        format!("Update {running} → {version}")
                                    } else if *running == version {
                                        format!("Reinstall {version}")
                                    } else if armed {
                                        format!("Click again to install the older {version}")
                                    } else {
                                        format!("Install older {version} (now {running})")
                                    };
                                    if primary(ui, v.valid && !busy, &label).clicked() {
                                        action = Some(if older && !armed {
                                            CheckAction::Arm
                                        } else {
                                            CheckAction::Install
                                        });
                                    }
                                }
                                PluginState::New => {
                                    if primary(ui, v.valid && !busy, "Install…").clicked() {
                                        action = Some(CheckAction::Install);
                                    }
                                }
                            }
                        });
                    });
                    match action {
                        Some(CheckAction::Install) => {
                            self.confirm_remove = None;
                            self.install_plugin();
                        }
                        Some(CheckAction::Update(e)) => self.market_install(ui.ctx(), *e),
                        Some(CheckAction::Arm) => self.confirm_remove = Some(v.path.clone()),
                        Some(CheckAction::Uninstall) => {
                            self.confirm_remove = None;
                            match ptk_core::plugins::uninstall(Path::new(&v.path)) {
                                Ok(()) => {
                                    self.notify(
                                        format!("Moved {} to the Trash", v.path),
                                        ToastKind::Ok,
                                    );
                                    self.plugin_validation = None;
                                    self.refresh_plugins(ui.ctx());
                                }
                                Err(e) => self.notify(e.to_string(), ToastKind::Error),
                            }
                        }
                        None => {}
                    }
                    ui.add_space(14.0);
                }
            }
        }
    }

    // ============================================================= targets

    fn targets_page(&mut self, ui: &mut Ui) {
        let host = self.registry.host().map(|t| t.id.clone());
        let targets: Vec<_> = self.registry.iter().cloned().collect();
        let cols = columns_for(ui, 300.0, 3);
        card_grid(ui, targets.len(), cols, |ui, i| {
            let t = &targets[i];
            card(ui, |ui| {
                ui.horizontal_wrapped(|ui| {
                    ui.label(
                        RichText::new(&t.id)
                            .monospace()
                            .size(15.0)
                            .color(LIGHT_BLUE),
                    );
                    let c = match t.status {
                        TargetStatus::Active => GREEN,
                        TargetStatus::Experimental => WARN,
                        TargetStatus::Planned => TEXT_FAINT,
                    };
                    badge(ui, &t.status.to_string(), c);
                    if host.as_deref() == Some(t.id.as_str()) {
                        badge(ui, "this Mac", FOCUS);
                    }
                });
                ui.label(RichText::new(&t.name).strong().color(Color32::WHITE));
                ui.label(RichText::new(format!("{} / {}", t.os, t.arch)).color(TEXT_WEAK));
                if !t.description.is_empty() {
                    hint(ui, &t.description);
                }
                if let TargetSource::File(p) = &t.source {
                    ui.label(
                        RichText::new(p.display().to_string())
                            .size(11.0)
                            .color(TEXT_FAINT),
                    );
                }
            });
        });
        let mut reveal = false;
        card(ui, |ui| {
            card_title(ui, "Add targets");
            hint(
                ui,
                "An SDK (for example the NoVA SDK) adds platforms by dropping a [[target]] TOML file into:",
            );
            ui.label(
                RichText::new(ptk_core::paths::targets_dir().display().to_string())
                    .monospace()
                    .size(12.0),
            );
            reveal = ui.button("Show Data Folder in Finder").clicked();
        });
        if reveal {
            self.run_command(Command::RevealData, ui.ctx());
        }
    }

    // ============================================================== doctor

    fn doctor_page(&mut self, ui: &mut Ui) {
        ui.horizontal(|ui| {
            self.command_button(ui, Command::RunDoctor, "Run checks", true);
            self.command_button(ui, Command::SetUpEngine, "Set up / update engine", false);
        });
        ui.add_space(14.0);
        let Some(checks) = &self.doctor else {
            empty_state(
                ui,
                "✚",
                "Not checked yet",
                "Run the checks to see whether this Mac is ready.",
            );
            return;
        };
        let problems = checks.iter().filter(|c| c.status == Status::Fail).count();
        if problems == 0 {
            widgets::message(ui, GREEN, "✔ ptk is ready");
        } else {
            widgets::message(ui, ERROR, &format!("✖ {problems} problem(s) found"));
        }
        ui.add_space(12.0);
        let idle = self.busy.is_none();
        let mut install = None;
        card(ui, |ui| {
            egui::Grid::new("doctor")
                .num_columns(3)
                .spacing([14.0, 10.0])
                .show(ui, |ui| {
                    for c in checks {
                        widgets::status_icon(ui, c.status);
                        ui.label(RichText::new(&c.name).color(Color32::WHITE));
                        ui.vertical(|ui| {
                            ui.add(egui::Label::new(&c.detail).wrap());
                            if let Some(h) = &c.hint {
                                ui.horizontal_wrapped(|ui| {
                                    ui.label(
                                        RichText::new(format!("→ {h}")).size(12.0).color(TEXT_WEAK),
                                    );
                                    if let Some(id) = h.strip_prefix("ptk tools install ")
                                        && ui
                                            .add_enabled(idle, egui::Button::new("Install").small())
                                            .on_hover_text(
                                                "Downloads a fixed release and checks its SHA-256",
                                            )
                                            .clicked()
                                    {
                                        install = Some(id.to_owned());
                                    }
                                });
                            }
                        });
                        ui.end_row();
                    }
                });
        });
        if let Some(id) = install {
            self.install_tool(ui.ctx(), id);
        }
    }

    // =============================================================== about

    /// Where the Marketplace is read from (several indexes are merged).
    fn market_sources_page(&mut self, ui: &mut Ui) {
        let mut remove = None;
        let mut reload = false;
        card(ui, |ui| {
            card_title(ui, "Marketplace sources");
            hint(
                ui,
                "The Marketplace merges every index listed here (the newest version of a plugin wins). Add a mirror on another host, for example a Codeberg repository's raw index.json, when the default one cannot be reached.",
            );
            ui.add_space(6.0);
            let env = std::env::var("PTK_MARKETPLACE_URL")
                .ok()
                .filter(|v| !v.is_empty());
            if let Some(v) = &env {
                widgets::message(
                    ui,
                    WARN,
                    &format!("PTK_MARKETPLACE_URL is set, so only {v} is read."),
                );
            }
            let sources = self.app_settings.marketplace_sources.clone();
            if sources.is_empty() {
                ui.horizontal_wrapped(|ui| {
                    ui.label(
                        RichText::new(ptk_core::market::DEFAULT_INDEX_URL)
                            .monospace()
                            .color(TEXT),
                    );
                    badge(ui, "default", TEXT_FAINT);
                });
            }
            for (i, s) in sources.iter().enumerate() {
                ui.horizontal_wrapped(|ui| {
                    ui.label(RichText::new(s).monospace().color(TEXT));
                    if ui.small_button("Remove").clicked() {
                        remove = Some(i);
                    }
                });
            }
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                ui.add(
                    egui::TextEdit::singleline(&mut self.market_source_input)
                        .hint_text(
                            "https://codeberg.org/you/ptk-marketplace/raw/branch/main/index.json",
                        )
                        .desired_width(ui.available_width() - 160.0),
                );
                let url = self.market_source_input.trim().to_string();
                let ok = ptk_core::market::valid_source(&url);
                if ui
                    .add_enabled(ok, egui::Button::new("Add source"))
                    .on_disabled_hover_text("An https:// (or file://) URL of an index.json")
                    .clicked()
                {
                    if self.app_settings.marketplace_sources.is_empty() {
                        // Keep reading the default index next to the new one.
                        self.app_settings
                            .marketplace_sources
                            .push(ptk_core::market::DEFAULT_INDEX_URL.into());
                    }
                    if !self.app_settings.marketplace_sources.contains(&url) {
                        self.app_settings.marketplace_sources.push(url);
                    }
                    self.market_source_input.clear();
                    reload = true;
                }
            });
            if !sources.is_empty() && ui.button("Use only the default index").clicked() {
                self.app_settings.marketplace_sources.clear();
                reload = true;
            }
        });
        if let Some(i) = remove {
            self.app_settings.marketplace_sources.remove(i);
            reload = true;
        }
        if reload {
            self.refresh_market(ui.ctx());
        }
    }

    fn about_page(&mut self, ui: &mut Ui) {
        card(ui, |ui| {
            let cell = ((ui.available_width() - 8.0) / 72.0).clamp(2.0, 5.0);
            widgets::wordmark(ui, cell);
            ui.add_space(10.0);
            egui::Grid::new("about")
                .num_columns(2)
                .spacing([18.0, 6.0])
                .show(ui, |ui| {
                    kv(ui, "Version", ptk_core::VERSION);
                    kv(ui, "License", "MIT");
                    kv(ui, "Source", ptk_core::REPOSITORY);
                    kv(
                        ui,
                        "Data",
                        ptk_core::paths::data_dir().display().to_string(),
                    );
                });
        });
        ui.add_space(14.0);
        card(ui, |ui| {
            card_title(ui, "Roadmap");
            for (n, text, now) in [
                (
                    "1",
                    "macOS + CLI infrastructure, banner, project format",
                    false,
                ),
                ("2", "Desktop app", false),
                (
                    "3",
                    "Import/export plugins (Godot, HTML5 → macOS, web), then AI-assisted options",
                    true,
                ),
                (
                    "4",
                    "Windows/Linux and the full community marketplace",
                    false,
                ),
            ] {
                ui.horizontal(|ui| {
                    badge(
                        ui,
                        &format!("Phase {n}"),
                        if now { GREEN } else { TEXT_FAINT },
                    );
                    ui.label(text);
                });
            }
        });
        ui.add_space(14.0);
        widgets::message(
            ui,
            FOCUS,
            "A tool, not a distribution channel: you bring your own game. .ptk files hold recipes, settings, community patches and SHA-256 hashes, never original game files or decompiled code.",
        );
    }
}

/// A game port in the marketplace: poster, game, engine → targets, and
/// where to get the game. Returns the entry when "Get port" is pressed.
enum PortAction {
    Get,
    Check,
}

fn port_card(
    ui: &mut Ui,
    e: &ptk_core::market::MarketEntry,
    poster: Option<&egui::TextureHandle>,
    idle: bool,
    check: Option<&Result<ptk_core::market::CopyCheck, String>>,
    update: Option<&ptk_core::market::Update>,
) -> Option<PortAction> {
    use ptk_core::market::CopyVerdict;
    let mut action = None;
    card(ui, |ui| {
        let w = ui.available_width();
        let (rect, _) =
            ui.allocate_exact_size(vec2(w, (w * 0.62).min(220.0)), egui::Sense::hover());
        match poster {
            Some(t) => widgets::image_in(ui, rect, t, 8),
            None => {
                ui.painter()
                    .rect_filled(rect, egui::CornerRadius::same(8), INPUT);
                let initial = e
                    .game
                    .as_deref()
                    .unwrap_or(&e.name)
                    .chars()
                    .next()
                    .unwrap_or('?')
                    .to_uppercase()
                    .to_string();
                widgets::centered_text(ui, rect, &initial, 46.0, TEXT_FAINT);
            }
        }
        ui.add_space(6.0);
        ui.label(
            RichText::new(e.game.as_deref().unwrap_or(&e.name))
                .size(16.0)
                .strong()
                .color(Color32::WHITE),
        );
        let by = if e.author.is_empty() {
            "community"
        } else {
            &e.author
        };
        ui.label(
            RichText::new(format!("Port by {by} · v{}", e.version))
                .size(11.5)
                .color(TEXT_WEAK),
        );
        if !e.description.is_empty() {
            hint(ui, &e.description);
        }
        ui.horizontal_wrapped(|ui| {
            for en in &e.engines {
                badge(ui, en, TEAL);
            }
            if !e.targets.is_empty() {
                ui.label(RichText::new("→").color(TEXT_FAINT));
            }
            for t in e.targets.iter().take(3) {
                badge(ui, t, LIGHT_BLUE);
            }
            if e.reviewed {
                badge(ui, "✔ reviewed", GREEN);
            }
            if let Some(u) = update {
                badge(ui, &format!("⬆ v{} → v{}", u.installed, u.available), FOCUS);
            }
        });
        match check {
            Some(Ok(c)) => {
                let color = match c.verdict {
                    CopyVerdict::Matches => GREEN,
                    CopyVerdict::OtherVersion | CopyVerdict::Unknown => WARN,
                    _ => ERROR,
                };
                let detail = if c.total > 0 {
                    format!(" ({} of {} files identical)", c.matched, c.total)
                } else {
                    String::new()
                };
                ui.label(RichText::new(format!("{}{detail}", c.verdict.label())).color(color));
            }
            Some(Err(err)) => {
                ui.label(RichText::new(err).color(ERROR));
            }
            None => {}
        }
        ui.add_space(4.0);
        ui.horizontal_wrapped(|ui| {
            let label = if update.is_some() {
                "Get update"
            } else {
                "Get port"
            };
            if primary(ui, idle, label).clicked() {
                action = Some(PortAction::Get);
            }
            if ui
                .add_enabled(
                    idle && !e.fingerprint.is_empty(),
                    egui::Button::new("Check my copy"),
                )
                .on_hover_text(
                    "Compares a few files of your game with the ones this port was made for",
                )
                .on_disabled_hover_text("This port was published without a fingerprint")
                .clicked()
            {
                action = Some(PortAction::Check);
            }
            if let Some(store) = &e.store
                && ui.button("Get the game ↗").on_hover_text(store).clicked()
            {
                crate::app::open_url(store);
            }
        });
        ui.label(
            RichText::new("Needs your own copy of the game.")
                .size(11.5)
                .color(TEXT_FAINT),
        );
    });
    action
}

type Preset = (&'static str, fn(&str) -> CategoryOptions);

const PRESETS: [Preset; 3] = [
    ("Keep everything", |_| CategoryOptions::default()),
    ("Smaller download", |cat| match cat {
        "textures" => CategoryOptions {
            action: AssetAction::Recompress,
            quality: Some(80),
            max_resolution: Some(2048),
            ..Default::default()
        },
        "audio" => CategoryOptions {
            action: AssetAction::Recompress,
            quality: Some(70),
            ..Default::default()
        },
        "video" => CategoryOptions {
            action: AssetAction::Recompress,
            quality: Some(60),
            max_resolution: Some(1024),
            ..Default::default()
        },
        _ => CategoryOptions::default(),
    }),
    ("Smaller videos", |cat| {
        if cat == "video" {
            CategoryOptions {
                action: AssetAction::Recompress,
                quality: Some(50),
                max_resolution: Some(1280),
                ..Default::default()
            }
        } else {
            CategoryOptions::default()
        }
    }),
];

/// Engines whose games load loose files by name: their files keep their
/// format (see `engine/ptk_engine/media/profiles.py`).
const LOOSE_FILE_ENGINES: &[&str] = &["html5", "rpgmaker", "love", "rgss"];

/// What "Automatic" means for a project: its engine and targets.
#[derive(Default)]
pub struct AutoInfo {
    pub engine: Option<String>,
    pub targets: Vec<ptk_core::targets::Target>,
}

impl AutoInfo {
    /// One line per media category, e.g. "Opus on Web, AAC on macOS".
    fn line(&self, category: &str) -> Option<String> {
        if !matches!(category, "textures" | "audio" | "video") {
            return None;
        }
        let engine = self.engine.as_deref()?;
        if engine == "godot" {
            return Some("Godot games keep their imported resources for now".into());
        }
        if LOOSE_FILE_ENGINES.contains(&engine) {
            return Some(
                "keeps each file's format, only smaller (the game loads files by name)".into(),
            );
        }
        let parts: Vec<String> = self
            .targets
            .iter()
            .filter_map(|t| {
                let f = t.media.for_category(category).first()?;
                Some(format!(
                    "{} on {}",
                    ptk_core::category::format_name(category, f),
                    t.name
                ))
            })
            .collect();
        (!parts.is_empty()).then(|| parts.join(", "))
    }

    /// GPU texture formats none of the targets can use.
    fn unusable(&self, category: &str, format: &str) -> bool {
        category == "textures"
            && ptk_core::manifest::GPU_TEXTURES.contains(&format)
            && !self.targets.is_empty()
            && !self
                .targets
                .iter()
                .any(|t| t.media.textures.iter().any(|x| x == format))
    }
}

/// The asset pipeline's report: sizes before and after, per category.
pub fn assets_report(ui: &mut Ui, a: &ptk_core::convert::AssetReport, salt: &str) {
    use ptk_core::convert::human_size;
    let about = if a.estimated { "≈ " } else { "" };
    ui.add_space(4.0);
    ui.horizontal_wrapped(|ui| {
        ui.label(RichText::new("Assets").strong().color(Color32::WHITE));
        ui.label(
            RichText::new(format!(
                "{} → {about}{}",
                human_size(a.before),
                human_size(a.after)
            ))
            .color(TEAL),
        );
        if a.saved() > 0 {
            badge(
                ui,
                &format!("{about}{} saved", human_size(a.saved())),
                GREEN,
            );
        }
        if a.estimated {
            badge(ui, "estimate", LIGHT_BLUE);
        }
    });
    egui::Grid::new(("assets", salt))
        .num_columns(4)
        .spacing([14.0, 4.0])
        .show(ui, |ui| {
            for (cat, c) in &a.categories {
                ui.horizontal(|ui| {
                    ui.label(RichText::new("■").color(category_color(cat)));
                    ui.label(RichText::new(cat).color(TEXT_WEAK));
                });
                let files = if c.files == 1 { "file" } else { "files" };
                ui.label(RichText::new(format!("{} {files}", c.files)).color(TEXT_FAINT));
                ui.label(format!(
                    "{} → {about}{}",
                    human_size(c.before),
                    human_size(c.after)
                ));
                let codecs: Vec<String> =
                    c.codecs.iter().map(|(k, v)| format!("{k} ×{v}")).collect();
                let mut text = codecs.join(", ");
                if c.failed > 0 {
                    text.push_str(&format!("  ·  {} kept after a failed check", c.failed));
                }
                ui.label(RichText::new(text).size(12.0).color(TEXT_FAINT));
                ui.end_row();
            }
        });
    for (cat, c) in &a.categories {
        for n in &c.notes {
            ui.label(
                RichText::new(format!("{cat}: {n}"))
                    .size(12.0)
                    .color(TEXT_WEAK),
            );
        }
    }
}

/// Presets plus one compact row per asset category: keep / drop /
/// recompress, and for recompressing: quality, size and format (algorithm).
pub fn options_editor(
    ui: &mut Ui,
    options: &mut std::collections::BTreeMap<String, CategoryOptions>,
    salt: &str,
    auto: &AutoInfo,
) {
    ui.horizontal_wrapped(|ui| {
        ui.label(RichText::new("Presets").color(TEXT_WEAK));
        for (name, f) in PRESETS {
            if ui.button(name).clicked() {
                for c in CATEGORIES {
                    options.insert(c.id.to_owned(), f(c.id));
                }
            }
        }
    });
    ui.add_space(8.0);
    // One row per category: name, Keep/Drop/Recompress, then the settings,
    // which wrap inside the row's width instead of pushing the page wider.
    for c in CATEGORIES {
        let o = options.entry(c.id.to_owned()).or_default();
        ui.horizontal_top(|ui| {
                ui.allocate_ui_with_layout(
                    vec2(130.0, 26.0),
                    Layout::left_to_right(Align::Center),
                    |ui| {
                        ui.set_min_width(130.0);
                        ui.label(RichText::new("■").color(category_color(c.id)));
                        ui.label(RichText::new(c.name).color(Color32::WHITE))
                            .on_hover_text(c.description);
                    },
                );
                widgets::segmented(
                    ui,
                    &mut o.action,
                    &[
                        (AssetAction::Keep, "Keep", GREEN),
                        (AssetAction::Drop, "Drop", ERROR),
                        (AssetAction::Recompress, "Recompress", YELLOW),
                    ],
                );
                ui.horizontal_wrapped(|ui| {
                    if o.action != AssetAction::Recompress {
                        return;
                    }
                    ui.spacing_mut().slider_width = 110.0;
                    let formats = ptk_core::category::formats(c.id);
                    if !formats.is_empty() {
                        let shown = o
                            .format
                            .as_deref()
                            .and_then(|f| formats.iter().find(|(id, _)| *id == f))
                            .map(|(_, l)| l.to_string())
                            .or_else(|| o.format.clone())
                            .unwrap_or_else(|| "Automatic".into());
                        egui::ComboBox::from_id_salt(("fmt", salt, c.id))
                            .width(150.0)
                            .selected_text(shown)
                            .show_ui(ui, |ui| {
                                ui.selectable_value(&mut o.format, None, "Automatic");
                                for (id, label) in formats {
                                    let usable = !auto.unusable(c.id, id);
                                    ui.add_enabled_ui(usable, |ui| {
                                        ui.selectable_value(
                                            &mut o.format,
                                            Some(id.to_string()),
                                            *label,
                                        )
                                    })
                                    .response
                                    .on_disabled_hover_text(
                                        "None of this project's targets can use this GPU format",
                                    );
                                }
                            })
                            .response
                            .on_hover_text(match auto.line(c.id) {
                                Some(l) => format!("Format (algorithm) to recompress into.\nAutomatic: {l}"),
                                None => "Format (algorithm) to recompress into.\nAutomatic: the target's preferred format".into(),
                            });
                    }
                    if c.lossy {
                        let mut q = o
                            .quality
                            .unwrap_or_else(|| ptk_core::category::default_quality(c.id));
                        if ui
                            .add(egui::Slider::new(&mut q, 1..=100).text("quality"))
                            .changed()
                        {
                            o.quality = Some(q);
                        }
                    }
                    if matches!(c.id, "textures" | "video") {
                        let label = |r: Option<u32>| {
                            r.map(|r| format!("max {r} px"))
                                .unwrap_or_else(|| "original size".into())
                        };
                        egui::ComboBox::from_id_salt(("res", salt, c.id))
                            .width(110.0)
                            .selected_text(label(o.max_resolution))
                            .show_ui(ui, |ui| {
                                for r in RESOLUTIONS {
                                    ui.selectable_value(&mut o.max_resolution, *r, label(*r));
                                }
                            });
                    }
                });
        });
        ui.add_space(8.0);
    }
    let auto_lines: Vec<String> = CATEGORIES
        .iter()
        .filter(|c| {
            options
                .get(c.id)
                .is_some_and(|o| o.action == AssetAction::Recompress && o.format.is_none())
        })
        .filter_map(|c| auto.line(c.id).map(|l| format!("{}: {l}", c.name)))
        .collect();
    if !auto_lines.is_empty() {
        ui.add_space(6.0);
        ui.label(RichText::new("Automatic").size(12.0).color(TEXT_WEAK));
        for l in auto_lines {
            ui.label(RichText::new(l).size(12.0).color(TEXT_FAINT));
        }
    }
}

/// The badge color of a porting route.
fn route_color(r: &ptk_core::routes::Route) -> Color32 {
    use ptk_core::routes::RouteKind;
    match r.kind {
        RouteKind::Retarget | RouteKind::Native => GREEN,
        RouteKind::Runtime if r.plugin.is_some() => TEAL,
        RouteKind::Partial | RouteKind::Runtime => WARN,
        RouteKind::NeedsSource => ERROR,
        RouteKind::NotApplicable | RouteKind::Unresolved => TEXT_WEAK,
    }
}

/// Whether a plugin with this id is installed (or built in).
fn exporter_for_engine_installed(installed: &[String], plugin: &str) -> bool {
    installed.iter().any(|id| id == plugin)
}

#[cfg(test)]
mod tests {
    use super::*;
    use ptk_core::plugins::ListedPlugin;

    fn listed(id: &str, version: &str, path: &str, builtin: bool) -> ListedPlugin {
        ListedPlugin {
            id: id.into(),
            name: id.into(),
            version: version.into(),
            kind: "export".into(),
            path: path.into(),
            valid: true,
            risk: "high".into(),
            permissions: vec![],
            reviewed: builtin,
            builtin,
            inputs: vec![],
            targets: vec![],
            description: String::new(),
        }
    }

    #[test]
    fn plugin_states() {
        let tmp = tempfile::tempdir().unwrap();
        let installed = tmp.path().join("plugins");
        let builtin = tmp.path().join("engine/plugins");
        for d in [installed.join("html5-export"), builtin.join("html5-export")] {
            std::fs::create_dir_all(&d).unwrap();
        }
        let b = builtin.join("html5-export").display().to_string();
        let i = installed.join("html5-export").display().to_string();
        let list = vec![
            listed("html5-export", "1.2.0", &b, true),
            listed("html5-export", "1.0.0", &i, false),
        ];
        let pins = Default::default();
        let state = |path: &str, id: &str, v: &str| {
            plugin_state(path, Some(id), v, &list, &pins, &installed)
        };
        assert_eq!(state(&b, "html5-export", "1.2.0"), PluginState::BuiltIn);
        assert_eq!(state(&i, "html5-export", "1.0.0"), PluginState::Installed);
        assert_eq!(
            state("/elsewhere/html5-export", "html5-export", "1.3.0"),
            PluginState::Replaces {
                running: "1.2.0".into(),
                newer: true
            }
        );
        assert_eq!(
            state("/elsewhere/html5-export", "html5-export", "0.9.0"),
            PluginState::Replaces {
                running: "1.2.0".into(),
                newer: false
            }
        );
        assert_eq!(
            state("/elsewhere/other", "other", "1.0.0"),
            PluginState::New
        );
    }

    #[test]
    fn updates_come_from_the_index() {
        let entry = |v: &str| {
            serde_json::from_value::<ptk_core::market::MarketEntry>(serde_json::json!({
                "id": "html5-export", "name": "HTML5", "kind": "plugin", "version": v,
                "description": "", "author": "a", "license": "MIT",
                "download": "https://example.org/x.zip", "sha256": "0".repeat(64)
            }))
            .unwrap()
        };
        let market = Some(Ok(ptk_core::market::MarketIndex {
            schema_version: 1,
            entries: vec![entry("1.3.0")],
        }));
        assert_eq!(
            update_for(&market, "html5-export", "1.2.0")
                .unwrap()
                .version,
            "1.3.0"
        );
        assert!(update_for(&market, "html5-export", "1.3.0").is_none());
        assert!(update_for(&Some(Err("404".into())), "html5-export", "1.0.0").is_none());
    }
}
