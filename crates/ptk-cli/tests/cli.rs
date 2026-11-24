//! End-to-end tests of the `ptk` binary.
//!
//! Every test uses its own PTK_HOME, so the real data folder is never
//! touched, and the engine runs from the source checkout.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use serde_json::Value;

const BANNER: &str = include_str!("snapshots/banner.txt");

fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/games")
}

struct Env {
    home: tempfile::TempDir,
    work: tempfile::TempDir,
}

impl Env {
    fn new() -> Self {
        Env {
            home: tempfile::tempdir().unwrap(),
            work: tempfile::tempdir().unwrap(),
        }
    }

    fn cmd(&self, args: &[&str]) -> Command {
        let mut c = Command::new(env!("CARGO_BIN_EXE_ptk"));
        c.args(args)
            .current_dir(self.work.path())
            .env("PTK_HOME", self.home.path())
            .env_remove("NO_COLOR")
            .env_remove("FORCE_COLOR")
            .env_remove("CLICOLOR_FORCE");
        c
    }

    fn run(&self, args: &[&str]) -> Output {
        self.cmd(args).output().unwrap()
    }

    fn ok(&self, args: &[&str]) -> String {
        let out = self.run(args);
        assert!(
            out.status.success(),
            "ptk {args:?} failed:\n{}\n{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        );
        String::from_utf8(out.stdout).unwrap()
    }

    fn path(&self, rel: &str) -> PathBuf {
        self.work.path().join(rel)
    }
}

#[test]
fn bare_ptk_prints_the_banner_and_help_without_color_in_a_pipe() {
    let env = Env::new();
    let out = env.ok(&[]);
    assert!(out.starts_with(BANNER), "{out}");
    assert!(out.contains("Usage: ptk"));
    assert!(out.contains("scan"));
    assert!(
        !out.contains('\x1b'),
        "no color when stdout is not a terminal"
    );

    let forced = env.cmd(&[]).env("FORCE_COLOR", "1").output().unwrap();
    assert!(String::from_utf8_lossy(&forced.stdout).contains("\x1b[38;5;"));

    let out = env
        .cmd(&[])
        .env("FORCE_COLOR", "1")
        .env("NO_COLOR", "1")
        .output()
        .unwrap();
    assert!(
        !String::from_utf8_lossy(&out.stdout).contains('\x1b'),
        "NO_COLOR wins over FORCE_COLOR"
    );
    let out = env
        .cmd(&["--no-color", "about"])
        .env("FORCE_COLOR", "1")
        .output()
        .unwrap();
    assert!(!String::from_utf8_lossy(&out.stdout).contains('\x1b'));

    let about = env.ok(&["about"]);
    assert!(about.contains("Roadmap") && about.contains("Phase 2: Desktop GUI"));
}

#[test]
fn project_lifecycle() {
    let env = Env::new();
    // A small fake game with real contents.
    let game = env.path("game");
    fs::create_dir_all(game.join("Demo_Data")).unwrap();
    fs::write(game.join("UnityPlayer.dll"), b"MZ unity player").unwrap();
    fs::write(game.join("Demo_Data/globalgamemanagers"), b"managers").unwrap();
    fs::write(game.join("Demo_Data/music.ogg"), vec![7u8; 4096]).unwrap();

    let out = env.ok(&[
        "init",
        "demo",
        "--title",
        "Demo Game",
        "--target",
        "macos-arm64",
    ]);
    assert!(out.contains("Created project demo"));
    assert!(env.path("demo/ptk.json").is_file());
    assert!(env.path("demo/plugins.lock").is_file());
    assert!(!env.run(&["init", "demo"]).status.success(), "init twice");

    let scan: Value = serde_json::from_str(&env.ok(&["scan", "game", "--json"])).unwrap();
    assert_eq!(scan["primary"]["id"], "unity");
    assert_eq!(scan["file_count"], 3);

    let table = env.ok(&["scan", "game", "--record", "demo"]);
    assert!(
        table.contains("Unity") && table.contains("audio"),
        "{table}"
    );
    assert!(table.contains("Recorded 3 file hashes"));

    env.ok(&[
        "options",
        "set",
        "textures",
        "--action",
        "recompress",
        "--quality",
        "80",
        "-p",
        "demo",
    ]);
    assert!(
        !env.run(&["options", "set", "textures", "--quality", "0", "-p", "demo"])
            .status
            .success()
    );
    assert!(
        !env.run(&["options", "set", "nonsense", "-p", "demo"])
            .status
            .success()
    );

    let info: Value = serde_json::from_str(&env.ok(&["info", "demo", "--json"])).unwrap();
    assert_eq!(info["game"]["engine"], "unity");
    assert_eq!(info["options"]["textures"]["action"], "recompress");
    assert_eq!(info["options"]["textures"]["quality"], 80);
    assert_eq!(info["source_files"].as_array().unwrap().len(), 3);
    let text = env.ok(&["info", "demo"]);
    assert!(text.contains("Advanced options") && text.contains("recompress"));

    let packed = env.ok(&["pack", "demo"]);
    assert!(packed.contains("demo-0.1.0.ptk"), "{packed}");
    assert!(env.path("demo-0.1.0.ptk").is_file());
    assert!(
        !env.run(&["pack", "demo"]).status.success(),
        "no silent overwrite"
    );
    env.ok(&["pack", "demo", "--force"]);

    let info_from_pack = env.ok(&["info", "demo-0.1.0.ptk"]);
    assert!(info_from_pack.contains("Demo Game"));

    env.ok(&["unpack", "demo-0.1.0.ptk", "-o", "copy"]);
    assert!(env.path("copy/ptk.json").is_file());

    let verify = env.ok(&["verify", "game", "-p", "copy"]);
    assert!(verify.contains("all 3 files match"), "{verify}");
    let report: Value =
        serde_json::from_str(&env.ok(&["verify", "game", "-p", "demo-0.1.0.ptk", "--json"]))
            .unwrap();
    assert_eq!(report["ok"], 3);

    fs::write(game.join("Demo_Data/music.ogg"), b"modded").unwrap();
    let bad = env.run(&["verify", "game", "-p", "copy"]);
    assert_eq!(bad.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&bad.stdout).contains("Demo_Data/music.ogg"));
}

#[test]
fn pack_refuses_game_files() {
    let env = Env::new();
    let game = env.path("game");
    fs::create_dir_all(&game).unwrap();
    fs::write(game.join("level.dat"), b"original level data").unwrap();
    env.ok(&["init", "p"]);
    env.ok(&["scan", "game", "--record", "p"]);
    fs::create_dir_all(env.path("p/extra")).unwrap();
    fs::copy(game.join("level.dat"), env.path("p/extra/renamed.bin")).unwrap();

    let out = env.run(&["pack", "p"]);
    assert!(!out.status.success());
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(
        err.contains("refusing to pack third-party content"),
        "{err}"
    );
    assert!(
        err.contains("identical to the game file level.dat"),
        "{err}"
    );
    assert!(!env.path("p-0.1.0.ptk").exists());
}

#[test]
fn scan_fixtures_detects_every_engine() {
    let env = Env::new();
    for (folder, engine) in [
        ("unity-windows", "unity"),
        ("unreal", "unreal"),
        ("godot", "godot"),
        ("rage", "rage"),
        ("source", "source"),
        ("gamemaker", "gamemaker"),
        ("renpy", "renpy"),
        ("rpgmaker-vxace", "rgss"),
        ("macos-app", "macos"),
    ] {
        let path = fixtures().join(folder);
        let out: Value =
            serde_json::from_str(&env.ok(&["scan", path.to_str().unwrap(), "--json"])).unwrap();
        assert_eq!(out["primary"]["id"], engine, "{folder}");
    }
    assert!(!env.run(&["scan", "/no/such/folder"]).status.success());
}

#[test]
fn targets_include_external_definitions() {
    let env = Env::new();
    fs::create_dir_all(env.home.path().join("targets")).unwrap();
    fs::write(
        env.home.path().join("targets/nova.toml"),
        "[[target]]\nid = \"nova-example\"\nname = \"NoVA example\"\nos = \"nova\"\narch = \"riscv64\"\nstatus = \"experimental\"\nexport_backend = \"nova-export\"\n",
    )
    .unwrap();
    let targets: Value = serde_json::from_str(&env.ok(&["targets", "--json"])).unwrap();
    let ids: Vec<_> = targets
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["id"].as_str().unwrap())
        .collect();
    assert_eq!(
        ids,
        [
            "macos-arm64",
            "macos-x64",
            "web",
            "windows-x64",
            "linux-x64",
            "android",
            "nova-example"
        ]
    );
    assert!(env.ok(&["targets"]).contains("experimental"));
    env.ok(&["init", "n", "--target", "nova-example"]);
}

#[test]
fn plugin_workflow() {
    let env = Env::new();
    env.ok(&["plugin", "new", "tex-export", "--kind", "export"]);
    let v: Value =
        serde_json::from_str(&env.ok(&["plugin", "validate", "tex-export", "--json"])).unwrap();
    assert_eq!(v["valid"], true);

    // Without a terminal, installing needs --yes.
    let refused = env.run(&["plugin", "install", "tex-export"]);
    assert!(!refused.status.success());
    assert!(String::from_utf8_lossy(&refused.stdout).contains("Permissions"));

    env.ok(&["plugin", "install", "tex-export", "--yes"]);
    assert!(
        env.home
            .path()
            .join("plugins/tex-export/plugin.toml")
            .is_file()
    );
    let list: Value = serde_json::from_str(&env.ok(&["plugin", "list", "--json"])).unwrap();
    assert_eq!(list[0]["id"], "tex-export");
    assert_eq!(list[0]["reviewed"], false);

    fs::write(env.path("tex-export/plugin.toml"), "schema_version = 1\n").unwrap();
    assert_eq!(
        env.run(&["plugin", "validate", "tex-export"]).status.code(),
        Some(1)
    );
}

#[test]
fn doctor_runs_without_installing() {
    let env = Env::new();
    let out = env.run(&["doctor", "--no-install"]);
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(text.contains("Engine source"), "{text}");
    assert!(text.contains("Managed environment"));
    assert!(!env.home.path().join("engine-env").exists());
}
