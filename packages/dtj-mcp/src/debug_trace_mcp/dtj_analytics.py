"""Advanced analytics family over native DTJ sessions.

Ports wire-trace-mcp analytics tools onto structured DTJ events. Field values
come from typed payload / correlation — not legacy message-string parsing.
Addressing uses ``event_sequence``; time windows use ``monotonic_ns``.
Baseline I/O requires an explicit local ``baseline_path``.
"""

from __future__ import annotations

import json
import math
import re
from collections import Counter
from pathlib import Path
from typing import Any

from .dtj_analyze import (
    EVENT_PAIRS,
    MAX_LIMIT,
    MAX_TOP,
    _compact,
    _entity_id_from_event,
    _entity_refs,
    _event_matches,
    _events,
    _fail,
    _haystack,
    _is_session_boundary,
    _load,
    _ok,
    _payload_kv,
    _scan,
    _scope_events,
    _tag,
    _validate_limit,
    _validate_top,
)

MAX_WINDOW = 500
MAX_MAX_LINES = 500
MAX_BUCKET_SEC = 3600.0
MAX_WINDOW_SEC = 3600.0
MAX_MIN_GAP_SEC = 86400.0
_TEMPLATE_NUM_RE = re.compile(r"\b-?\d+(?:\.\d+)?\b")


def _mono_sec(event: dict[str, Any]) -> float | None:
    mono = event.get("monotonic_ns")
    if isinstance(mono, bool) or not isinstance(mono, int):
        return None
    return mono / 1_000_000_000.0


def _validate_window(window: Any) -> dict[str, Any] | None:
    if (
        isinstance(window, bool)
        or not isinstance(window, int)
        or window < 0
        or window > MAX_WINDOW
    ):
        return {
            "kind": "InvalidLimit",
            "message": f"window must be an integer in 0..{MAX_WINDOW}",
            "max": MAX_WINDOW,
        }
    return None


def _validate_positive_float(
    value: Any, *, name: str, max_v: float
) -> dict[str, Any] | None:
    if isinstance(value, bool) or not isinstance(value, (int, float)):
        return {
            "kind": "InvalidQuery",
            "message": f"{name} must be a positive number",
        }
    if not math.isfinite(float(value)) or float(value) <= 0 or float(value) > max_v:
        return {
            "kind": "InvalidQuery",
            "message": f"{name} must be in (0, {max_v}]",
            "max": max_v,
        }
    return None


def _session_slices(events: list[dict[str, Any]]) -> list[list[dict[str, Any]]]:
    starts = [
        i for i, e in enumerate(events) if _is_session_boundary(e)
    ]
    if not starts:
        return [events] if events else []
    slices: list[list[dict[str, Any]]] = []
    for idx, start in enumerate(starts):
        end = starts[idx + 1] if idx + 1 < len(starts) else len(events)
        slices.append(events[start:end])
    return slices


def _event_template(event: dict[str, Any]) -> str:
    """Abstract field values to {} (DTJ analogue of message_templates)."""
    parts: list[str] = []
    for name, value in sorted(_payload_kv(event).items()):
        # Abstract all payload values; keep field names.
        abstracted = _TEMPLATE_NUM_RE.sub("{}", value)
        if abstracted == value and value:
            abstracted = "{}"
        parts.append(f"{name}={abstracted}")
    body = " ".join(parts)
    body = re.sub(r"\{\}(\s+\{\})+", "{}", body)
    body = re.sub(r"\s+", " ", body).strip()
    return f"{_tag(event)}\t{body}"


def _entity_field_match(event: dict[str, Any], entity_id: str) -> bool:
    """Exact structured match on entity-like payload fields or correlation."""
    if event.get("correlation") == entity_id:
        return True
    kv = _payload_kv(event)
    for key in ("id", "wire", "networkId", "sourceId"):
        if kv.get(key) == entity_id:
            return True
    return False


def _snapshot_events(events: list[dict[str, Any]]) -> list[dict[str, Any]]:
    return [e for e in events if e.get("category") == "Snapshot"]


def repetition_dtj(
    session_path: str | Path,
    *,
    top: int = 10,
    min_run: int = 3,
    since_last_clear: bool = False,
    category: str | None = None,
) -> dict[str, Any]:
    path_str = str(Path(session_path).expanduser())
    top_err = _validate_top(top)
    if top_err:
        return _fail(path_str, top_err)
    if isinstance(min_run, bool) or not isinstance(min_run, int) or min_run < 1 or min_run > MAX_TOP:
        return _fail(
            path_str,
            {
                "kind": "InvalidLimit",
                "message": f"min_run must be an integer in 1..{MAX_TOP}",
                "max": MAX_TOP,
            },
        )
    decoded = _load(session_path)
    if not decoded.get("ok"):
        return decoded
    scoped = _scope_events(
        _events(decoded), since_last_clear=since_last_clear, category=category
    )
    sr = _scan(scoped, sample_limit=1)
    runs: list[dict[str, Any]] = []
    prev_key: tuple[str, str] | None = None
    run_start: dict[str, Any] | None = None
    run_len = 0

    def flush() -> None:
        nonlocal run_start, run_len
        if run_start is not None and run_len >= min_run:
            runs.append(
                {
                    "run": run_len,
                    "event_sequence": run_start.get("event_sequence"),
                    "tag": _tag(run_start),
                    "text": (
                        f"run={run_len}  seq={run_start.get('event_sequence')}  "
                        f"{_tag(run_start)}"
                    ),
                }
            )
        run_start = None
        run_len = 0

    for event in sr.events:
        key = (_tag(event), _haystack(event))
        if key == prev_key:
            run_len += 1
        else:
            flush()
            prev_key = key
            run_start = event
            run_len = 1
    flush()
    freqs = [
        {"count": n, "tag": tag, "text": f"{n:>5}  {tag}"}
        for (tag, _), n in sr.freq.most_common(top)
    ]
    text_parts = ["=== Consecutive runs ==="]
    text_parts.extend(r["text"] for r in runs[:top]) if runs else text_parts.append("(none)")
    text_parts.append("")
    text_parts.append("=== Top payloads ===")
    text_parts.extend(f["text"] for f in freqs) if freqs else text_parts.append("(none)")
    return _ok(
        path_str,
        decoded,
        top=top,
        min_run=min_run,
        runs=runs[:top],
        top_payloads=freqs,
        text="\n".join(text_parts),
    )


def entity_timeline_dtj(
    session_path: str | Path,
    entity_id: str,
    *,
    limit: int = 100,
) -> dict[str, Any]:
    path_str = str(Path(session_path).expanduser())
    if not isinstance(entity_id, str) or not entity_id:
        return _fail(
            path_str,
            {"kind": "InvalidQuery", "message": "entity_id must be a non-empty string"},
        )
    limit_err = _validate_limit(limit)
    if limit_err:
        return _fail(path_str, limit_err)
    decoded = _load(session_path)
    if not decoded.get("ok"):
        return decoded
    matched = [
        e for e in _events(decoded) if _entity_field_match(e, entity_id)
    ]
    if not matched:
        return _ok(
            path_str,
            decoded,
            entity_id=entity_id,
            matched_count=0,
            returned_count=0,
            events=[],
            text=f"No lines for entity {entity_id!r}.",
        )
    returned = matched[:limit]
    header = (
        f"{len(matched)} line(s) for entity {entity_id!r}; "
        f"showing {len(returned)}.\n"
    )
    return _ok(
        path_str,
        decoded,
        entity_id=entity_id,
        matched_count=len(matched),
        returned_count=len(returned),
        events=returned,
        text=header + "\n".join(_compact(e) for e in returned),
    )


def entity_cluster_dtj(
    session_path: str | Path,
    entity_id: str,
    *,
    window: int = 20,
    limit: int = 30,
) -> dict[str, Any]:
    path_str = str(Path(session_path).expanduser())
    if not isinstance(entity_id, str) or not entity_id:
        return _fail(
            path_str,
            {"kind": "InvalidQuery", "message": "entity_id must be a non-empty string"},
        )
    win_err = _validate_window(window)
    if win_err:
        return _fail(path_str, win_err)
    limit_err = _validate_limit(limit)
    if limit_err:
        return _fail(path_str, limit_err)
    decoded = _load(session_path)
    if not decoded.get("ok"):
        return decoded
    events = _events(decoded)
    anchors = [i for i, e in enumerate(events) if _entity_field_match(e, entity_id)]
    if not anchors:
        return _ok(
            path_str,
            decoded,
            entity_id=entity_id,
            text=f"No lines for entity {entity_id!r}.",
            related=[],
            events=[],
        )
    related: Counter[str] = Counter()
    cluster_events: list[dict[str, Any]] = []
    seen_seq: set[int] = set()
    for idx in anchors:
        lo = max(0, idx - window)
        hi = min(len(events), idx + window + 1)
        for ev in events[lo:hi]:
            seq = ev.get("event_sequence")
            if not isinstance(seq, int) or seq in seen_seq:
                continue
            seen_seq.add(seq)
            for ref in _entity_refs(ev):
                if ref != entity_id:
                    related[ref] += 1
            cluster_events.append(ev)
    related_rows = [
        {"id": rid, "count": n} for rid, n in related.most_common(15)
    ]
    shown = cluster_events[:limit]
    rows = [
        f"primary={entity_id}  anchor_hits={len(anchors)}  window=±{window}",
        "related ids:",
    ]
    rows.extend(f"  {r['id']} (×{r['count']})" for r in related_rows) if related_rows else rows.append("  (none)")
    rows.append(f"events (up to {limit}):")
    rows.extend(_compact(e) for e in shown) if shown else rows.append("  (none)")
    if len(cluster_events) > limit:
        rows.append(f"  ... {len(cluster_events) - limit} more")
    return _ok(
        path_str,
        decoded,
        entity_id=entity_id,
        window=window,
        limit=limit,
        anchor_hits=len(anchors),
        related=related_rows,
        events=shown,
        text="\n".join(rows),
    )


def message_templates_dtj(
    session_path: str | Path,
    *,
    top: int = 10,
    since_last_clear: bool = False,
    category: str | None = None,
) -> dict[str, Any]:
    path_str = str(Path(session_path).expanduser())
    top_err = _validate_top(top)
    if top_err:
        return _fail(path_str, top_err)
    decoded = _load(session_path)
    if not decoded.get("ok"):
        return decoded
    scoped = _scope_events(
        _events(decoded), since_last_clear=since_last_clear, category=category
    )
    counts: Counter[str] = Counter(_event_template(e) for e in scoped)
    templates = [
        {"count": n, "template": tmpl, "text": f"{n:>5}  {tmpl[:100]}"}
        for tmpl, n in counts.most_common(top)
    ]
    text = "\n".join(t["text"] for t in templates) if templates else "No templates."
    return _ok(path_str, decoded, top=top, templates=templates, text=text)


def snapshot_diff_dtj(
    session_path: str | Path,
    *,
    event_sequence: int | None = None,
    limit: int = 5,
) -> dict[str, Any]:
    path_str = str(Path(session_path).expanduser())
    top_err = _validate_top(limit)
    if top_err:
        return _fail(path_str, top_err)
    if event_sequence is not None and (
        isinstance(event_sequence, bool) or not isinstance(event_sequence, int)
    ):
        return _fail(
            path_str,
            {
                "kind": "InvalidRange",
                "message": "event_sequence must be an integer when provided",
            },
        )
    decoded = _load(session_path)
    if not decoded.get("ok"):
        return decoded
    suspicious = ("mismatch", "<null>", "skip", "fail")
    blocks = _snapshot_events(_events(decoded))
    if event_sequence is not None:
        blocks = [e for e in blocks if e.get("event_sequence") == event_sequence]
    rows: list[str] = []
    structured: list[dict[str, Any]] = []
    for head in blocks:
        kv = _payload_kv(head)
        hay = _haystack(head).lower()
        if event_sequence is None and not any(s in hay for s in suspicious):
            continue
        if len(structured) >= limit:
            break
        flags = [s for s in suspicious if s in hay]
        structured.append(
            {
                "event_sequence": head.get("event_sequence"),
                "tag": _tag(head),
                "flags": flags or ["ok"],
                "fields": kv,
            }
        )
        rows.append(
            f"block@seq={head.get('event_sequence')}  {_tag(head)}  "
            f"flags={flags or ['ok']}"
        )
        for key in sorted(kv):
            val = kv[key]
            mark = (
                " ⚠"
                if any(x in val.lower() for x in ("null", "mismatch", "fail", "skip"))
                else ""
            )
            rows.append(f"  {key}={val}{mark}")
    text = "\n".join(rows) if rows else "No matching snapshot blocks."
    return _ok(
        path_str,
        decoded,
        event_sequence=event_sequence,
        limit=limit,
        blocks=structured,
        text=text,
    )


def pair_latency_dtj(
    session_path: str | Path,
    *,
    kind: str = "Dangling",
    since_last_clear: bool = False,
) -> dict[str, Any]:
    path_str = str(Path(session_path).expanduser())
    pair = next((p for p in EVENT_PAIRS if p[0] == kind), None)
    if pair is None:
        names = ", ".join(p[0] for p in EVENT_PAIRS)
        return _fail(
            path_str,
            {
                "kind": "InvalidQuery",
                "message": f"Unknown kind {kind!r}. Choose from: {names}",
            },
        )
    decoded = _load(session_path)
    if not decoded.get("ok"):
        return decoded
    scoped = _scope_events(_events(decoded), since_last_clear=since_last_clear)
    _name, opens, closes = pair
    opens_by_id: dict[str, dict[str, Any]] = {}
    latencies_ms: list[float] = []
    for event in scoped:
        eid = _entity_id_from_event(event)
        ev = str(event.get("event_name") or "")
        if _event_matches(ev, opens):
            if eid:
                opens_by_id[eid] = event
        elif _event_matches(ev, closes):
            if eid and eid in opens_by_id:
                a = _mono_sec(opens_by_id[eid])
                b = _mono_sec(event)
                if a is not None and b is not None:
                    latencies_ms.append((b - a) * 1000.0)
                del opens_by_id[eid]
    rows = [f"{kind} open→close latency (ms):"]
    stats: dict[str, Any] = {
        "closed": len(latencies_ms),
        "still_open": len(opens_by_id),
    }
    if latencies_ms:
        latencies_ms.sort()
        n = len(latencies_ms)
        p50 = latencies_ms[n // 2]
        p95 = latencies_ms[min(int(n * 0.95), n - 1)]
        stats.update({"p50": p50, "p95": p95, "max": latencies_ms[-1]})
        rows.append(
            f"  closed={n}  p50={p50:.1f}  p95={p95:.1f}  max={latencies_ms[-1]:.1f}"
        )
    else:
        rows.append("  closed=0")
    rows.append(f"  still_open={len(opens_by_id)}")
    open_samples = []
    if opens_by_id:
        rows.append("  open samples:")
        for eid, ev in list(opens_by_id.items())[:5]:
            open_samples.append(
                {
                    "id": eid,
                    "event_sequence": ev.get("event_sequence"),
                    "monotonic_ns": ev.get("monotonic_ns"),
                }
            )
            rows.append(
                f"    id={eid}  opened@seq={ev.get('event_sequence')}  "
                f"mono_ns={ev.get('monotonic_ns')}"
            )
    return _ok(
        path_str,
        decoded,
        kind=kind,
        stats=stats,
        open_samples=open_samples,
        text="\n".join(rows),
    )


def field_breakdown_dtj(
    session_path: str | Path,
    field: str,
    *,
    category: str | None = None,
    top: int = 20,
    since_last_clear: bool = False,
) -> dict[str, Any]:
    path_str = str(Path(session_path).expanduser())
    if not isinstance(field, str) or not field:
        return _fail(
            path_str,
            {"kind": "InvalidQuery", "message": "field must be a non-empty string"},
        )
    top_err = _validate_top(top)
    if top_err:
        return _fail(path_str, top_err)
    decoded = _load(session_path)
    if not decoded.get("ok"):
        return decoded
    scoped = _scope_events(
        _events(decoded), since_last_clear=since_last_clear, category=category
    )
    values: Counter[str] = Counter()
    for event in scoped:
        if category and (event.get("category") or "").lower() != category.lower():
            continue
        kv = _payload_kv(event)
        if field in kv:
            values[kv[field]] += 1
    rows = [{"value": v, "count": n, "text": f"{v} : {n}"} for v, n in values.most_common(top)]
    text = "\n".join(r["text"] for r in rows) if rows else f"No values for field {field!r}."
    return _ok(path_str, decoded, field=field, rows=rows, text=text)


def field_crosstab_dtj(
    session_path: str | Path,
    field_a: str,
    field_b: str,
    *,
    category: str | None = None,
    top: int = 20,
    since_last_clear: bool = False,
) -> dict[str, Any]:
    path_str = str(Path(session_path).expanduser())
    for name, val in (("field_a", field_a), ("field_b", field_b)):
        if not isinstance(val, str) or not val:
            return _fail(
                path_str,
                {
                    "kind": "InvalidQuery",
                    "message": f"{name} must be a non-empty string",
                },
            )
    top_err = _validate_top(top)
    if top_err:
        return _fail(path_str, top_err)
    decoded = _load(session_path)
    if not decoded.get("ok"):
        return decoded
    scoped = _scope_events(
        _events(decoded), since_last_clear=since_last_clear, category=category
    )
    pairs: Counter[tuple[str, str]] = Counter()
    for event in scoped:
        if category and (event.get("category") or "").lower() != category.lower():
            continue
        kv = _payload_kv(event)
        if field_a in kv and field_b in kv:
            pairs[(kv[field_a], kv[field_b])] += 1
    rows = [
        {
            "a": a,
            "b": b,
            "count": n,
            "text": f"{a} × {b} : {n}",
        }
        for (a, b), n in pairs.most_common(top)
    ]
    text = (
        "\n".join(r["text"] for r in rows)
        if rows
        else f"No pairs for {field_a!r} × {field_b!r}."
    )
    return _ok(path_str, decoded, field_a=field_a, field_b=field_b, rows=rows, text=text)


def sequence_gap_dtj(
    session_path: str | Path,
    open_event: str,
    close_event: str,
    *,
    max_lines: int = 100,
    limit: int = 50,
    since_last_clear: bool = False,
) -> dict[str, Any]:
    path_str = str(Path(session_path).expanduser())
    for name, val in (("open_event", open_event), ("close_event", close_event)):
        if not isinstance(val, str) or not val:
            return _fail(
                path_str,
                {
                    "kind": "InvalidQuery",
                    "message": f"{name} must be a non-empty string",
                },
            )
    if (
        isinstance(max_lines, bool)
        or not isinstance(max_lines, int)
        or max_lines < 1
        or max_lines > MAX_MAX_LINES
    ):
        return _fail(
            path_str,
            {
                "kind": "InvalidLimit",
                "message": f"max_lines must be an integer in 1..{MAX_MAX_LINES}",
                "max": MAX_MAX_LINES,
            },
        )
    limit_err = _validate_limit(limit)
    if limit_err:
        return _fail(path_str, limit_err)
    decoded = _load(session_path)
    if not decoded.get("ok"):
        return decoded
    close_parts = [p.strip() for p in close_event.split("|") if p.strip()]
    materialized = _scope_events(_events(decoded), since_last_clear=since_last_clear)
    gaps: list[dict[str, Any]] = []
    unclosed = 0
    for i, event in enumerate(materialized):
        ev = str(event.get("event_name") or "")
        if open_event.lower() not in ev.lower():
            continue
        if len(gaps) >= limit:
            break
        found = None
        for j in range(i + 1, min(i + 1 + max_lines, len(materialized))):
            other = str(materialized[j].get("event_name") or "").lower()
            if any(c.lower() in other for c in close_parts):
                found = materialized[j]
                break
        if found:
            delta = int(found["event_sequence"]) - int(event["event_sequence"])
            gaps.append(
                {
                    "open_event_sequence": event.get("event_sequence"),
                    "close_event_sequence": found.get("event_sequence"),
                    "delta": delta,
                    "closed": True,
                    "text": (
                        f"open@seq={event.get('event_sequence')} "
                        f"close@seq={found.get('event_sequence')} delta={delta}  "
                        f"{_tag(event)} -> {_tag(found)}"
                    ),
                }
            )
        else:
            unclosed += 1
            gaps.append(
                {
                    "open_event_sequence": event.get("event_sequence"),
                    "close_event_sequence": None,
                    "delta": None,
                    "closed": False,
                    "text": (
                        f"open@seq={event.get('event_sequence')} UNCLOSED within "
                        f"{max_lines} events  {_tag(event)}"
                    ),
                }
            )
    text_rows = [g["text"] for g in gaps]
    if unclosed:
        text_rows.insert(
            0, f"summary: {unclosed} unclosed (showing up to {limit} opens)"
        )
    text = (
        "\n".join(text_rows)
        if text_rows
        else f"No lines matching open_event {open_event!r}."
    )
    return _ok(
        path_str,
        decoded,
        open_event=open_event,
        close_event=close_event,
        max_lines=max_lines,
        limit=limit,
        unclosed=unclosed,
        gaps=gaps,
        text=text,
    )


def bursts_dtj(
    session_path: str | Path,
    *,
    window_sec: float = 1.0,
    min_count: int = 5,
    top: int = 10,
    since_last_clear: bool = False,
) -> dict[str, Any]:
    path_str = str(Path(session_path).expanduser())
    err = _validate_positive_float(window_sec, name="window_sec", max_v=MAX_WINDOW_SEC)
    if err:
        return _fail(path_str, err)
    if (
        isinstance(min_count, bool)
        or not isinstance(min_count, int)
        or min_count < 1
        or min_count > MAX_LIMIT
    ):
        return _fail(
            path_str,
            {
                "kind": "InvalidLimit",
                "message": f"min_count must be an integer in 1..{MAX_LIMIT}",
            },
        )
    top_err = _validate_top(top)
    if top_err:
        return _fail(path_str, top_err)
    decoded = _load(session_path)
    if not decoded.get("ok"):
        return decoded
    materialized = [
        e
        for e in _scope_events(_events(decoded), since_last_clear=since_last_clear)
        if _mono_sec(e) is not None
    ]
    if not materialized:
        return _ok(path_str, decoded, bursts=[], text="No timed lines.")
    results: list[tuple[int, dict[str, Any]]] = []
    i = 0
    while i < len(materialized):
        tag = _tag(materialized[i])
        t0 = _mono_sec(materialized[i]) or 0.0
        j = i + 1
        while j < len(materialized):
            tj = _mono_sec(materialized[j]) or 0.0
            if tj - t0 > window_sec or _tag(materialized[j]) != tag:
                break
            j += 1
        count = j - i
        if count >= min_count:
            results.append(
                (
                    count,
                    {
                        "count": count,
                        "tag": tag,
                        "start_event_sequence": materialized[i].get("event_sequence"),
                        "end_event_sequence": materialized[j - 1].get("event_sequence"),
                        "text": (
                            f"burst={count}  tag={tag}  "
                            f"seq={materialized[i].get('event_sequence')}-"
                            f"{materialized[j - 1].get('event_sequence')}  "
                            f"window={window_sec}s"
                        ),
                    },
                )
            )
        i = max(i + 1, j)
    results.sort(key=lambda x: -x[0])
    bursts = [r[1] for r in results[:top]]
    text = "\n".join(b["text"] for b in bursts) if bursts else "No bursts found."
    return _ok(path_str, decoded, bursts=bursts, text=text)


def gaps_dtj(
    session_path: str | Path,
    *,
    min_gap_sec: float = 2.0,
    top: int = 10,
    since_last_clear: bool = False,
) -> dict[str, Any]:
    path_str = str(Path(session_path).expanduser())
    err = _validate_positive_float(
        min_gap_sec, name="min_gap_sec", max_v=MAX_MIN_GAP_SEC
    )
    if err:
        return _fail(path_str, err)
    top_err = _validate_top(top)
    if top_err:
        return _fail(path_str, top_err)
    decoded = _load(session_path)
    if not decoded.get("ok"):
        return decoded
    materialized = [
        e
        for e in _scope_events(_events(decoded), since_last_clear=since_last_clear)
        if _mono_sec(e) is not None
    ]
    if len(materialized) < 2:
        return _ok(path_str, decoded, gaps=[], text="Not enough timed lines.")
    found: list[tuple[float, dict[str, Any]]] = []
    for i in range(len(materialized) - 1):
        a, b = materialized[i], materialized[i + 1]
        dt = (_mono_sec(b) or 0.0) - (_mono_sec(a) or 0.0)
        if dt >= min_gap_sec:
            found.append(
                (
                    dt,
                    {
                        "gap_sec": dt,
                        "after_event_sequence": a.get("event_sequence"),
                        "before_event_sequence": b.get("event_sequence"),
                        "text": (
                            f"gap={dt:.2f}s  after seq={a.get('event_sequence')}  "
                            f"before seq={b.get('event_sequence')}"
                        ),
                    },
                )
            )
    found.sort(key=lambda x: -x[0])
    gaps = [g[1] for g in found[:top]]
    text = (
        "\n".join(g["text"] for g in gaps)
        if gaps
        else f"No gaps >= {min_gap_sec}s."
    )
    return _ok(path_str, decoded, gaps=gaps, text=text)


def compare_sessions_dtj(session_path: str | Path) -> dict[str, Any]:
    path_str = str(Path(session_path).expanduser())
    decoded = _load(session_path)
    if not decoded.get("ok"):
        return decoded
    slices = _session_slices(_events(decoded))
    if len(slices) < 2:
        return _ok(
            path_str,
            decoded,
            text="Need at least 2 session segments (Session.Begin / trace cleared).",
            previous=None,
            last=None,
        )
    prev, last = slices[-2], slices[-1]
    prev_tags = Counter(_tag(e) for e in prev)
    last_tags = Counter(_tag(e) for e in last)
    prev_rf = Counter(dict(_scan(prev, sample_limit=0).red_counts))
    last_rf = Counter(dict(_scan(last, sample_limit=0).red_counts))
    rows = [
        f"previous: {len(prev)} lines  |  last: {len(last)} lines",
        "tag changes (last vs previous):",
    ]
    changes: list[tuple[int, str]] = []
    for tag in set(prev_tags) | set(last_tags):
        a, b = prev_tags.get(tag, 0), last_tags.get(tag, 0)
        if a != b:
            delta = b - a
            sign = "+" if delta > 0 else ""
            changes.append((abs(delta), f"  {tag}: {a} → {b} ({sign}{delta})"))
    changes.sort(key=lambda x: -x[0])
    rows.extend(c[1] for c in changes[:30])
    if not changes:
        rows.append("  (no tag count changes)")
    rows.append("red flag changes:")
    rf_changes = []
    for label in set(prev_rf) | set(last_rf):
        a, b = prev_rf.get(label, 0), last_rf.get(label, 0)
        if a != b:
            rf_changes.append(f"  {label}: {a} → {b}")
    rows.extend(rf_changes if rf_changes else ["  (none)"])
    return _ok(
        path_str,
        decoded,
        previous={"event_count": len(prev), "tags": dict(prev_tags)},
        last={"event_count": len(last), "tags": dict(last_tags)},
        text="\n".join(rows),
    )


def transition_matrix_dtj(
    session_path: str | Path,
    *,
    category: str | None = None,
    top: int = 10,
    since_last_clear: bool = False,
) -> dict[str, Any]:
    path_str = str(Path(session_path).expanduser())
    top_err = _validate_top(top)
    if top_err:
        return _fail(path_str, top_err)
    decoded = _load(session_path)
    if not decoded.get("ok"):
        return decoded
    materialized = [
        e
        for e in _scope_events(
            _events(decoded), since_last_clear=since_last_clear, category=category
        )
        if not category or (e.get("category") or "").lower() == category.lower()
    ]
    if len(materialized) < 2:
        return _ok(path_str, decoded, transitions=[], text="Not enough lines.")
    by_from: dict[str, Counter[str]] = {}
    for i in range(len(materialized) - 1):
        a, b = _tag(materialized[i]), _tag(materialized[i + 1])
        by_from.setdefault(a, Counter())[b] += 1
    ranked = sorted(by_from.items(), key=lambda x: -sum(x[1].values()))[:top]
    transitions = []
    rows = []
    for from_tag, counter in ranked:
        total = sum(counter.values())
        parts = [
            {"tag": t, "probability": n / total, "count": n}
            for t, n in counter.most_common(5)
        ]
        transitions.append({"from": from_tag, "total": total, "to": parts})
        rows.append(
            "after "
            + from_tag
            + ": "
            + ", ".join(f"{p['tag']}={p['probability']:.0%}" for p in parts)
        )
    return _ok(path_str, decoded, transitions=transitions, text="\n".join(rows))


def snapshot_before_after_dtj(
    session_path: str | Path,
    *,
    entity_id: str | None = None,
    event_sequence: int | None = None,
    limit: int = 20,
) -> dict[str, Any]:
    path_str = str(Path(session_path).expanduser())
    top_err = _validate_top(limit)
    if top_err:
        return _fail(path_str, top_err)
    decoded = _load(session_path)
    if not decoded.get("ok"):
        return decoded
    before: dict[str, tuple[dict[str, Any], dict[str, str]]] = {}
    after: dict[str, tuple[dict[str, Any], dict[str, str]]] = {}
    for head in _snapshot_events(_events(decoded)):
        if event_sequence is not None and head.get("event_sequence") != event_sequence:
            # Keep pairing global; filter pairs that involve this seq below.
            pass
        kv = _payload_kv(head)
        key = entity_id or kv.get("id") or str(head.get("event_sequence"))
        if entity_id and kv.get("id") != entity_id and not _entity_field_match(head, entity_id):
            continue
        ev = str(head.get("event_name") or "").lower()
        if "before" in ev:
            before[key] = (head, kv)
        elif "after" in ev:
            after[key] = (head, kv)
    keys = sorted(set(before) & set(after))[:limit]
    if event_sequence is not None:
        keys = [
            k
            for k in keys
            if before[k][0].get("event_sequence") == event_sequence
            or after[k][0].get("event_sequence") == event_sequence
        ]
    if not keys:
        return _ok(
            path_str,
            decoded,
            pairs=[],
            text="No matching before/after snapshot pairs.",
        )
    pairs = []
    rows: list[str] = []
    for key in keys:
        bh, bkv = before[key]
        ah, akv = after[key]
        diffs = []
        rows.append(f"=== id={key}  after@seq={ah.get('event_sequence')} ===")
        for field in sorted(set(bkv) | set(akv)):
            old, new = bkv.get(field), akv.get(field)
            if old != new:
                mark = (
                    " ⚠"
                    if new
                    and any(x in str(new).lower() for x in ("null", "mismatch", "fail"))
                    else ""
                )
                diffs.append({"field": field, "before": old, "after": new})
                rows.append(f"  {field}: {old} → {new}{mark}")
        pairs.append(
            {
                "id": key,
                "before_event_sequence": bh.get("event_sequence"),
                "after_event_sequence": ah.get("event_sequence"),
                "diffs": diffs,
            }
        )
        if len(rows) > limit * 6:
            break
    return _ok(path_str, decoded, pairs=pairs, text="\n".join(rows))


def first_last_dtj(
    session_path: str | Path,
    *,
    tag: str | None = None,
    entity_id: str | None = None,
) -> dict[str, Any]:
    path_str = str(Path(session_path).expanduser())
    if not tag and not entity_id:
        return _fail(
            path_str,
            {
                "kind": "InvalidQuery",
                "message": "Provide tag and/or entity_id.",
            },
        )
    decoded = _load(session_path)
    if not decoded.get("ok"):
        return decoded
    first = None
    last = None
    count = 0
    for event in _events(decoded):
        hit = False
        if tag and tag.lower() in _tag(event).lower():
            hit = True
        if entity_id and _entity_field_match(event, entity_id):
            hit = True
        if hit:
            count += 1
            if first is None:
                first = event
            last = event
    if not first or not last:
        return _ok(path_str, decoded, count=0, text="No matches.")
    text = "\n".join(
        [
            f"count={count}",
            f"first: seq={first.get('event_sequence')}  {_compact(first)}",
            f"last:  seq={last.get('event_sequence')}  {_compact(last)}",
        ]
    )
    return _ok(
        path_str,
        decoded,
        count=count,
        first=first,
        last=last,
        text=text,
    )


def density_timeline_dtj(
    session_path: str | Path,
    *,
    bucket_sec: float = 10.0,
    top_categories: int = 5,
    since_last_clear: bool = False,
) -> dict[str, Any]:
    path_str = str(Path(session_path).expanduser())
    err = _validate_positive_float(bucket_sec, name="bucket_sec", max_v=MAX_BUCKET_SEC)
    if err:
        return _fail(path_str, err)
    top_err = _validate_top(top_categories)
    if top_err:
        return _fail(path_str, top_err)
    decoded = _load(session_path)
    if not decoded.get("ok"):
        return decoded
    materialized = [
        e
        for e in _scope_events(_events(decoded), since_last_clear=since_last_clear)
        if _mono_sec(e) is not None
    ]
    if not materialized:
        return _ok(path_str, decoded, buckets=[], text="No timed lines.")
    t0 = _mono_sec(materialized[0]) or 0.0
    buckets: dict[int, Counter[str]] = {}
    for event in materialized:
        dt = (_mono_sec(event) or 0.0) - t0
        idx = int(dt // bucket_sec)
        buckets.setdefault(idx, Counter())[event.get("category") or "<none>"] += 1
    top_cats = {
        c
        for c, _ in Counter(
            cat for ctr in buckets.values() for cat in ctr
        ).most_common(top_categories)
    }
    out_buckets = []
    rows = []
    for idx in sorted(buckets):
        ctr = buckets[idx]
        start_s = t0 + idx * bucket_sec
        parts = ", ".join(f"{c}={ctr[c]}" for c in sorted(top_cats) if ctr.get(c))
        out_buckets.append(
            {
                "bucket_index": idx,
                "start_monotonic_s": start_s,
                "counts": {c: ctr[c] for c in sorted(top_cats) if ctr.get(c)},
            }
        )
        rows.append(f"+{start_s:.3f}s/+{bucket_sec:.0f}s  {parts}")
    return _ok(path_str, decoded, buckets=out_buckets, text="\n".join(rows))


def minimal_repro_dtj(
    session_path: str | Path,
    start: int,
    end: int,
) -> dict[str, Any]:
    path_str = str(Path(session_path).expanduser())
    if (
        isinstance(start, bool)
        or isinstance(end, bool)
        or not isinstance(start, int)
        or not isinstance(end, int)
    ):
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
    if end - start + 1 > MAX_LIMIT:
        return _fail(
            path_str,
            {
                "kind": "ResultTooLarge",
                "message": f"range size exceeds max {MAX_LIMIT}",
                "max": MAX_LIMIT,
            },
        )
    decoded = _load(session_path)
    if not decoded.get("ok"):
        return decoded
    subset = [
        e
        for e in _events(decoded)
        if isinstance(e.get("event_sequence"), int)
        and start <= e["event_sequence"] <= end
    ]
    if not subset:
        return _ok(
            path_str,
            decoded,
            events=[],
            text=f"No lines in [{start}, {end}].",
        )
    kept: list[dict[str, Any]] = []
    seen: set[tuple[str, frozenset[str]]] = set()
    for event in subset:
        sig = (_tag(event), frozenset(_entity_refs(event)))
        if sig not in seen:
            seen.add(sig)
            kept.append(event)
    rows = [
        f"minimal repro: {len(kept)} of {len(subset)} lines (range {start}-{end})",
    ]
    rows.extend(_compact(e) for e in kept)
    return _ok(
        path_str,
        decoded,
        start=start,
        end=end,
        original_count=len(subset),
        kept_count=len(kept),
        events=kept,
        text="\n".join(rows),
    )


def _baseline_build(events: list[dict[str, Any]]) -> dict[str, Any]:
    sr = _scan(events, sample_limit=0)
    return {
        "version": 1,
        "format": "dtj-baseline-v1",
        "lines": len(events),
        "tags": dict(Counter(_tag(e) for e in events)),
        "categories": dict(sr.categories),
        "red_flags": dict(sr.red_counts),
        "last_monotonic_ns": sr.last_mono,
    }


def baseline_save_dtj(
    session_path: str | Path,
    baseline_path: str | Path,
) -> dict[str, Any]:
    """Save session stats baseline to an explicit local path (no hidden global)."""
    path_str = str(Path(session_path).expanduser())
    base_path = Path(baseline_path).expanduser()
    if not str(baseline_path).strip():
        return _fail(
            path_str,
            {
                "kind": "InvalidPath",
                "message": "baseline_path must be an explicit non-empty path",
            },
        )
    decoded = _load(session_path)
    if not decoded.get("ok"):
        return decoded
    data = _baseline_build(_events(decoded))
    data["session_path"] = path_str
    try:
        base_path.parent.mkdir(parents=True, exist_ok=True)
        with base_path.open("w", encoding="utf-8") as fh:
            json.dump(data, fh, indent=2, ensure_ascii=False)
            fh.write("\n")
    except OSError as exc:
        return _fail(
            path_str,
            {
                "kind": "Io",
                "message": f"failed to write baseline: {exc}",
                "baseline_path": str(base_path),
            },
        )
    text = f"baseline saved: {base_path}  lines={data['lines']}"
    return _ok(
        path_str,
        decoded,
        baseline_path=str(base_path),
        baseline=data,
        text=text,
    )


def baseline_diff_dtj(
    session_path: str | Path,
    baseline_path: str | Path,
) -> dict[str, Any]:
    path_str = str(Path(session_path).expanduser())
    base_path = Path(baseline_path).expanduser()
    if not str(baseline_path).strip():
        return _fail(
            path_str,
            {
                "kind": "InvalidPath",
                "message": "baseline_path must be an explicit non-empty path",
            },
        )
    if not base_path.is_file():
        return _fail(
            path_str,
            {
                "kind": "BaselineMissing",
                "message": f"No baseline at {base_path}. Call baseline_save first.",
                "baseline_path": str(base_path),
            },
        )
    decoded = _load(session_path)
    if not decoded.get("ok"):
        return decoded
    try:
        with base_path.open(encoding="utf-8") as fh:
            base = json.load(fh)
    except (OSError, json.JSONDecodeError) as exc:
        return _fail(
            path_str,
            {
                "kind": "CorruptBaseline",
                "message": f"baseline is not readable JSON: {exc}",
                "baseline_path": str(base_path),
            },
        )
    if not isinstance(base, dict):
        return _fail(
            path_str,
            {
                "kind": "CorruptBaseline",
                "message": "baseline root must be a JSON object",
            },
        )
    current = _baseline_build(_events(decoded))
    rows = [
        f"baseline: {base_path}",
        f"lines: {base.get('lines')} → {current['lines']}",
    ]
    base_tags = base.get("tags", {}) if isinstance(base.get("tags"), dict) else {}
    cur_tags = current["tags"]
    changes: list[tuple[int, str]] = []
    for tag in set(base_tags) | set(cur_tags):
        a, b = int(base_tags.get(tag, 0) or 0), int(cur_tags.get(tag, 0) or 0)
        if a != b:
            changes.append((abs(b - a), f"  {tag}: {a} → {b}"))
    changes.sort(key=lambda x: -x[0])
    rows.append("tag diff:")
    rows.extend(c[1] for c in changes[:25])
    if not changes:
        rows.append("  (none)")
    base_rf = base.get("red_flags", {}) if isinstance(base.get("red_flags"), dict) else {}
    cur_rf = current["red_flags"]
    rf = [
        f"  {k}: {int(base_rf.get(k, 0) or 0)} → {int(cur_rf.get(k, 0) or 0)}"
        for k in set(base_rf) | set(cur_rf)
        if int(base_rf.get(k, 0) or 0) != int(cur_rf.get(k, 0) or 0)
    ]
    rows.append("red flag diff:")
    rows.extend(rf if rf else ["  (none)"])
    return _ok(
        path_str,
        decoded,
        baseline_path=str(base_path),
        baseline=base,
        current=current,
        text="\n".join(rows),
    )


def persistence_mismatches_dtj(
    session_path: str | Path,
    *,
    top: int = 10,
) -> dict[str, Any]:
    path_str = str(Path(session_path).expanduser())
    top_err = _validate_top(top)
    if top_err:
        return _fail(path_str, top_err)
    decoded = _load(session_path)
    if not decoded.get("ok"):
        return decoded
    events = _events(decoded)
    sync_lines = [
        _compact(e)
        for e in events
        if e.get("category") == "Snapshot" and "AfterSync" in str(e.get("event_name") or "")
    ][:top]
    snap = snapshot_diff_dtj(session_path, limit=top)
    mismatch = [
        _compact(e)
        for e in events
        if "mismatch" in _haystack(e).lower()
    ][:top]
    parts = [
        "=== Snapshot AfterSync ===",
        *(sync_lines if sync_lines else ["(none)"]),
        "\n=== Snapshot diff ===",
        snap.get("text", "(none)"),
        "\n=== mismatch markers ===",
        *(mismatch if mismatch else ["(none)"]),
    ]
    return _ok(
        path_str,
        decoded,
        top=top,
        after_sync=sync_lines,
        mismatches=mismatch,
        text="\n".join(parts),
    )
