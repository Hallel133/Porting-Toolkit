"""``python -m ptk_engine``.

- ``rpc``: serve JSON-RPC on stdin/stdout (what ``ptk`` runs).
- ``scan PATH``: print a scan as JSON, for debugging.
- ``version``: print the engine and protocol versions.
"""

from __future__ import annotations

import sys

if sys.version_info < (3, 11):  # noqa: UP036 - friendly message on old Pythons
    sys.stderr.write(
        f"ptk-engine needs Python 3.11 or newer (this is {sys.version.split()[0]}). Run `ptk doctor`.\n"
    )
    raise SystemExit(3)


def main(argv: list[str] | None = None) -> int:
    import argparse
    import json

    from . import PROTOCOL_VERSION, EngineError, __version__

    parser = argparse.ArgumentParser(prog="ptk-engine")
    sub = parser.add_subparsers(dest="command", required=True)
    sub.add_parser("rpc", help="serve JSON-RPC on stdin/stdout")
    scan_p = sub.add_parser("scan", help="scan a game folder and print JSON")
    scan_p.add_argument("path")
    sub.add_parser("version", help="print versions")
    args = parser.parse_args(argv)

    if args.command == "rpc":
        from .rpc import main as rpc_main

        rpc_main()
    elif args.command == "scan":
        from .scan import scan

        try:
            print(json.dumps(scan(args.path), indent=2))
        except EngineError as e:
            print(f"error: {e}", file=sys.stderr)
            return 1
    else:
        print(f"ptk-engine {__version__} (protocol {PROTOCOL_VERSION})")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
