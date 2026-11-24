//! The pages. Each one is a column of rounded cards that reflows to the
//! window width (two or three columns when there is room, one when narrow).

use eframe::egui::{self, Align, Color32, Layout, RichText, Ui, vec2};
use ptk_core::category::CATEGORIES;
use ptk_core::doctor::Status;
use ptk_core::manifest::{AssetAction, CategoryOptions, Level, OWN_WORK_LICENSE};
use ptk_core::market::EntryKind;
use ptk_core::targets::{TargetSource, TargetStatus};

use crate::app::{App, MarketTab, ProjectSort, SettingsTab, View};
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

    /// "Unsaved changes" bar with Revert and Save.
    fn unsaved_bar(&mut self, ui: &mut Ui) {
        if !self.project.as_ref().is_some_and(|p| p.dirty()) {
            return;
        }
        egui::Frame::new()
            .fill(WARN.gamma_multiply(0.12))
            .stroke(egui::Stroke::new(1.0_f32, WARN.gamma_multiply(0.5)))
            .corner_radius(egui::CornerRadius::same(10))
            .inner_margin(egui::Margin::symmetric(14, 8))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal(|ui| {
                    ui.label(RichText::new("●").color(WARN));
                    ui.label("Unsaved changes in ptk.json");
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        self.command_button(ui, Command::Save, "Save  ⌘S", true);
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
                            "The index is read from {}. Until it is published, you can still install plugins and SDKs from a folder (Installed tab) and prepare your own (Publish tab).",
                            ptk_core::market::index_url()
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
                    .cloned()
                    .collect();
                // Game ports first.
                entries.sort_by_key(|e| e.kind != EntryKind::Project);
                let ctx = ui.ctx().clone();
                let posters: Vec<Option<egui::TextureHandle>> = entries
                    .iter()
                    .map(|e| e.poster.as_deref().and_then(|u| self.poster(&ctx, u)))
                    .collect();
                let idle = self.busy.is_none();
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
                            if let Some(e) = port_card(ui, e, posters[i].as_ref(), idle) {
                                install = Some(e);
                            }
                            return;
                        }
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
                            });
                            ui.add_space(4.0);
                            if primary(
                                ui,
                                idle,
                                if e.kind == EntryKind::Project {
                                    "Download"
                                } else {
                                    "Install…"
                                },
                            )
                            .clicked()
                            {
                                install = Some(e.clone());
                            }
                        });
                    });
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
                        "Upload the zip (for example to a GitHub release of your repository), put its URL in \"download\", and open a pull request that adds the entry to index.json. Reviewed entries get a ✔.",
                    );
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
        let exporter_for = |t: &str| -> Option<(String, bool)> {
            let Some(Ok(list)) = &self.plugins else {
                return None;
            };
            list.iter()
                .filter(|p| p.kind == "export" && p.valid && p.targets.iter().any(|x| x == t))
                .filter(|p| {
                    p.inputs.is_empty()
                        || engine
                            .as_deref()
                            .is_some_and(|e| p.inputs.iter().any(|i| i == e))
                })
                .min_by_key(|p| p.builtin)
                .map(|p| (p.name.clone(), p.builtin))
        };
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
        let idle = self.busy.is_none();
        let mut find = None;
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
                                let c = match r.kind {
                                    ptk_core::routes::RouteKind::Retarget
                                    | ptk_core::routes::RouteKind::Native => GREEN,
                                    ptk_core::routes::RouteKind::Partial => WARN,
                                    ptk_core::routes::RouteKind::NeedsSource => ERROR,
                                };
                                badge(ui, r.kind.label(), c);
                            });
                            ui.label(r.how);
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
                                        badge(ui, &format!("{pname} · {tag}"), GREEN);
                                    }
                                    None => {
                                        ui.horizontal(|ui| {
                                            badge(ui, "no exporter", TEXT_FAINT);
                                            if ui.small_button("Find one").clicked() {
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
                    let options_set = self.project.as_ref().is_some_and(|p| {
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
                            .on_disabled_hover_text("Set some advanced options to Recompress first (Game page).")
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
                            for r in ptk_core::routes::ROUTES
                                .iter()
                                .filter(|r| r.engine != "macos")
                            {
                                ui.label(RichText::new(r.name).color(Color32::WHITE));
                                let (text, c) = match r.kind {
                                    ptk_core::routes::RouteKind::Retarget => ("yes", GREEN),
                                    ptk_core::routes::RouteKind::Partial => ("depends", WARN),
                                    ptk_core::routes::RouteKind::NeedsSource => {
                                        ("needs source", ERROR)
                                    }
                                    ptk_core::routes::RouteKind::Native => ("native", GREEN),
                                };
                                ui.label(RichText::new(text).color(c)).on_hover_text(r.how);
                                ui.end_row();
                            }
                        });
                    hint(ui, "Hover a row for details.");
                });
            }
        });
        if let Some(t) = find {
            self.market_search = t;
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
                            RichText::new(format!("{} {}", r.plugin_name, r.plugin_version))
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
                (SettingsTab::Targets, "Targets", FOCUS),
                (SettingsTab::About, "About", FOCUS),
            ],
        );
        ui.add_space(14.0);
        match self.settings_tab {
            SettingsTab::Defaults => self.defaults_page(ui),
            SettingsTab::Environment => self.doctor_page(ui),
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
        let dirty = self.app_settings != self.app_settings_saved;
        ui.horizontal(|ui| {
            hint(
                ui,
                "New projects start with these. Each project keeps its own copy, so changing them never changes existing projects.",
            );
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if primary(ui, dirty, "Save defaults").clicked() {
                    self.save_app_settings();
                }
                if dirty && ui.button("Revert").clicked() {
                    self.app_settings = self.app_settings_saved.clone();
                }
            });
        });
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
                self.options_section(ui);
                ui.add_space(14.0);
                self.project_details(ui);
            } else {
                self.art_card(ui);
                ui.add_space(14.0);
                self.project_side(ui, &issues);
            }
        });
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
                    changed > 0,
                    "Choose what is lost",
                    if changed > 0 {
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
            let auto = AutoInfo {
                engine: p.manifest.game.engine.clone(),
                targets: p
                    .manifest
                    .targets
                    .iter()
                    .filter_map(|t| self.registry.get(t).cloned())
                    .collect(),
            };
            options_editor(ui, &mut p.manifest.options, "project", &auto);
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
                    for p in list {
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
                                    if !p.reviewed {
                                        badge(ui, "not reviewed", TEXT_FAINT);
                                    }
                                });
                            });
                        });
                        ui.add_space(6.0);
                    }
                }
            }
        });
        if refresh {
            self.refresh_plugins(ui.ctx());
        }
        if let Some(path) = pick {
            self.plugin_dir = path;
            self.validate_plugin(ui.ctx());
        }
    }

    fn plugin_tools(&mut self, ui: &mut Ui) {
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
                    let mut install = false;
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
                            badge(ui, "not reviewed", TEXT_FAINT);
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
                        install = primary(ui, v.valid && !busy, "Install…").clicked();
                    });
                    if install {
                        self.install_plugin();
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
fn port_card(
    ui: &mut Ui,
    e: &ptk_core::market::MarketEntry,
    poster: Option<&egui::TextureHandle>,
    idle: bool,
) -> Option<ptk_core::market::MarketEntry> {
    let mut get = None;
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
        });
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            if primary(ui, idle, "Get port").clicked() {
                get = Some(e.clone());
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
    get
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
    egui::Grid::new(("options", salt))
        .num_columns(3)
        .spacing([16.0, 10.0])
        .min_col_width(150.0)
        .show(ui, |ui| {
            for c in CATEGORIES {
                let o = options.entry(c.id.to_owned()).or_default();
                ui.horizontal(|ui| {
                    ui.label(RichText::new("■").color(category_color(c.id)));
                    ui.label(RichText::new(c.name).color(Color32::WHITE))
                        .on_hover_text(c.description);
                });
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
                            .width(170.0)
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
                            .width(120.0)
                            .selected_text(label(o.max_resolution))
                            .show_ui(ui, |ui| {
                                for r in RESOLUTIONS {
                                    ui.selectable_value(&mut o.max_resolution, *r, label(*r));
                                }
                            });
                    }
                });
                ui.end_row();
            }
        });
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
