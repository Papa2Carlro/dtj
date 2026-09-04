"""Atomic DebugTraceConfig read/write. Paths are always caller-supplied."""

from __future__ import annotations

import json
import os
import tempfile
from pathlib import Path
from typing import Any

from .protocol import default_config, validate_config


def read_config(config_path: str | Path) -> dict[str, Any]:
    path = Path(config_path)
    if not path.is_file():
        raise FileNotFoundError(f"config not found: {path}")
    with path.open(encoding="utf-8") as fh:
        data = json.load(fh)
    errors = validate_config(data)
    if errors:
        raise ValueError("invalid DebugTraceConfig: " + "; ".join(errors))
    return data


def write_config_atomic(config_path: str | Path, config: dict[str, Any]) -> Path:
    """Validate and atomically replace the config file (temp + os.replace)."""
    errors = validate_config(config)
    if errors:
        raise ValueError("invalid DebugTraceConfig: " + "; ".join(errors))

    path = Path(config_path)
    path.parent.mkdir(parents=True, exist_ok=True)
    payload = json.dumps(config, indent=2, ensure_ascii=False) + "\n"

    fd, tmp_name = tempfile.mkstemp(
        prefix=f".{path.name}.",
        suffix=".tmp",
        dir=str(path.parent),
    )
    try:
        with os.fdopen(fd, "w", encoding="utf-8") as fh:
            fh.write(payload)
            fh.flush()
            os.fsync(fh.fileno())
        os.replace(tmp_name, path)
    except Exception:
        try:
            os.unlink(tmp_name)
        except OSError:
            pass
        raise
    return path


def get_or_default(config_path: str | Path) -> dict[str, Any]:
    path = Path(config_path)
    if not path.is_file():
        return default_config()
    return read_config(path)
