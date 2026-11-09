//! Importing builds, SDKs and the marketplace, end to end with the engine.
//! Every test points PTK_HOME at its own folder (one test binary, so the
//! tests share a lock around the environment variable).

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Mutex;

use ptk_core::engine::{EngineClient, EngineCommand, EngineOrigin, engine_source_dir, which};
use ptk_core::import::{BuildKind, prepare};
use ptk_core::market::{self, EntryKind, MarketIndex};
use ptk_core::sdk;
use ptk_core::targets::TargetRegistry;

static ENV: Mutex<()> = Mutex::new(());

fn engine() -> EngineClient {
    let python = std::env::var_os("PTK_PYTHON")
        .map(PathBuf::from)
        .or_else(|| which("python3"))
        .expect("python3");
    EngineClient::spawn(EngineCommand::new(
        python,
        EngineOrigin::SystemPython,
        engine_source_dir(),
    ))
    .unwrap()
}

fn with_home<T>(f: impl FnOnce(&Path) -> T) -> T {
    let _guard = ENV.lock().unwrap_or_else(|e| e.into_inner());
    let home = tempfile::tempdir().unwrap();
    // SAFETY: tests touching the environment hold the ENV lock.
    unsafe { std::env::set_var("PTK_HOME", home.path()) };
    let out = f(home.path());
    unsafe { std::env::remove_var("PTK_HOME") };
    out
}

fn godot_exe(path: &Path) {
    let mut bytes = b"MZ".to_vec();
    bytes.extend([0u8; 64]);
    bytes.extend(b"packdata");
    bytes.extend(8u64.to_le_bytes());
    bytes.extend(b"GDPC");
    fs::write(path, bytes).unwrap();
}

#[test]
fn import_single_exe_and_scan_it() {
    with_home(|_| {
        let dir = tempfile::tempdir().unwrap();
        let exe = dir.path().join("MyGame.exe");
        godot_exe(&exe);
        let p = prepare(&exe).unwrap();
        assert_eq!(p.kind, BuildKind::Executable);
        assert_eq!(p.title, "MyGame");
        let mut e = engine();
        let (scan, _) = ptk_core::scan::scan(&mut e, &p.game, &mut |_| {}).unwrap();
        assert!(scan.single_file);
        assert_eq!(scan.primary.as_ref().unwrap().id, "godot");

        // Record into a project, then verify the same exe.
        let project = dir.path().join("port");
        ptk_core::project::init(&project, Default::default(), &TargetRegistry::builtin()).unwrap();
        let r = ptk_core::scan::record(&project, &p.game, &scan).unwrap();
        assert_eq!(r.files, 1);
        let m = ptk_core::manifest::Manifest::load(&project).unwrap();
        assert_eq!(m.source_files[0].path, "MyGame.exe");
        assert_eq!(m.game.title.as_deref(), Some("MyGame"));
        assert!(
            ptk_core::verify::verify_game(&exe, &m, |_, _| {})
                .unwrap()
                .is_ok()
        );
    });
}

#[test]
fn import_zip_and_deb() {
    with_home(|home| {
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("src");
        fs::create_dir_all(src.join("opt/mygame")).unwrap();
        godot_exe(&src.join("opt/mygame/mygame.x86_64"));

        let zip = dir.path().join("MyGame-win.zip");
        market::zip_dir(&src, &zip, "MyGame").unwrap();
        let p = prepare(&zip).unwrap();
        assert_eq!(p.kind, BuildKind::Zip);
        assert!(p.game.starts_with(home.join("imports")));
        assert!(p.game.join("MyGame/opt/mygame/mygame.x86_64").is_file());
        // Unpacked once, reused afterwards.
        assert_eq!(prepare(&zip).unwrap().game, p.game);

        // A .deb: an ar archive (GNU names) with debian-binary, control and data.
        let debdir = dir.path().join("deb");
        fs::create_dir_all(&debdir).unwrap();
        let ok = Command::new("tar")
            .args(["-cJf"])
            .arg(debdir.join("data.tar.xz"))
            .arg("-C")
            .arg(&src)
            .arg("opt")
            .status()
            .unwrap()
            .success();
        assert!(ok, "tar");
        let mut deb_bytes = b"!<arch>\n".to_vec();
        for (name, data) in [
            ("debian-binary/", b"2.0\n".to_vec()),
            ("control.tar.gz/", b"not used".to_vec()),
            (
                "data.tar.xz/",
                fs::read(debdir.join("data.tar.xz")).unwrap(),
            ),
        ] {
            let header = format!(
                "{name:<16}{:<12}{:<6}{:<6}{:<8}{:<10}`\n",
                0,
                0,
                0,
                "100644",
                data.len()
            );
            assert_eq!(header.len(), 60);
            deb_bytes.extend(header.as_bytes());
            deb_bytes.extend(&data);
            if data.len() % 2 == 1 {
                deb_bytes.push(b'\n');
            }
        }
        fs::write(debdir.join("mygame_1.0_amd64.deb"), deb_bytes).unwrap();
        let deb = debdir.join("mygame_1.0_amd64.deb");
        let p = prepare(&deb).unwrap();
        assert_eq!(p.kind, BuildKind::Deb);
        assert_eq!(p.title, "mygame");
        assert!(p.game.join("opt/mygame/mygame.x86_64").is_file());
        assert!(!p.game.join(".deb").exists());
        let mut e = engine();
        let (scan, _) = ptk_core::scan::scan(&mut e, &p.game, &mut |_| {}).unwrap();
        assert_eq!(scan.primary.unwrap().id, "godot");
    });
}

#[cfg(target_os = "macos")]
#[test]
fn import_dmg() {
    with_home(|_| {
        let dir = tempfile::tempdir().unwrap();
        let app = dir.path().join("vol/Demo.app/Contents/MacOS");
        fs::create_dir_all(&app).unwrap();
        fs::write(app.join("Demo"), [0xcf, 0xfa, 0xed, 0xfe]).unwrap();
        fs::write(
            dir.path().join("vol/Demo.app/Contents/Info.plist"),
            "<plist/>",
        )
        .unwrap();
        let dmg = dir.path().join("Demo.dmg");
        let ok = Command::new("hdiutil")
            .args(["create", "-quiet", "-fs", "HFS+", "-srcfolder"])
            .arg(dir.path().join("vol"))
            .arg(&dmg)
            .status()
            .map(|s| s.success())
            .unwrap_or(false);
        if !ok {
            eprintln!("skipping .dmg: hdiutil create failed");
            return;
        }
        let p = prepare(&dmg).unwrap();
        assert_eq!(p.kind, BuildKind::Dmg);
        assert!(p.game.join("volume/Demo.app/Contents/Info.plist").is_file());
    });
}

#[test]
fn sdk_create_validate_install() {
    with_home(|home| {
        let dir = tempfile::tempdir().unwrap();
        let sdk_dir = dir.path().join("nova-sdk");
        let mut e = engine();
        let files = sdk::create(&sdk_dir, "nova", &mut e).unwrap();
        assert!(files.contains(&"ptk-sdk.toml".to_owned()));
        let report = sdk::validate(&sdk_dir, &mut e).unwrap();
        assert!(report.valid, "{:?}", report.errors);
        assert!(report.warnings.is_empty(), "{:?}", report.warnings);
        assert_eq!(report.targets[0].id, "nova-device");
        assert_eq!(report.plugins.len(), 1);

        let installed = sdk::install(&report).unwrap();
        assert_eq!(installed.targets.len(), 1);
        let (reg, errors) = TargetRegistry::load(&home.join("targets"));
        assert!(errors.is_empty());
        assert!(reg.get("nova-device").is_some());
        assert!(home.join("plugins/nova-export/plugin.toml").is_file());
        assert_eq!(sdk::installed()[0].id, "nova");

        // A target clashing with a built-in one is an error.
        fs::write(
            sdk_dir.join("targets/bad.toml"),
            "[[target]]\nid = \"web\"\nname = \"x\"\nos = \"x\"\narch = \"x\"\nstatus = \"planned\"\n",
        )
        .unwrap();
        assert!(!sdk::validate(&sdk_dir, &mut e).unwrap().valid);
    });
}

#[test]
fn publish_then_install_from_a_local_index() {
    with_home(|home| {
        let dir = tempfile::tempdir().unwrap();
        let mut e = engine();
        let sdk_dir = dir.path().join("nova-sdk");
        sdk::create(&sdk_dir, "nova", &mut e).unwrap();

        let out = dir.path().join("dist");
        let published = market::package(&sdk_dir, &out, &mut e).unwrap();
        assert_eq!(published.kind, EntryKind::Sdk);
        assert!(published.archive.is_file());
        assert_eq!(published.entry.targets, ["nova-device"]);
        assert_eq!(published.entry.download, market::DOWNLOAD_PLACEHOLDER);

        let plugin =
            ptk_core::plugins::create(&mut e, &dir.path().join("tex-import"), "import", None)
                .unwrap();
        let p2 = market::package(Path::new(&plugin.path), &out, &mut e).unwrap();
        assert_eq!(p2.kind, EntryKind::Plugin);
        assert_eq!(p2.entry.engines, ["unity"]);

        // The maintainer fills in download URLs and merges.
        let mut entries = vec![published.entry.clone(), p2.entry.clone()];
        entries[0].download = format!("file://{}", published.archive.display());
        entries[1].download = format!("file://{}", p2.archive.display());
        let index_path = dir.path().join("index.json");
        fs::write(
            &index_path,
            serde_json::to_vec(&MarketIndex {
                schema_version: 1,
                entries,
            })
            .unwrap(),
        )
        .unwrap();
        let (index, problems) =
            market::fetch_index(&format!("file://{}", index_path.display())).unwrap();
        assert!(problems.is_empty());
        assert_eq!(index.entries.len(), 2);

        let folder = market::download(&index.entries[0]).unwrap();
        assert!(folder.join("ptk-sdk.toml").is_file());
        let report = sdk::validate(&folder, &mut e).unwrap();
        sdk::install(&report).unwrap();
        assert!(home.join("plugins/nova-export").is_dir());

        // A tampered download is refused.
        let mut bad = index.entries[1].clone();
        bad.sha256 = ptk_core::hash::sha256_bytes(b"other");
        assert!(
            market::download(&bad)
                .unwrap_err()
                .to_string()
                .contains("SHA-256")
        );
    });
}
