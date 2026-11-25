"""The incremental build layer: sync, clones, zips, signing stamps."""

from __future__ import annotations

import json
import os
import time
import zipfile
from pathlib import Path

import pytest

from ptk_engine.build import Build, BuildError, STATE_FILE, can_clone, copy_file, zip_dir
from ptk_engine.build import clone as clone_mod
from ptk_engine.build.sign import needs_signing, fingerprint


def game(tmp: Path) -> Path:
    g = tmp / "game"
    (g / "data" / "sub").mkdir(parents=True)
    (g / "data" / "a.txt").write_text("alpha " * 100)
    (g / "data" / "b.png").write_bytes(b"\x89PNG" + bytes(4000))
    (g / "data" / "sub" / "c.bin").write_bytes(os.urandom(3000))
    (g / "run.sh").write_text("#!/bin/sh\n")
    (g / "run.sh").chmod(0o755)
    (g / ".DS_Store").write_bytes(b"junk")
    return g


def declare(out: Path, g: Path, variant: str = "") -> Build:
    b = Build(out, variant=variant)
    b.add_tree(g, "Game", skip=lambda p: p.name == ".DS_Store")
    b.add_text("Game/README.txt", "hello")
    b.artifact("Game.zip")
    return b


def test_sync_writes_only_what_changed(tmp_path):
    g = game(tmp_path)
    out = tmp_path / "out"
    r = declare(out, g).sync()
    assert r.rebuilt and len(r.written) == 5 and not r.removed
    assert (out / "Game/data/sub/c.bin").read_bytes() == (g / "data/sub/c.bin").read_bytes()
    assert os.access(out / "Game/run.sh", os.X_OK) and not (out / "Game/.DS_Store").exists()
    assert json.loads((out / STATE_FILE).read_text())["version"] == 1

    r = declare(out, g).sync()
    assert (r.written, r.removed, r.unchanged) == ([], [], 5)

    time.sleep(0.01)
    (g / "data" / "a.txt").write_text("changed")
    (g / "data" / "b.png").unlink()
    (out / "Game" / "stray.tmp").write_text("x")
    r = declare(out, g).sync()
    assert r.written == ["Game/data/a.txt"]
    assert sorted(r.removed) == ["Game/data/b.png", "Game/stray.tmp"]
    assert (out / "Game/data/a.txt").read_text() == "changed"


def test_output_touched_after_sync_is_rewritten_unless_sealed(tmp_path):
    g = game(tmp_path)
    out = tmp_path / "out"
    declare(out, g).sync()
    (out / "Game/data/a.txt").write_text("edited by hand")
    assert declare(out, g).sync().written == ["Game/data/a.txt"]
    b = declare(out, g)
    b.sync()
    (out / "Game/data/a.txt").write_text("re-encoded by the exporter")
    b.seal()
    assert declare(out, g).sync().written == []
    assert declare(out, g, variant="other options").sync().written  # a new variant rewrites


def test_foreign_folders_are_refused_and_old_markers_rebuilt(tmp_path):
    g = game(tmp_path)
    foreign = tmp_path / "mine"
    foreign.mkdir()
    (foreign / "photo.jpg").write_bytes(b"x")
    with pytest.raises(BuildError, match="not made by Porting Toolkit"):
        declare(foreign, g).sync()
    assert (foreign / "photo.jpg").exists()
    old = tmp_path / "old"
    old.mkdir()
    (old / STATE_FILE).write_text("Made by Porting Toolkit. This folder is replaced on the next export.\n")
    (old / "leftover").write_text("x")
    r = declare(old, g).sync()
    assert r.rebuilt and not (old / "leftover").exists()


def test_paths_must_stay_inside(tmp_path):
    b = Build(tmp_path / "out")
    for bad in ("../x", "/etc/passwd", "", ".", STATE_FILE, "a/../../x"):
        with pytest.raises(BuildError):
            b.add_text(bad, "x")
    with pytest.raises(BuildError):
        b.add_link("a/link", "../../outside")
    b.add_link("a/b/link", "../file")
    assert b.add_text("a/./b/../c.txt", "x") == "a/c.txt"


def test_links_in_trees(tmp_path):
    src = tmp_path / "App.app" / "Contents"
    (src / "Frameworks" / "A.framework" / "Versions" / "A").mkdir(parents=True)
    (src / "Frameworks" / "A.framework" / "Versions" / "A" / "A").write_bytes(b"bin")
    os.symlink("Versions/A/A", src / "Frameworks" / "A.framework" / "A")
    os.symlink("/etc/hosts", src / "outside")
    out = tmp_path / "out"
    b = Build(out)
    b.add_tree(tmp_path / "App.app", "Game.app", links=True)
    b.sync()
    link = out / "Game.app/Contents/Frameworks/A.framework/A"
    assert link.is_symlink() and link.read_bytes() == b"bin"
    assert not (out / "Game.app/Contents/outside").exists()


def test_copy_falls_back_without_clones(tmp_path, monkeypatch):
    src = tmp_path / "f"
    src.write_bytes(b"data")
    monkeypatch.setenv("PTK_NO_CLONE", "1")
    assert copy_file(src, tmp_path / "g") == "copy"
    monkeypatch.delenv("PTK_NO_CLONE")
    how = copy_file(src, tmp_path / "h")
    assert how == ("clone" if can_clone() else "copy")
    assert (tmp_path / "h").read_bytes() == b"data"
    monkeypatch.setattr(clone_mod, "_clonefile", None)
    monkeypatch.setattr(clone_mod, "_loaded", True)
    assert copy_file(src, tmp_path / "h") == "copy"


def test_incremental_zip_is_identical_to_a_fresh_one(tmp_path):
    g = game(tmp_path)
    (g / "data" / "big.txt").write_text("compress me " * 900_000)  # above the in-memory limit
    z1 = tmp_path / "a" / "Game.zip"
    r = zip_dir(g, z1, executables={"run.sh"})
    assert r["compressed"] == r["members"] == 5  # .DS_Store left out
    r = zip_dir(g, z1, executables={"run.sh"})
    assert r["reused"] == 5 and r["compressed"] == 0
    time.sleep(0.01)
    (g / "data" / "a.txt").write_text("new text")
    r = zip_dir(g, z1, executables={"run.sh"})
    assert (r["reused"], r["compressed"]) == (4, 1)
    fresh = tmp_path / "b" / "Game.zip"
    zip_dir(g, fresh, executables={"run.sh"})
    assert z1.read_bytes() == fresh.read_bytes()
    with zipfile.ZipFile(z1) as z:
        assert z.testzip() is None
        info = {i.filename: i for i in z.infolist()}
        assert info["game/data/b.png"].compress_type == zipfile.ZIP_STORED
        assert info["game/data/big.txt"].compress_type == zipfile.ZIP_DEFLATED
        assert (info["game/run.sh"].external_attr >> 16) & 0o777 == 0o755
        assert z.read("game/data/a.txt") == b"new text"


def test_a_damaged_index_rebuilds_the_zip(tmp_path):
    g = game(tmp_path)
    z = tmp_path / "Game.zip"
    zip_dir(g, z)
    (tmp_path / "Game.zip.ptkindex").write_text("{broken")
    assert zip_dir(g, z)["compressed"] == 4
    z.write_bytes(z.read_bytes()[:-10])  # the zip changed behind our back
    assert zip_dir(g, z)["compressed"] == 4
    with zipfile.ZipFile(z) as zf:
        assert zf.testzip() is None


def test_signing_stamp(tmp_path):
    from ptk_engine.build import codesign_if_changed
    app = tmp_path / "Game.app"
    (app / "Contents" / "MacOS").mkdir(parents=True)
    (app / "Contents" / "MacOS" / "Game").write_bytes(b"x")
    calls = []
    assert codesign_if_changed(app, calls.append)
    assert not codesign_if_changed(app, calls.append) and len(calls) == 1
    (app / "Contents" / "MacOS" / "Game").write_bytes(b"changed")
    assert needs_signing(app) and fingerprint(app)
    assert codesign_if_changed(app, calls.append) and len(calls) == 2


def test_files_made_after_sync_are_kept_by_seal(tmp_path):
    g = game(tmp_path)
    out = tmp_path / "out"
    b = declare(out, g)
    b.sync()
    sig = out / "Game" / "data" / "_CodeSignature" / "CodeResources"
    sig.parent.mkdir()
    sig.write_text("signed")
    (out / "Game.zip").write_text("zip")
    (out / "Game.zip.ptkindex").write_text("{}")
    (out / "Game.ptksign").write_text("stamp")
    b.seal()
    r = declare(out, g).sync()
    assert r.removed == [] and sig.is_file() and (out / "Game.zip.ptkindex").is_file()
    assert (out / "Game.ptksign").is_file()
    # A new variant rewrites everything and drops what an exporter added.
    declare(out, g, variant="v2").sync()
    assert not sig.exists()
