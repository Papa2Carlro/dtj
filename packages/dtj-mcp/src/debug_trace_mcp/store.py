"""Local session store under an explicitly supplied store_dir."""

from __future__ import annotations

import json
from dataclasses import asdict, dataclass, field
from pathlib import Path
from typing import Any, Iterator


@dataclass
class SessionMeta:
    session_id: str
    source_path: str
    event_count: int
    skipped_lines: int
    domains: list[str] = field(default_factory=list)
    categories: list[str] = field(default_factory=list)
    first_timestamp_utc: str | None = None
    last_timestamp_utc: str | None = None
    imported_at_utc: str | None = None

    def to_dict(self) -> dict[str, Any]:
        return asdict(self)

    @classmethod
    def from_dict(cls, data: dict[str, Any]) -> SessionMeta:
        return cls(
            session_id=str(data["session_id"]),
            source_path=str(data.get("source_path", "")),
            event_count=int(data.get("event_count", 0)),
            skipped_lines=int(data.get("skipped_lines", 0)),
            domains=list(data.get("domains") or []),
            categories=list(data.get("categories") or []),
            first_timestamp_utc=data.get("first_timestamp_utc"),
            last_timestamp_utc=data.get("last_timestamp_utc"),
            imported_at_utc=data.get("imported_at_utc"),
        )


class SessionStore:
    """Filesystem layout (all under caller-supplied store_dir):

    store_dir/
      index/sessions.json
      sessions/<sessionId>/meta.json
      sessions/<sessionId>/events.jsonl
    """

    def __init__(self, store_dir: str | Path) -> None:
        self.root = Path(store_dir)
        self.index_path = self.root / "index" / "sessions.json"
        self.sessions_dir = self.root / "sessions"

    def ensure(self) -> None:
        self.sessions_dir.mkdir(parents=True, exist_ok=True)
        self.index_path.parent.mkdir(parents=True, exist_ok=True)
        if not self.index_path.is_file():
            self._write_index([])

    def session_dir(self, session_id: str) -> Path:
        return self.sessions_dir / session_id

    def events_path(self, session_id: str) -> Path:
        return self.session_dir(session_id) / "events.jsonl"

    def meta_path(self, session_id: str) -> Path:
        return self.session_dir(session_id) / "meta.json"

    def list_sessions(self) -> list[SessionMeta]:
        self.ensure()
        rows = self._read_index()
        return [SessionMeta.from_dict(r) for r in rows]

    def get_meta(self, session_id: str) -> SessionMeta | None:
        path = self.meta_path(session_id)
        if not path.is_file():
            return None
        with path.open(encoding="utf-8") as fh:
            return SessionMeta.from_dict(json.load(fh))

    def save_session(
        self,
        meta: SessionMeta,
        events: list[dict[str, Any]],
    ) -> SessionMeta:
        self.ensure()
        sdir = self.session_dir(meta.session_id)
        sdir.mkdir(parents=True, exist_ok=True)
        events_path = self.events_path(meta.session_id)
        with events_path.open("w", encoding="utf-8") as fh:
            for event in events:
                fh.write(json.dumps(event, ensure_ascii=False) + "\n")
        with self.meta_path(meta.session_id).open("w", encoding="utf-8") as fh:
            json.dump(meta.to_dict(), fh, indent=2, ensure_ascii=False)
            fh.write("\n")
        self._upsert_index(meta)
        return meta

    def iter_events(
        self,
        session_id: str | None = None,
    ) -> Iterator[dict[str, Any]]:
        self.ensure()
        if session_id:
            path = self.events_path(session_id)
            if path.is_file():
                yield from self._iter_jsonl(path)
            return
        for meta in self.list_sessions():
            path = self.events_path(meta.session_id)
            if path.is_file():
                yield from self._iter_jsonl(path)

    def _iter_jsonl(self, path: Path) -> Iterator[dict[str, Any]]:
        with path.open(encoding="utf-8") as fh:
            for line in fh:
                line = line.strip()
                if not line:
                    continue
                try:
                    obj = json.loads(line)
                except json.JSONDecodeError:
                    continue
                if isinstance(obj, dict):
                    yield obj

    def _read_index(self) -> list[dict[str, Any]]:
        if not self.index_path.is_file():
            return []
        with self.index_path.open(encoding="utf-8") as fh:
            data = json.load(fh)
        if isinstance(data, list):
            return data
        return list(data.get("sessions") or [])

    def _write_index(self, rows: list[dict[str, Any]]) -> None:
        self.index_path.parent.mkdir(parents=True, exist_ok=True)
        with self.index_path.open("w", encoding="utf-8") as fh:
            json.dump({"sessions": rows}, fh, indent=2, ensure_ascii=False)
            fh.write("\n")

    def _upsert_index(self, meta: SessionMeta) -> None:
        rows = self._read_index()
        out = [r for r in rows if r.get("session_id") != meta.session_id]
        out.append(meta.to_dict())
        out.sort(key=lambda r: r.get("imported_at_utc") or "", reverse=True)
        self._write_index(out)
