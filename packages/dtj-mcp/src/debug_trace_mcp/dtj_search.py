"""In-memory read-only search over one native DTJ session.

Reuses ``read_session_dtj`` normalization. No SessionStore, index, or reports.

``list_traces`` parity filters (legacy Wire Trace MCP):
- ``category`` / ``exclude_category``: case-insensitive exact
- ``event``: case-insensitive substring on ``event_name``
- ``event_name``: exact match (DTJ structured filter; kept for index/tools)
- ``text`` / ``contains``: case-insensitive substring on human-readable fields
- ``exclude``: case-insensitive substring drop
- ``mono_from_ns`` / ``mono_to_ns``: monotonic time window (DTJ SoT; replaces
  legacy wall-clock HH:MM:SS — DTJ has no time-of-day clock)
- ``offset`` / ``limit``: page matched events in event_sequence order
"""

from __future__ import annotations

from pathlib import Path
from typing import Any

from .dtj_read import ADAPTER_NAME, ADAPTER_VERSION, read_session_dtj

# Explicit safe bound; invalid/out-of-range limits fail closed (no silent coerce).
MAX_SEARCH_LIMIT = 500
DEFAULT_SEARCH_LIMIT = 100  # legacy list_traces default
MAX_OFFSET = 100_000


def search_session_dtj(
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
    dtj_bin: str | Path | None = None,
) -> dict[str, Any]:
    """Decode one `.dtj` session and return bounded AND-filtered events."""
    path_str = str(Path(session_path).expanduser())
    limit_error = validate_search_limit(limit)
    if limit_error is not None:
        return {
            "ok": False,
            "adapter": {"name": ADAPTER_NAME, "version": ADAPTER_VERSION},
            "session_path": path_str,
            "error": limit_error,
        }
    offset_error = validate_search_offset(offset)
    if offset_error is not None:
        return {
            "ok": False,
            "adapter": {"name": ADAPTER_NAME, "version": ADAPTER_VERSION},
            "session_path": path_str,
            "error": offset_error,
        }
    mono_error = validate_mono_window(mono_from_ns, mono_to_ns)
    if mono_error is not None:
        return {
            "ok": False,
            "adapter": {"name": ADAPTER_NAME, "version": ADAPTER_VERSION},
            "session_path": path_str,
            "error": mono_error,
        }

    decoded = read_session_dtj(session_path, dtj_bin=dtj_bin)
    if not decoded.get("ok"):
        # Propagate structured reader/adapter failure without inventing partial hits.
        return decoded

    events = decoded.get("events")
    if not isinstance(events, list):
        return {
            "ok": False,
            "adapter": decoded.get("adapter")
            or {"name": ADAPTER_NAME, "version": ADAPTER_VERSION},
            "session_path": decoded.get("session_path", path_str),
            "error": {
                "kind": "AdapterInvalidJson",
                "message": "native DTJ decode missing events list",
            },
        }

    # Reader already emits ascending event_sequence; keep that order.
    matched = [
        event_obj
        for event_obj in events
        if isinstance(event_obj, dict)
        and event_matches_native_filters(
            event_obj,
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
        )
    ]
    returned = matched[offset : offset + limit]
    text_out = _list_traces_text(
        matched_count=len(matched),
        returned=returned,
        offset=offset,
    )
    return {
        "ok": True,
        "adapter": decoded.get("adapter")
        or {"name": ADAPTER_NAME, "version": ADAPTER_VERSION},
        "session_path": decoded.get("session_path", path_str),
        "torn_tail": bool(decoded.get("torn_tail", False)),
        "matched_count": len(matched),
        "returned_count": len(returned),
        "limit": limit,
        "offset": offset,
        "filters": {
            "domain": domain,
            "category": category,
            "event_name": event_name,
            "event": event,
            "correlation_id": correlation_id,
            "severity": severity,
            "text": text,
            "exclude": exclude,
            "exclude_category": exclude_category,
            "mono_from_ns": mono_from_ns,
            "mono_to_ns": mono_to_ns,
        },
        "events": returned,
        "text": text_out,
    }


def event_matches_native_filters(
    event_obj: dict[str, Any],
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
) -> bool:
    """AND filters matching legacy ``list_traces`` + DTJ structured extras."""
    ev_category = event_obj.get("category")
    if exclude_category is not None and isinstance(ev_category, str):
        if ev_category.casefold() == exclude_category.casefold():
            return False

    haystack = " ".join(_human_readable_text_parts(event_obj))
    if exclude is not None and exclude != "":
        if exclude.casefold() in haystack.casefold():
            return False

    if mono_from_ns is not None or mono_to_ns is not None:
        mono = event_obj.get("monotonic_ns")
        if isinstance(mono, bool) or not isinstance(mono, int):
            return False
        if mono_from_ns is not None and mono < mono_from_ns:
            return False
        if mono_to_ns is not None and mono > mono_to_ns:
            return False

    if domain is not None and event_obj.get("domain") != domain:
        return False
    if category is not None:
        if not isinstance(ev_category, str) or ev_category.casefold() != category.casefold():
            return False
    if event_name is not None and event_obj.get("event_name") != event_name:
        return False
    if event is not None and event != "":
        name = event_obj.get("event_name")
        if not isinstance(name, str) or event.casefold() not in name.casefold():
            return False
    # correlation_id filter matches the resolved correlation name (normalized string).
    if correlation_id is not None and event_obj.get("correlation") != correlation_id:
        return False
    if severity is not None and event_obj.get("severity") != severity:
        return False
    if text is not None and text != "":
        if text.casefold() not in haystack.casefold():
            return False
    return True


def validate_search_limit(limit: Any) -> dict[str, Any] | None:
    """Return an InvalidLimit error dict, or None when limit is in 1..MAX_SEARCH_LIMIT."""
    if isinstance(limit, bool) or not isinstance(limit, int):
        return {
            "kind": "InvalidLimit",
            "message": (
                f"limit must be an integer in 1..{MAX_SEARCH_LIMIT}, "
                f"got {type(limit).__name__}"
            ),
            "max": MAX_SEARCH_LIMIT,
        }
    if limit < 1 or limit > MAX_SEARCH_LIMIT:
        return {
            "kind": "InvalidLimit",
            "message": (
                f"limit must be an integer in 1..{MAX_SEARCH_LIMIT}, got {limit}"
            ),
            "limit": limit,
            "max": MAX_SEARCH_LIMIT,
        }
    return None


def validate_search_offset(offset: Any) -> dict[str, Any] | None:
    if isinstance(offset, bool) or not isinstance(offset, int):
        return {
            "kind": "InvalidLimit",
            "message": (
                f"offset must be an integer in 0..{MAX_OFFSET}, "
                f"got {type(offset).__name__}"
            ),
            "max": MAX_OFFSET,
        }
    if offset < 0 or offset > MAX_OFFSET:
        return {
            "kind": "InvalidLimit",
            "message": f"offset must be an integer in 0..{MAX_OFFSET}, got {offset}",
            "offset": offset,
            "max": MAX_OFFSET,
        }
    return None


def validate_mono_window(
    mono_from_ns: Any, mono_to_ns: Any
) -> dict[str, Any] | None:
    for name, value in (("mono_from_ns", mono_from_ns), ("mono_to_ns", mono_to_ns)):
        if value is None:
            continue
        if isinstance(value, bool) or not isinstance(value, int) or value < 0:
            return {
                "kind": "InvalidQuery",
                "message": f"{name} must be a non-negative integer (nanoseconds)",
            }
    if (
        isinstance(mono_from_ns, int)
        and isinstance(mono_to_ns, int)
        and mono_to_ns < mono_from_ns
    ):
        return {
            "kind": "InvalidQuery",
            "message": "mono_to_ns must be >= mono_from_ns",
            "mono_from_ns": mono_from_ns,
            "mono_to_ns": mono_to_ns,
        }
    return None


def _list_traces_text(
    *,
    matched_count: int,
    returned: list[dict[str, Any]],
    offset: int,
) -> str:
    """Presentation text shaped like legacy list_traces header/footer."""
    if matched_count == 0:
        return "No matching lines."
    lines = [_compact_event(e) for e in returned]
    header = (
        f"{matched_count} match(es); showing {len(returned)} (offset {offset}).\n"
    )
    footer = ""
    if offset + len(returned) < matched_count:
        more = matched_count - offset - len(returned)
        footer = f"\n... {more} more. Increase offset or tighten filters."
    return header + "\n".join(lines) + footer


def _compact_event(event: dict[str, Any]) -> str:
    seq = event.get("event_sequence")
    cat = event.get("category") or ""
    name = event.get("event_name") or ""
    tag = f"{cat}.{name}" if cat or name else "<none>"
    corr = event.get("correlation") or "-"
    mono = event.get("monotonic_ns")
    fields: list[str] = []
    payload = event.get("payload")
    if isinstance(payload, list):
        for item in payload:
            if not isinstance(item, dict):
                continue
            fname = item.get("name")
            if not isinstance(fname, str):
                continue
            if item.get("type") == "interned_string" and isinstance(
                item.get("value"), str
            ):
                fields.append(f"{fname}={item['value']}")
    msg = " ".join(fields)
    if len(msg) > 120:
        msg = msg[:117] + "..."
    return f"{seq}\tmono_ns={mono}\t{tag}\tcorr={corr}\t{msg}".rstrip()


def _human_readable_text_parts(event: dict[str, Any]) -> list[str]:
    """Fields eligible for ``text``/``exclude`` search — never bytes/hex."""
    parts: list[str] = []
    for key in ("domain", "category", "event_name", "correlation"):
        value = event.get(key)
        if isinstance(value, str) and value:
            parts.append(value)

    payload = event.get("payload")
    if not isinstance(payload, list):
        return parts

    for field in payload:
        if not isinstance(field, dict):
            continue
        name = field.get("name")
        if isinstance(name, str) and name:
            parts.append(name)
        # String payload values only (resolved interned strings).
        if field.get("type") == "interned_string":
            resolved = field.get("value")
            if isinstance(resolved, str) and resolved:
                parts.append(resolved)
    return parts
