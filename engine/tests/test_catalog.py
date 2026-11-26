"""The engine catalog (``ptk_engine/data/engines.json``): every engine's
sample files detect that engine and no other, and the entries are valid."""

from __future__ import annotations

import pytest

from ptk_engine.scan import scan
from ptk_engine.signatures import CATALOG, ENGINES, THRESHOLD, GameIndex, detect

STATUSES = {"supported", "web", "partial", "runtime", "closed", "not_engine", "unresolved", "native"}
KINDS = {"name", "path", "dir", "dirpath", "magic", "embedded", "mark", "head"}
BY_ID = {e["id"]: e for e in CATALOG}


def index_of(samples: list[str]) -> GameIndex:
    files, heads, dirs = [], {}, set()
    for s in samples:
        path, _, head = s.partition("=")
        files.append(path)
        if head:
            heads[path] = head.encode("latin-1")
        parts = path.split("/")[:-1]
        for i in range(1, len(parts) + 1):
            dirs.add("/".join(parts[:i]))
    return GameIndex(files, sorted(dirs), heads=heads)


def test_entries_are_valid():
    assert len(BY_ID) == len(CATALOG)
    for e in CATALOG:
        assert e["status"] in STATUSES, e["id"]
        assert e["name"] and e["how"], e["id"]
        if e["plugin"]:
            assert e["status"] == "runtime" and e["runtime"], e["id"]
        if e["via"]:
            assert e["via"] in BY_ID and not e["rules"], e["id"]
        if e["status"] == "unresolved":
            assert not e["rules"], e["id"]
        for rule in e["rules"]:
            assert rule[1] in KINDS and (len(rule) == 4) == (rule[1] == "head"), (e["id"], rule)
        if e["rules"]:
            assert e["samples"], f"{e['id']} has rules but no samples"


@pytest.mark.parametrize("engine", [e for e in CATALOG if e["rules"]], ids=lambda e: e["id"])
def test_samples_detect_this_engine_and_no_other(engine):
    found = detect(index_of(engine["samples"]))
    assert found and found[0]["id"] == engine["id"], (engine["id"], [(f["id"], f["confidence"]) for f in found])
    others = [f["id"] for f in found[1:] if f["kind"] == "engine" and f["confidence"] >= THRESHOLD]
    assert others == [], f"{engine['id']} samples also detect {others}"


def test_head_rules_read_real_files(tmp_path):
    (tmp_path / "DOOM2.WAD").write_bytes(b"IWAD" + bytes(20))
    (tmp_path / "other.wad").write_bytes(b"WAD3" + bytes(20))
    r = scan(str(tmp_path))
    assert r["primary"]["id"] == "doom"
    assert "DOOM2.WAD" in r["primary"]["evidence"]


def test_negative_rules_keep_morrowind_out_of_gamebryo():
    ids = [f["id"] for f in detect(index_of(BY_ID["openmw"]["samples"]))]
    assert "gamebryo" not in ids


def test_every_runtime_plugin_names_a_runtime():
    plugins = {e["plugin"] for e in CATALOG if e["plugin"]}
    assert {"scummvm-export", "doom-export", "infinity-export", "mugen-export"} <= plugins
    assert len(ENGINES) == sum(1 for e in CATALOG if e["rules"])


def test_engines_doc_is_up_to_date():
    import subprocess
    import sys
    from pathlib import Path

    root = Path(__file__).resolve().parents[2]
    r = subprocess.run([sys.executable, str(root / "tools" / "gen_engines_doc.py"), "--check"], capture_output=True)
    assert r.returncode == 0, r.stderr.decode()


def test_gamesalad_is_an_html5_variant():
    found = detect(index_of(["index.html", "js/gse/gse-export.js"]))
    assert found[0]["id"] == "html5" and found[0]["variant"] == "GameSalad"
