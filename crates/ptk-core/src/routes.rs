//! What it takes to port a game of each engine, with and without source code.
//!
//! Porting Toolkit works on builds you own (folders, `.app`, `.exe`, `.deb`
//! …). Whether a build can move to another platform without source code
//! depends on where the game logic lives: in platform-independent data
//! (a Godot `.pck`, Ren'Py scripts, RPG Maker JavaScript) or in native
//! machine code (Unreal, RAGE, Source, Unity IL2CPP).

//!
//! The routes come from the engine catalog shared with the Python engine
//! (`engine/ptk_engine/data/engines.json`), which also holds the detection
//! rules.

use std::sync::OnceLock;

use serde::{Deserialize, Serialize};

/// How a game can reach another platform.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RouteKind {
    /// The game data is portable: pair it with the engine runtime for the
    /// target. No source code needed.
    Retarget,
    /// Possible for some builds only (e.g. Unity Mono, not IL2CPP).
    Partial,
    /// The game data runs on an open-source reimplementation or port of the
    /// engine (ScummVM, GemRB, source ports...).
    Runtime,
    /// Native code: a real port needs the source. A compatibility layer
    /// (running the Windows build through Wine on macOS) is the alternative.
    NeedsSource,
    /// Already a build for this platform.
    Native,
    /// Not something to port (an editor, a server, a hosted platform).
    NotApplicable,
    /// Listed, but no reliable way to recognise its builds is known.
    Unresolved,
}

impl RouteKind {
    pub fn label(self) -> &'static str {
        match self {
            RouteKind::Retarget => "No source code needed",
            RouteKind::Partial => "Depends on the build",
            RouteKind::Runtime => "Runs on an open-source runtime",
            RouteKind::NeedsSource => "Needs source code (or a compatibility layer)",
            RouteKind::Native => "Already native",
            RouteKind::NotApplicable => "Nothing to port",
            RouteKind::Unresolved => "Not recognised yet",
        }
    }

    fn from_status(status: &str) -> RouteKind {
        match status {
            "supported" | "web" => RouteKind::Retarget,
            "partial" => RouteKind::Partial,
            "runtime" => RouteKind::Runtime,
            "native" => RouteKind::Native,
            "not_engine" => RouteKind::NotApplicable,
            "unresolved" => RouteKind::Unresolved,
            _ => RouteKind::NeedsSource,
        }
    }
}

/// The route for one engine.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct Route {
    pub engine: &'static str,
    pub name: &'static str,
    pub kind: RouteKind,
    pub how: &'static str,
    /// The catalog status (`supported`, `web`, `runtime`, `closed`...).
    pub status: &'static str,
    /// The open-source runtime, for [`RouteKind::Runtime`].
    pub runtime: Option<&'static str>,
    pub runtime_url: Option<&'static str>,
    /// The marketplace plugin that exports it, if there is one.
    pub plugin: Option<&'static str>,
    /// For engines detected as another one (Phaser as HTML5...).
    pub via: Option<&'static str>,
}

/// The catalog file, embedded at build time.
pub const CATALOG_JSON: &str = include_str!("../../../engine/ptk_engine/data/engines.json");

#[derive(Deserialize)]
struct CatalogFile {
    engines: Vec<CatalogEntry>,
}

#[derive(Deserialize)]
struct CatalogEntry {
    id: String,
    name: String,
    status: String,
    how: String,
    #[serde(default)]
    runtime: Option<CatalogRuntime>,
    #[serde(default)]
    plugin: Option<String>,
    #[serde(default)]
    via: Option<String>,
}

#[derive(Deserialize)]
struct CatalogRuntime {
    name: String,
    url: String,
}

fn leak(s: String) -> &'static str {
    Box::leak(s.into_boxed_str())
}

/// Every engine in the catalog, in catalog order (parsed once).
pub fn routes() -> &'static [Route] {
    static ROUTES: OnceLock<Vec<Route>> = OnceLock::new();
    ROUTES.get_or_init(|| {
        let file: CatalogFile =
            serde_json::from_str(CATALOG_JSON).expect("engines.json is checked by tests");
        file.engines
            .into_iter()
            .map(|e| Route {
                kind: RouteKind::from_status(&e.status),
                engine: leak(e.id),
                name: leak(e.name),
                how: leak(e.how),
                status: leak(e.status),
                runtime: e.runtime.as_ref().map(|r| leak(r.name.clone())),
                runtime_url: e.runtime.map(|r| leak(r.url)),
                plugin: e.plugin.map(leak),
                via: e.via.map(leak),
            })
            .collect()
    })
}

/// The route for a detected engine id.
pub fn route_for(engine: Option<&str>) -> Option<&'static Route> {
    let id = engine?;
    routes().iter().find(|r| r.engine == id)
}

/// Engines whose route is shown in "Without source code?" lists: the ones
/// Porting Toolkit or a marketplace plugin can export.
pub fn exportable() -> impl Iterator<Item = &'static Route> {
    routes().iter().filter(|r| {
        r.via.is_none()
            && r.engine != "macos"
            && (matches!(r.kind, RouteKind::Retarget | RouteKind::Partial) || r.plugin.is_some())
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_route_has_text() {
        assert_eq!(route_for(Some("godot")).unwrap().kind, RouteKind::Retarget);
        assert!(route_for(Some("nope")).is_none());
        assert!(route_for(None).is_none());
        assert!(
            routes()
                .iter()
                .all(|r| !r.how.is_empty() && !r.name.is_empty())
        );
    }

    #[test]
    fn catalog_routes() {
        assert!(routes().len() > 150);
        let scumm = route_for(Some("scumm")).unwrap();
        assert_eq!(scumm.kind, RouteKind::Runtime);
        assert_eq!(scumm.plugin, Some("scummvm-export"));
        assert_eq!(scumm.runtime, Some("ScummVM"));
        assert_eq!(
            route_for(Some("frostbite")).unwrap().kind,
            RouteKind::NeedsSource
        );
        assert_eq!(route_for(Some("q")).unwrap().kind, RouteKind::Unresolved);
        assert_eq!(route_for(Some("phaser")).unwrap().via, Some("html5"));
        assert_eq!(route_for(Some("unity")).unwrap().kind, RouteKind::Partial);
        assert!(exportable().any(|r| r.engine == "doom"));
        assert!(!exportable().any(|r| r.engine == "frostbite" || r.engine == "phaser"));
    }
}
