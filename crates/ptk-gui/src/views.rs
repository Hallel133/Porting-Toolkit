//! Side bar and editor content of every view.

use std::path::PathBuf;

use eframe::egui::{self, Color32, RichText, Ui};
use ptk_core::category::CATEGORIES;
use ptk_core::doctor::Status;
use ptk_core::manifest::{AssetAction, CategoryOptions, Level, OWN_WORK_LICENSE};
use ptk_core::targets::{TargetSource, TargetStatus};

use crate::app::{App, View};
use crate::theme::*;
use crate::widgets::{self, Pick, badge, card, card_title, hint, human_size, kv, primary, section};

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

fn action_color(a: AssetAction) -> Color32 {
    match a {
        AssetAction::Keep => GREEN,
        AssetAction::Drop => ERROR,
        AssetAction::Recompress => YELLOW,
    }
}

fn risk_color(r: &str) -> Color32 {
    match r {
        "low" => GREEN,
        "medium" => WARN,
        "high" => ERROR,
        _ => TEXT_WEAK,
    }
}

impl App {
    // ============================================================ side bar

    pub fn side_bar(&mut self, ui: &mut Ui) {
        ui.label(
            RichText::new(self.view.title().to_uppercase())
                .size(11.5)
                .color(TEXT_WEAK),
        );
        ui.add_space(4.0);
        match self.view {
            View::Welcome | View::Project => self.side_explorer(ui),
            View::Scan => self.side_scan(ui),
            View::Options => self.side_options(ui),
            View::Share => self.side_share(ui),
            View::Plugins => self.side_plugins(ui),
            View::Targets => {
                hint(
                    ui,
                    "Platforms a project can target. SDKs can add more by dropping a [[target]] TOML file into:",
                );
                ui.label(
                    RichText::new(ptk_core::paths::targets_dir().display().to_string())
                        .monospace()
                        .size(11.5),
                );
            }
            View::Doctor => {
                hint(
                    ui,
                    "Checks Xcode tools, clang, rustup and uv, and sets up the Python engine.",
                );
                ui.add_space(6.0);
                let busy = self.busy.is_some();
                if primary(ui, !busy, "Run checks").clicked() {
                    self.start_doctor(ui.ctx(), false);
                }
                if ui
                    .add_enabled(!busy, egui::Button::new("Set up / update engine"))
                    .clicked()
                {
                    self.start_doctor(ui.ctx(), true);
                }
            }
            View::About => {
                hint(
                    ui,
                    "A community tool to move game projects you own between platforms.",
                );
            }
        }
    }

    fn side_explorer(&mut self, ui: &mut Ui) {
        if let Some(p) = &self.project {
            section(ui, &p.manifest.name);
            let c = p.contents;
            for (icon, name, detail) in [
                ("{}", "ptk.json".to_owned(), String::new()),
                ("🗀", "recipes".to_owned(), format!("{}", c.recipes)),
                ("🗀", "patches".to_owned(), format!("{}", c.patches)),
                (
                    "🔒",
                    "plugins.lock".to_owned(),
                    format!("{}", c.locked_plugins),
                ),
            ] {
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new(icon)
                            .color(if icon == "{}" { YELLOW } else { BLUE })
                            .monospace(),
                    );
                    ui.label(name);
                    if !detail.is_empty() {
                        ui.label(RichText::new(detail).color(TEXT_FAINT).size(11.5));
                    }
                });
            }
            ui.add_space(4.0);
            if ui.button("Close project").clicked() {
                self.project = None;
                self.view = View::Welcome;
                return;
            }
        }

        section(ui, "Open project");
        widgets::path_field(ui, &mut self.open_input, "project folder", Pick::Folder);
        if ui.button("Open").clicked() {
            let dir = PathBuf::from(self.open_input.trim());
            self.open_project(&dir);
        }

        section(ui, "New project");
        widgets::path_field(ui, &mut self.new_dir, "new folder", Pick::Folder);
        ui.add(
            egui::TextEdit::singleline(&mut self.new_title)
                .hint_text("game title (optional)")
                .desired_width(f32::INFINITY),
        );
        if ui.button("Create project").clicked() {
            self.create_project();
        }

        if !self.state.recent_projects.is_empty() {
            section(ui, "Recent");
            let mut open = None;
            for p in &self.state.recent_projects {
                let name = p
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_default();
                if ui
                    .link(RichText::new(name).color(LIGHT_BLUE))
                    .on_hover_text(p.display().to_string())
                    .clicked()
                {
                    open = Some(p.clone());
                }
            }
            if let Some(p) = open {
                self.open_project(&p);
            }
        }
    }

    fn side_scan(&mut self, ui: &mut Ui) {
        section(ui, "Game folder");
        widgets::path_field(ui, &mut self.game_input, "your game folder", Pick::Folder);
        let busy = self.busy.is_some();
        if primary(ui, !busy, "🔍  Scan").clicked() {
            self.start_scan(ui.ctx());
        }
        hint(
            ui,
            "Detection uses file names and executable headers only. Game archives are never opened.",
        );
        if !self.state.recent_games.is_empty() {
            section(ui, "Recent games");
            let mut pick = None;
            for g in &self.state.recent_games {
                let name = g
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_default();
                if ui
                    .link(RichText::new(name).color(LIGHT_BLUE))
                    .on_hover_text(g.display().to_string())
                    .clicked()
                {
                    pick = Some(g.display().to_string());
                }
            }
            if let Some(g) = pick {
                self.game_input = g;
            }
        }
    }

    fn side_options(&mut self, ui: &mut Ui) {
        hint(
            ui,
            "Before porting, choose per asset category what is kept, dropped or recompressed, so you control what is lost.",
        );
        let Some(p) = &mut self.project else {
            ui.add_space(8.0);
            hint(ui, "Open a project first.");
            return;
        };
        section(ui, "Presets");
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
        section(ui, "Changes");
        let dirty = p.dirty();
        ui.horizontal(|ui| {
            if primary(ui, dirty, "Save").clicked() {
                self.save_project();
            }
            if ui.add_enabled(dirty, egui::Button::new("Revert")).clicked() {
                self.revert_project();
            }
        });
    }

    fn side_share(&mut self, ui: &mut Ui) {
        hint(
            ui,
            "Share recipes, settings and patches as a .ptk file. Game files are never packed: others bring their own copy and check it with Verify.",
        );
        if let Some(p) = &self.project {
            section(ui, "Project");
            egui::Grid::new("share-side").num_columns(2).show(ui, |ui| {
                kv(ui, "Name", p.manifest.name.as_str());
                kv(ui, "Version", p.manifest.version.as_str());
                kv(
                    ui,
                    "Hashes",
                    format!("{} game files", p.manifest.source_files.len()),
                );
            });
        }
    }

    fn side_plugins(&mut self, ui: &mut Ui) {
        ui.horizontal(|ui| {
            section(ui, "Installed");
            if ui.small_button("⟳").on_hover_text("Refresh").clicked() {
                self.refresh_plugins(ui.ctx());
            }
        });
        match &self.plugins {
            None => hint(ui, "Loading…"),
            Some(Err(e)) => widgets::message(ui, ERROR, e),
            Some(Ok(list)) if list.is_empty() => hint(ui, "No plugins installed."),
            Some(Ok(list)) => {
                let mut pick = None;
                for p in list {
                    ui.horizontal(|ui| {
                        ui.label(RichText::new("🔌").color(TEAL));
                        ui.vertical(|ui| {
                            if ui.link(RichText::new(&p.id).strong().color(TEXT)).clicked() {
                                pick = Some(p.path.clone());
                            }
                            ui.label(
                                RichText::new(format!(
                                    "{} · {} · {} risk",
                                    p.version, p.kind, p.risk
                                ))
                                .size(11.5)
                                .color(TEXT_WEAK),
                            );
                        });
                    });
                }
                if let Some(path) = pick {
                    self.plugin_dir = path;
                    self.validate_plugin(ui.ctx());
                }
            }
        }
        section(ui, "New plugin");
        widgets::path_field(
            ui,
            &mut self.plugin_new_dir,
            "new plugin folder",
            Pick::Folder,
        );
        ui.horizontal(|ui| {
            ui.radio_value(&mut self.plugin_kind_export, false, "Import");
            ui.radio_value(&mut self.plugin_kind_export, true, "Export");
        });
        if ui
            .add_enabled(self.busy.is_none(), egui::Button::new("Create plugin"))
            .clicked()
        {
            self.create_plugin(ui.ctx());
        }
    }

    // ============================================================== editor

    pub fn editor(&mut self, ui: &mut Ui) {
        match self.view {
            View::Welcome => self.welcome(ui),
            View::Project => self.project_view(ui),
            View::Scan => self.scan_view(ui),
            View::Options => self.options_view(ui),
            View::Share => self.share_view(ui),
            View::Plugins => self.plugins_view(ui),
            View::Targets => self.targets_view(ui),
            View::Doctor => self.doctor_view(ui),
            View::About => self.about_view(ui),
        }
    }

    fn welcome(&mut self, ui: &mut Ui) {
        ui.add_space(12.0);
        widgets::wordmark(ui, 6.0);
        ui.add_space(14.0);
        ui.label(
            RichText::new("Manage, convert, export and import game projects between platforms")
                .size(17.0)
                .color(TEXT_WEAK),
        );
        ui.add_space(22.0);
        ui.columns(2, |cols| {
            let ui = &mut cols[0];
            ui.label(RichText::new("Start").size(17.0).color(Color32::WHITE));
            ui.add_space(4.0);
            for (icon, text, view) in [
                ("📄", "New project…", View::Project),
                ("🗀", "Open project…", View::Project),
                ("🔍", "Scan a game folder…", View::Scan),
                ("✚", "Check this Mac (Doctor)", View::Doctor),
            ] {
                if ui
                    .link(
                        RichText::new(format!("{icon}  {text}"))
                            .color(FOCUS)
                            .size(14.0),
                    )
                    .clicked()
                {
                    self.view = view;
                }
            }
            ui.add_space(14.0);
            ui.label(RichText::new("Recent").size(17.0).color(Color32::WHITE));
            ui.add_space(4.0);
            if self.state.recent_projects.is_empty() {
                hint(
                    ui,
                    "No recent projects. Drop a project or game folder on this window.",
                );
            }
            let mut open = None;
            for p in self.state.recent_projects.iter().take(6) {
                ui.horizontal(|ui| {
                    let name = p
                        .file_name()
                        .map(|n| n.to_string_lossy().into_owned())
                        .unwrap_or_default();
                    if ui.link(RichText::new(name).color(FOCUS)).clicked() {
                        open = Some(p.clone());
                    }
                    ui.label(
                        RichText::new(
                            p.parent()
                                .map(|d| d.display().to_string())
                                .unwrap_or_default(),
                        )
                        .color(TEXT_FAINT)
                        .size(12.0),
                    );
                });
            }
            if let Some(p) = open {
                self.open_project(&p);
            }

            let ui = &mut cols[1];
            ui.label(
                RichText::new("How it works")
                    .size(17.0)
                    .color(Color32::WHITE),
            );
            ui.add_space(4.0);
            for (n, title, text) in [
                (
                    "1",
                    "Bring your own game",
                    "Porting Toolkit works on game files you own and never ships them.",
                ),
                (
                    "2",
                    "Scan and record",
                    "Detect the engine and record SHA-256 hashes of your copy.",
                ),
                (
                    "3",
                    "Choose what is lost",
                    "Keep, drop or recompress each asset category before porting.",
                ),
                (
                    "4",
                    "Share the recipe",
                    "Pack recipes, settings and patches; others verify their own copy.",
                ),
            ] {
                card(ui, |ui| {
                    ui.horizontal(|ui| {
                        badge(ui, n, FOCUS);
                        ui.label(RichText::new(title).strong().color(Color32::WHITE));
                    });
                    hint(ui, text);
                });
                ui.add_space(6.0);
            }
        });
    }

    fn need_project(&mut self, ui: &mut Ui) -> bool {
        if self.project.is_some() {
            return true;
        }
        ui.add_space(30.0);
        ui.vertical_centered(|ui| {
            ui.label(RichText::new("No project open").size(20.0).color(TEXT_WEAK));
            ui.add_space(6.0);
            hint(
                ui,
                "Open or create a project in the Explorer, or drop a project folder here.",
            );
            ui.add_space(8.0);
            if primary(ui, true, "Open Explorer").clicked() {
                self.view = View::Project;
            }
        });
        false
    }

    fn project_view(&mut self, ui: &mut Ui) {
        if self.project.is_none() {
            ui.add_space(12.0);
            widgets::wordmark(ui, 4.0);
            ui.add_space(16.0);
            hint(ui, "Open a project or create a new one from the side bar.");
            return;
        }
        let issues = self.issues();
        let registry = self.registry.clone();
        let p = self.project.as_mut().expect("checked above");
        let m = &mut p.manifest;

        card(ui, |ui| {
            card_title(ui, "Project");
            egui::Grid::new("project-form")
                .num_columns(2)
                .spacing([18.0, 8.0])
                .show(ui, |ui| {
                    ui.label(RichText::new("Name").color(TEXT_WEAK));
                    ui.text_edit_singleline(&mut m.name);
                    ui.end_row();
                    ui.label(RichText::new("Version").color(TEXT_WEAK));
                    ui.text_edit_singleline(&mut m.version);
                    ui.end_row();
                    ui.label(RichText::new("Description").color(TEXT_WEAK));
                    ui.text_edit_singleline(&mut m.description);
                    ui.end_row();
                    ui.label(RichText::new("Authors").color(TEXT_WEAK));
                    if ui.text_edit_singleline(&mut p.authors_text).changed() {
                        m.authors = p
                            .authors_text
                            .split(',')
                            .map(|a| a.trim().to_owned())
                            .filter(|a| !a.is_empty())
                            .collect();
                    }
                    ui.end_row();
                    ui.label(RichText::new("License").color(TEXT_WEAK))
                        .on_hover_text("License of the community work: recipes and patches");
                    ui.text_edit_singleline(&mut m.license);
                    ui.end_row();
                });
        });
        ui.add_space(10.0);

        card(ui, |ui| {
            card_title(ui, "Game");
            egui::Grid::new("game-form")
                .num_columns(2)
                .spacing([18.0, 8.0])
                .show(ui, |ui| {
                    ui.label(RichText::new("Title").color(TEXT_WEAK));
                    let mut title = m.game.title.clone().unwrap_or_default();
                    if ui.text_edit_singleline(&mut title).changed() {
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
                                .unwrap_or_else(|| "not scanned".into()),
                        )
                        .color(TEAL),
                    );
                    kv(
                        ui,
                        "Platform",
                        m.game.source_platform.clone().unwrap_or_else(|| "-".into()),
                    );
                    let total: u64 = m.source_files.iter().map(|f| f.size).sum();
                    kv(
                        ui,
                        "Hashes",
                        if m.source_files.is_empty() {
                            "none recorded: scan your game and press Record".to_owned()
                        } else {
                            format!("{} files, {}", m.source_files.len(), human_size(total))
                        },
                    );
                    ui.label(RichText::new("Game files license").color(TEXT_WEAK));
                    let current = m.game.content_license.clone().unwrap_or_default();
                    let shown = CONTENT_LICENSES
                        .iter()
                        .find(|(id, _)| *id == current)
                        .map(|(_, l)| l.to_string())
                        .unwrap_or_else(|| current.clone());
                    egui::ComboBox::from_id_salt("content-license")
                        .width(240.0)
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
                    "Original game files are never packed. Only your own or an open-source game may change that.",
                );
            }
        });
        ui.add_space(10.0);

        card(ui, |ui| {
            card_title(ui, "Targets");
            ui.horizontal_wrapped(|ui| {
                for t in registry.iter() {
                    let mut on = m.targets.contains(&t.id);
                    let text = RichText::new(&t.id).monospace();
                    if ui.checkbox(&mut on, text).on_hover_text(&t.name).changed() {
                        if on {
                            m.targets.push(t.id.clone());
                        } else {
                            m.targets.retain(|x| x != &t.id);
                        }
                    }
                    ui.add_space(8.0);
                }
            });
        });
        ui.add_space(10.0);

        let dirty = p.dirty();
        ui.horizontal(|ui| {
            if primary(ui, dirty, "Save  ⌘S").clicked() {
                self.save_project();
            }
            if ui.add_enabled(dirty, egui::Button::new("Revert")).clicked() {
                self.revert_project();
            }
        });

        ui.add_space(14.0);
        ui.label(
            RichText::new(format!("PROBLEMS  {}", issues.len()))
                .size(11.5)
                .color(TEXT_WEAK),
        );
        ui.separator();
        if issues.is_empty() {
            hint(ui, "No problems have been detected in ptk.json.");
        }
        for i in &issues {
            ui.horizontal(|ui| {
                let (icon, color) = match i.level {
                    Level::Error => ("✖", ERROR),
                    Level::Warning => ("▲", WARN),
                };
                ui.label(RichText::new(icon).color(color));
                ui.label(&i.message);
            });
        }
    }

    fn scan_view(&mut self, ui: &mut Ui) {
        if let Some(n) = self.scan_progress {
            ui.horizontal(|ui| {
                ui.spinner();
                ui.label(format!("Scanning… {n} files"));
            });
            return;
        }
        let Some(scan) = self.scan.clone() else {
            ui.add_space(30.0);
            ui.vertical_centered(|ui| {
                ui.label(RichText::new("🔍").size(42.0).color(TEXT_FAINT));
                ui.label(
                    RichText::new("Scan a game folder")
                        .size(20.0)
                        .color(TEXT_WEAK),
                );
                hint(
                    ui,
                    "Choose a folder in the side bar, or drop one on this window.",
                );
            });
            return;
        };

        card(ui, |ui| match &scan.primary {
            Some(p) => {
                ui.horizontal(|ui| {
                    ui.label(RichText::new(&p.name).size(22.0).strong().color(TEAL));
                    if let Some(v) = &p.variant {
                        ui.label(RichText::new(v).size(16.0).color(LIGHT_BLUE));
                    }
                    let c = match p.label.as_str() {
                        "high" => GREEN,
                        "medium" => WARN,
                        _ => TEXT_WEAK,
                    };
                    badge(
                        ui,
                        &format!("{} confidence {:.2}", p.label, p.confidence),
                        c,
                    );
                    if p.kind != "engine" {
                        badge(ui, "format", PURPLE);
                    }
                });
                ui.add_space(4.0);
                egui::Grid::new("scan-summary")
                    .num_columns(2)
                    .spacing([18.0, 4.0])
                    .show(ui, |ui| {
                        kv(
                            ui,
                            "Evidence",
                            RichText::new({
                                let mut e = p.evidence[..p.evidence.len().min(3)].join(",  ");
                                if p.evidence.len() > 3 {
                                    e.push_str(&format!("  +{} more", p.evidence.len() - 3));
                                }
                                e
                            })
                            .monospace()
                            .size(12.0)
                            .color(ORANGE),
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
                        .size(20.0)
                        .color(WARN),
                );
                hint(
                    ui,
                    &format!("{} files, {}", scan.file_count, human_size(scan.total_size)),
                );
            }
        });
        ui.add_space(10.0);

        let can_record = self.project.is_some() && self.busy.is_none();
        ui.horizontal(|ui| {
            if primary(ui, can_record, "Record into project")
                .on_hover_text(
                    "Saves the engine and SHA-256 hashes of every file (never the files)",
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
        ui.add_space(10.0);

        card(ui, |ui| {
            card_title(
                ui,
                if scan.total_size > 0 {
                    "Assets by size"
                } else {
                    "Assets by file count"
                },
            );
            egui::Grid::new("scan-cats")
                .num_columns(5)
                .spacing([16.0, 6.0])
                .show(ui, |ui| {
                    for h in ["Category", "Files", "Size", "Share", ""] {
                        ui.label(RichText::new(h).size(11.5).color(TEXT_WEAK));
                    }
                    ui.end_row();
                    for c in scan.categories_by_size() {
                        let share = scan.share(c);
                        ui.label(RichText::new(&c.id).color(LIGHT_BLUE));
                        ui.label(c.files.to_string());
                        ui.label(human_size(c.size));
                        widgets::share_bar(ui, share, 260.0);
                        ui.label(RichText::new(format!("{:.0}%", share * 100.0)).color(TEXT_WEAK));
                        ui.end_row();
                    }
                });
        });
        if !scan.largest.is_empty() {
            ui.add_space(10.0);
            card(ui, |ui| {
                card_title(ui, "Largest files");
                egui::Grid::new("scan-largest")
                    .num_columns(3)
                    .spacing([16.0, 4.0])
                    .striped(true)
                    .show(ui, |ui| {
                        for f in &scan.largest {
                            ui.label(RichText::new(&f.path).monospace().size(12.0));
                            ui.label(RichText::new(&f.category).color(TEXT_WEAK));
                            ui.label(human_size(f.size));
                            ui.end_row();
                        }
                    });
            });
        }
        ui.add_space(6.0);
        hint(
            ui,
            "Detected from file names and headers only; no game archive was opened or decoded.",
        );
    }

    fn options_view(&mut self, ui: &mut Ui) {
        if !self.need_project(ui) {
            return;
        }
        let p = self.project.as_mut().expect("checked");
        card(ui, |ui| {
            card_title(ui, "Advanced options");
            hint(
                ui,
                "Applied before porting. Quality and resolution apply when recompressing textures, models, audio and video.",
            );
            ui.add_space(6.0);
            egui::Grid::new("options-grid")
                .num_columns(5)
                .spacing([18.0, 8.0])
                .striped(true)
                .show(ui, |ui| {
                    for h in ["Category", "Action", "Quality", "Max resolution", "Format"] {
                        ui.label(RichText::new(h).size(11.5).color(TEXT_WEAK));
                    }
                    ui.end_row();
                    for c in CATEGORIES {
                        let o = p.manifest.options.entry(c.id.to_owned()).or_default();
                        ui.vertical(|ui| {
                            ui.set_min_width(210.0);
                            ui.label(RichText::new(c.name).color(Color32::WHITE));
                            ui.label(RichText::new(c.description).size(11.0).color(TEXT_FAINT));
                        });
                        egui::ComboBox::from_id_salt(("action", c.id))
                            .width(120.0)
                            .selected_text(
                                RichText::new(o.action.to_string()).color(action_color(o.action)),
                            )
                            .show_ui(ui, |ui| {
                                for a in [
                                    AssetAction::Keep,
                                    AssetAction::Drop,
                                    AssetAction::Recompress,
                                ] {
                                    ui.selectable_value(
                                        &mut o.action,
                                        a,
                                        RichText::new(a.to_string()).color(action_color(a)),
                                    );
                                }
                            });
                        let lossy = c.lossy && o.action == AssetAction::Recompress;
                        ui.add_enabled_ui(lossy, |ui| {
                            let mut q = o.quality.unwrap_or(80);
                            if ui.add(egui::Slider::new(&mut q, 1..=100)).changed() {
                                o.quality = Some(q);
                            }
                        });
                        let res_ok = lossy && matches!(c.id, "textures" | "video");
                        ui.add_enabled_ui(res_ok, |ui| {
                            let label = |r: Option<u32>| {
                                r.map(|r| format!("{r} px"))
                                    .unwrap_or_else(|| "Original".into())
                            };
                            egui::ComboBox::from_id_salt(("res", c.id))
                                .width(110.0)
                                .selected_text(label(o.max_resolution))
                                .show_ui(ui, |ui| {
                                    for r in RESOLUTIONS {
                                        ui.selectable_value(&mut o.max_resolution, *r, label(*r));
                                    }
                                });
                        });
                        ui.add_enabled_ui(o.action == AssetAction::Recompress, |ui| {
                            let mut f = o.format.clone().unwrap_or_default();
                            if ui
                                .add(
                                    egui::TextEdit::singleline(&mut f)
                                        .hint_text("auto")
                                        .desired_width(80.0),
                                )
                                .changed()
                            {
                                o.format = Some(f).filter(|f| !f.trim().is_empty());
                            }
                        });
                        ui.end_row();
                    }
                });
        });
        // `entry().or_default()` adds missing categories; that is not an edit.
        if let Some(p) = &mut self.project {
            for c in CATEGORIES {
                p.saved.options.entry(c.id.to_owned()).or_default();
            }
        }
        ui.add_space(10.0);
        let dirty = self.project.as_ref().is_some_and(|p| p.dirty());
        ui.horizontal(|ui| {
            if primary(ui, dirty, "Save  ⌘S").clicked() {
                self.save_project();
            }
            if ui.add_enabled(dirty, egui::Button::new("Revert")).clicked() {
                self.revert_project();
            }
        });
    }

    fn share_view(&mut self, ui: &mut Ui) {
        let busy = self.busy.is_some();
        card(ui, |ui| {
            card_title(ui, "📦  Pack");
            if self.project.is_none() {
                hint(ui, "Open a project to pack it.");
                return;
            }
            hint(
                ui,
                "Packs ptk.json, README, LICENSE, plugins.lock, recipes/, patches/ and extra/. Third-party files are refused.",
            );
            widgets::path_field(
                ui,
                &mut self.pack_output,
                "output .ptk file",
                Pick::SavePack,
            );
            ui.horizontal(|ui| {
                if primary(ui, !busy, "Pack").clicked() {
                    self.start_pack(ui.ctx());
                }
                ui.checkbox(&mut self.pack_overwrite, "Replace an existing file");
            });
            match &self.pack_result {
                Some(Ok(s)) => {
                    widgets::message(
                        ui,
                        GREEN,
                        &format!(
                            "✔ {} ({} files, {}). No game files inside.",
                            s.output.display(),
                            s.files.len(),
                            human_size(s.bytes)
                        ),
                    );
                    egui::Grid::new("packed")
                        .num_columns(2)
                        .spacing([16.0, 2.0])
                        .show(ui, |ui| {
                            for (path, size) in &s.files {
                                ui.label(RichText::new(path).monospace().size(12.0));
                                ui.label(RichText::new(human_size(*size)).color(TEXT_WEAK));
                                ui.end_row();
                            }
                        });
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
        ui.add_space(10.0);

        card(ui, |ui| {
            card_title(ui, "📥  Unpack");
            hint(
                ui,
                "Checks every file against SHA256SUMS and refuses unexpected paths and links.",
            );
            widgets::path_field(ui, &mut self.unpack_input, ".ptk file", Pick::PackFile);
            widgets::path_field(
                ui,
                &mut self.unpack_dest,
                "destination folder (new or empty)",
                Pick::Folder,
            );
            if primary(ui, !busy, "Unpack").clicked() {
                self.start_unpack(ui.ctx());
            }
            let mut open = None;
            match &self.unpack_result {
                Some(Ok(r)) => {
                    widgets::message(
                        ui,
                        GREEN,
                        &format!(
                            "✔ Unpacked {} {} into {} ({} files, all checksums match)",
                            r.manifest.name,
                            r.manifest.version,
                            r.dest.display(),
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
            if let Some(d) = open {
                self.open_project(&d);
                self.view = View::Project;
            }
        });
        ui.add_space(10.0);

        card(ui, |ui| {
            card_title(ui, "✔  Verify your game");
            let hashes = self
                .project
                .as_ref()
                .map(|p| p.saved.source_files.len())
                .unwrap_or(0);
            if hashes == 0 {
                hint(
                    ui,
                    "Open a project with recorded game hashes to check your own copy against them.",
                );
                return;
            }
            hint(
                ui,
                &format!(
                    "Compares your game folder with the {hashes} hashes recorded in the project."
                ),
            );
            widgets::path_field(ui, &mut self.verify_game, "your game folder", Pick::Folder);
            if primary(ui, !busy, "Verify").clicked() {
                self.start_verify(ui.ctx());
            }
            match &self.verify_result {
                Some(Ok(r)) if r.is_ok() => widgets::message(
                    ui,
                    GREEN,
                    &format!(
                        "✔ All {} files match: this is the game version the project was made for.",
                        r.ok
                    ),
                ),
                Some(Ok(r)) => {
                    widgets::message(
                        ui,
                        ERROR,
                        &format!(
                            "✖ {} of {} files match, {} different, {} missing",
                            r.ok,
                            r.checked,
                            r.mismatched.len(),
                            r.missing.len()
                        ),
                    );
                    for f in &r.missing {
                        ui.label(
                            RichText::new(format!("missing    {f}"))
                                .monospace()
                                .size(12.0)
                                .color(ERROR),
                        );
                    }
                    for f in &r.mismatched {
                        ui.label(
                            RichText::new(format!("different  {}", f.path))
                                .monospace()
                                .size(12.0)
                                .color(WARN),
                        );
                    }
                }
                Some(Err(e)) => widgets::message(ui, ERROR, e),
                None => {}
            }
        });
    }

    fn plugins_view(&mut self, ui: &mut Ui) {
        card(ui, |ui| {
            card_title(ui, "Validate a plugin");
            hint(
                ui,
                "Checks plugin.toml and shows the permissions the plugin declares. No plugin code runs.",
            );
            widgets::path_field(ui, &mut self.plugin_dir, "plugin folder", Pick::Folder);
            if primary(ui, self.busy.is_none(), "Validate").clicked() {
                self.validate_plugin(ui.ctx());
            }
        });
        ui.add_space(10.0);
        let validation = self.plugin_validation.clone();
        match validation {
            None => {}
            Some(Err(e)) => widgets::message(ui, ERROR, &e),
            Some(Ok(v)) => {
                card(ui, |ui| {
                    let info = v.plugin.clone();
                    ui.horizontal(|ui| {
                        ui.label(RichText::new("🔌").size(26.0).color(TEAL));
                        ui.vertical(|ui| {
                            let id = info
                                .as_ref()
                                .and_then(|p| p.id.clone())
                                .unwrap_or_else(|| v.path.clone());
                            ui.label(RichText::new(id).size(18.0).strong().color(Color32::WHITE));
                            if let Some(p) = &info {
                                ui.label(
                                    RichText::new(format!(
                                        "{}  ·  {} plugin  ·  {}",
                                        p.version.clone().unwrap_or_default(),
                                        p.kind.clone().unwrap_or_default(),
                                        p.license.clone().unwrap_or_default()
                                    ))
                                    .color(TEXT_WEAK),
                                );
                            }
                        });
                    });
                    if let Some(d) = info
                        .as_ref()
                        .and_then(|p| p.description.clone())
                        .filter(|d| !d.is_empty())
                    {
                        ui.label(d);
                    }
                    ui.add_space(6.0);
                    ui.horizontal(|ui| {
                        badge(
                            ui,
                            if v.valid { "valid" } else { "invalid" },
                            if v.valid { GREEN } else { ERROR },
                        );
                        badge(ui, &format!("{} risk", v.risk), risk_color(&v.risk));
                        badge(ui, "not reviewed", TEXT_WEAK);
                    });
                    if !v.permissions.is_empty() {
                        ui.add_space(6.0);
                        ui.label(RichText::new("PERMISSIONS").size(11.5).color(TEXT_WEAK));
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
                    if primary(ui, v.valid && self.busy.is_none(), "Install…").clicked() {
                        self.confirm_install = true;
                    }
                });
            }
        }
        ui.add_space(10.0);
        hint(
            ui,
            "Plugins are Python packages with a plugin.toml. Phase 2 validates and installs them; running them arrives in phase 3.",
        );
    }

    fn targets_view(&mut self, ui: &mut Ui) {
        let host = self.registry.host().map(|t| t.id.clone());
        card(ui, |ui| {
            card_title(ui, "Targets");
            egui::Grid::new("targets")
                .num_columns(5)
                .spacing([18.0, 8.0])
                .striped(true)
                .show(ui, |ui| {
                    for h in ["Id", "Name", "OS / arch", "Status", "Notes"] {
                        ui.label(RichText::new(h).size(11.5).color(TEXT_WEAK));
                    }
                    ui.end_row();
                    for t in self.registry.iter() {
                        ui.label(RichText::new(&t.id).monospace().color(LIGHT_BLUE));
                        ui.label(&t.name);
                        ui.label(RichText::new(format!("{} / {}", t.os, t.arch)).color(TEXT_WEAK));
                        let c = match t.status {
                            TargetStatus::Active => GREEN,
                            TargetStatus::Experimental => WARN,
                            TargetStatus::Planned => TEXT_WEAK,
                        };
                        badge(ui, &t.status.to_string(), c);
                        ui.horizontal(|ui| {
                            if host.as_deref() == Some(t.id.as_str()) {
                                badge(ui, "this Mac", FOCUS);
                            }
                            ui.label(RichText::new(&t.description).color(TEXT_WEAK));
                            if let TargetSource::File(p) = &t.source {
                                ui.label(
                                    RichText::new(p.display().to_string())
                                        .size(11.0)
                                        .color(TEXT_FAINT),
                                );
                            }
                        });
                        ui.end_row();
                    }
                });
        });
    }

    fn doctor_view(&mut self, ui: &mut Ui) {
        let Some(checks) = &self.doctor else {
            ui.add_space(30.0);
            ui.vertical_centered(|ui| {
                ui.label(RichText::new("✚").size(40.0).color(TEXT_FAINT));
                hint(ui, "Run the checks from the side bar.");
            });
            return;
        };
        let problems = checks.iter().filter(|c| c.status == Status::Fail).count();
        card(ui, |ui| {
            card_title(ui, "Environment");
            egui::Grid::new("doctor")
                .num_columns(3)
                .spacing([14.0, 8.0])
                .show(ui, |ui| {
                    for c in checks {
                        widgets::status_icon(ui, c.status);
                        ui.label(RichText::new(&c.name).color(Color32::WHITE));
                        ui.vertical(|ui| {
                            ui.label(&c.detail);
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
        ui.add_space(8.0);
        if problems == 0 {
            widgets::message(ui, GREEN, "✔ ptk is ready");
        } else {
            widgets::message(ui, ERROR, &format!("✖ {problems} problem(s) found"));
        }
    }

    fn about_view(&mut self, ui: &mut Ui) {
        ui.add_space(8.0);
        widgets::wordmark(ui, 5.0);
        ui.add_space(14.0);
        card(ui, |ui| {
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
        ui.add_space(10.0);
        card(ui, |ui| {
            card_title(ui, "Roadmap");
            for (n, text, now) in [
                (
                    "1",
                    "macOS + CLI infrastructure, banner, project format",
                    false,
                ),
                ("2", "Desktop GUI", true),
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
                        if now { GREEN } else { TEXT_WEAK },
                    );
                    ui.label(text);
                });
            }
        });
        ui.add_space(10.0);
        widgets::message(
            ui,
            FOCUS,
            "A tool, not a distribution channel: you bring your own game. .ptk files hold recipes, settings, community patches and SHA-256 hashes, never original game files or decompiled code.",
        );
    }
}
