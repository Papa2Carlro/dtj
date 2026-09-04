"""Read-only DTJ v1 session decode — unified via dtj_core.

Uses Rust dtj_python extension when available, falls back to Python.
Never executes payload bytes as paths/URLs/code.
"""

from __future__ import annotations

from pathlib import Path
from typing import Any

from .dtj_core import (
    ADAPTER_NAME,
    ADAPTER_VERSION,
    read_session,
)

__all__ = [
    "ADAPTER_NAME",
    "ADAPTER_VERSION",
    "read_session_dtj",
]


def read_session_dtj(
    session_path: str | Path,
    *,
    dtj_bin: str | Path | None = None,  # noqa: ARG001 — retained for call-site compat
    timeout_s: float = 120,  # noqa: ARG001 — unused
) -> dict[str, Any]:
    """Decode a native `.dtj` session via Rust or Python."""
    del dtj_bin, timeout_s  # explicit: CLI adapter retired
    path = Path(session_path).expanduser()
    session_path_str = str(path)

    if not path.is_file():
        return error_projection(
            session_path_str,
            "Io",
            f"session file not found: {session_path_str}",
        )

    return read_session(path)
