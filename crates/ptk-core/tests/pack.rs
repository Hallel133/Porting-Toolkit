//! Pack, unpack and verify, including the third-party refusal rules.

use std::fs;
use std::io::Write;
use std::path::Path;

use ptk_core::hash::{hash_tree, sha256_bytes};
use ptk_core::manifest::{Manifest, OWN_WORK_LICENSE};
use ptk_core::pack::{
    CHECKSUMS_FILE, PackError, PackOptions, PluginsLock, ThirdPartyReason, pack,
    read_pack_manifest, unpack,
};
use ptk_core::targets::TargetRegistry;
use ptk_core::verify::verify_game;

fn write(path: &Path, bytes: &[u8]) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, bytes).unwrap();
}

/// A fake game folder and a project that recorded its hashes.
fn setup(root: &Path) -> (std::path::PathBuf, std::path::PathBuf) {
    let game = root.join("game");
    write(&game.join("Demo.exe"), b"MZ fake exe");
    write(&game.join("Demo_Data/globalgamemanagers"), b"unity data");
    write(&game.join("Demo_Data/sharedassets0.assets"), b"textures");

    let project = root.join("project");
    fs::create_dir_all(&project).unwrap();
    let mut m = Manifest::new("demo-port");
    m.game.title = Some("Demo".into());
    m.targets = vec!["macos-arm64".into()];
    m.source_files = hash_tree(&game).unwrap();
    m.save(&project).unwrap();
    write(
        &project.join("recipes/textures.toml"),
        b"[textures]\naction = \"recompress\"\n",
    );
    write(
        &project.join("patches/0001-fix-input.patch"),
        b"--- a\n+++ b\n",
    );
    write(&project.join("README.md"), b"# Demo port\n");
    write(
        &project.join("plugins.lock"),
        PluginsLock::TEMPLATE.as_bytes(),
    );
    // Files outside the pack layout are never packed.
    write(&project.join("notes/private.txt"), b"not packed");
    write(&project.join("recipes/.DS_Store"), b"junk");
    (game, project)
}

fn reg() -> TargetRegistry {
    TargetRegistry::builtin()
}

#[test]
fn pack_unpack_verify_round_trip() {
    let tmp = tempfile::tempdir().unwrap();
    let (game, project) = setup(tmp.path());
    let out = tmp.path().join("demo.ptk");

    let report = pack(&project, &out, &reg(), PackOptions::default()).unwrap();
    let paths: Vec<_> = report.entries.iter().map(|e| e.path.as_str()).collect();
    assert_eq!(
        paths,
        [
            "ptk.json",
            "README.md",
            "patches/0001-fix-input.patch",
            "plugins.lock",
            "recipes/textures.toml"
        ]
    );
    assert!(report.licensed_third_party.is_empty());

    // Packs are reproducible.
    let again = tmp.path().join("again.ptk");
    pack(&project, &again, &reg(), PackOptions::default()).unwrap();
    assert_eq!(fs::read(&out).unwrap(), fs::read(&again).unwrap());

    // An existing output is only replaced on request.
    assert!(matches!(
        pack(&project, &out, &reg(), PackOptions::default()),
        Err(PackError::Exists(_))
    ));
    pack(&project, &out, &reg(), PackOptions { overwrite: true }).unwrap();

    assert_eq!(read_pack_manifest(&out).unwrap().name, "demo-port");

    let dest = tmp.path().join("unpacked");
    let unpacked = unpack(&out, &dest).unwrap();
    assert_eq!(unpacked.entries.len(), 5);
    assert_eq!(
        fs::read(dest.join("patches/0001-fix-input.patch")).unwrap(),
        b"--- a\n+++ b\n"
    );
    assert!(!dest.join("notes").exists());
    assert!(!dest.join(CHECKSUMS_FILE).exists());
    assert_eq!(
        Manifest::load(&dest).unwrap(),
        Manifest::load(&project).unwrap()
    );

    // Unpacking over a non-empty folder is refused.
    assert!(matches!(unpack(&out, &dest), Err(PackError::Exists(_))));

    // The user's own copy matches the recorded hashes...
    let ok = verify_game(&game, &unpacked.manifest, |_, _| {}).unwrap();
    assert!(ok.is_ok(), "{ok:?}");
    assert_eq!(ok.ok, 3);

    // ...and a modified or missing file is reported.
    fs::write(game.join("Demo_Data/globalgamemanagers"), b"other data").unwrap();
    fs::remove_file(game.join("Demo.exe")).unwrap();
    let bad = verify_game(&game, &unpacked.manifest, |_, _| {}).unwrap();
    assert!(!bad.is_ok());
    assert_eq!(bad.missing, ["Demo.exe"]);
    assert_eq!(bad.mismatched.len(), 1);
    assert_eq!(bad.mismatched[0].path, "Demo_Data/globalgamemanagers");
}

#[test]
fn pack_refuses_third_party_files() {
    let tmp = tempfile::tempdir().unwrap();
    let (game, project) = setup(tmp.path());
    let out = tmp.path().join("demo.ptk");

    // 1. A copy of an original game file, renamed, is detected by its hash.
    fs::create_dir_all(project.join("extra")).unwrap();
    fs::copy(game.join("Demo.exe"), project.join("extra/runtime.bin")).unwrap();
    // 2. An engine archive.
    write(&project.join("extra/textures.rpf"), b"archive");
    // 3. A file the author marked as third-party.
    write(&project.join("extra/vendor/font.ttf"), b"font");
    let mut m = Manifest::load(&project).unwrap();
    m.third_party = vec!["extra/vendor/".into()];
    m.save(&project).unwrap();

    let err = pack(&project, &out, &reg(), PackOptions::default()).unwrap_err();
    let PackError::ThirdParty(files) = &err else {
        panic!("expected a third-party refusal, got {err}");
    };
    let mut found: Vec<_> = files.iter().map(|f| (f.path.as_str(), &f.reason)).collect();
    found.sort_by_key(|(p, _)| *p);
    assert_eq!(found.len(), 3);
    assert_eq!(found[0].0, "extra/runtime.bin");
    assert_eq!(
        found[0].1,
        &ThirdPartyReason::CopyOfGameFile("Demo.exe".into())
    );
    assert_eq!(found[1].1, &ThirdPartyReason::GameArchive);
    assert_eq!(
        found[2].1,
        &ThirdPartyReason::Marked("extra/vendor/".into())
    );
    assert!(
        err.to_string()
            .contains("refusing to pack third-party content")
    );
    assert!(!out.exists());
    assert!(!tmp.path().join("demo.ptk.partial").exists());

    // A proprietary declaration does not help...
    m.game.content_license = Some("LicenseRef-Proprietary".into());
    m.save(&project).unwrap();
    assert!(matches!(
        pack(&project, &out, &reg(), PackOptions::default()),
        Err(PackError::ThirdParty(_))
    ));

    // ...but the author's own game, or an open-source one, may be packed.
    for license in [OWN_WORK_LICENSE, "GPL-3.0-or-later"] {
        m.game.content_license = Some(license.into());
        m.save(&project).unwrap();
        let report = pack(&project, &out, &reg(), PackOptions { overwrite: true }).unwrap();
        assert_eq!(report.licensed_third_party.len(), 3);
    }
}

#[test]
fn pack_requires_a_valid_manifest() {
    let tmp = tempfile::tempdir().unwrap();
    let (_, project) = setup(tmp.path());
    let mut m = Manifest::load(&project).unwrap();
    m.options.get_mut("textures").unwrap().quality = Some(101);
    m.save(&project).unwrap();
    let err = pack(
        &project,
        &tmp.path().join("x.ptk"),
        &reg(),
        PackOptions::default(),
    )
    .unwrap_err();
    assert!(err.to_string().contains("quality must be 1-100"), "{err}");

    let empty = tmp.path().join("empty");
    fs::create_dir_all(&empty).unwrap();
    assert!(
        pack(
            &empty,
            &tmp.path().join("y.ptk"),
            &reg(),
            PackOptions::default()
        )
        .is_err()
    );
}

fn zip_with(path: &Path, entries: &[(&str, &[u8])], sums: Option<String>) {
    let file = fs::File::create(path).unwrap();
    let mut zip = zip::ZipWriter::new(file);
    let opts = zip::write::SimpleFileOptions::default();
    for (name, data) in entries {
        zip.start_file(*name, opts).unwrap();
        zip.write_all(data).unwrap();
    }
    if let Some(s) = sums {
        zip.start_file(CHECKSUMS_FILE, opts).unwrap();
        zip.write_all(s.as_bytes()).unwrap();
    }
    zip.finish().unwrap();
}

#[test]
fn unpack_rejects_damaged_or_hostile_packs() {
    let tmp = tempfile::tempdir().unwrap();
    let manifest = Manifest::new("x").to_json();
    let good_sum = format!("{}  ptk.json\n", sha256_bytes(manifest.as_bytes()));

    type Case<'a> = (&'a str, Vec<(&'a str, &'a [u8])>, Option<String>, &'a str);
    let cases: Vec<Case> = vec![
        (
            "no-sums",
            vec![("ptk.json", manifest.as_bytes())],
            None,
            "no SHA256SUMS",
        ),
        (
            "tampered",
            vec![("ptk.json", manifest.as_bytes())],
            Some(format!("{}  ptk.json\n", sha256_bytes(b"other"))),
            "checksum mismatch",
        ),
        (
            "zip-slip",
            vec![("ptk.json", manifest.as_bytes()), ("../evil.sh", b"x")],
            Some(good_sum.clone()),
            "unexpected entry",
        ),
        (
            "outside-layout",
            vec![("ptk.json", manifest.as_bytes()), ("game/GTA5.exe", b"x")],
            Some(good_sum.clone()),
            "unexpected entry",
        ),
        (
            "unlisted",
            vec![("ptk.json", manifest.as_bytes()), ("recipes/a.toml", b"x")],
            Some(good_sum.clone()),
            "not listed",
        ),
    ];
    for (name, entries, sums, expected) in cases {
        let pack_path = tmp.path().join(format!("{name}.ptk"));
        zip_with(&pack_path, &entries, sums);
        let dest = tmp.path().join(format!("out-{name}"));
        let err = unpack(&pack_path, &dest).unwrap_err().to_string();
        assert!(err.contains(expected), "{name}: {err}");
        assert!(!dest.exists(), "{name}: destination must not be created");
        assert!(!tmp.path().join("evil.sh").exists());
    }
    // Nothing half-extracted is left behind.
    let leftovers: Vec<_> = fs::read_dir(tmp.path())
        .unwrap()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_name().to_string_lossy().contains("unpacking"))
        .collect();
    assert!(leftovers.is_empty());
}

#[test]
fn plugins_lock_is_validated() {
    assert!(
        PluginsLock::parse(PluginsLock::TEMPLATE)
            .unwrap()
            .plugin
            .is_empty()
    );
    let lock =
        PluginsLock::parse("version = 1\n[[plugin]]\nid = \"unity-import\"\nversion = \"0.1.0\"\n")
            .unwrap();
    assert_eq!(lock.plugin[0].id, "unity-import");
    assert!(PluginsLock::parse("version = 2\n").is_err());
    assert!(
        PluginsLock::parse("version = 1\n[[plugin]]\nid = \"Bad Id\"\nversion = \"1\"\n").is_err()
    );
}
