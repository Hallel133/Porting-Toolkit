//! What it takes to port a game of each engine, with and without source code.
//!
//! Porting Toolkit works on builds you own (folders, `.app`, `.exe`, `.deb`
//! …). Whether a build can move to another platform without source code
//! depends on where the game logic lives: in platform-independent data
//! (a Godot `.pck`, Ren'Py scripts, RPG Maker JavaScript) or in native
//! machine code (Unreal, RAGE, Source, Unity IL2CPP).

use serde::Serialize;

/// How a game can reach another platform.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RouteKind {
    /// The game data is portable: pair it with the engine runtime for the
    /// target. No source code needed.
    Retarget,
    /// Possible for some builds only (e.g. Unity Mono, not IL2CPP).
    Partial,
    /// Native code: a real port needs the source. A compatibility layer
    /// (running the Windows build through Wine on macOS) is the alternative.
    NeedsSource,
    /// Already a build for this platform.
    Native,
}

impl RouteKind {
    pub fn label(self) -> &'static str {
        match self {
            RouteKind::Retarget => "No source code needed",
            RouteKind::Partial => "Depends on the build",
            RouteKind::NeedsSource => "Needs source code (or a compatibility layer)",
            RouteKind::Native => "Already native",
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
}

pub const ROUTES: &[Route] = &[
    Route {
        engine: "godot",
        name: "Godot",
        kind: RouteKind::Retarget,
        how: "Scripts and assets live in the .pck (or inside the .exe). The same .pck runs with the Godot export template of the same version for the target.",
    },
    Route {
        engine: "renpy",
        name: "Ren'Py",
        kind: RouteKind::Retarget,
        how: "The game/ folder (.rpa, .rpyc) runs on any platform with the Ren'Py runtime of the same version.",
    },
    Route {
        engine: "rpgmaker",
        name: "RPG Maker",
        kind: RouteKind::Retarget,
        how: "MV/MZ games are HTML5 + JavaScript: wrap www/ with a runtime for the target or play it in a browser. XP/VX/VX Ace need a compatible RGSS runtime.",
    },
    Route {
        engine: "gamemaker",
        name: "GameMaker",
        kind: RouteKind::Partial,
        how: "data.win holds the game; it needs a GameMaker runner for the target. Compatibility varies by version.",
    },
    Route {
        engine: "unity",
        name: "Unity",
        kind: RouteKind::Partial,
        how: "Mono builds keep game code in Managed/*.dll and can pair with the Unity player of the same version for the target. IL2CPP builds are native code.",
    },
    Route {
        engine: "unreal",
        name: "Unreal Engine",
        kind: RouteKind::NeedsSource,
        how: "The game code is compiled into the executable. A native port needs the project source; otherwise run the Windows build through a compatibility layer.",
    },
    Route {
        engine: "rage",
        name: "RAGE",
        kind: RouteKind::NeedsSource,
        how: "Native engine code. Without source, only a compatibility layer (or a hosted/streamed build) can run it elsewhere.",
    },
    Route {
        engine: "source",
        name: "Source",
        kind: RouteKind::NeedsSource,
        how: "Native engine code. Without source, only a compatibility layer can run it elsewhere.",
    },
    Route {
        engine: "macos",
        name: "macOS app",
        kind: RouteKind::Native,
        how: "Already a macOS build. Porting Toolkit can still apply advanced options and community patches.",
    },
];

/// The route for a detected engine id.
pub fn route_for(engine: Option<&str>) -> Option<&'static Route> {
    let id = engine?;
    ROUTES.iter().find(|r| r.engine == id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_route_has_text() {
        assert_eq!(route_for(Some("godot")).unwrap().kind, RouteKind::Retarget);
        assert!(route_for(Some("nope")).is_none());
        assert!(route_for(None).is_none());
        assert!(ROUTES.iter().all(|r| !r.how.is_empty()));
    }
}
