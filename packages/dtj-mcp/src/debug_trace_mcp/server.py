"""Standalone Debug Trace MCP server (DocHub plugin — not part of doc-memory).

All filesystem paths are supplied by the MCP caller. No project-specific defaults.
Unity must never call these tools per event; config is polled from a local JSON file.

Production surface is native DTJ only (`*_dtj` + config/protocol helpers).
MCP wire names omit the redundant `debug_trace_` prefix (server is already
`debug-trace`). JSONL prototype helpers remain as library modules for unit
tests and are **not** registered on this server.
"""

from __future__ import annotations

import json
from pathlib import Path
from typing import Any

from mcp.server.fastmcp import FastMCP

from . import config_io
from .dtj_index import index_session_dtj, search_session_dtj_index
from .dtj_analytics import (
    baseline_diff_dtj,
    baseline_save_dtj,
    bursts_dtj,
    compare_sessions_dtj,
    density_timeline_dtj,
    entity_cluster_dtj,
    entity_timeline_dtj,
    field_breakdown_dtj,
    field_crosstab_dtj,
    first_last_dtj,
    gaps_dtj,
    message_templates_dtj,
    minimal_repro_dtj,
    pair_latency_dtj,
    persistence_mismatches_dtj,
    repetition_dtj,
    sequence_gap_dtj,
    snapshot_before_after_dtj,
    snapshot_diff_dtj,
    transition_matrix_dtj,
)
from .dtj_catalog import event_catalog_dtj, render_event_catalog_markdown
from .dtj_prompts import PROMPTS, prompt_text
from .dtj_analyze import (
    analyze_bundle_dtj,
    analyze_dtj,
    causal_chain_dtj,
    event_balance_dtj,
    last_session_dtj,
    line_bundle_dtj,
    preset_report_dtj,
    red_flags_dtj,
    since_last_repro_dtj,
    trace_brief_dtj,
    unmatched_entities_dtj,
)
from .dtj_query import (
    context_around_event_dtj,
    correlation_neighbourhood_dtj,
    get_event_range_dtj,
    regex_search_dtj,
    session_info_dtj,
    stats_report_dtj,
    structured_search_dtj,
    tail_events_dtj,
    text_search_dtj,
)
from .dtj_read import read_session_dtj
from .dtj_report import performance_report_dtj_index
from .dtj_store import DtjSessionStore
from .protocol import (
    PERFORMANCE_RULE,
    default_config,
    protocol_document,
    validate_config,
)

mcp = FastMCP("debug-trace")


def _json(data: Any) -> str:
    return json.dumps(data, indent=2, ensure_ascii=False)


def _parse_json_object(raw: str, label: str) -> dict[str, Any]:
    try:
        obj = json.loads(raw)
    except json.JSONDecodeError as exc:
        raise ValueError(f"{label} is not valid JSON: {exc.msg}") from exc
    if not isinstance(obj, dict):
        raise ValueError(f"{label} must be a JSON object")
    return obj


@mcp.tool(name="protocol_get")
def debug_trace_protocol_get() -> str:
    """Return Debug Trace protocol docs: ownership, schemas, examples, performance rule."""
    return _json(protocol_document())


@mcp.resource(
    "debug-trace://event-catalog",
    title="Event catalog",
    description=(
        "Catalog of registered DTG/DTJ event schemas (empty until a registry "
        "is loaded via event_catalog)."
    ),
)
def debug_trace_event_catalog_resource() -> str:
    """Markdown catalog SoT = schema registry (not UI chips / Wire Trace strings)."""
    return render_event_catalog_markdown(event_catalog_dtj(None))


@mcp.tool(name="event_catalog")
def debug_trace_event_catalog(registry_path: str | None = None) -> str:
    """Build event catalog from an explicit local Event Schema Registry path.

    Omit registry_path for an empty catalog. Unknown/corrupt registries fail closed.
    """
    try:
        return _json(event_catalog_dtj(registry_path))
    except Exception as exc:  # noqa: BLE001
        return _json(
            {
                "ok": False,
                "error": {"kind": "Io", "message": str(exc)},
                "registry_path": registry_path,
            }
        )


@mcp.prompt(
    name="triage",
    title=PROMPTS["triage"]["title"],
    description=PROMPTS["triage"]["description"],
)
def debug_trace_triage() -> str:
    """Recommended first steps after reproducing a Debug Trace bug."""
    return prompt_text("triage")


@mcp.prompt(
    name="dangling",
    title=PROMPTS["dangling"]["title"],
    description=PROMPTS["dangling"]["description"],
)
def debug_trace_dangling() -> str:
    """Investigate a dangling that was not destroyed."""
    return prompt_text("dangling")


@mcp.prompt(
    name="persistence",
    title=PROMPTS["persistence"]["title"],
    description=PROMPTS["persistence"]["description"],
)
def debug_trace_persistence() -> str:
    """Investigate Snapshot / persistence sync mismatches."""
    return prompt_text("persistence")


@mcp.tool(name="config_get")
def debug_trace_config_get(config_path: str) -> str:
    """Read DebugTraceConfig from an explicit local path (default stub if missing)."""
    try:
        cfg = config_io.get_or_default(config_path)
        exists = Path(config_path).is_file()
        return _json(
            {
                "ok": True,
                "config_path": config_path,
                "exists": exists,
                "config": cfg,
                "performanceRule": PERFORMANCE_RULE,
            }
        )
    except Exception as exc:  # noqa: BLE001 — surface to MCP caller
        return _json({"ok": False, "error": str(exc), "config_path": config_path})


@mcp.tool(name="config_set")
def debug_trace_config_set(config_path: str, config_json: str) -> str:
    """Validate and atomically write DebugTraceConfig to config_path (temp + replace).

    Unity should poll this file at safe boundaries. Never call MCP per event.
    """
    try:
        cfg = _parse_json_object(config_json, "config_json")
        base = default_config()
        base.update(cfg)
        errors = validate_config(base)
        if errors:
            return _json({"ok": False, "errors": errors})
        path = config_io.write_config_atomic(config_path, base)
        return _json(
            {
                "ok": True,
                "config_path": str(path),
                "config": base,
                "performanceRule": PERFORMANCE_RULE,
            }
        )
    except Exception as exc:  # noqa: BLE001
        return _json({"ok": False, "error": str(exc), "config_path": config_path})


# ---------------------------------------------------------------------------
# Native DTJ tool family (first complete diagnostic set)
# ---------------------------------------------------------------------------


@mcp.tool(name="sessions_list_dtj")
def debug_trace_sessions_list_dtj(store_dir: str) -> str:
    """List native `.dtj` sessions under an explicit store_dir (source of truth)."""
    try:
        return _json(DtjSessionStore(store_dir).list_sessions())
    except Exception as exc:  # noqa: BLE001
        return _json(
            {
                "ok": False,
                "error": {"kind": "Io", "message": str(exc)},
                "store_dir": store_dir,
            }
        )


@mcp.tool(name="session_info_dtj")
def debug_trace_session_info_dtj(session_path: str) -> str:
    """Open metadata for one native `.dtj` session (domains/categories/counts)."""
    try:
        return _json(session_info_dtj(session_path))
    except Exception as exc:  # noqa: BLE001
        return _json(
            {
                "ok": False,
                "error": {"kind": "Io", "message": str(exc)},
                "session_path": session_path,
            }
        )


@mcp.tool(name="session_read_dtj")
def debug_trace_session_read_dtj(session_path: str) -> str:
    """Read-only decode of a native DTJ v1 `.dtj` session (Python byte reader).

    Hard limits, CRC, committed-chunk semantics, torn-tail recovery, unknown
    chunk skip, and malformed-file errors follow Docs/guides/dtj-format-v1.md.
    """
    try:
        return _json(read_session_dtj(session_path))
    except Exception as exc:  # noqa: BLE001
        return _json(
            {
                "ok": False,
                "error": {"kind": "Io", "message": str(exc)},
                "session_path": session_path,
            }
        )


@mcp.tool(name="session_tail_dtj")
def debug_trace_session_tail_dtj(session_path: str, n: int = 50) -> str:
    """Return the last ``n`` events from a native `.dtj` session (bounded)."""
    try:
        return _json(tail_events_dtj(session_path, n=n))
    except Exception as exc:  # noqa: BLE001
        return _json(
            {
                "ok": False,
                "error": {"kind": "Io", "message": str(exc)},
                "session_path": session_path,
            }
        )


@mcp.tool(name="session_events_dtj")
def debug_trace_session_events_dtj(
    session_path: str, start: int, end: int
) -> str:
    """Return events by inclusive ``event_sequence`` range [start, end]."""
    try:
        return _json(get_event_range_dtj(session_path, start, end))
    except Exception as exc:  # noqa: BLE001
        return _json(
            {
                "ok": False,
                "error": {"kind": "Io", "message": str(exc)},
                "session_path": session_path,
            }
        )


@mcp.tool(name="session_context_dtj")
def debug_trace_session_context_dtj(
    session_path: str,
    event_sequence: int,
    before: int = 30,
    after: int = 30,
) -> str:
    """Context window around an ``event_sequence`` in a native `.dtj` session."""
    try:
        return _json(
            context_around_event_dtj(
                session_path,
                event_sequence,
                before=before,
                after=after,
            )
        )
    except Exception as exc:  # noqa: BLE001
        return _json(
            {
                "ok": False,
                "error": {"kind": "Io", "message": str(exc)},
                "session_path": session_path,
            }
        )


@mcp.tool(name="session_search_dtj")
def debug_trace_session_search_dtj(
    session_path: str,
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
    limit: int = 100,
    offset: int = 0,
) -> str:
    """list_traces parity: AND filters over one `.dtj` (event_sequence order).

    Time window uses mono_from_ns/mono_to_ns (DTJ monotonic), not wall-clock.
    ``event`` is substring on event_name; ``event_name`` is exact.
    ``text`` ≈ legacy contains; ``exclude`` / ``exclude_category`` drop matches.
    """
    try:
        return _json(
            structured_search_dtj(
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
        )
    except Exception as exc:  # noqa: BLE001
        return _json(
            {
                "ok": False,
                "error": {"kind": "Io", "message": str(exc)},
                "session_path": session_path,
            }
        )


@mcp.tool(name="session_text_search_dtj")
def debug_trace_session_text_search_dtj(
    session_path: str,
    query: str,
    domain: str | None = None,
    category: str | None = None,
    limit: int = 50,
) -> str:
    """Case-insensitive text substring search over resolved DTJ string fields."""
    try:
        return _json(
            text_search_dtj(
                session_path,
                query,
                domain=domain,
                category=category,
                limit=limit,
            )
        )
    except Exception as exc:  # noqa: BLE001
        return _json(
            {
                "ok": False,
                "error": {"kind": "Io", "message": str(exc)},
                "session_path": session_path,
            }
        )


@mcp.tool(name="session_regex_search_dtj")
def debug_trace_session_regex_search_dtj(
    session_path: str,
    pattern: str,
    domain: str | None = None,
    category: str | None = None,
    limit: int = 50,
) -> str:
    """Bounded regex search over resolved DTJ text (pattern length + shape gated)."""
    try:
        return _json(
            regex_search_dtj(
                session_path,
                pattern,
                domain=domain,
                category=category,
                limit=limit,
            )
        )
    except Exception as exc:  # noqa: BLE001
        return _json(
            {
                "ok": False,
                "error": {"kind": "Io", "message": str(exc)},
                "session_path": session_path,
            }
        )


@mcp.tool(name="session_correlation_dtj")
def debug_trace_session_correlation_dtj(
    session_path: str,
    correlation_id: str,
    before: int = 5,
    after: int = 5,
    limit: int = 50,
) -> str:
    """Correlation neighbourhood: matching events plus bounded event neighbours."""
    try:
        return _json(
            correlation_neighbourhood_dtj(
                session_path,
                correlation_id,
                before=before,
                after=after,
                limit=limit,
            )
        )
    except Exception as exc:  # noqa: BLE001
        return _json(
            {
                "ok": False,
                "error": {"kind": "Io", "message": str(exc)},
                "session_path": session_path,
            }
        )


@mcp.tool(name="session_stats_dtj")
def debug_trace_session_stats_dtj(
    session_path: str,
    domain: str | None = None,
    category: str | None = None,
) -> str:
    """Stats / performance report: counts by facet + duration-like field aggregates."""
    try:
        return _json(
            stats_report_dtj(session_path, domain=domain, category=category)
        )
    except Exception as exc:  # noqa: BLE001
        return _json(
            {
                "ok": False,
                "error": {"kind": "Io", "message": str(exc)},
                "session_path": session_path,
            }
        )


# ---------------------------------------------------------------------------
# Native DTJ diagnostic analysis family (Wire Trace analyze / brief / presets)
# ---------------------------------------------------------------------------


@mcp.tool(name="session_analyze_dtj")
def debug_trace_session_analyze_dtj(
    session_path: str,
    top: int = 5,
    since_last_clear: bool = False,
    category: str | None = None,
    mono_from_ns: int | None = None,
    mono_to_ns: int | None = None,
) -> str:
    """First-call summary: overview, event balance, repetition, red flags."""
    try:
        return _json(
            analyze_dtj(
                session_path,
                top=top,
                since_last_clear=since_last_clear,
                category=category,
                mono_from_ns=mono_from_ns,
                mono_to_ns=mono_to_ns,
            )
        )
    except Exception as exc:  # noqa: BLE001
        return _json(
            {
                "ok": False,
                "error": {"kind": "Io", "message": str(exc)},
                "session_path": session_path,
            }
        )


@mcp.tool(name="session_since_last_repro_dtj")
def debug_trace_session_since_last_repro_dtj(
    session_path: str, top: int = 5
) -> str:
    """Shortcut: analyze scoped since last Session.Begin / Lifecycle.trace cleared."""
    try:
        return _json(since_last_repro_dtj(session_path, top=top))
    except Exception as exc:  # noqa: BLE001
        return _json(
            {
                "ok": False,
                "error": {"kind": "Io", "message": str(exc)},
                "session_path": session_path,
            }
        )


@mcp.tool(name="session_trace_brief_dtj")
def debug_trace_session_trace_brief_dtj(
    session_path: str,
    top: int = 5,
    since_last_clear: bool = True,
    category: str | None = None,
    mono_from_ns: int | None = None,
    mono_to_ns: int | None = None,
) -> str:
    """Agent-first structured JSON brief with findings and next-call hints."""
    try:
        return _json(
            trace_brief_dtj(
                session_path,
                top=top,
                since_last_clear=since_last_clear,
                category=category,
                mono_from_ns=mono_from_ns,
                mono_to_ns=mono_to_ns,
            )
        )
    except Exception as exc:  # noqa: BLE001
        return _json(
            {
                "ok": False,
                "error": {"kind": "Io", "message": str(exc)},
                "session_path": session_path,
            }
        )


@mcp.tool(name="session_red_flags_dtj")
def debug_trace_session_red_flags_dtj(
    session_path: str,
    top: int = 5,
    since_last_clear: bool = False,
    category: str | None = None,
    mono_from_ns: int | None = None,
    mono_to_ns: int | None = None,
) -> str:
    """Scan suspicious markers (<null>, mismatch, result=Failed, …)."""
    try:
        return _json(
            red_flags_dtj(
                session_path,
                top=top,
                since_last_clear=since_last_clear,
                category=category,
                mono_from_ns=mono_from_ns,
                mono_to_ns=mono_to_ns,
            )
        )
    except Exception as exc:  # noqa: BLE001
        return _json(
            {
                "ok": False,
                "error": {"kind": "Io", "message": str(exc)},
                "session_path": session_path,
            }
        )


@mcp.tool(name="session_event_balance_dtj")
def debug_trace_session_event_balance_dtj(
    session_path: str,
    only_unbalanced: bool = True,
    since_last_clear: bool = False,
    category: str | None = None,
    mono_from_ns: int | None = None,
    mono_to_ns: int | None = None,
) -> str:
    """Open/close event pair imbalance (Created/Destroyed, Begin/End, …)."""
    try:
        return _json(
            event_balance_dtj(
                session_path,
                only_unbalanced=only_unbalanced,
                since_last_clear=since_last_clear,
                category=category,
                mono_from_ns=mono_from_ns,
                mono_to_ns=mono_to_ns,
            )
        )
    except Exception as exc:  # noqa: BLE001
        return _json(
            {
                "ok": False,
                "error": {"kind": "Io", "message": str(exc)},
                "session_path": session_path,
            }
        )


@mcp.tool(name="session_causal_chain_dtj")
def debug_trace_session_causal_chain_dtj(
    session_path: str, event_sequence: int, hops: int = 5
) -> str:
    """Walk backward from an event following shared entity/correlation ids."""
    try:
        return _json(
            causal_chain_dtj(session_path, event_sequence, hops=hops)
        )
    except Exception as exc:  # noqa: BLE001
        return _json(
            {
                "ok": False,
                "error": {"kind": "Io", "message": str(exc)},
                "session_path": session_path,
            }
        )


@mcp.tool(name="session_last_session_dtj")
def debug_trace_session_last_session_dtj(session_path: str) -> str:
    """Describe the most recent session/repro boundary inside one `.dtj` file."""
    try:
        return _json(last_session_dtj(session_path))
    except Exception as exc:  # noqa: BLE001
        return _json(
            {
                "ok": False,
                "error": {"kind": "Io", "message": str(exc)},
                "session_path": session_path,
            }
        )


@mcp.tool(name="session_unmatched_entities_dtj")
def debug_trace_session_unmatched_entities_dtj(
    session_path: str,
    kind: str = "Dangling",
    limit: int = 50,
    since_last_clear: bool = False,
) -> str:
    """Per-entity open/close lifecycle for an EVENT_PAIR kind."""
    try:
        return _json(
            unmatched_entities_dtj(
                session_path,
                kind=kind,
                limit=limit,
                since_last_clear=since_last_clear,
            )
        )
    except Exception as exc:  # noqa: BLE001
        return _json(
            {
                "ok": False,
                "error": {"kind": "Io", "message": str(exc)},
                "session_path": session_path,
            }
        )


@mcp.tool(name="session_preset_report_dtj")
def debug_trace_session_preset_report_dtj(
    session_path: str, report: str, top: int = 10
) -> str:
    """Domain preset report: branch/dangling/graph_undo/tip_hold/commit_boundary/gesture_route."""
    try:
        return _json(preset_report_dtj(session_path, report, top=top))
    except Exception as exc:  # noqa: BLE001
        return _json(
            {
                "ok": False,
                "error": {"kind": "Io", "message": str(exc)},
                "session_path": session_path,
            }
        )


@mcp.tool(name="session_analyze_bundle_dtj")
def debug_trace_session_analyze_bundle_dtj(
    session_path: str, top: int = 5, since_last_clear: bool = True
) -> str:
    """One-shot bundle: session + since_last_repro + brief + balance + flags."""
    try:
        return _json(
            analyze_bundle_dtj(
                session_path, top=top, since_last_clear=since_last_clear
            )
        )
    except Exception as exc:  # noqa: BLE001
        return _json(
            {
                "ok": False,
                "error": {"kind": "Io", "message": str(exc)},
                "session_path": session_path,
            }
        )


@mcp.tool(name="session_line_bundle_dtj")
def debug_trace_session_line_bundle_dtj(
    session_path: str,
    event_sequence: int,
    before: int = 8,
    after: int = 8,
    hops: int = 5,
) -> str:
    """One-shot bundle: context_around + causal_chain for an event_sequence."""
    try:
        return _json(
            line_bundle_dtj(
                session_path,
                event_sequence,
                before=before,
                after=after,
                hops=hops,
            )
        )
    except Exception as exc:  # noqa: BLE001
        return _json(
            {
                "ok": False,
                "error": {"kind": "Io", "message": str(exc)},
                "session_path": session_path,
            }
        )


# ---------------------------------------------------------------------------
# Native DTJ advanced analytics family
# ---------------------------------------------------------------------------


@mcp.tool(name="session_repetition_dtj")
def debug_trace_session_repetition_dtj(
    session_path: str,
    top: int = 10,
    min_run: int = 3,
    since_last_clear: bool = False,
    category: str | None = None,
) -> str:
    """Consecutive identical-event runs and top payload frequencies."""
    try:
        return _json(
            repetition_dtj(
                session_path,
                top=top,
                min_run=min_run,
                since_last_clear=since_last_clear,
                category=category,
            )
        )
    except Exception as exc:  # noqa: BLE001
        return _json(
            {
                "ok": False,
                "error": {"kind": "Io", "message": str(exc)},
                "session_path": session_path,
            }
        )


@mcp.tool(name="session_entity_timeline_dtj")
def debug_trace_session_entity_timeline_dtj(
    session_path: str, entity_id: str, limit: int = 100
) -> str:
    """Events matching entity id on structured id/wire/networkId/sourceId/correlation."""
    try:
        return _json(
            entity_timeline_dtj(session_path, entity_id, limit=limit)
        )
    except Exception as exc:  # noqa: BLE001
        return _json(
            {
                "ok": False,
                "error": {"kind": "Io", "message": str(exc)},
                "session_path": session_path,
            }
        )


@mcp.tool(name="session_entity_cluster_dtj")
def debug_trace_session_entity_cluster_dtj(
    session_path: str, entity_id: str, window: int = 20, limit: int = 30
) -> str:
    """Related entity ids and events near entity_id anchors (±window events)."""
    try:
        return _json(
            entity_cluster_dtj(
                session_path, entity_id, window=window, limit=limit
            )
        )
    except Exception as exc:  # noqa: BLE001
        return _json(
            {
                "ok": False,
                "error": {"kind": "Io", "message": str(exc)},
                "session_path": session_path,
            }
        )


@mcp.tool(name="session_message_templates_dtj")
def debug_trace_session_message_templates_dtj(
    session_path: str,
    top: int = 10,
    since_last_clear: bool = False,
    category: str | None = None,
) -> str:
    """Frequency of abstracted payload templates (field values → {})."""
    try:
        return _json(
            message_templates_dtj(
                session_path,
                top=top,
                since_last_clear=since_last_clear,
                category=category,
            )
        )
    except Exception as exc:  # noqa: BLE001
        return _json(
            {
                "ok": False,
                "error": {"kind": "Io", "message": str(exc)},
                "session_path": session_path,
            }
        )


@mcp.tool(name="session_snapshot_diff_dtj")
def debug_trace_session_snapshot_diff_dtj(
    session_path: str,
    event_sequence: int | None = None,
    limit: int = 5,
) -> str:
    """Inspect Snapshot category events (optional event_sequence filter)."""
    try:
        return _json(
            snapshot_diff_dtj(
                session_path, event_sequence=event_sequence, limit=limit
            )
        )
    except Exception as exc:  # noqa: BLE001
        return _json(
            {
                "ok": False,
                "error": {"kind": "Io", "message": str(exc)},
                "session_path": session_path,
            }
        )


@mcp.tool(name="session_pair_latency_dtj")
def debug_trace_session_pair_latency_dtj(
    session_path: str, kind: str = "Dangling", since_last_clear: bool = False
) -> str:
    """Open→close latency stats for an EVENT_PAIR kind (monotonic_ns)."""
    try:
        return _json(
            pair_latency_dtj(
                session_path, kind=kind, since_last_clear=since_last_clear
            )
        )
    except Exception as exc:  # noqa: BLE001
        return _json(
            {
                "ok": False,
                "error": {"kind": "Io", "message": str(exc)},
                "session_path": session_path,
            }
        )


@mcp.tool(name="session_field_breakdown_dtj")
def debug_trace_session_field_breakdown_dtj(
    session_path: str,
    field: str,
    category: str | None = None,
    top: int = 20,
    since_last_clear: bool = False,
) -> str:
    """Distribution of a structured payload field."""
    try:
        return _json(
            field_breakdown_dtj(
                session_path,
                field,
                category=category,
                top=top,
                since_last_clear=since_last_clear,
            )
        )
    except Exception as exc:  # noqa: BLE001
        return _json(
            {
                "ok": False,
                "error": {"kind": "Io", "message": str(exc)},
                "session_path": session_path,
            }
        )


@mcp.tool(name="session_field_crosstab_dtj")
def debug_trace_session_field_crosstab_dtj(
    session_path: str,
    field_a: str,
    field_b: str,
    category: str | None = None,
    top: int = 20,
    since_last_clear: bool = False,
) -> str:
    """Co-occurrence table for two structured payload fields."""
    try:
        return _json(
            field_crosstab_dtj(
                session_path,
                field_a,
                field_b,
                category=category,
                top=top,
                since_last_clear=since_last_clear,
            )
        )
    except Exception as exc:  # noqa: BLE001
        return _json(
            {
                "ok": False,
                "error": {"kind": "Io", "message": str(exc)},
                "session_path": session_path,
            }
        )


@mcp.tool(name="session_sequence_gap_dtj")
def debug_trace_session_sequence_gap_dtj(
    session_path: str,
    open_event: str,
    close_event: str,
    max_lines: int = 100,
    limit: int = 50,
    since_last_clear: bool = False,
) -> str:
    """Find open_event→close_event gaps by event_name substring match."""
    try:
        return _json(
            sequence_gap_dtj(
                session_path,
                open_event,
                close_event,
                max_lines=max_lines,
                limit=limit,
                since_last_clear=since_last_clear,
            )
        )
    except Exception as exc:  # noqa: BLE001
        return _json(
            {
                "ok": False,
                "error": {"kind": "Io", "message": str(exc)},
                "session_path": session_path,
            }
        )


@mcp.tool(name="session_bursts_dtj")
def debug_trace_session_bursts_dtj(
    session_path: str,
    window_sec: float = 1.0,
    min_count: int = 5,
    top: int = 10,
    since_last_clear: bool = False,
) -> str:
    """Same-tag bursts within a monotonic time window."""
    try:
        return _json(
            bursts_dtj(
                session_path,
                window_sec=window_sec,
                min_count=min_count,
                top=top,
                since_last_clear=since_last_clear,
            )
        )
    except Exception as exc:  # noqa: BLE001
        return _json(
            {
                "ok": False,
                "error": {"kind": "Io", "message": str(exc)},
                "session_path": session_path,
            }
        )


@mcp.tool(name="session_gaps_dtj")
def debug_trace_session_gaps_dtj(
    session_path: str,
    min_gap_sec: float = 2.0,
    top: int = 10,
    since_last_clear: bool = False,
) -> str:
    """Long silences between consecutive timed events (monotonic_ns)."""
    try:
        return _json(
            gaps_dtj(
                session_path,
                min_gap_sec=min_gap_sec,
                top=top,
                since_last_clear=since_last_clear,
            )
        )
    except Exception as exc:  # noqa: BLE001
        return _json(
            {
                "ok": False,
                "error": {"kind": "Io", "message": str(exc)},
                "session_path": session_path,
            }
        )


@mcp.tool(name="session_compare_sessions_dtj")
def debug_trace_session_compare_sessions_dtj(session_path: str) -> str:
    """Diff tag/red-flag counts between the last two session segments."""
    try:
        return _json(compare_sessions_dtj(session_path))
    except Exception as exc:  # noqa: BLE001
        return _json(
            {
                "ok": False,
                "error": {"kind": "Io", "message": str(exc)},
                "session_path": session_path,
            }
        )


@mcp.tool(name="session_transition_matrix_dtj")
def debug_trace_session_transition_matrix_dtj(
    session_path: str,
    category: str | None = None,
    top: int = 10,
    since_last_clear: bool = False,
) -> str:
    """Tag→tag transition probabilities for consecutive events."""
    try:
        return _json(
            transition_matrix_dtj(
                session_path,
                category=category,
                top=top,
                since_last_clear=since_last_clear,
            )
        )
    except Exception as exc:  # noqa: BLE001
        return _json(
            {
                "ok": False,
                "error": {"kind": "Io", "message": str(exc)},
                "session_path": session_path,
            }
        )


@mcp.tool(name="session_snapshot_before_after_dtj")
def debug_trace_session_snapshot_before_after_dtj(
    session_path: str,
    entity_id: str | None = None,
    event_sequence: int | None = None,
    limit: int = 20,
) -> str:
    """Pair Snapshot Before/After events and show structured field diffs."""
    try:
        return _json(
            snapshot_before_after_dtj(
                session_path,
                entity_id=entity_id,
                event_sequence=event_sequence,
                limit=limit,
            )
        )
    except Exception as exc:  # noqa: BLE001
        return _json(
            {
                "ok": False,
                "error": {"kind": "Io", "message": str(exc)},
                "session_path": session_path,
            }
        )


@mcp.tool(name="session_first_last_dtj")
def debug_trace_session_first_last_dtj(
    session_path: str,
    tag: str | None = None,
    entity_id: str | None = None,
) -> str:
    """First and last occurrence for a tag substring and/or entity id."""
    try:
        return _json(
            first_last_dtj(session_path, tag=tag, entity_id=entity_id)
        )
    except Exception as exc:  # noqa: BLE001
        return _json(
            {
                "ok": False,
                "error": {"kind": "Io", "message": str(exc)},
                "session_path": session_path,
            }
        )


@mcp.tool(name="session_density_timeline_dtj")
def debug_trace_session_density_timeline_dtj(
    session_path: str,
    bucket_sec: float = 10.0,
    top_categories: int = 5,
    since_last_clear: bool = False,
) -> str:
    """Activity histogram by monotonic time buckets and top categories."""
    try:
        return _json(
            density_timeline_dtj(
                session_path,
                bucket_sec=bucket_sec,
                top_categories=top_categories,
                since_last_clear=since_last_clear,
            )
        )
    except Exception as exc:  # noqa: BLE001
        return _json(
            {
                "ok": False,
                "error": {"kind": "Io", "message": str(exc)},
                "session_path": session_path,
            }
        )


@mcp.tool(name="session_minimal_repro_dtj")
def debug_trace_session_minimal_repro_dtj(
    session_path: str, start: int, end: int
) -> str:
    """Deduplicate events in an event_sequence range into a minimal repro set."""
    try:
        return _json(minimal_repro_dtj(session_path, start, end))
    except Exception as exc:  # noqa: BLE001
        return _json(
            {
                "ok": False,
                "error": {"kind": "Io", "message": str(exc)},
                "session_path": session_path,
            }
        )


@mcp.tool(name="session_baseline_save_dtj")
def debug_trace_session_baseline_save_dtj(
    session_path: str, baseline_path: str
) -> str:
    """Save session tag/category/red-flag baseline to an explicit local path."""
    try:
        return _json(baseline_save_dtj(session_path, baseline_path))
    except Exception as exc:  # noqa: BLE001
        return _json(
            {
                "ok": False,
                "error": {"kind": "Io", "message": str(exc)},
                "session_path": session_path,
            }
        )


@mcp.tool(name="session_baseline_diff_dtj")
def debug_trace_session_baseline_diff_dtj(
    session_path: str, baseline_path: str
) -> str:
    """Diff current session stats against an explicit local baseline JSON."""
    try:
        return _json(baseline_diff_dtj(session_path, baseline_path))
    except Exception as exc:  # noqa: BLE001
        return _json(
            {
                "ok": False,
                "error": {"kind": "Io", "message": str(exc)},
                "session_path": session_path,
            }
        )


@mcp.tool(name="session_persistence_mismatches_dtj")
def debug_trace_session_persistence_mismatches_dtj(
    session_path: str, top: int = 10
) -> str:
    """Snapshot AfterSync + snapshot_diff + mismatch-marker scan."""
    try:
        return _json(persistence_mismatches_dtj(session_path, top=top))
    except Exception as exc:  # noqa: BLE001
        return _json(
            {
                "ok": False,
                "error": {"kind": "Io", "message": str(exc)},
                "session_path": session_path,
            }
        )


@mcp.tool(name="session_index_dtj")
def debug_trace_session_index_dtj(
    session_path: str,
    index_path: str,
    rebuild: bool = False,
) -> str:
    """Build or reuse a rebuildable JSON read index for one native `.dtj` session.

    The source file is never modified. ``rebuild=false`` only reuses a valid
    matching index; ``rebuild=true`` atomically creates/replaces ``index_path``.
    """
    try:
        return _json(
            index_session_dtj(session_path, index_path, rebuild=rebuild)
        )
    except Exception as exc:  # noqa: BLE001
        return _json(
            {
                "ok": False,
                "error": {"kind": "Io", "message": str(exc)},
                "session_path": session_path,
                "index_path": index_path,
            }
        )


@mcp.tool(name="session_search_dtj_index")
def debug_trace_session_search_dtj_index(
    session_path: str,
    index_path: str,
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
    limit: int = 100,
    offset: int = 0,
) -> str:
    """Search a persistent DTJ JSON index (no source re-decode when valid)."""
    try:
        return _json(
            search_session_dtj_index(
                session_path,
                index_path,
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
        )
    except Exception as exc:  # noqa: BLE001
        return _json(
            {
                "ok": False,
                "error": {"kind": "Io", "message": str(exc)},
                "session_path": session_path,
                "index_path": index_path,
            }
        )


@mcp.tool(name="session_performance_report_dtj_index")
def debug_trace_session_performance_report_dtj_index(
    session_path: str,
    index_path: str,
    domain: str | None = None,
    category: str | None = None,
    field: str | None = None,
) -> str:
    """Read-only duration aggregates over a validated persistent DTJ JSON index."""
    try:
        return _json(
            performance_report_dtj_index(
                session_path,
                index_path,
                domain=domain,
                category=category,
                field=field,
            )
        )
    except Exception as exc:  # noqa: BLE001
        return _json(
            {
                "ok": False,
                "error": {"kind": "Io", "message": str(exc)},
                "session_path": session_path,
                "index_path": index_path,
            }
        )


def main() -> None:
    mcp.run()


if __name__ == "__main__":
    main()
