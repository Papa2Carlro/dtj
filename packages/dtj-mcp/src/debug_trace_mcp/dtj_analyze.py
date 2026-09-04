"""Diagnostic analysis family over native DTJ sessions.

Semantics ported from ``wire-trace-mcp`` analytics (EVENT_PAIRS, RED_FLAGS,
session boundaries, causal entity walk, presets). Addressing uses
``event_sequence`` instead of log lineno. Presentation text is derived from
structured DTJ events — never stored as Wire Trace log lines.
"""

from __future__ import annotations

from collections import Counter, defaultdict
from dataclasses import dataclass, field
from pathlib import Path
from typing import Any

from .dtj_read import ADAPTER_NAME, ADAPTER_VERSION, read_session_dtj

MAX_TOP = 100
MAX_HOPS = 50
MAX_LIMIT = 500
DEFAULT_TOP = 5
DEFAULT_HOPS = 5

ENTITY_FIELD_KEYS = frozenset({"id", "wire", "networkId", "sourceId", "route"})

# Same pair table as wire-trace-mcp.analytics.EVENT_PAIRS
EVENT_PAIRS: tuple[tuple[str, tuple[str, ...], tuple[str, ...]], ...] = (
    ("Dangling", ("Created",), ("Destroyed", "Consumed")),
    ("Begin", ("Begin",), ("Success", "Commit", "Dispose", "End")),
    ("Arm", ("Arm", "Arming"), ("Release",)),
    ("Candidate", ("CandidateBegin",), ("Commit", "Cancel")),
)

# (label, substring, case_insensitive) — same as wire-trace-mcp.analytics.RED_FLAGS
RED_FLAGS: tuple[tuple[str, str, bool], ...] = (
    ("null-angle", "<null>", True),
    ("null-paren", "(null)", True),
    ("skip", "skip", True),
    ("clamped", "clamped=True", False),
    ("cached-polyline", "usedCachedPolylineSnapshot=true", False),
    ("canBranch-false", "canBranch=false", False),
    ("failed", "result=Failed", False),
    ("mismatch", "mismatch", True),
)

PRESET_REPORTS = frozenset(
    {
        "branch",
        "dangling",
        "graph_undo",
        "tip_hold",
        "commit_boundary",
        "gesture_route",
    }
)


@dataclass
class _Scan:
    events: list[dict[str, Any]]
    categories: Counter[str] = field(default_factory=Counter)
    opens: Counter[str] = field(default_factory=Counter)
    closes: Counter[str] = field(default_factory=Counter)
    freq: Counter[tuple[str, str]] = field(default_factory=Counter)
    red_counts: Counter[str] = field(default_factory=Counter)
    red_samples: dict[str, list[dict[str, Any]]] = field(
        default_factory=lambda: defaultdict(list)
    )
    first_mono: int | None = None
    last_mono: int | None = None


def _ok(session_path: str, decoded: dict[str, Any], **extra: Any) -> dict[str, Any]:
    out = {
        "ok": True,
        "adapter": decoded.get("adapter")
        or {"name": ADAPTER_NAME, "version": ADAPTER_VERSION},
        "session_path": session_path,
        "torn_tail": bool(decoded.get("torn_tail", False)),
    }
    out.update(extra)
    return out


def _fail(session_path: str, error: dict[str, Any]) -> dict[str, Any]:
    return {
        "ok": False,
        "adapter": {"name": ADAPTER_NAME, "version": ADAPTER_VERSION},
        "session_path": session_path,
        "error": error,
    }


def _validate_top(top: Any) -> dict[str, Any] | None:
    if isinstance(top, bool) or not isinstance(top, int) or top < 1 or top > MAX_TOP:
        return {
            "kind": "InvalidLimit",
            "message": f"top must be an integer in 1..{MAX_TOP}",
            "max": MAX_TOP,
        }
    return None


def _validate_limit(limit: Any) -> dict[str, Any] | None:
    if (
        isinstance(limit, bool)
        or not isinstance(limit, int)
        or limit < 1
        or limit > MAX_LIMIT
    ):
        return {
            "kind": "InvalidLimit",
            "message": f"limit must be an integer in 1..{MAX_LIMIT}",
            "max": MAX_LIMIT,
        }
    return None


def _validate_hops(hops: Any) -> dict[str, Any] | None:
    if isinstance(hops, bool) or not isinstance(hops, int) or hops < 1 or hops > MAX_HOPS:
        return {
            "kind": "InvalidLimit",
            "message": f"hops must be an integer in 1..{MAX_HOPS}",
            "max": MAX_HOPS,
        }
    return None


def _load(session_path: str | Path) -> dict[str, Any]:
    return read_session_dtj(session_path)


def _events(decoded: dict[str, Any]) -> list[dict[str, Any]]:
    raw = decoded.get("events") or []
    return [e for e in raw if isinstance(e, dict)]


def _tag(event: dict[str, Any]) -> str:
    cat = event.get("category") or ""
    name = event.get("event_name") or ""
    return f"{cat}.{name}" if cat or name else "<none>"


def _string_parts(event: dict[str, Any]) -> list[str]:
    parts: list[str] = []
    for key in ("domain", "category", "event_name", "correlation", "severity"):
        val = event.get(key)
        if isinstance(val, str) and val:
            parts.append(val)
    payload = event.get("payload")
    if isinstance(payload, list):
        for item in payload:
            if not isinstance(item, dict):
                continue
            name = item.get("name")
            if isinstance(name, str):
                parts.append(name)
            typ = item.get("type")
            val = item.get("value")
            if typ == "interned_string" and isinstance(val, str):
                parts.append(val)
            elif typ in {"bool", "i32", "i64", "u32", "u64", "f32", "f64", "enum"}:
                parts.append(str(val))
            elif typ == "bytes" and isinstance(item.get("hex"), str):
                parts.append(item["hex"])
    return parts


def _haystack(event: dict[str, Any]) -> str:
    """Text used for substring red-flag / cancel scans.

    Includes ``name=value`` pairs so markers like ``result=Failed`` match the
    legacy Wire Trace message shape.
    """
    parts = _string_parts(event)
    payload = event.get("payload")
    if isinstance(payload, list):
        for item in payload:
            if not isinstance(item, dict):
                continue
            name = item.get("name")
            if not isinstance(name, str):
                continue
            if item.get("type") == "interned_string" and isinstance(
                item.get("value"), str
            ):
                parts.append(f"{name}={item['value']}")
            elif item.get("type") in {
                "bool",
                "i32",
                "i64",
                "u32",
                "u64",
                "f32",
                "f64",
                "enum",
            }:
                parts.append(f"{name}={item.get('value')}")
    return " ".join(parts)


def _compact(event: dict[str, Any]) -> str:
    seq = event.get("event_sequence")
    corr = event.get("correlation") or "-"
    fields = []
    payload = event.get("payload")
    if isinstance(payload, list):
        for item in payload:
            if not isinstance(item, dict):
                continue
            name = item.get("name")
            if not isinstance(name, str):
                continue
            if item.get("type") == "interned_string" and isinstance(
                item.get("value"), str
            ):
                fields.append(f"{name}={item['value']}")
            elif item.get("type") in {
                "bool",
                "i32",
                "i64",
                "u32",
                "u64",
                "f32",
                "f64",
                "enum",
            }:
                fields.append(f"{name}={item.get('value')}")
    msg = " ".join(fields)
    if len(msg) > 120:
        msg = msg[:117] + "..."
    return f"{seq}\t{_tag(event)}\tcorr={corr}\t{msg}".rstrip()


def _entity_refs(event: dict[str, Any]) -> set[str]:
    refs: set[str] = set()
    corr = event.get("correlation")
    if isinstance(corr, str) and corr:
        refs.add(corr)
    payload = event.get("payload")
    if not isinstance(payload, list):
        return refs
    for item in payload:
        if not isinstance(item, dict):
            continue
        name = item.get("name")
        if name not in ENTITY_FIELD_KEYS:
            continue
        if item.get("type") == "interned_string" and isinstance(item.get("value"), str):
            refs.add(item["value"])
        elif item.get("type") in {"i32", "i64", "u32", "u64"} and item.get("value") is not None:
            refs.add(str(item["value"]))
    return refs


def _entity_id_from_event(event: dict[str, Any]) -> str | None:
    payload = event.get("payload")
    if not isinstance(payload, list):
        return None
    by_name: dict[str, str] = {}
    for item in payload:
        if not isinstance(item, dict):
            continue
        name = item.get("name")
        if name not in ENTITY_FIELD_KEYS:
            continue
        if item.get("type") == "interned_string" and isinstance(item.get("value"), str):
            by_name[str(name)] = item["value"]
        elif item.get("type") in {"i32", "i64", "u32", "u64"} and item.get("value") is not None:
            by_name[str(name)] = str(item["value"])
    return (
        by_name.get("id")
        or by_name.get("networkId")
        or by_name.get("sourceId")
        or by_name.get("wire")
    )


def _event_matches(event_name: str, substrings: tuple[str, ...]) -> bool:
    return any(s in event_name for s in substrings)


def _is_session_boundary(event: dict[str, Any]) -> bool:
    cat = event.get("category") or ""
    name = event.get("event_name") or ""
    if cat == "Session" and "Begin" in name:
        return True
    if _tag(event) == "Session.Begin":
        return True
    if cat == "Lifecycle" and name == "trace":
        return "cleared" in _haystack(event).lower()
    return False


def _last_boundary_seq(events: list[dict[str, Any]]) -> int | None:
    boundaries = [
        e.get("event_sequence")
        for e in events
        if _is_session_boundary(e) and isinstance(e.get("event_sequence"), int)
    ]
    return boundaries[-1] if boundaries else None


def _count_sessions(events: list[dict[str, Any]]) -> int:
    n = sum(1 for e in events if _is_session_boundary(e))
    return n if n > 0 else (1 if events else 0)


def _scope_events(
    events: list[dict[str, Any]],
    *,
    since_last_clear: bool = False,
    category: str | None = None,
    mono_from_ns: int | None = None,
    mono_to_ns: int | None = None,
) -> list[dict[str, Any]]:
    since_seq = _last_boundary_seq(events) if since_last_clear else None
    out: list[dict[str, Any]] = []
    for event in events:
        seq = event.get("event_sequence")
        if since_seq is not None and isinstance(seq, int) and seq < since_seq:
            continue
        if category is not None and event.get("category") != category:
            continue
        mono = event.get("monotonic_ns")
        if mono_from_ns is not None and isinstance(mono, int) and mono < mono_from_ns:
            continue
        if mono_to_ns is not None and isinstance(mono, int) and mono > mono_to_ns:
            continue
        out.append(event)
    return out


def _scan(events: list[dict[str, Any]], *, sample_limit: int) -> _Scan:
    sr = _Scan(events=events)
    for event in events:
        cat = event.get("category") or "<none>"
        sr.categories[cat] += 1
        mono = event.get("monotonic_ns")
        if isinstance(mono, int):
            if sr.first_mono is None:
                sr.first_mono = mono
            sr.last_mono = mono
        ev_name = str(event.get("event_name") or "")
        for pair_name, opens, closes in EVENT_PAIRS:
            if _event_matches(ev_name, opens):
                sr.opens[pair_name] += 1
            elif _event_matches(ev_name, closes):
                sr.closes[pair_name] += 1
        sr.freq[(_tag(event), _haystack(event))] += 1
        text = _haystack(event)
        for label, substr, ci in RED_FLAGS:
            hay = text.lower() if ci else text
            needle = substr.lower() if ci else substr
            if needle in hay:
                sr.red_counts[label] += 1
                if len(sr.red_samples[label]) < sample_limit:
                    sr.red_samples[label].append(event)
    return sr


def _balance_rows(sr: _Scan, *, only_unbalanced: bool) -> list[dict[str, Any]]:
    rows: list[dict[str, Any]] = []
    for pair_name, _, _ in EVENT_PAIRS:
        o, c = sr.opens[pair_name], sr.closes[pair_name]
        diff = o - c
        if only_unbalanced and diff == 0:
            continue
        rows.append(
            {
                "pair": pair_name,
                "created": o,
                "closed": c,
                "unmatched": abs(diff),
                "balanced": diff == 0,
                "text": (
                    f"{pair_name}: created={o} closed={c} -> balanced"
                    if diff == 0
                    else f"{pair_name}: created={o} closed={c} -> {abs(diff)} unmatched ⚠"
                ),
            }
        )
    rows.sort(key=lambda r: (-r["unmatched"], r["pair"]))
    return rows


def _repetition(
    sr: _Scan, *, top: int, min_run: int = 3
) -> tuple[list[dict[str, Any]], list[dict[str, Any]]]:
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
                    "text": f"run={run_len}  seq={run_start.get('event_sequence')}  {_tag(run_start)}",
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
        {
            "count": n,
            "tag": tag,
            "text": f"{n:>5}  {tag}",
        }
        for (tag, _msg), n in sr.freq.most_common(top)
    ]
    return runs[:top], freqs


def last_session_dtj(session_path: str | Path) -> dict[str, Any]:
    path_str = str(Path(session_path).expanduser())
    decoded = _load(session_path)
    if not decoded.get("ok"):
        return decoded
    events = _events(decoded)
    if not events:
        return _ok(
            path_str,
            decoded,
            text="No events.",
            session={
                "boundary_event_sequence": None,
                "events_in_session": 0,
            },
        )
    boundaries = [e for e in events if _is_session_boundary(e)]
    if not boundaries:
        text = (
            f"session start: event_sequence 1 (no explicit boundary)\n"
            f"events in session: {len(events)}"
        )
        session = {
            "boundary_event_sequence": 1,
            "explicit_boundary": False,
            "events_in_session": len(events),
        }
    else:
        last = boundaries[-1]
        seq = int(last["event_sequence"])
        count = sum(
            1
            for e in events
            if isinstance(e.get("event_sequence"), int)
            and e["event_sequence"] >= seq
        )
        text = (
            f"session start: event_sequence {seq}  tag {_tag(last)}\n"
            f"message: {_compact(last)[:80]}\n"
            f"events since boundary: {count}"
        )
        session = {
            "boundary_event_sequence": seq,
            "explicit_boundary": True,
            "tag": _tag(last),
            "events_in_session": count,
        }
    return _ok(path_str, decoded, text=text, session=session)


def event_balance_dtj(
    session_path: str | Path,
    *,
    only_unbalanced: bool = True,
    since_last_clear: bool = False,
    category: str | None = None,
    mono_from_ns: int | None = None,
    mono_to_ns: int | None = None,
) -> dict[str, Any]:
    path_str = str(Path(session_path).expanduser())
    decoded = _load(session_path)
    if not decoded.get("ok"):
        return decoded
    scoped = _scope_events(
        _events(decoded),
        since_last_clear=since_last_clear,
        category=category,
        mono_from_ns=mono_from_ns,
        mono_to_ns=mono_to_ns,
    )
    rows = _balance_rows(_scan(scoped, sample_limit=1), only_unbalanced=only_unbalanced)
    text = (
        "\n".join(r["text"] for r in rows)
        if rows
        else "All event pairs balanced."
    )
    return _ok(
        path_str,
        decoded,
        only_unbalanced=only_unbalanced,
        since_last_clear=since_last_clear,
        category=category,
        rows=rows,
        text=text,
    )


def unmatched_entities_dtj(
    session_path: str | Path,
    *,
    kind: str = "Dangling",
    limit: int = 50,
    since_last_clear: bool = False,
) -> dict[str, Any]:
    path_str = str(Path(session_path).expanduser())
    limit_err = _validate_limit(limit)
    if limit_err:
        return _fail(path_str, limit_err)
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
    opened: dict[str, dict[str, Any]] = {}
    closed: set[str] = set()
    last_seen: dict[str, dict[str, Any]] = {}
    for event in scoped:
        eid = _entity_id_from_event(event)
        if not eid:
            continue
        last_seen[eid] = event
        ev = str(event.get("event_name") or "")
        if _event_matches(ev, opens):
            opened[eid] = event
        elif _event_matches(ev, closes):
            closed.add(eid)
    unmatched = [eid for eid in opened if eid not in closed]
    entities = []
    for eid in unmatched[:limit]:
        create = opened[eid]
        last = last_seen.get(eid, create)
        entities.append(
            {
                "id": eid,
                "created_event_sequence": create.get("event_sequence"),
                "last_event_sequence": last.get("event_sequence"),
                "last_tag": _tag(last),
                "text": (
                    f"  id={eid}  created@seq={create.get('event_sequence')}  "
                    f"last@seq={last.get('event_sequence')}  tag={_tag(last)}"
                ),
            }
        )
    if unmatched:
        lines = [f"{kind}: {len(unmatched)} unmatched of {len(opened)} opened"]
        lines.extend(e["text"] for e in entities)
        if len(unmatched) > limit:
            lines.append(f"  ... {len(unmatched) - limit} more")
        text = "\n".join(lines)
    else:
        text = f"{kind}: all {len(opened)} opened entities were closed"
    return _ok(
        path_str,
        decoded,
        kind=kind,
        limit=limit,
        unmatched_count=len(unmatched),
        opened_count=len(opened),
        entities=entities,
        text=text,
    )


def red_flags_dtj(
    session_path: str | Path,
    *,
    top: int = DEFAULT_TOP,
    since_last_clear: bool = False,
    category: str | None = None,
    mono_from_ns: int | None = None,
    mono_to_ns: int | None = None,
) -> dict[str, Any]:
    path_str = str(Path(session_path).expanduser())
    top_err = _validate_top(top)
    if top_err:
        return _fail(path_str, top_err)
    decoded = _load(session_path)
    if not decoded.get("ok"):
        return decoded
    scoped = _scope_events(
        _events(decoded),
        since_last_clear=since_last_clear,
        category=category,
        mono_from_ns=mono_from_ns,
        mono_to_ns=mono_to_ns,
    )
    sr = _scan(scoped, sample_limit=top)
    flags = []
    text_rows: list[str] = []
    for label, _, _ in RED_FLAGS:
        n = sr.red_counts[label]
        if n == 0:
            continue
        samples = sr.red_samples[label]
        flags.append(
            {
                "label": label,
                "count": n,
                "samples": [
                    {
                        "event_sequence": e.get("event_sequence"),
                        "tag": _tag(e),
                        "compact": _compact(e),
                    }
                    for e in samples
                ],
            }
        )
        text_rows.append(f"{label}: {n}")
        text_rows.extend(f"  {_compact(e)}" for e in samples)
    text = "\n".join(text_rows) if text_rows else "No red flags found."
    return _ok(
        path_str,
        decoded,
        top=top,
        since_last_clear=since_last_clear,
        category=category,
        flags=flags,
        text=text,
    )


def causal_chain_dtj(
    session_path: str | Path,
    event_sequence: int,
    *,
    hops: int = DEFAULT_HOPS,
) -> dict[str, Any]:
    path_str = str(Path(session_path).expanduser())
    hops_err = _validate_hops(hops)
    if hops_err:
        return _fail(path_str, hops_err)
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
    events = _events(decoded)
    index_by_seq = {
        e["event_sequence"]: i
        for i, e in enumerate(events)
        if isinstance(e.get("event_sequence"), int)
    }
    if event_sequence not in index_by_seq:
        return _fail(
            path_str,
            {
                "kind": "MissingEvent",
                "message": f"event_sequence {event_sequence} not found",
                "event_sequence": event_sequence,
            },
        )
    chain = [events[index_by_seq[event_sequence]]]
    refs = _entity_refs(chain[0])
    search = index_by_seq[event_sequence] - 1
    # Bound walk: at most len(events) steps, stop at hops+1 chain length.
    steps = 0
    max_steps = len(events)
    while len(chain) < hops + 1 and search >= 0 and steps < max_steps:
        steps += 1
        ln = events[search]
        ln_refs = _entity_refs(ln)
        if refs & ln_refs:
            chain.append(ln)
            refs |= ln_refs
        search -= 1
    chain.reverse()
    hops_out = []
    text_rows = []
    for i, ev in enumerate(chain):
        marker = "  <<" if ev.get("event_sequence") == event_sequence else ""
        hops_out.append(
            {
                "hop": i,
                "event_sequence": ev.get("event_sequence"),
                "tag": _tag(ev),
                "refs": sorted(_entity_refs(ev)),
                "compact": _compact(ev),
                "is_target": ev.get("event_sequence") == event_sequence,
            }
        )
        text_rows.append(f"hop {i}: {_compact(ev)}{marker}")
    return _ok(
        path_str,
        decoded,
        event_sequence=event_sequence,
        hops=hops,
        chain=hops_out,
        text="\n".join(text_rows),
    )


def analyze_dtj(
    session_path: str | Path,
    *,
    top: int = DEFAULT_TOP,
    since_last_clear: bool = False,
    category: str | None = None,
    mono_from_ns: int | None = None,
    mono_to_ns: int | None = None,
) -> dict[str, Any]:
    path_str = str(Path(session_path).expanduser())
    top_err = _validate_top(top)
    if top_err:
        return _fail(path_str, top_err)
    decoded = _load(session_path)
    if not decoded.get("ok"):
        return decoded
    all_events = _events(decoded)
    scoped = _scope_events(
        all_events,
        since_last_clear=since_last_clear,
        category=category,
        mono_from_ns=mono_from_ns,
        mono_to_ns=mono_to_ns,
    )
    if not scoped:
        empty = (
            "=== Overview ===\n(none)\n\n"
            "=== Event balance ===\n(none)\n\n"
            "=== Repetition ===\n(none)\n\n"
            "=== Red flags ===\n(none)"
        )
        return _ok(
            path_str,
            decoded,
            top=top,
            since_last_clear=since_last_clear,
            category=category,
            text=empty,
            overview={"event_count": 0, "sessions": 0, "top_categories": []},
            balance=[],
            repetition={"runs": [], "top_payloads": []},
            red_flags=[],
        )
    sr = _scan(scoped, sample_limit=top)
    scope_note = ""
    if since_last_clear:
        boundary = _last_boundary_seq(all_events)
        scope_note = f" (since event_sequence {boundary})"
    cat_rows = ", ".join(f"{c}={n}" for c, n in sr.categories.most_common(top))
    overview_text = (
        f"mono span: {sr.first_mono} .. {sr.last_mono}\n"
        f"events: {len(sr.events)}\n"
        f"sessions: {_count_sessions(sr.events)}\n"
        f"top categories: {cat_rows}"
    )
    bal = _balance_rows(sr, only_unbalanced=True)
    runs, freqs = _repetition(sr, top=top)
    flags = []
    flag_text: list[str] = []
    for label, _, _ in RED_FLAGS:
        n = sr.red_counts[label]
        if n == 0:
            continue
        samples = sr.red_samples[label]
        flags.append(
            {
                "label": label,
                "count": n,
                "samples": [_compact(e) for e in samples],
            }
        )
        flag_text.append(f"{label}: {n}")
        flag_text.extend(f"  {_compact(e)}" for e in samples)

    rep_parts = []
    if runs:
        rep_parts.append("consecutive runs:")
        rep_parts.extend(f"  {r['text']}" for r in runs)
    else:
        rep_parts.append("consecutive runs: (none)")
    if freqs:
        rep_parts.append("top payloads:")
        rep_parts.extend(f"  {f['text']}" for f in freqs)
    else:
        rep_parts.append("top payloads: (none)")

    sections = [
        f"=== Overview{scope_note} ===\n{overview_text}",
        "=== Event balance ===\n"
        + ("\n".join(r["text"] for r in bal) if bal else "(none)"),
        "=== Repetition ===\n" + "\n".join(rep_parts),
        "=== Red flags ===\n" + ("\n".join(flag_text) if flag_text else "(none)"),
    ]
    return _ok(
        path_str,
        decoded,
        top=top,
        since_last_clear=since_last_clear,
        category=category,
        text="\n\n".join(sections),
        overview={
            "first_monotonic_ns": sr.first_mono,
            "last_monotonic_ns": sr.last_mono,
            "event_count": len(sr.events),
            "sessions": _count_sessions(sr.events),
            "top_categories": [
                {"category": c, "count": n} for c, n in sr.categories.most_common(top)
            ],
        },
        balance=bal,
        repetition={"runs": runs, "top_payloads": freqs},
        red_flags=flags,
    )


def since_last_repro_dtj(session_path: str | Path, *, top: int = DEFAULT_TOP) -> dict[str, Any]:
    return analyze_dtj(session_path, top=top, since_last_clear=True)


def trace_brief_dtj(
    session_path: str | Path,
    *,
    top: int = DEFAULT_TOP,
    since_last_clear: bool = True,
    category: str | None = None,
    mono_from_ns: int | None = None,
    mono_to_ns: int | None = None,
) -> dict[str, Any]:
    path_str = str(Path(session_path).expanduser())
    top_err = _validate_top(top)
    if top_err:
        return _fail(path_str, top_err)
    decoded = _load(session_path)
    if not decoded.get("ok"):
        return decoded
    all_events = _events(decoded)
    since_seq = _last_boundary_seq(all_events) if since_last_clear else None
    scoped = _scope_events(
        all_events,
        since_last_clear=since_last_clear,
        category=category,
        mono_from_ns=mono_from_ns,
        mono_to_ns=mono_to_ns,
    )
    start_seq = (
        scoped[0]["event_sequence"]
        if scoped and isinstance(scoped[0].get("event_sequence"), int)
        else (since_seq or 1)
    )
    scope_block = {
        "since_last_clear": since_last_clear,
        "category": category,
        "start_event_sequence": start_seq,
        "event_count": len(scoped),
    }
    if not scoped:
        brief = {
            "version": 1,
            "scope": scope_block,
            "overview": {
                "first_monotonic_ns": None,
                "last_monotonic_ns": None,
                "sessions": 0,
                "top_categories": [],
                "top_tags": [],
            },
            "findings": [],
            "red_flags": [],
            "recommended_next_calls": [
                {
                    "tool": "session_stats_dtj",
                    "args": {},
                    "reason": "No events in scope; confirm session path and repro.",
                }
            ],
        }
        return _ok(path_str, decoded, brief=brief, text=_json_indent(brief))

    sr = _scan(scoped, sample_limit=top)
    tags = Counter(_tag(e) for e in scoped)
    overview = {
        "first_monotonic_ns": sr.first_mono,
        "last_monotonic_ns": sr.last_mono,
        "sessions": _count_sessions(scoped),
        "top_categories": [
            {"category": c, "count": n} for c, n in sr.categories.most_common(top)
        ],
        "top_tags": [{"tag": t, "count": n} for t, n in tags.most_common(top)],
    }
    findings: list[dict[str, Any]] = []
    has_balance_issue = False
    for pair_name, _, _ in EVENT_PAIRS:
        o, c = sr.opens[pair_name], sr.closes[pair_name]
        diff = o - c
        if diff == 0:
            continue
        has_balance_issue = True
        findings.append(
            {
                "severity": "high",
                "type": "event_balance",
                "summary": f"{pair_name}: created={o} closed={c} -> {abs(diff)} unmatched",
                "event_refs": [],
            }
        )
    red_flags_out: list[dict[str, Any]] = []
    first_red: dict[str, Any] | None = None
    for label, _, _ in RED_FLAGS:
        count = sr.red_counts[label]
        if count == 0:
            continue
        samples = []
        for e in sr.red_samples[label]:
            samples.append(
                {
                    "event_sequence": e.get("event_sequence"),
                    "tag": _tag(e),
                    "category": e.get("category"),
                    "event_name": e.get("event_name"),
                    "fields": _payload_kv(e),
                    "compact": _compact(e)[:160],
                }
            )
        if samples and first_red is None:
            first_red = sr.red_samples[label][0]
        red_flags_out.append({"label": label, "count": count, "samples": samples})
        findings.append(
            {
                "severity": "medium",
                "type": "red_flag",
                "summary": f"{label}: {count} hit(s)",
                "event_refs": [s["event_sequence"] for s in samples],
            }
        )
    recommended: list[dict[str, Any]] = []
    if first_red is not None:
        recommended.append(
            {
                "tool": "session_context_dtj",
                "args": {
                    "event_sequence": first_red.get("event_sequence"),
                    "before": 15,
                    "after": 15,
                },
                "reason": "Inspect first red-flag sample in local context.",
            }
        )
    elif has_balance_issue:
        recommended.append(
            {
                "tool": "session_event_balance_dtj",
                "args": {"since_last_clear": since_last_clear},
                "reason": "Event pair imbalance detected; review open/close counts.",
            }
        )
    elif not findings:
        recommended.append(
            {
                "tool": "session_stats_dtj",
                "args": {},
                "reason": "No high-priority findings; inspect tag distribution.",
            }
        )
    brief = {
        "version": 1,
        "scope": scope_block,
        "overview": overview,
        "findings": findings,
        "red_flags": red_flags_out,
        "recommended_next_calls": recommended,
    }
    return _ok(path_str, decoded, brief=brief, text=_json_indent(brief))


def _payload_kv(event: dict[str, Any]) -> dict[str, str]:
    out: dict[str, str] = {}
    payload = event.get("payload")
    if not isinstance(payload, list):
        return out
    for item in payload:
        if not isinstance(item, dict):
            continue
        name = item.get("name")
        if not isinstance(name, str):
            continue
        if item.get("type") == "interned_string" and isinstance(item.get("value"), str):
            out[name] = item["value"]
        elif item.get("type") in {
            "bool",
            "i32",
            "i64",
            "u32",
            "u64",
            "f32",
            "f64",
            "enum",
        }:
            out[name] = str(item.get("value"))
    return out


def _json_indent(data: dict[str, Any]) -> str:
    import json

    return json.dumps(data, ensure_ascii=False, indent=2)


def _filter_category(
    events: list[dict[str, Any]], category: str, *, top: int
) -> list[str]:
    rows = [_compact(e) for e in events if e.get("category") == category][:top]
    return rows if rows else ["(none)"]


def preset_report_dtj(
    session_path: str | Path,
    report: str,
    *,
    top: int = 10,
) -> dict[str, Any]:
    path_str = str(Path(session_path).expanduser())
    top_err = _validate_top(top)
    if top_err:
        return _fail(path_str, top_err)
    if report not in PRESET_REPORTS:
        return _fail(
            path_str,
            {
                "kind": "InvalidQuery",
                "message": (
                    f"Unknown wire trace report: {report}. Use branch, dangling, "
                    "graph_undo, tip_hold, commit_boundary, gesture_route."
                ),
            },
        )
    decoded = _load(session_path)
    if not decoded.get("ok"):
        return decoded
    events = _events(decoded)
    if report == "dangling":
        um = unmatched_entities_dtj(session_path, kind="Dangling", limit=top)
        null_lines = [
            _compact(e)
            for e in events
            if "<null>" in _haystack(e).lower()
            and e.get("category") in ("Lifecycle", "Visual")
        ][:top]
        parts = [
            "=== Unmatched danglings ===",
            um.get("text", ""),
            "\n=== signal=<null> on Lifecycle/Visual ===",
            *(null_lines if null_lines else ["(none)"]),
        ]
        text = "\n".join(parts)
    elif report == "branch":
        bal = event_balance_dtj(session_path, only_unbalanced=True)
        parts = [
            "=== Event balance ===",
            bal.get("text", ""),
            "\n=== BranchRelease ===",
            *_filter_category(events, "BranchRelease", top=top),
            "\n=== BranchSplit ===",
            *_filter_category(events, "BranchSplit", top=top),
        ]
        text = "\n".join(parts)
    elif report == "graph_undo":
        undo = _filter_category(events, "GraphUndo", top=top)
        fails = [
            _compact(e)
            for e in events
            if e.get("category") == "GraphUndo"
            and ("Failed" in _haystack(e) or "fail" in _haystack(e).lower())
        ][:top]
        parts = [
            "=== GraphUndo (all) ===",
            *undo,
            "\n=== Failures ===",
            *(fails if fails else ["(none)"]),
        ]
        text = "\n".join(parts)
    elif report == "gesture_route":
        gr = [e for e in events if e.get("category") == "GestureRoute"]
        sr = _scan(gr, sample_limit=top)
        runs, _ = _repetition(sr, top=top)
        parts = [
            "=== GestureRoute events ===",
            *_filter_category(events, "GestureRoute", top=top),
            "\n=== Repetition (GestureRoute) ===",
            *([r["text"] for r in runs] if runs else ["(none)"]),
        ]
        text = "\n".join(parts)
    elif report == "tip_hold":
        cancels = [
            _compact(e)
            for e in events
            if e.get("category") == "TipHold" and "cancel" in _haystack(e).lower()
        ][:top]
        parts = [
            "=== TipHold events ===",
            *_filter_category(events, "TipHold", top=top),
            "\n=== Cancels ===",
            *(cancels if cancels else ["(none)"]),
        ]
        text = "\n".join(parts)
    else:  # commit_boundary
        bal = event_balance_dtj(session_path, only_unbalanced=True)
        parts = ["=== Event balance ===", bal.get("text", "")]
        for cat in ("CommitBoundary", "CleanupBoundary", "DropCommit", "GraphUndo"):
            parts.append(f"\n=== {cat} ===")
            parts.extend(_filter_category(events, cat, top=top))
        text = "\n".join(parts)
    return _ok(path_str, decoded, report=report, top=top, text=text)


def analyze_bundle_dtj(
    session_path: str | Path,
    *,
    top: int = DEFAULT_TOP,
    since_last_clear: bool = True,
) -> dict[str, Any]:
    """Parity with Tauri analyze_bundle: session + since_last_repro + brief + balance + flags."""
    path_str = str(Path(session_path).expanduser())
    top_err = _validate_top(top)
    if top_err:
        return _fail(path_str, top_err)
    session = last_session_dtj(session_path)
    if not session.get("ok"):
        return session
    analyze = since_last_repro_dtj(session_path, top=top)
    if not analyze.get("ok"):
        return analyze
    brief = trace_brief_dtj(
        session_path, top=top, since_last_clear=since_last_clear
    )
    if not brief.get("ok"):
        return brief
    balance = event_balance_dtj(session_path, since_last_clear=since_last_clear)
    if not balance.get("ok"):
        return balance
    flags = red_flags_dtj(session_path, top=top)
    if not flags.get("ok"):
        return flags
    bundle = {
        "session": session.get("text", ""),
        "analyze": analyze.get("text", ""),
        "brief": brief.get("text", ""),
        "balance": balance.get("text", ""),
        "flags": flags.get("text", ""),
    }
    return _ok(
        path_str,
        session,
        top=top,
        since_last_clear=since_last_clear,
        bundle=bundle,
    )


def line_bundle_dtj(
    session_path: str | Path,
    event_sequence: int,
    *,
    before: int = 8,
    after: int = 8,
    hops: int = DEFAULT_HOPS,
) -> dict[str, Any]:
    """Parity with Tauri line_bundle: context + causal_chain for one event."""
    from .dtj_query import context_around_event_dtj

    path_str = str(Path(session_path).expanduser())
    ctx = context_around_event_dtj(
        session_path, event_sequence, before=before, after=after
    )
    if not ctx.get("ok"):
        return ctx
    chain = causal_chain_dtj(session_path, event_sequence, hops=hops)
    if not chain.get("ok"):
        return chain
    context_text = "\n".join(
        (">> " if e.get("event_sequence") == event_sequence else "   ")
        + _compact(e)
        for e in ctx.get("events") or []
        if isinstance(e, dict)
    )
    bundle = {
        "context": context_text or "No context.",
        "chain": chain.get("text", ""),
    }
    return _ok(
        path_str,
        ctx,
        event_sequence=event_sequence,
        before=before,
        after=after,
        hops=hops,
        bundle=bundle,
    )
