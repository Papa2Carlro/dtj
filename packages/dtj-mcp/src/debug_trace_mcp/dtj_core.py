"""Unified DTJ core — wraps Rust dtj_python extension.

This is the SINGLE SOURCE OF TRUTH for .dtj parsing.
Both CLI and MCP use this module, ensuring identical results.

Falls back to pure Python implementation if Rust extension unavailable.
"""

from __future__ import annotations

from pathlib import Path
from typing import Any

try:
    from dtj_python import open_session as _rust_open, DtjError as _DtjError

    _HAS_RUST = True
except ImportError:
    _HAS_RUST = False

ADAPTER_NAME = "dtj-core"
ADAPTER_VERSION = 1


def open_session(path: str | Path) -> SessionReader:
    """Open a .dtj session using Rust if available, else Python."""
    path = str(Path(path).expanduser())
    if _HAS_RUST:
        return _rust_open(path)
    return decode_session_file(path)


def read_session(
    path: str | Path,
    *,
    format: str = "json",
) -> dict[str, Any]:
    """Read a .dtj session and return structured data.

    Args:
        path: Path to .dtj session file
        format: Output format - "json" or "summary"

    Returns:
        dict with keys: ok, path, header, events, dictionary, chunks_committed, torn_tail
    """
    path = Path(path).expanduser()
    path_str = str(path)

    if not path.is_file():
        return error_projection(path_str, "Io", f"session file not found: {path_str}")

    try:
        if _HAS_RUST:
            reader = _rust_open(path_str)
            return reader.to_dict()
        else:
            from .dtj_native import open_session as py_open, session_to_projection, DtjError
            reader = py_open(path_str)
            return session_to_projection(path_str, reader)
    except DtjError as exc:
        return error_projection(path_str, exc)
    except OSError as exc:
        from .dtj_native import DtjError
        return error_projection(path_str, DtjError("Io", str(exc)))
    except Exception as exc:
        from .dtj_native import DtjError
        return error_projection(path_str, DtjError("MalformedRecord", f"malformed: {exc}"))


def error_projection(path: str, kind_or_exc: str | Any, message: str | None = None) -> dict[str, Any]:
    """Create error response matching contract."""
    from .dtj_native import DtjError, ADAPTER_NAME, ADAPTER_VERSION
    if isinstance(kind_or_exc, DtjError):
        return {
            "ok": False,
            "adapter": {"name": ADAPTER_NAME, "version": ADAPTER_VERSION},
            "session_path": path,
            "error": kind_or_exc.to_error_dict(),
        }
    return {
        "ok": False,
        "adapter": {"name": ADAPTER_NAME, "version": ADAPTER_VERSION},
        "session_path": path,
        "error": {"kind": kind_or_exc, "message": message or ""},
    }
