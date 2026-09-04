"""Search imported Debug Trace events."""

from __future__ import annotations

from typing import Any

from .protocol import event_matches_config
from .store import SessionStore


def search_events(
    store_dir: str,
    *,
    session_id: str | None = None,
    domain: str | None = None,
    category: str | None = None,
    event_name: str | None = None,
    correlation_id: str | None = None,
    severity: str | None = None,
    text: str | None = None,
    config: dict[str, Any] | None = None,
    apply_config_filter: bool = False,
    limit: int = 50,
) -> dict[str, Any]:
    store = SessionStore(store_dir)
    store.ensure()
    limit = max(1, min(int(limit), 500))

    hits: list[dict[str, Any]] = []
    scanned = 0
    for event in store.iter_events(session_id):
        scanned += 1
        if domain and event.get("domain") != domain:
            continue
        if category and event.get("category") != category:
            continue
        if event_name and event.get("eventName") != event_name:
            continue
        if correlation_id and event.get("correlationId") != correlation_id:
            continue
        if severity and event.get("severity") != severity:
            continue
        if apply_config_filter and config is not None:
            if not event_matches_config(event, config):
                continue
        if text:
            blob = _event_text(event)
            if text.lower() not in blob.lower():
                continue
        hits.append(event)
        if len(hits) >= limit:
            break

    return {
        "ok": True,
        "store_dir": store_dir,
        "scanned": scanned,
        "returned": len(hits),
        "limit": limit,
        "events": hits,
    }


def _event_text(event: dict[str, Any]) -> str:
    parts = [
        str(event.get("domain", "")),
        str(event.get("category", "")),
        str(event.get("eventName", "")),
        str(event.get("correlationId", "")),
        str(event.get("severity", "")),
    ]
    payload = event.get("payload")
    if isinstance(payload, dict):
        parts.append(str(payload))
    return " ".join(parts)
