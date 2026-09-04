"""Read-only performance aggregates over a validated persistent DTJ index.

Does not decode `.dtj` source, does not rebuild/mutate the index, and does not
use the JSONL SessionStore path. Reuses the JSONL report's duration/percentile
helpers for semantic parity.
"""

from __future__ import annotations

from collections import defaultdict
from pathlib import Path
from typing import Any

from .dtj_index import load_validated_index_events
from .dtj_read import ADAPTER_NAME, ADAPTER_VERSION
from .report import _is_duration_field, _percentile

# Scalar numeric DTJ typed-payload tags eligible for metric extraction.
_SCALAR_NUMERIC_TYPES = frozenset({"i32", "i64", "u32", "u64", "f32", "f64"})


def performance_report_dtj_index(
    session_path: str | Path,
    index_path: str | Path,
    *,
    domain: str | None = None,
    category: str | None = None,
    field: str | None = None,
) -> dict[str, Any]:
    """Aggregate count/p50/p95/max for duration-like scalar numeric DTJ fields.

    ``events_scanned`` is the number of events loaded from the validated index
    after SQL-level ``domain``/``category`` filtering and before payload-field
    eligibility filtering. The tool never re-decodes the `.dtj` source.
    """
    loaded = load_validated_index_events(
        session_path,
        index_path,
        domain=domain,
        category=category,
    )
    if not loaded.get("ok"):
        # Propagate IndexMissing / StaleIndex / CorruptIndex / … unchanged.
        return loaded

    events = loaded.get("events")
    if not isinstance(events, list):
        return {
            "ok": False,
            "adapter": {"name": ADAPTER_NAME, "version": ADAPTER_VERSION},
            "session_path": loaded.get("session_path", str(session_path)),
            "index_path": loaded.get("index_path", str(index_path)),
            "error": {
                "kind": "CorruptIndex",
                "message": "validated index load missing events list",
            },
        }

    # After domain/category SQL filter; before payload eligibility filtering.
    events_scanned = len(events)

    buckets: dict[tuple[str, str, str, str], list[float]] = defaultdict(list)
    for event in events:
        if not isinstance(event, dict):
            return {
                "ok": False,
                "adapter": {"name": ADAPTER_NAME, "version": ADAPTER_VERSION},
                "session_path": loaded["session_path"],
                "index_path": loaded["index_path"],
                "error": {
                    "kind": "CorruptIndex",
                    "message": "index event projection is not an object",
                },
            }
        for name, value in _iter_eligible_numeric_fields(event):
            if field is not None and name != field:
                continue
            if not _is_duration_field(name, value):
                continue
            bucket_key = (
                str(event.get("domain") or ""),
                str(event.get("category") or ""),
                str(event.get("event_name") or ""),
                str(name),
            )
            buckets[bucket_key].append(float(value))

    rows: list[dict[str, Any]] = []
    for (dom, cat, ev, fld), values in buckets.items():
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

    # Match existing JSONL report sort: count desc, then domain/category/eventName.
    rows.sort(
        key=lambda r: (-r["count"], r["domain"], r["category"], r["eventName"], r["field"])
    )

    return {
        "ok": True,
        "adapter": {"name": ADAPTER_NAME, "version": ADAPTER_VERSION},
        "session_path": loaded["session_path"],
        "index_path": loaded["index_path"],
        "torn_tail": bool(loaded.get("torn_tail", False)),
        "events_scanned": events_scanned,
        "metrics": rows,
    }


def _iter_eligible_numeric_fields(event: dict[str, Any]):
    """Yield (resolved_field_name, float_or_int) for scalar numeric payload fields."""
    payload = event.get("payload")
    if not isinstance(payload, list):
        return
    for item in payload:
        if not isinstance(item, dict):
            continue
        name = item.get("name")
        if not isinstance(name, str) or not name:
            continue
        tag = item.get("type")
        if tag not in _SCALAR_NUMERIC_TYPES:
            continue
        value = item.get("value")
        # Mirror JSONL guard: bool is not numeric; no string coercion.
        if isinstance(value, bool) or not isinstance(value, (int, float)):
            continue
        yield name, value
