//! Commands and the menu bar.
//!
//! Every action the menus offer is a [`Command`]. On macOS they appear in
//! the real (native) menu bar at the top of the screen, built with `muda`;
//! on other systems a menu button in the header shows the same list.
//! Keyboard shortcuts are attached to the native items, so ⌘N, ⌘O, ⌘S,
//! ⌘1-⌘7 and friends work like in any Mac app.

use crate::app::View;

/// Where the Help menu links go.
pub const HELP_LINKS: &[(&str, &str)] = &[
    (
        "Porting Toolkit Help",
        "https://github.com/Hallel133/Porting-Toolkit#readme",
    ),
    (
        "Project Format",
        "https://github.com/Hallel133/Porting-Toolkit/blob/main/docs/project-format.md",
    ),
    (
        "Writing Plugins",
        "https://github.com/Hallel133/Porting-Toolkit/blob/main/docs/plugins.md",
    ),
    (
        "Bring Your Own Game (Legal)",
        "https://github.com/Hallel133/Porting-Toolkit/blob/main/docs/legal.md",
    ),
    (
        "Report an Issue",
        "https://github.com/Hallel133/Porting-Toolkit/issues",
    ),
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Command {
    NewProject,
    OpenProject,
    OpenRecent(usize),
    Save,
    Revert,
    CloseProject,
    UnpackPtk,
    PackProject,
    Undo,
    Redo,
    Cut,
    Copy,
    Paste,
    SelectAll,
    Go(View),
    ScanGame,
    RescanGame,
    VerifyGame,
    RunDoctor,
    SetUpEngine,
    RevealData,
    RevealPlugins,
    About,
    Help(usize),
}

/// Views in navigation order, with their ⌘-number.
pub const NAV: [View; 7] = [
    View::Home,
    View::Project,
    View::Scan,
    View::Options,
    View::Share,
    View::Plugins,
    View::Targets,
];

impl Command {
    /// Stable menu id.
    pub fn id(self) -> String {
        match self {
            Command::OpenRecent(i) => format!("recent:{i}"),
            Command::Help(i) => format!("help:{i}"),
            Command::Go(v) => format!("go:{v:?}"),
            other => format!("cmd:{other:?}"),
        }
    }

    pub fn from_id(id: &str) -> Option<Command> {
        if let Some(i) = id.strip_prefix("recent:") {
            return i.parse().ok().map(Command::OpenRecent);
        }
        if let Some(i) = id.strip_prefix("help:") {
            return i.parse().ok().map(Command::Help);
        }
        if let Some(v) = id.strip_prefix("go:") {
            return View::parse(v).map(Command::Go);
        }
        ALL.iter().copied().find(|c| c.id() == id)
    }

    pub fn label(self) -> String {
        match self {
            Command::NewProject => "New Project…".into(),
            Command::OpenProject => "Open Project…".into(),
            Command::OpenRecent(i) => format!("Recent {i}"),
            Command::Save => "Save".into(),
            Command::Revert => "Revert to Saved".into(),
            Command::CloseProject => "Close Project".into(),
            Command::UnpackPtk => "Unpack .ptk File…".into(),
            Command::PackProject => "Pack Project".into(),
            Command::Undo => "Undo".into(),
            Command::Redo => "Redo".into(),
            Command::Cut => "Cut".into(),
            Command::Copy => "Copy".into(),
            Command::Paste => "Paste".into(),
            Command::SelectAll => "Select All".into(),
            Command::Go(v) => v.title().into(),
            Command::ScanGame => "Scan Game Folder…".into(),
            Command::RescanGame => "Scan Again".into(),
            Command::VerifyGame => "Verify My Game…".into(),
            Command::RunDoctor => "Check Environment".into(),
            Command::SetUpEngine => "Set Up / Update Engine".into(),
            Command::RevealData => "Show Data Folder in Finder".into(),
            Command::RevealPlugins => "Show Plugins Folder in Finder".into(),
            Command::About => "About Porting Toolkit".into(),
            Command::Help(i) => HELP_LINKS.get(i).map(|h| h.0).unwrap_or("Help").into(),
        }
    }

    /// Shortcut as (⌘, ⇧, key). Shown in menus and handled by them.
    pub fn shortcut(self) -> Option<(bool, char)> {
        Some(match self {
            Command::NewProject => (false, 'N'),
            Command::OpenProject => (false, 'O'),
            Command::Save => (false, 'S'),
            Command::CloseProject => (true, 'W'),
            Command::UnpackPtk => (true, 'O'),
            Command::PackProject => (true, 'E'),
            Command::Undo => (false, 'Z'),
            Command::Redo => (true, 'Z'),
            Command::Cut => (false, 'X'),
            Command::Copy => (false, 'C'),
            Command::Paste => (false, 'V'),
            Command::SelectAll => (false, 'A'),
            Command::ScanGame => (true, 'G'),
            Command::RescanGame => (false, 'R'),
            Command::RunDoctor => (true, 'D'),
            Command::Go(v) => {
                let n = NAV.iter().position(|x| *x == v)? + 1;
                (false, char::from_digit(n as u32, 10)?)
            }
            _ => return None,
        })
    }

    /// `⌘S`, `⇧⌘E` for in-window menus.
    pub fn shortcut_text(self) -> String {
        match self.shortcut() {
            Some((shift, key)) => format!("{}⌘{key}", if shift { "⇧" } else { "" }),
            None => String::new(),
        }
    }
}

/// Every fixed command (for id lookups).
pub const ALL: &[Command] = &[
    Command::NewProject,
    Command::OpenProject,
    Command::Save,
    Command::Revert,
    Command::CloseProject,
    Command::UnpackPtk,
    Command::PackProject,
    Command::Undo,
    Command::Redo,
    Command::Cut,
    Command::Copy,
    Command::Paste,
    Command::SelectAll,
    Command::ScanGame,
    Command::RescanGame,
    Command::VerifyGame,
    Command::RunDoctor,
    Command::SetUpEngine,
    Command::RevealData,
    Command::RevealPlugins,
    Command::About,
];

/// One menu of the bar: a title and entries (`None` = separator).
pub struct MenuSpec {
    pub title: &'static str,
    pub items: Vec<Option<Command>>,
}

/// The menus, shared by the native bar and the in-window fallback. The
/// application menu (About, Hide, Quit) and the Window menu are added by
/// the native bar only.
pub fn menus() -> Vec<MenuSpec> {
    use Command::*;
    vec![
        MenuSpec {
            title: "File",
            items: vec![
                Some(NewProject),
                Some(OpenProject),
                None,
                Some(Save),
                Some(Revert),
                None,
                Some(PackProject),
                Some(UnpackPtk),
                None,
                Some(CloseProject),
            ],
        },
        MenuSpec {
            title: "Edit",
            items: vec![
                Some(Undo),
                Some(Redo),
                None,
                Some(Cut),
                Some(Copy),
                Some(Paste),
                Some(SelectAll),
            ],
        },
        MenuSpec {
            title: "View",
            items: NAV.iter().map(|v| Some(Go(*v))).collect(),
        },
        MenuSpec {
            title: "Tools",
            items: vec![
                Some(ScanGame),
                Some(RescanGame),
                Some(VerifyGame),
                None,
                Some(RunDoctor),
                Some(SetUpEngine),
                None,
                Some(RevealData),
                Some(RevealPlugins),
            ],
        },
        MenuSpec {
            title: "Help",
            items: (0..HELP_LINKS.len()).map(|i| Some(Help(i))).collect(),
        },
    ]
}

/// What decides whether items are enabled.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct MenuState {
    pub project_open: bool,
    pub dirty: bool,
    pub busy: bool,
    pub has_game: bool,
    pub has_hashes: bool,
}

impl MenuState {
    pub fn enabled(&self, c: Command) -> bool {
        match c {
            Command::Save | Command::Revert => self.dirty,
            Command::CloseProject => self.project_open,
            Command::PackProject => self.project_open && !self.busy,
            Command::VerifyGame => self.has_hashes && !self.busy,
            Command::RescanGame => self.has_game && !self.busy,
            Command::ScanGame | Command::UnpackPtk | Command::RunDoctor | Command::SetUpEngine => {
                !self.busy
            }
            _ => true,
        }
    }
}

#[cfg(target_os = "macos")]
pub use native::NativeMenu;

#[cfg(target_os = "macos")]
mod native {
    use std::collections::HashMap;
    use std::path::PathBuf;
    use std::sync::mpsc::Sender;

    use muda::accelerator::{Accelerator, Code, Modifiers};
    use muda::{AboutMetadata, Menu, MenuEvent, MenuItem, PredefinedMenuItem, Submenu};

    use super::{Command, MenuState, menus};

    fn code(key: char) -> Option<Code> {
        Some(match key {
            'A' => Code::KeyA,
            'C' => Code::KeyC,
            'D' => Code::KeyD,
            'E' => Code::KeyE,
            'G' => Code::KeyG,
            'N' => Code::KeyN,
            'O' => Code::KeyO,
            'R' => Code::KeyR,
            'S' => Code::KeyS,
            'V' => Code::KeyV,
            'W' => Code::KeyW,
            'X' => Code::KeyX,
            'Z' => Code::KeyZ,
            '1' => Code::Digit1,
            '2' => Code::Digit2,
            '3' => Code::Digit3,
            '4' => Code::Digit4,
            '5' => Code::Digit5,
            '6' => Code::Digit6,
            '7' => Code::Digit7,
            _ => return None,
        })
    }

    fn accelerator(c: Command) -> Option<Accelerator> {
        let (shift, key) = c.shortcut()?;
        let mods = if shift {
            Modifiers::META | Modifiers::SHIFT
        } else {
            Modifiers::META
        };
        Some(Accelerator::new(mods, code(key)?))
    }

    /// The native macOS menu bar. Keep it alive for the app's lifetime.
    pub struct NativeMenu {
        _menu: Menu,
        items: HashMap<String, (Command, MenuItem)>,
        recent: Submenu,
        recent_items: Vec<MenuItem>,
        recent_paths: Vec<PathBuf>,
        state: Option<MenuState>,
    }

    impl NativeMenu {
        /// Builds the bar and installs it. Menu clicks arrive as commands on
        /// `tx`, and `wake` is called so an idle window repaints.
        pub fn install(tx: Sender<Command>, wake: impl Fn() + Send + Sync + 'static) -> Self {
            MenuEvent::set_event_handler(Some(move |e: MenuEvent| {
                if let Some(c) = Command::from_id(e.id().as_ref()) {
                    let _ = tx.send(c);
                    wake();
                }
            }));

            let menu = Menu::new();
            let about = PredefinedMenuItem::about(
                Some("About Porting Toolkit"),
                Some(AboutMetadata {
                    name: Some("Porting Toolkit".into()),
                    version: Some(ptk_core::VERSION.into()),
                    comments: Some(
                        "Manage, convert, export and import game projects between platforms. Bring your own game."
                            .into(),
                    ),
                    copyright: Some("© 2026 Hallel133 and contributors. MIT License.".into()),
                    website: Some(ptk_core::REPOSITORY.into()),
                    ..Default::default()
                }),
            );
            let app = Submenu::with_items(
                "Porting Toolkit",
                true,
                &[
                    &about,
                    &PredefinedMenuItem::separator(),
                    &PredefinedMenuItem::services(None),
                    &PredefinedMenuItem::separator(),
                    &PredefinedMenuItem::hide(None),
                    &PredefinedMenuItem::hide_others(None),
                    &PredefinedMenuItem::show_all(None),
                    &PredefinedMenuItem::separator(),
                    &PredefinedMenuItem::quit(None),
                ],
            )
            .expect("app menu");
            let _ = menu.append(&app);

            let mut items = HashMap::new();
            let recent = Submenu::new("Open Recent", true);
            for spec in menus() {
                let sub = Submenu::new(spec.title, true);
                for (i, entry) in spec.items.iter().enumerate() {
                    match entry {
                        None => {
                            let _ = sub.append(&PredefinedMenuItem::separator());
                        }
                        Some(c) => {
                            let item = MenuItem::with_id(c.id(), c.label(), true, accelerator(*c));
                            let _ = sub.append(&item);
                            items.insert(c.id(), (*c, item));
                            if *c == Command::OpenProject && i == 1 {
                                let _ = sub.append(&recent);
                            }
                        }
                    }
                }
                if spec.title == "View" {
                    let _ = sub.append(&PredefinedMenuItem::separator());
                    let _ = sub.append(&PredefinedMenuItem::fullscreen(None));
                }
                let _ = menu.append(&sub);
                if spec.title == "Tools" {
                    let window = Submenu::with_items(
                        "Window",
                        true,
                        &[
                            &PredefinedMenuItem::minimize(None),
                            &PredefinedMenuItem::maximize(Some("Zoom")),
                            &PredefinedMenuItem::separator(),
                            &PredefinedMenuItem::bring_all_to_front(None),
                        ],
                    )
                    .expect("window menu");
                    let _ = menu.append(&window);
                    window.set_as_windows_menu_for_nsapp();
                }
                if spec.title == "Help" {
                    sub.set_as_help_menu_for_nsapp();
                }
            }
            menu.init_for_nsapp();
            NativeMenu {
                _menu: menu,
                items,
                recent,
                recent_items: Vec::new(),
                recent_paths: Vec::new(),
                state: None,
            }
        }

        /// Enables and disables items; cheap when nothing changed.
        pub fn sync(&mut self, state: MenuState, recent: &[PathBuf]) {
            if self.state != Some(state) {
                for (c, item) in self.items.values() {
                    item.set_enabled(state.enabled(*c));
                }
                self.state = Some(state);
            }
            if self.recent_paths != recent {
                for item in self.recent_items.drain(..) {
                    let _ = self.recent.remove(&item);
                }
                for (i, p) in recent.iter().enumerate() {
                    let name = p
                        .file_name()
                        .map(|n| n.to_string_lossy().into_owned())
                        .unwrap_or_else(|| p.display().to_string());
                    let item = MenuItem::with_id(Command::OpenRecent(i).id(), name, true, None);
                    let _ = self.recent.append(&item);
                    self.recent_items.push(item);
                }
                self.recent.set_enabled(!recent.is_empty());
                self.recent_paths = recent.to_vec();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_round_trip() {
        for c in ALL {
            assert_eq!(Command::from_id(&c.id()), Some(*c), "{c:?}");
        }
        for v in NAV {
            assert_eq!(Command::from_id(&Command::Go(v).id()), Some(Command::Go(v)));
        }
        assert_eq!(Command::from_id("recent:3"), Some(Command::OpenRecent(3)));
        assert_eq!(Command::from_id("help:1"), Some(Command::Help(1)));
        assert_eq!(Command::from_id("nope"), None);
    }

    #[test]
    fn shortcuts_are_unique() {
        let mut seen = std::collections::HashSet::new();
        for spec in menus() {
            for c in spec.items.into_iter().flatten() {
                if let Some(s) = c.shortcut() {
                    assert!(seen.insert(s), "duplicate shortcut for {c:?}");
                }
            }
        }
        assert_eq!(Command::Go(View::Scan).shortcut_text(), "⌘3");
        assert_eq!(Command::PackProject.shortcut_text(), "⇧⌘E");
    }

    #[test]
    fn enabled_states() {
        let s = MenuState::default();
        assert!(!s.enabled(Command::Save));
        assert!(!s.enabled(Command::PackProject));
        assert!(s.enabled(Command::NewProject));
        let s = MenuState {
            project_open: true,
            dirty: true,
            ..Default::default()
        };
        assert!(s.enabled(Command::Save) && s.enabled(Command::PackProject));
        let busy = MenuState { busy: true, ..s };
        assert!(!busy.enabled(Command::PackProject) && !busy.enabled(Command::ScanGame));
    }
}
