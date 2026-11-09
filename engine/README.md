# ptk-engine

The Python half of Porting Toolkit. The `ptk` binary starts it as a child
process and talks JSON-RPC 2.0 over stdin/stdout (one JSON message per line).

```bash
uv run pytest                                   # tests
uv run python -m ptk_engine scan ../fixtures/games/unity-windows   # debug a scan
uv run python -m ptk_engine rpc                 # what ptk runs
```

It uses only the Python standard library (3.11+). See `../ARCHITECTURE.md`
for the protocol and `../docs/plugins.md` for the plugin format.
