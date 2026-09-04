"""JSONL session import — skip malformed lines without aborting."""

from __future__ import annotations

import json
from datetime import datetime, timezone
from pathlib import Path
from typing import Any

from .protocol import event_matches_config, validate_event
from .store import SessionMeta, SessionStore


def _utc_now() -> str:
    return datetime.now(timezone.utc).strftime("%Y-%m-%dT%H:%M:%S.%f")[:-3] + "Z"


def import_session_jsonl(
    session_path: str | Path,
    store_dir: str | Path,
    *,
    config: dict[str, Any] | None = None,
    apply_config_filter: bool = False,
    session_id: str | None = None,
) -> dict[str, Any]:
    """Import a completed (or closed window) JSONL session into store_dir.

    Malformed lines are reported and skipped; import never aborts on a bad line.
    When apply_config_filter=True and config is provided, only events that pass
    domain/category/profile gates are stored (useful for replaying producer rules).
    """
    path = Path(session_path)
    if not path.is_file():
        raise FileNotFoundError(f"session file not found: {path}")

    store = SessionStore(store_dir)
    store.ensure()

    events: list[dict[str, Any]] = []
    skipped: list[dict[str, Any]] = []
    domains: set[str] = set()
    categories: set[str] = set()
    first_ts: str | None = None
    last_ts: str | None = None
    resolved_session_id = session_id

    with path.open(encoding="utf-8") as fh:
        for lineno, raw in enumerate(fh, start=1):
            line = raw.strip()
            if not line:
                continue
            try:
                obj = json.loads(line)
            except json.JSONDecodeError as exc:
                skipped.append(
                    {
                        "line": lineno,
                        "reason": f"json: {exc.msg}",
                        "snippet": line[:120],
                    }
                )
                continue

            errors = validate_event(obj)
            if errors:
                skipped.append(
                    {
                        "line": lineno,
                        "reason": "; ".join(errors),
                        "snippet": line[:120],
                    }
                )
                continue

            if apply_config_filter and config is not None:
                if not event_matches_config(obj, config):
                    skipped.append(
                        {
                            "line": lineno,
                            "reason": "filtered_by_config",
                            "snippet": f"{obj.get('domain')}/{obj.get('category')}",
                        }
                    )
                    continue

            if resolved_session_id is None:
                resolved_session_id = str(obj["sessionId"])
            elif str(obj["sessionId"]) != resolved_session_id:
                skipped.append(
                    {
                        "line": lineno,
                        "reason": (
                            f"sessionId mismatch: expected {resolved_session_id}, "
                            f"got {obj['sessionId']}"
                        ),
                        "snippet": line[:120],
                    }
                )
                continue

            events.append(obj)
            domains.add(str(obj["domain"]))
            categories.add(str(obj["category"]))
            ts = obj.get("timestampUtc")
            if isinstance(ts, str) and ts:
                if first_ts is None:
                    first_ts = ts
                last_ts = ts

    if resolved_session_id is None:
        resolved_session_id = path.stem

    meta = SessionMeta(
        session_id=resolved_session_id,
        source_path=str(path.resolve()),
        event_count=len(events),
        skipped_lines=len(skipped),
        domains=sorted(domains),
        categories=sorted(categories),
        first_timestamp_utc=first_ts,
        last_timestamp_utc=last_ts,
        imported_at_utc=_utc_now(),
    )
    store.save_session(meta, events)
    return {
        "ok": True,
        "session": meta.to_dict(),
        "imported": len(events),
        "skipped": len(skipped),
        "skipped_samples": skipped[:20],
        "store_dir": str(Path(store_dir).resolve()),
        "events_path": str(store.events_path(resolved_session_id)),
    }
