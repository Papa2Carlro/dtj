"""DTJ session store — `.dtj` files are the source of truth.

Layout (caller-supplied store_dir):

  store_dir/
    *.dtj                         # sessions (or sessions/<id>.dtj)
    indexes/<stem>.idx.json       # optional rebuildable JSON indexes

No JSONL import/export. Indexes are derived and may be rebuilt anytime.
"""

from __future__ import annotations

from dataclasses import asdict, dataclass, field
from pathlib import Path
from typing import Any, Iterator

from .dtj_index import index_session_dtj
from .dtj_read import ADAPTER_NAME, ADAPTER_VERSION, read_session_dtj


@dataclass
class DtjSessionMeta:
    session_id: str
    session_path: str
    event_count: int
    chunks_committed: int
    torn_tail: bool
    producer_name: str | None = None
    producer_version: str | None = None
    session_id_hex: str | None = None
    start_utc_unix_ms: int | None = None
    domains: list[str] = field(default_factory=list)
    categories: list[str] = field(default_factory=list)
    event_names: list[str] = field(default_factory=list)
    correlations: list[str] = field(default_factory=list)
    size_bytes: int = 0

    def to_dict(self) -> dict[str, Any]:
        return asdict(self)


class DtjSessionStore:
    """Filesystem store over native `.dtj` sessions."""

    def __init__(self, store_dir: str | Path) -> None:
        self.root = Path(store_dir).expanduser()
        self.indexes_dir = self.root / "indexes"

    def ensure(self) -> None:
        self.root.mkdir(parents=True, exist_ok=True)
        self.indexes_dir.mkdir(parents=True, exist_ok=True)

    def resolve_session_path(self, session_id_or_path: str) -> Path | None:
        """Resolve a session id (stem) or absolute/relative `.dtj` path."""
        candidate = Path(session_id_or_path).expanduser()
        if candidate.is_file() and candidate.suffix == ".dtj":
            return candidate
        # Prefer direct child, then sessions/ subdirectory.
        for path in (
            self.root / f"{session_id_or_path}.dtj",
            self.root / "sessions" / f"{session_id_or_path}.dtj",
            self.root / session_id_or_path,
        ):
            if path.is_file() and path.suffix == ".dtj":
                return path
        return None

    def index_path_for(self, session_path: Path) -> Path:
        return self.indexes_dir / f"{session_path.stem}.idx.json"

    def iter_session_paths(self) -> Iterator[Path]:
        if not self.root.is_dir():
            return
        seen: set[Path] = set()
        for pattern in (self.root.glob("*.dtj"), (self.root / "sessions").glob("*.dtj")):
            for path in pattern:
                resolved = path.resolve()
                if resolved in seen:
                    continue
                seen.add(resolved)
                yield path

    def list_sessions(self) -> dict[str, Any]:
        if not self.root.exists():
            return {
                "ok": False,
                "adapter": {"name": ADAPTER_NAME, "version": ADAPTER_VERSION},
                "store_dir": str(self.root),
                "error": {
                    "kind": "InvalidPath",
                    "message": f"store_dir does not exist: {self.root}",
                },
            }
        if not self.root.is_dir():
            return {
                "ok": False,
                "adapter": {"name": ADAPTER_NAME, "version": ADAPTER_VERSION},
                "store_dir": str(self.root),
                "error": {
                    "kind": "InvalidPath",
                    "message": f"store_dir is not a directory: {self.root}",
                },
            }

        sessions: list[dict[str, Any]] = []
        errors: list[dict[str, Any]] = []
        for path in sorted(self.iter_session_paths(), key=lambda p: p.name):
            meta = self.open_session_meta(path)
            if meta.get("ok"):
                sessions.append(meta["session"])
            else:
                errors.append(
                    {
                        "session_path": str(path),
                        "error": meta.get("error"),
                    }
                )
        return {
            "ok": True,
            "adapter": {"name": ADAPTER_NAME, "version": ADAPTER_VERSION},
            "store_dir": str(self.root),
            "count": len(sessions),
            "sessions": sessions,
            "decode_errors": errors,
        }

    def open_session_meta(self, session_path: str | Path) -> dict[str, Any]:
        path = Path(session_path).expanduser()
        path_str = str(path)
        if not path.is_file():
            return {
                "ok": False,
                "adapter": {"name": ADAPTER_NAME, "version": ADAPTER_VERSION},
                "session_path": path_str,
                "error": {
                    "kind": "MissingSession",
                    "message": f"session not found: {path_str}",
                },
            }
        if path.suffix != ".dtj":
            return {
                "ok": False,
                "adapter": {"name": ADAPTER_NAME, "version": ADAPTER_VERSION},
                "session_path": path_str,
                "error": {
                    "kind": "InvalidPath",
                    "message": f"session path must end with .dtj: {path_str}",
                },
            }

        decoded = read_session_dtj(path)
        if not decoded.get("ok"):
            return decoded

        header = decoded.get("header") or {}
        events = decoded.get("events") or []
        domains = sorted(
            {e["domain"] for e in events if isinstance(e, dict) and e.get("domain")}
        )
        categories = sorted(
            {
                e["category"]
                for e in events
                if isinstance(e, dict) and e.get("category")
            }
        )
        event_names = sorted(
            {
                e["event_name"]
                for e in events
                if isinstance(e, dict) and e.get("event_name")
            }
        )
        correlations = sorted(
            {
                e["correlation"]
                for e in events
                if isinstance(e, dict) and isinstance(e.get("correlation"), str)
            }
        )
        try:
            size_bytes = path.stat().st_size
        except OSError:
            size_bytes = 0

        meta = DtjSessionMeta(
            session_id=path.stem,
            session_path=path_str,
            event_count=int(decoded.get("event_count") or 0),
            chunks_committed=int(decoded.get("chunks_committed") or 0),
            torn_tail=bool(decoded.get("torn_tail", False)),
            producer_name=header.get("producer_name"),
            producer_version=header.get("producer_version"),
            session_id_hex=header.get("session_id_hex"),
            start_utc_unix_ms=header.get("start_utc_unix_ms"),
            domains=domains,
            categories=categories,
            event_names=event_names,
            correlations=correlations,
            size_bytes=size_bytes,
        )
        return {
            "ok": True,
            "adapter": decoded.get("adapter")
            or {"name": ADAPTER_NAME, "version": ADAPTER_VERSION},
            "session_path": path_str,
            "torn_tail": meta.torn_tail,
            "session": meta.to_dict(),
            "header": header,
        }

    def ensure_index(
        self, session_path: str | Path, *, rebuild: bool = False
    ) -> dict[str, Any]:
        path = Path(session_path).expanduser()
        self.ensure()
        return index_session_dtj(
            path, self.index_path_for(path), rebuild=rebuild
        )
