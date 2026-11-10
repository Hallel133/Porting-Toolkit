"""Porting Toolkit engine.

Detects game engines and formats, and runs import/export plugins. The Rust ``ptk`` binary drives it over JSON-RPC.
"""

__version__ = "0.1.0"

#: Version of the Rust<->Python protocol. Must match ``PROTOCOL_VERSION``
#: in ``crates/ptk-core/src/engine.rs``.
PROTOCOL_VERSION = 1


class EngineError(Exception):
    """An expected failure reported to ``ptk`` as a normal error message."""
