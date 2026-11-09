//! The pages. Each one is a column of rounded cards that reflows to the
//! window width (two or three columns when there is room, one when narrow).

use eframe::egui::{self, Align, Color32, Layout, RichText, Ui, vec2};
use ptk_core::category::CATEGORIES;
use ptk_core::doctor::Status;
use ptk_core::manifest::{AssetAction, CategoryOptions, Level, OWN_WORK_LICENSE};
use ptk_core::targets::{TargetSource, TargetStatus};

use crate::app::{App, View};
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
            View::Home => self.home(ui),
            View::Project => self.project_page(ui),
            View::Scan => self.scan_page(ui),
            View::Options => self.options_page(ui),
            View::Share => self.share_page(ui),
            View::Plugins => self.plugins_page(ui),
            View::Targets => self.targets_page(ui),
            View::Doctor => self.doctor_page(ui),
            View::About => self.about_page(ui),
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

    // ================================================================ home

    fn home(&mut self, ui: &mut Ui) {
        card(ui, |ui| {
            ui.add_space(6.0);
            let cell = ((ui.available_width() - 8.0) / 72.0).clamp(2.0, 6.0);
            widgets::wordmark(ui, cell);
            ui.add_space(12.0);
            ui.label(
                RichText::new(
                    "Manage, convert, export and import game projects between platforms.",
                )
                .size(16.0)
                .color(TEXT),
            );
            hint(
                ui,
                "Bring your own game: Porting Toolkit works on files you own and never ships them.",
            );
            ui.add_space(12.0);
            ui.horizontal_wrapped(|ui| {
                self.command_button(ui, Command::NewProject, "+  New Project…", true);
                self.command_button(ui, Command::OpenProject, "🗀  Open Project…", false);
                self.command_button(ui, Command::ScanGame, "🔍  Scan a Game…", false);
            });
        });
        ui.add_space(22.0);

        ui.label(
            RichText::new("Recent projects")
                .size(17.0)
                .strong()
                .color(Color32::WHITE),
        );
        ui.add_space(6.0);
        let recent = self.state.recent_projects.clone();
        if recent.is_empty() {
            hint(
                ui,
                "Nothing yet. Create a project, or drop a project or game folder on this window.",
            );
            ui.add_space(14.0);
        } else {
            let cols = columns_for(ui, 260.0, 3);
            let mut open = None;
            card_grid(ui, recent.len().min(6), cols, |ui, i| {
                let p = &recent[i];
                card(ui, |ui| {
                    let name = p
                        .file_name()
                        .map(|n| n.to_string_lossy().into_owned())
                        .unwrap_or_default();
                    if ui
                        .link(
                            RichText::new(format!("🗀  {name}"))
                                .size(15.0)
                                .color(LIGHT_BLUE),
                        )
                        .clicked()
                    {
                        open = Some(p.clone());
                    }
                    ui.label(
                        RichText::new(
                            p.parent()
                                .map(|d| d.display().to_string())
                                .unwrap_or_default(),
                        )
                        .size(11.5)
                        .color(TEXT_FAINT),
                    );
                });
            });
            if let Some(p) = open {
                self.open_project(&p);
                self.view = View::Project;
            }
        }

        ui.label(
            RichText::new("How it works")
                .size(17.0)
                .strong()
                .color(Color32::WHITE),
        );
        ui.add_space(6.0);
        let steps = [
            (
                "1",
                "Bring your own game",
                "Porting Toolkit works on game files you own.",
            ),
            (
                "2",
                "Scan and record",
                "Detect the engine and record SHA-256 hashes of your copy.",
            ),
            (
                "3",
                "Choose what is lost",
                "Keep, drop or recompress each asset category.",
            ),
            (
                "4",
                "Share the recipe",
                "Pack recipes and patches; others verify their own copy.",
            ),
        ];
        let cols = columns_for(ui, 230.0, 4);
        card_grid(ui, steps.len(), cols, |ui, i| {
            let (n, title, text) = steps[i];
            card(ui, |ui| {
                badge(ui, n, FOCUS);
                ui.label(RichText::new(title).strong().color(Color32::WHITE));
                hint(ui, text);
            });
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
                self.command_button(ui, Command::NewProject, "+  New Project…", true);
                self.command_button(ui, Command::OpenProject, "🗀  Open Project…", false);
            });
            return;
        };
        let title = p.manifest.name.clone();
        let path = p.dir.display().to_string();
        page_header(ui, &title, &path);
        self.unsaved_bar(ui);
        let issues = self.issues();
        split(ui, 0.6, 880.0, |ui, col| {
            if col == 0 {
                self.project_forms(ui);
            } else {
                self.project_side(ui, &issues);
            }
        });
    }

    fn project_forms(&mut self, ui: &mut Ui) {
        let registry = self.registry.clone();
        let Some(p) = self.project.as_mut() else {
            return;
        };
        let m = &mut p.manifest;
        card(ui, |ui| {
            card_title(ui, "Details");
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
        ui.add_space(14.0);

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
                                .engine
                                .clone()
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
                    View::Options,
                ),
                (
                    packed,
                    "Share",
                    if packed {
                        "Packed this session".into()
                    } else {
                        "Pack a .ptk for others".into()
                    },
                    View::Share,
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

    fn options_page(&mut self, ui: &mut Ui) {
        page_header(
            ui,
            "Advanced options",
            "Before porting, decide per asset category what is kept, dropped or recompressed, so you control what is lost.",
        );
        if self.project.is_none() {
            empty_state(
                ui,
                "⚙",
                "No project open",
                "Open a project to edit its options.",
            );
            return;
        }
        self.unsaved_bar(ui);
        let Some(p) = self.project.as_mut() else {
            return;
        };
        // Missing categories mean "keep"; filling them in is not an edit.
        for c in CATEGORIES {
            p.manifest.options.entry(c.id.to_owned()).or_default();
            p.saved.options.entry(c.id.to_owned()).or_default();
        }

        ui.horizontal_wrapped(|ui| {
            ui.label(RichText::new("Presets").color(TEXT_WEAK));
            type Preset = (&'static str, fn(&str) -> CategoryOptions);
            let presets: [Preset; 3] = [
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
                ("Drop videos", |cat| {
                    if cat == "video" {
                        CategoryOptions {
                            action: AssetAction::Drop,
                            ..Default::default()
                        }
                    } else {
                        CategoryOptions::default()
                    }
                }),
            ];
            for (name, f) in presets {
                if ui.button(name).clicked() {
                    for c in CATEGORIES {
                        p.manifest.options.insert(c.id.to_owned(), f(c.id));
                    }
                }
            }
        });
        ui.add_space(12.0);

        let cols = columns_for(ui, 400.0, 2);
        card_grid(ui, CATEGORIES.len(), cols, |ui, i| {
            let c = &CATEGORIES[i];
            let o = p.manifest.options.entry(c.id.to_owned()).or_default();
            card(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new("■").color(category_color(c.id)));
                    ui.label(
                        RichText::new(c.name)
                            .size(15.0)
                            .strong()
                            .color(Color32::WHITE),
                    );
                });
                hint(ui, c.description);
                ui.add_space(4.0);
                widgets::segmented(
                    ui,
                    &mut o.action,
                    &[
                        (AssetAction::Keep, "Keep", GREEN),
                        (AssetAction::Drop, "Drop", ERROR),
                        (AssetAction::Recompress, "Recompress", YELLOW),
                    ],
                );
                if o.action == AssetAction::Recompress {
                    ui.add_space(4.0);
                    egui::Grid::new(("opt", c.id))
                        .num_columns(2)
                        .spacing([14.0, 6.0])
                        .show(ui, |ui| {
                            if c.lossy {
                                ui.label(RichText::new("Quality").color(TEXT_WEAK));
                                let mut q = o.quality.unwrap_or(80);
                                if ui.add(egui::Slider::new(&mut q, 1..=100)).changed() {
                                    o.quality = Some(q);
                                }
                                ui.end_row();
                            }
                            if matches!(c.id, "textures" | "video") {
                                ui.label(RichText::new("Max resolution").color(TEXT_WEAK));
                                let label = |r: Option<u32>| {
                                    r.map(|r| format!("{r} px"))
                                        .unwrap_or_else(|| "Original".into())
                                };
                                egui::ComboBox::from_id_salt(("res", c.id))
                                    .width(120.0)
                                    .selected_text(label(o.max_resolution))
                                    .show_ui(ui, |ui| {
                                        for r in RESOLUTIONS {
                                            ui.selectable_value(
                                                &mut o.max_resolution,
                                                *r,
                                                label(*r),
                                            );
                                        }
                                    });
                                ui.end_row();
                            }
                            ui.label(RichText::new("Format").color(TEXT_WEAK));
                            let mut f = o.format.clone().unwrap_or_default();
                            if ui
                                .add(
                                    egui::TextEdit::singleline(&mut f)
                                        .hint_text("auto")
                                        .desired_width(120.0),
                                )
                                .changed()
                            {
                                o.format = Some(f).filter(|f| !f.trim().is_empty());
                            }
                            ui.end_row();
                        });
                }
            });
        });
    }

    // =============================================================== share

    fn share_page(&mut self, ui: &mut Ui) {
        page_header(
            ui,
            "Share",
            "A .ptk file carries recipes, settings, patches and hashes. Never game files: others bring their own copy and verify it.",
        );
        let cols = columns_for(ui, 330.0, 3);
        card_grid(ui, 3, cols, |ui, i| match i {
            0 => self.pack_card(ui),
            1 => self.unpack_card(ui),
            _ => self.verify_card(ui),
        });
    }

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
            self.view = View::Project;
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

    fn plugins_page(&mut self, ui: &mut Ui) {
        page_header(
            ui,
            "Plugins",
            "Import and export plugins are Python packages with a plugin.toml. They are validated and installed now; running them arrives in phase 3.",
        );
        split(ui, 0.45, 880.0, |ui, col| {
            if col == 0 {
                self.installed_plugins(ui);
            } else {
                self.plugin_tools(ui);
            }
        });
    }

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
                        self.confirm_install = true;
                    }
                    ui.add_space(14.0);
                }
            }
        }
        let mut create = false;
        card(ui, |ui| {
            card_title(ui, "New plugin");
            widgets::path_field(
                ui,
                &mut self.plugin_new_dir,
                "new plugin folder",
                Pick::Folder,
            );
            ui.horizontal(|ui| {
                ui.radio_value(&mut self.plugin_kind_export, false, "Import");
                ui.radio_value(&mut self.plugin_kind_export, true, "Export");
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    create = ui.add_enabled(!busy, egui::Button::new("Create")).clicked();
                });
            });
        });
        if create {
            self.create_plugin(ui.ctx());
        }
    }

    // ============================================================= targets

    fn targets_page(&mut self, ui: &mut Ui) {
        page_header(
            ui,
            "Targets",
            "Platforms a project can be ported to. macOS comes first; SDKs can register more.",
        );
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
        page_header(
            ui,
            "Doctor",
            "Checks Xcode Command Line Tools, clang, rustup and uv, and sets up the Python engine.",
        );
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
                                ui.label(
                                    RichText::new(format!("→ {h}")).size(12.0).color(TEXT_WEAK),
                                );
                            }
                        });
                        ui.end_row();
                    }
                });
        });
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
                ("2", "Desktop app", true),
                (
                    "3",
                    "Import/export plugins and smart (AI-assisted) advanced options",
                    false,
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
