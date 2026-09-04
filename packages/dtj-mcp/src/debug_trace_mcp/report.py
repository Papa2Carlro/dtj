"""Performance report aggregation over numeric duration-like payload fields."""

from __future__ import annotations

import math
import re
from collections import defaultdict
from typing import Any

from .store import SessionStore

_DURATION_KEY = re.compile(
    r"(duration|elapsed|latency|ms$|_ms$)",
    re.IGNORECASE,
)


def _percentile(sorted_vals: list[float], p: float) -> float | None:
    if not sorted_vals:
        return None
    if len(sorted_vals) == 1:
        return sorted_vals[0]
    # Nearest-rank style on 0-index
    k = (len(sorted_vals) - 1) * (p / 100.0)
    f = math.floor(k)
    c = math.ceil(k)
    if f == c:
        return sorted_vals[int(k)]
    d0 = sorted_vals[f] * (c - k)
    d1 = sorted_vals[c] * (k - f)
    return d0 + d1


def _is_duration_field(key: str, value: Any) -> bool:
    if isinstance(value, bool) or not isinstance(value, (int, float)):
        return False
    return bool(_DURATION_KEY.search(key))


def performance_report(
    store_dir: str,
    *,
    session_id: str | None = None,
    domain: str | None = None,
    category: str | None = None,
    field: str | None = None,
) -> dict[str, Any]:
    """Aggregate count / p50 / p95 / max for numeric duration fields in payloads."""
    store = SessionStore(store_dir)
    store.ensure()

    buckets: dict[tuple[str, str, str, str], list[float]] = defaultdict(list)
    events_scanned = 0

    for event in store.iter_events(session_id):
        events_scanned += 1
        if domain and event.get("domain") != domain:
            continue
        if category and event.get("category") != category:
            continue
        payload = event.get("payload")
        if not isinstance(payload, dict):
            continue
        for key, value in payload.items():
            if field and key != field:
                continue
            if not _is_duration_field(key, value):
                continue
            bucket_key = (
                str(event.get("domain", "")),
                str(event.get("category", "")),
                str(event.get("eventName", "")),
                str(key),
            )
            buckets[bucket_key].append(float(value))

    rows: list[dict[str, Any]] = []
    for (dom, cat, ev, fld), values in sorted(buckets.items()):
        values_sorted = sorted(values)
        rows.append(
            {
                "domain": dom,
                "category": cat,
                "eventName": ev,
                "field": fld,
                "count": len(values_sorted),
                "p50": _percentile(values_sorted, 50),
                "p95": _percentile(values_sorted, 95),
                "max": values_sorted[-1] if values_sorted else None,
            }
        )

    rows.sort(key=lambda r: (-r["count"], r["domain"], r["category"], r["eventName"]))
    return {
        "ok": True,
        "store_dir": store_dir,
        "session_id": session_id,
        "events_scanned": events_scanned,
        "metrics": rows,
    }
