import io
import json
import os
import subprocess
import sys
from pathlib import Path

from ptk_engine import PROTOCOL_VERSION
from ptk_engine.rpc import (
    ENGINE_ERROR,
    INVALID_PARAMS,
    INVALID_REQUEST,
    METHOD_NOT_FOUND,
    PARSE_ERROR,
    Server,
)

ENGINE_DIR = Path(__file__).resolve().parents[1]


def call(server, method, params=None, req_id=1):
    line = json.dumps({"jsonrpc": "2.0", "id": req_id, "method": method, "params": params or {}})
    return server.handle_line(line.encode())


def test_engine_info():
    r = call(Server(io.BytesIO()), "engine.info")
    assert r["result"]["protocol"] == PROTOCOL_VERSION
    assert "scan" in r["result"]["methods"]
    assert "unity" in r["result"]["engines"]


def test_errors():
    s = Server(io.BytesIO())
    assert s.handle_line(b"{not json")["error"]["code"] == PARSE_ERROR
    assert s.handle_line(b'{"id": 1}')["error"]["code"] == INVALID_REQUEST
    assert call(s, "nope")["error"]["code"] == METHOD_NOT_FOUND
    assert call(s, "scan", {})["error"]["code"] == INVALID_PARAMS
    missing = call(s, "scan", {"path": "/definitely/not/here"})
    assert missing["error"]["code"] == ENGINE_ERROR
    assert "does not exist" in missing["error"]["message"]


def test_notification_gets_no_response():
    s = Server(io.BytesIO())
    assert s.handle_line(b'{"jsonrpc": "2.0", "method": "engine.info"}') is None


def test_subprocess_round_trip(games):
    """The engine as ptk runs it: stdout carries only JSON-RPC lines."""
    requests = [
        {"jsonrpc": "2.0", "id": 1, "method": "engine.info"},
        {"jsonrpc": "2.0", "id": 2, "method": "scan", "params": {"path": str(games / "godot")}},
        {"jsonrpc": "2.0", "id": 3, "method": "shutdown"},
        {"jsonrpc": "2.0", "id": 4, "method": "engine.info"},  # never answered
    ]
    env = dict(os.environ, PYTHONPATH=str(ENGINE_DIR))
    proc = subprocess.run(
        [sys.executable, "-m", "ptk_engine", "rpc"],
        input="".join(json.dumps(r) + "\n" for r in requests).encode(),
        capture_output=True,
        env=env,
        timeout=30,
    )
    assert proc.returncode == 0, proc.stderr
    lines = [json.loads(line) for line in proc.stdout.decode().splitlines()]
    assert [m["id"] for m in lines] == [1, 2, 3]
    assert lines[1]["result"]["primary"]["id"] == "godot"
