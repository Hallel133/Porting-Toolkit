//! Rust <-> Python: the real engine as a subprocess over JSON-RPC.
//!
//! Uses `PTK_PYTHON` if set (CI points it at the engine's uv environment),
//! otherwise `python3` on PATH with the engine source on PYTHONPATH.

use std::path::{Path, PathBuf};

use ptk_core::category::category_ids;
use ptk_core::engine::{
    EngineClient, EngineCommand, EngineError, EngineOrigin, PROTOCOL_VERSION, codes,
    engine_source_dir, which,
};
use serde_json::{Value, json};

fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/games")
}

fn engine() -> EngineClient {
    let python = std::env::var_os("PTK_PYTHON")
        .map(PathBuf::from)
        .or_else(|| which("python3"))
        .expect("python3 (3.11+) is needed for the engine tests");
    let source = engine_source_dir().expect("engine/ source folder");
    EngineClient::spawn(EngineCommand::new(
        python,
        EngineOrigin::SystemPython,
        Some(source),
    ))
    .expect("engine starts")
}

#[test]
fn handshake_and_shared_categories() {
    let e = engine();
    assert_eq!(e.info.protocol, PROTOCOL_VERSION);
    assert_eq!(e.info.name, "ptk-engine");
    // The Python classifier and the manifest's options use the same ids.
    assert_eq!(e.info.categories, category_ids());
    for engine_id in [
        "unity",
        "unreal",
        "godot",
        "rage",
        "source",
        "gamemaker",
        "renpy",
        "rpgmaker",
        "rgss",
        "love",
        "html5",
        "macos",
    ] {
        assert!(e.info.engines.iter().any(|x| x == engine_id), "{engine_id}");
    }
    // Every format a built-in target prefers can be encoded (GPU texture
    // formats need an engine's own texture pipeline, so they are exempt).
    for t in ptk_core::targets::TargetRegistry::builtin().iter() {
        for cat in ["textures", "audio", "video"] {
            for f in t.media.for_category(cat) {
                if ptk_core::manifest::GPU_TEXTURES.contains(&f.as_str()) {
                    continue;
                }
                assert!(e.info.codecs.contains(f), "{}: no encoder for {f}", t.id);
            }
        }
    }
    e.shutdown().unwrap();
}

#[test]
fn tools_status_over_rpc() {
    let mut e = engine();
    let dir = tempfile::tempdir().unwrap();
    let status: Value = e
        .call("tools.status", json!({"tools_dir": dir.path()}))
        .unwrap();
    let ids: Vec<&str> = status["tools"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["id"].as_str().unwrap())
        .collect();
    for id in ["ffmpeg", "cwebp", "oxipng"] {
        assert!(ids.contains(&id), "{ids:?}");
    }
    let err = e
        .call(
            "tools.install",
            json!({"id": "nope", "tools_dir": dir.path()}),
        )
        .unwrap_err();
    assert!(err.to_string().contains("unknown tool"), "{err}");
    e.shutdown().unwrap();
}

#[test]
fn scan_over_rpc() {
    let mut e = engine();
    for (folder, expected) in [
        ("godot", "godot"),
        ("rage", "rage"),
        ("rpgmaker-mv", "rpgmaker"),
    ] {
        let path = fixtures().join(folder).canonicalize().unwrap();
        let r = e.call("scan", json!({ "path": path })).unwrap();
        assert_eq!(r["primary"]["id"], expected, "{folder}: {r}");
        assert_eq!(r["path"], path.to_string_lossy().as_ref());
    }
    e.shutdown().unwrap();
}

#[test]
fn progress_notifications_arrive_before_the_result() {
    let dir = tempfile::tempdir().unwrap();
    for i in 0..2001 {
        std::fs::write(dir.path().join(format!("f{i}.png")), b"").unwrap();
    }
    let mut e = engine();
    let mut notes = Vec::new();
    let r = e
        .call_with(
            "scan",
            json!({ "path": dir.path() }),
            &mut |m, p: &Value| notes.push((m.to_owned(), p.clone())),
        )
        .unwrap();
    assert_eq!(r["file_count"], 2001);
    assert_eq!(
        notes,
        [(
            "progress".to_owned(),
            json!({"phase": "scan", "files": 2000})
        )]
    );
}

#[test]
fn errors_map_to_codes_and_the_engine_keeps_serving() {
    let mut e = engine();
    let code = |r: Result<Value, EngineError>| match r {
        Err(EngineError::Rpc { code, .. }) => code,
        other => panic!("expected an RPC error, got {other:?}"),
    };
    assert_eq!(
        code(e.call("no.such.method", json!({}))),
        codes::METHOD_NOT_FOUND
    );
    assert_eq!(code(e.call("scan", json!({}))), codes::INVALID_PARAMS);
    assert_eq!(
        code(e.call("scan", json!({"path": "/no/such/folder"}))),
        codes::ENGINE_ERROR
    );
    // Still alive after errors.
    assert_eq!(
        e.call("engine.info", json!({})).unwrap()["protocol"],
        PROTOCOL_VERSION
    );
}

#[test]
fn plugins_over_rpc() {
    let dir = tempfile::tempdir().unwrap();
    let mut e = engine();
    let created = e
        .call(
            "plugin.new",
            json!({ "path": dir.path().join("my-import"), "kind": "import" }),
        )
        .unwrap();
    assert_eq!(created["id"], "my-import");
    let v = e
        .call(
            "plugin.validate",
            json!({ "path": created["path"], "known_targets": ["macos-arm64"] }),
        )
        .unwrap();
    assert_eq!(v["valid"], true, "{v}");
    let list = e
        .call("plugin.list", json!({ "dirs": [dir.path()] }))
        .unwrap();
    assert_eq!(list[0]["id"], "my-import");
}

#[test]
fn missing_interpreter_is_a_clear_error() {
    let err = EngineClient::spawn(EngineCommand::new(
        PathBuf::from("/no/such/python3"),
        EngineOrigin::EnvOverride,
        None,
    ))
    .err()
    .expect("spawn must fail");
    assert!(err.to_string().contains("ptk doctor"), "{err}");
}
