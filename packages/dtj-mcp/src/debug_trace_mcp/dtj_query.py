"""Bounded diagnostic queries over one native DTJ session.

Implements the first complete DTJ tool family used by MCP tools. All paths take
an explicit `.dtj` session; no JSONL, no network.
"""

from __future__ import annotations

import re
from collections import Counter
from pathlib import Path
from typing import Any

from .dtj_read import ADAPTER_NAME, ADAPTER_VERSION, read_session_dtj
from .dtj_search import (
    DEFAULT_SEARCH_LIMIT,
    MAX_SEARCH_LIMIT,
    event_matches_native_filters,
    search_session_dtj,
    validate_search_limit,
)
from .report import _is_duration_field, _percentile

MAX_REGEX_PATTERN_LEN = 256
MAX_CONTEXT_WINDOW = 200
MAX_NEIGHBOURHOOD = 100
_SCALAR_NUMERIC_TYPES = frozenset({"i32", "i64", "u32", "u64", "f32", "f64"})


def _base(session_path: str, decoded: dict[str, Any] | None = None) -> dict[str, Any]:
    adapter = (
        (decoded or {}).get("adapter")
        if decoded
        else None
    ) or {"name": ADAPTER_NAME, "version": ADAPTER_VERSION}
    out: dict[str, Any] = {
        "ok": True,
        "adapter": adapter,
        "session_path": session_path,
    }
    if decoded is not None:
        out["torn_tail"] = bool(decoded.get("torn_tail", False))
    return out


def _fail(session_path: str, error: dict[str, Any]) -> dict[str, Any]:
    return {
        "ok": False,
        "adapter": {"name": ADAPTER_NAME, "version": ADAPTER_VERSION},
        "session_path": session_path,
        "error": error,
    }


def _load(session_path: str | Path) -> dict[str, Any]:
    return read_session_dtj(session_path)


def session_info_dtj(session_path: str | Path) -> dict[str, Any]:
    """Open metadata for one `.dtj` session (no full event dump in result)."""
    from .dtj_store import DtjSessionStore

    # Reuse store meta projection for a single path.
    return DtjSessionStore(Path(session_path).expanduser().parent).open_session_meta(
        session_path
    )


def tail_events_dtj(session_path: str | Path, n: int = 50) -> dict[str, Any]:
    path_str = str(Path(session_path).expanduser())
    limit_error = validate_search_limit(n)
    if limit_error is not None:
        # Reuse InvalidLimit shape; rename message for tail.
        return _fail(path_str, limit_error)

    decoded = _load(session_path)
    if not decoded.get("ok"):
        return decoded
    events = decoded.get("events") or []
    if not isinstance(events, list):
        return _fail(
            path_str,
            {"kind": "MalformedRecord", "message": "events list missing"},
        )
    returned = events[-n:] if n < len(events) else list(events)
    out = _base(path_str, decoded)
    out.update(
        {
            "n": n,
            "event_count": len(events),
            "returned_count": len(returned),
            "events": returned,
        }
    )
    return out


def get_event_range_dtj(
    session_path: str | Path,
    start: int,
    end: int,
) -> dict[str, Any]:
    """Inclusive event_sequence range [start, end]."""
    path_str = str(Path(session_path).expanduser())
    if not isinstance(start, int) or not isinstance(end, int) or isinstance(
        start, bool
    ) or isinstance(end, bool):
        return _fail(
            path_str,
            {
                "kind": "InvalidRange",
                "message": "start and end must be integers (event_sequence)",
            },
        )
    if end < start:
        return _fail(
            path_str,
            {
                "kind": "InvalidRange",
                "message": "end must be >= start",
                "start": start,
                "end": end,
            },
        )
    if end - start + 1 > MAX_SEARCH_LIMIT:
        return _fail(
            path_str,
            {
                "kind": "ResultTooLarge",
                "message": (
                    f"requested range size {end - start + 1} exceeds "
                    f"max {MAX_SEARCH_LIMIT}"
                ),
                "max": MAX_SEARCH_LIMIT,
            },
        )

    decoded = _load(session_path)
    if not decoded.get("ok"):
        return decoded
    events = [
        e
        for e in (decoded.get("events") or [])
        if isinstance(e, dict)
        and isinstance(e.get("event_sequence"), int)
        and start <= e["event_sequence"] <= end
    ]
    out = _base(path_str, decoded)
    out.update(
        {
            "start": start,
            "end": end,
            "returned_count": len(events),
            "events": events,
        }
    )
    return out


def context_around_event_dtj(
    session_path: str | Path,
    event_sequence: int,
    before: int = 30,
    after: int = 30,
) -> dict[str, Any]:
    path_str = str(Path(session_path).expanduser())
    for name, value in (("before", before), ("after", after)):
        if (
            isinstance(value, bool)
            or not isinstance(value, int)
            or value < 0
            or value > MAX_CONTEXT_WINDOW
        ):
            return _fail(
                path_str,
                {
                    "kind": "InvalidLimit",
                    "message": (
                        f"{name} must be an integer in 0..{MAX_CONTEXT_WINDOW}"
                    ),
                    "max": MAX_CONTEXT_WINDOW,
                },
            )
    if isinstance(event_sequence, bool) or not isinstance(event_sequence, int):
        return _fail(
            path_str,
            {
                "kind": "InvalidRange",
                "message": "event_sequence must be an integer",
            },
        )

    decoded = _load(session_path)
    if not decoded.get("ok"):
        return decoded
    events = [e for e in (decoded.get("events") or []) if isinstance(e, dict)]
    idx = next(
        (
            i
            for i, e in enumerate(events)
            if e.get("event_sequence") == event_sequence
        ),
        None,
    )
    if idx is None:
        return _fail(
            path_str,
            {
                "kind": "MissingEvent",
                "message": f"event_sequence {event_sequence} not found",
                "event_sequence": event_sequence,
            },
        )
    lo = max(0, idx - before)
    hi = min(len(events), idx + after + 1)
    window = events[lo:hi]
    out = _base(path_str, decoded)
    out.update(
        {
            "event_sequence": event_sequence,
            "before": before,
            "after": after,
            "returned_count": len(window),
            "events": window,
        }
    )
    return out


def structured_search_dtj(
    session_path: str | Path,
    *,
    domain: str | None = None,
    category: str | None = None,
    event_name: str | None = None,
    event: str | None = None,
    correlation_id: str | None = None,
    severity: str | None = None,
    text: str | None = None,
    exclude: str | None = None,
    exclude_category: str | None = None,
    mono_from_ns: int | None = None,
    mono_to_ns: int | None = None,
    limit: int = DEFAULT_SEARCH_LIMIT,
    offset: int = 0,
) -> dict[str, Any]:
    """AND-filtered structured search (list_traces parity over one `.dtj`)."""
    return search_session_dtj(
        session_path,
        domain=domain,
        category=category,
        event_name=event_name,
        event=event,
        correlation_id=correlation_id,
        severity=severity,
        text=text,
        exclude=exclude,
        exclude_category=exclude_category,
        mono_from_ns=mono_from_ns,
        mono_to_ns=mono_to_ns,
        limit=limit,
        offset=offset,
    )


def text_search_dtj(
    session_path: str | Path,
    query: str,
    *,
    domain: str | None = None,
    category: str | None = None,
    limit: int = DEFAULT_SEARCH_LIMIT,
) -> dict[str, Any]:
    if not isinstance(query, str) or query == "":
        path_str = str(Path(session_path).expanduser())
        return _fail(
            path_str,
            {
                "kind": "InvalidQuery",
                "message": "query must be a non-empty string",
            },
        )
    return search_session_dtj(
        session_path,
        domain=domain,
        category=category,
        text=query,
        limit=limit,
    )


def regex_search_dtj(
    session_path: str | Path,
    pattern: str,
    *,
    domain: str | None = None,
    category: str | None = None,
    limit: int = DEFAULT_SEARCH_LIMIT,
) -> dict[str, Any]:
    path_str = str(Path(session_path).expanduser())
    limit_error = validate_search_limit(limit)
    if limit_error is not None:
        return _fail(path_str, limit_error)
    if not isinstance(pattern, str) or not pattern:
        return _fail(
            path_str,
            {
                "kind": "InvalidQuery",
                "message": "pattern must be a non-empty string",
            },
        )
    if len(pattern) > MAX_REGEX_PATTERN_LEN:
        return _fail(
            path_str,
            {
                "kind": "InvalidQuery",
                "message": (
                    f"pattern length {len(pattern)} exceeds "
                    f"max {MAX_REGEX_PATTERN_LEN}"
                ),
                "max": MAX_REGEX_PATTERN_LEN,
            },
        )
    try:
        # Bounded: no timeout API in stdlib; reject catastrophic patterns by
        # forbidding nested unbounded quantifiers on wide classes.
        if re.search(r"(\.\*){2,}|\(\?[^)]*\)\+", pattern):
            return _fail(
                path_str,
                {
                    "kind": "InvalidQuery",
                    "message": "pattern rejected as potentially catastrophic",
                },
            )
        compiled = re.compile(pattern, re.IGNORECASE)
    except re.error as exc:
        return _fail(
            path_str,
            {
                "kind": "InvalidQuery",
                "message": f"invalid regex: {exc}",
            },
        )

    decoded = _load(session_path)
    if not decoded.get("ok"):
        return decoded
    matched: list[dict[str, Any]] = []
    for event in decoded.get("events") or []:
        if not isinstance(event, dict):
            continue
        if domain is not None and event.get("domain") != domain:
            continue
        if category is not None and event.get("category") != category:
            continue
        haystack = " ".join(_text_parts(event))
        if compiled.search(haystack):
            matched.append(event)
    returned = matched[:limit]
    out = _base(path_str, decoded)
    out.update(
        {
            "pattern": pattern,
            "matched_count": len(matched),
            "returned_count": len(returned),
            "limit": limit,
            "events": returned,
        }
    )
    return out


def correlation_neighbourhood_dtj(
    session_path: str | Path,
    correlation_id: str,
    *,
    before: int = 5,
    after: int = 5,
    limit: int = DEFAULT_SEARCH_LIMIT,
) -> dict[str, Any]:
    """Events sharing correlation plus bounded neighbours around each hit."""
    path_str = str(Path(session_path).expanduser())
    limit_error = validate_search_limit(limit)
    if limit_error is not None:
        return _fail(path_str, limit_error)
    if not isinstance(correlation_id, str) or not correlation_id:
        return _fail(
            path_str,
            {
                "kind": "InvalidQuery",
                "message": "correlation_id must be a non-empty string",
            },
        )
    for name, value in (("before", before), ("after", after)):
        if (
            isinstance(value, bool)
            or not isinstance(value, int)
            or value < 0
            or value > MAX_NEIGHBOURHOOD
        ):
            return _fail(
                path_str,
                {
                    "kind": "InvalidLimit",
                    "message": (
                        f"{name} must be an integer in 0..{MAX_NEIGHBOURHOOD}"
                    ),
                    "max": MAX_NEIGHBOURHOOD,
                },
            )

    decoded = _load(session_path)
    if not decoded.get("ok"):
        return decoded
    events = [e for e in (decoded.get("events") or []) if isinstance(e, dict)]
    hit_indexes = [
        i
        for i, e in enumerate(events)
        if e.get("correlation") == correlation_id
    ]
    if not hit_indexes:
        out = _base(path_str, decoded)
        out.update(
            {
                "correlation_id": correlation_id,
                "matched_count": 0,
                "returned_count": 0,
                "limit": limit,
                "events": [],
            }
        )
        return out

    selected: dict[int, dict[str, Any]] = {}
    for idx in hit_indexes:
        lo = max(0, idx - before)
        hi = min(len(events), idx + after + 1)
        for j in range(lo, hi):
            selected[j] = events[j]
    ordered = [selected[i] for i in sorted(selected)]
    if len(ordered) > limit:
        return _fail(
            path_str,
            {
                "kind": "ResultTooLarge",
                "message": (
                    f"neighbourhood result {len(ordered)} exceeds limit {limit}; "
                    "tighten before/after or raise limit"
                ),
                "matched_count": len(ordered),
                "limit": limit,
                "max": MAX_SEARCH_LIMIT,
            },
        )
    out = _base(path_str, decoded)
    out.update(
        {
            "correlation_id": correlation_id,
            "before": before,
            "after": after,
            "matched_count": len(ordered),
            "returned_count": len(ordered),
            "limit": limit,
            "events": ordered,
        }
    )
    return out


def stats_report_dtj(
    session_path: str | Path,
    *,
    domain: str | None = None,
    category: str | None = None,
) -> dict[str, Any]:
    """Category/event/severity counts + duration-like field aggregates."""
    path_str = str(Path(session_path).expanduser())
    decoded = _load(session_path)
    if not decoded.get("ok"):
        return decoded

    events = [
        e
        for e in (decoded.get("events") or [])
        if isinstance(e, dict)
        and event_matches_native_filters(
            e, domain=domain, category=category
        )
    ]
    by_domain = Counter(
        str(e.get("domain") or "<none>") for e in events
    )
    by_category = Counter(
        str(e.get("category") or "<none>") for e in events
    )
    by_event = Counter(
        str(e.get("event_name") or "<none>") for e in events
    )
    by_severity = Counter(
        str(e.get("severity") or "<none>") for e in events
    )

    duration_buckets: dict[tuple[str, str, str, str], list[float]] = {}
    for event in events:
        payload = event.get("payload")
        if not isinstance(payload, list):
            continue
        for item in payload:
            if not isinstance(item, dict):
                continue
            name = item.get("name")
            if not isinstance(name, str) or not name:
                continue
            if item.get("type") not in _SCALAR_NUMERIC_TYPES:
                continue
            value = item.get("value")
            if isinstance(value, bool) or not isinstance(value, (int, float)):
                continue
            if not _is_duration_field(name, value):
                continue
            key = (
                str(event.get("domain") or ""),
                str(event.get("category") or ""),
                str(event.get("event_name") or ""),
                name,
            )
            duration_buckets.setdefault(key, []).append(float(value))

    metrics: list[dict[str, Any]] = []
    for (dom, cat, ev, fld), values in duration_buckets.items():
        values_sorted = sorted(values)
        metrics.append(
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
    metrics.sort(
        key=lambda r: (
            -r["count"],
            r["domain"],
            r["category"],
            r["eventName"],
            r["field"],
        )
    )

    out = _base(path_str, decoded)
    out.update(
        {
            "events_scanned": len(events),
            "counts": {
                "by_domain": dict(by_domain.most_common()),
                "by_category": dict(by_category.most_common()),
                "by_event_name": dict(by_event.most_common()),
                "by_severity": dict(by_severity.most_common()),
            },
            "metrics": metrics,
        }
    )
    return out


def _text_parts(event: dict[str, Any]) -> list[str]:
    parts: list[str] = []
    for key in ("domain", "category", "event_name", "correlation", "severity"):
        value = event.get(key)
        if isinstance(value, str) and value:
            parts.append(value)
    payload = event.get("payload")
    if isinstance(payload, list):
        for field in payload:
            if not isinstance(field, dict):
                continue
            name = field.get("name")
            if isinstance(name, str) and name:
                parts.append(name)
            if field.get("type") == "interned_string":
                resolved = field.get("value")
                if isinstance(resolved, str) and resolved:
                    parts.append(resolved)
            elif field.get("type") in ("bool", "i32", "i64", "u32", "u64", "f32", "f64", "enum"):
                parts.append(str(field.get("value")))
    return parts
