"""DTJ / DTG event catalog rendered from a pinned schema registry.

Source of truth is a local Event Schema Registry artifact (ADR 0011), not
hardcoded Wire Trace UI category strings. Empty registry / missing path yield
an explicit empty catalog; corrupt/unknown schemas fail closed.
"""

from __future__ import annotations

from pathlib import Path
from typing import Any

from .dtg_schema import DtgSchemaError, EventSchema, LoadedRegistry, load_registry
from .dtj_read import ADAPTER_NAME, ADAPTER_VERSION

CATALOG_FORMAT = "dtj-event-catalog-v1"
MAX_SCHEMAS_IN_CATALOG = 500


def event_catalog_dtj(registry_path: str | Path | None = None) -> dict[str, Any]:
    """Build a bounded catalog from an explicit local registry path.

    - ``registry_path`` omitted / empty → ok empty catalog (no invented events)
    - missing / corrupt / oversized registry → structured error
    - known registry → typed fields, producer profile, correlation hints
    """
    if registry_path is None or (
        isinstance(registry_path, str) and not registry_path.strip()
    ):
        return _empty_catalog(registry_path=None)

    path = Path(registry_path).expanduser()
    try:
        registry = load_registry(path)
    except DtgSchemaError as exc:
        return {
            "ok": False,
            "adapter": {"name": ADAPTER_NAME, "version": ADAPTER_VERSION},
            "registry_path": str(path),
            "error": {"kind": exc.code, "message": str(exc)},
        }

    return _catalog_from_registry(registry)


def render_event_catalog_markdown(catalog: dict[str, Any]) -> str:
    """Markdown presentation for MCP resource / agent reading."""
    if not catalog.get("ok"):
        err = catalog.get("error") or {}
        return (
            f"# Debug Trace event catalog\n\n"
            f"Error: {err.get('kind')}: {err.get('message')}\n"
        )

    lines = [
        "# Debug Trace event catalog",
        "",
        f"format: {catalog.get('format')}",
        f"registry_path: {catalog.get('registry_path') or '(none)'}",
        f"registry_digest: {catalog.get('registry_digest') or '(none)'}",
        f"schema_count: {catalog.get('schema_count', 0)}",
        "",
        "Addressing: DTJ `event_sequence` / `correlation` (not log lineno).",
        "Catalog SoT: pinned Event Schema Registry — not UI filter chips.",
        "",
    ]
    schemas = catalog.get("schemas") or []
    if not schemas:
        lines.append("*(empty — pass registry_path to load registered schemas)*")
        lines.append("")
        lines.append("Recommended tools: `session_since_last_repro_dtj`,")
        lines.append("`session_search_dtj`, domain presets via")
        lines.append("`session_preset_report_dtj`.")
        return "\n".join(lines)

    # Group by domain → category
    by_domain: dict[str, dict[str, list[dict[str, Any]]]] = {}
    for schema in schemas:
        if not isinstance(schema, dict):
            continue
        domain = str(schema.get("domain") or "<none>")
        category = str(schema.get("category") or "<none>")
        by_domain.setdefault(domain, {}).setdefault(category, []).append(schema)

    for domain in sorted(by_domain):
        lines.append(f"## Domain `{domain}`")
        lines.append("")
        for category in sorted(by_domain[domain]):
            lines.append(f"### Category `{category}`")
            lines.append("")
            for schema in by_domain[domain][category]:
                lines.extend(_schema_markdown(schema))
                lines.append("")
    lines.append("Recommended tools: `session_analyze_dtj` first,")
    lines.append("then `session_preset_report_dtj` / search / context.")
    return "\n".join(lines)


def _empty_catalog(*, registry_path: str | None) -> dict[str, Any]:
    return {
        "ok": True,
        "adapter": {"name": ADAPTER_NAME, "version": ADAPTER_VERSION},
        "format": CATALOG_FORMAT,
        "registry_path": registry_path,
        "registry_digest": None,
        "schema_count": 0,
        "schemas": [],
        "text": render_event_catalog_markdown(
            {
                "ok": True,
                "format": CATALOG_FORMAT,
                "registry_path": registry_path,
                "registry_digest": None,
                "schema_count": 0,
                "schemas": [],
            }
        ),
    }


def _catalog_from_registry(registry: LoadedRegistry) -> dict[str, Any]:
    schemas = list(registry.schemas[:MAX_SCHEMAS_IN_CATALOG])
    projected = [_project_schema(s) for s in schemas]
    out: dict[str, Any] = {
        "ok": True,
        "adapter": {"name": ADAPTER_NAME, "version": ADAPTER_VERSION},
        "format": CATALOG_FORMAT,
        "registry_path": str(registry.path),
        "registry_digest": registry.digest_sha256,
        "registry_version": registry.registry_version,
        "schema_count": len(projected),
        "truncated": len(registry.schemas) > MAX_SCHEMAS_IN_CATALOG,
        "schemas": projected,
    }
    out["text"] = render_event_catalog_markdown(out)
    return out


def _project_schema(schema: EventSchema) -> dict[str, Any]:
    fields = []
    correlation_fields: list[str] = []
    for field in schema.fields:
        fields.append(
            {
                "name": field.name,
                "value_type": field.value_type,
                "required": field.required,
                "unit": field.unit,
                "dimension": field.dimension,
                "semantic_role": field.semantic_role,
                "sensitivity": field.sensitivity,
                "metric_eligible": field.metric_eligible,
                "graph_role": field.graph_role,
                "redaction_action": field.redaction_action,
            }
        )
        if field.graph_role in {"event_reference", "span_reference"}:
            correlation_fields.append(field.name)
        if field.semantic_role in {"identifier", "event_reference", "span_reference"}:
            if field.name not in correlation_fields:
                correlation_fields.append(field.name)

    return {
        "schema_id": schema.schema_id,
        "schema_version": schema.schema_version,
        "producer_profile_id": schema.producer_profile_id,
        "compatibility_policy": schema.compatibility_policy,
        "domain": schema.domain,
        "category": schema.category,
        "event_name": schema.event_name,
        "fields": fields,
        "correlation": {
            "dtj_record_field": "correlation",
            "expectation": (
                "optional"
                if not correlation_fields
                else "recommended_when_entity_or_span_refs_present"
            ),
            "reference_fields": correlation_fields,
        },
        # DTJ event severity is on the record; schema expresses field sensitivity.
        "severity": {
            "record_field": "severity",
            "note": "DTJ event severity is producer-assigned on the record",
            "field_sensitivities": sorted({f.sensitivity for f in schema.fields}),
        },
    }


def _schema_markdown(schema: dict[str, Any]) -> list[str]:
    lines = [
        f"- **{schema.get('event_name')}** "
        f"(`{schema.get('schema_id')}` v{schema.get('schema_version')})",
        f"  - producer_profile: `{schema.get('producer_profile_id')}`",
        f"  - compatibility_policy: `{schema.get('compatibility_policy')}`",
    ]
    corr = schema.get("correlation") or {}
    refs = corr.get("reference_fields") or []
    lines.append(
        f"  - correlation: {corr.get('expectation')} "
        f"(record field `{corr.get('dtj_record_field')}`"
        + (f"; refs: {', '.join(refs)}" if refs else "")
        + ")"
    )
    sev = schema.get("severity") or {}
    lines.append(
        f"  - severity: record `{sev.get('record_field')}`; "
        f"field sensitivities: {', '.join(sev.get('field_sensitivities') or [])}"
    )
    lines.append("  - fields:")
    for field in schema.get("fields") or []:
        req = "required" if field.get("required") else "optional"
        unit = f", unit={field['unit']}" if field.get("unit") else ""
        lines.append(
            f"    - `{field.get('name')}`: {field.get('value_type')} ({req}"
            f", sensitivity={field.get('sensitivity')}"
            f", graph_role={field.get('graph_role')}{unit})"
        )
    return lines
