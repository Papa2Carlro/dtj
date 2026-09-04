"""DebugTraceEvent / DebugTraceConfig validation and protocol docs."""

from __future__ import annotations

import json
from pathlib import Path
from typing import Any

PROTOCOL_VERSION = 1

PROFILES = (
    "Off",
    "ErrorsOnly",
    "GestureSummary",
    "Verbose",
    "ParityStress",
)
SEVERITIES = ("trace", "debug", "info", "warn", "error", "fatal")
ERROR_SEVERITIES = frozenset({"error", "fatal"})
GESTURE_SUMMARY_CATEGORIES = frozenset({"gesture", "gesture_summary", "summary"})

PERFORMANCE_RULE = (
    "If a domain or category is disabled by DebugTraceConfig (or profile=Off), "
    "the Unity producer MUST NOT allocate or format a payload for that event. "
    "Gate before string formatting / object construction."
)

_SCHEMA_DIR = Path(__file__).resolve().parent / "schemas"
# Editable installs may keep schemas next to the package root.
if not _SCHEMA_DIR.is_dir():
    _SCHEMA_DIR = Path(__file__).resolve().parents[2] / "schemas"


def schema_dir() -> Path:
    return _SCHEMA_DIR


def load_schema(name: str) -> dict[str, Any]:
    path = schema_dir() / name
    if not path.is_file():
        # Fallback: package-relative schemas/ shipped via force-include
        alt = Path(__file__).resolve().parent / "schemas" / name
        path = alt if alt.is_file() else path
    with path.open(encoding="utf-8") as fh:
        return json.load(fh)


def default_config() -> dict[str, Any]:
    return {
        "version": PROTOCOL_VERSION,
        "enabled": False,
        "profile": "Off",
        "enabledDomains": [],
        "enabledCategoriesByDomain": {},
        "sampleEveryN": 1,
        "maxEventsPerSession": 50_000,
        "captureUntilFirstAnomaly": False,
    }


def validate_config(obj: Any) -> list[str]:
    errors: list[str] = []
    if not isinstance(obj, dict):
        return ["config must be a JSON object"]

    if obj.get("version") != PROTOCOL_VERSION:
        errors.append(f"version must be {PROTOCOL_VERSION}")

    if "enabled" not in obj or not isinstance(obj["enabled"], bool):
        errors.append("enabled must be a boolean")

    profile = obj.get("profile")
    if profile not in PROFILES:
        errors.append(f"profile must be one of {list(PROFILES)}")

    domains = obj.get("enabledDomains")
    if not isinstance(domains, list) or not all(
        isinstance(d, str) and d for d in domains
    ):
        errors.append("enabledDomains must be an array of non-empty strings")

    cats = obj.get("enabledCategoriesByDomain")
    if not isinstance(cats, dict):
        errors.append("enabledCategoriesByDomain must be an object")
    else:
        for key, value in cats.items():
            if not isinstance(key, str) or not key:
                errors.append("enabledCategoriesByDomain keys must be non-empty strings")
                break
            if not isinstance(value, list) or not all(
                isinstance(c, str) and c for c in value
            ):
                errors.append(
                    f"enabledCategoriesByDomain[{key!r}] must be an array of non-empty strings"
                )

    sample = obj.get("sampleEveryN")
    if not isinstance(sample, int) or isinstance(sample, bool) or sample < 1:
        errors.append("sampleEveryN must be an integer >= 1")

    max_events = obj.get("maxEventsPerSession")
    if (
        not isinstance(max_events, int)
        or isinstance(max_events, bool)
        or max_events < 1
    ):
        errors.append("maxEventsPerSession must be an integer >= 1")

    if "captureUntilFirstAnomaly" not in obj or not isinstance(
        obj["captureUntilFirstAnomaly"], bool
    ):
        errors.append("captureUntilFirstAnomaly must be a boolean")

    if "pendingEventCapacity" in obj:
        capacity = obj["pendingEventCapacity"]
        if (
            not isinstance(capacity, int)
            or isinstance(capacity, bool)
            or capacity < 1
        ):
            errors.append("pendingEventCapacity must be an integer >= 1")

    unknown = set(obj) - {
        "version",
        "enabled",
        "profile",
        "enabledDomains",
        "enabledCategoriesByDomain",
        "sampleEveryN",
        "maxEventsPerSession",
        "captureUntilFirstAnomaly",
        "pendingEventCapacity",
    }
    if unknown:
        errors.append(f"unknown config fields: {sorted(unknown)}")

    return errors


def validate_event(obj: Any) -> list[str]:
    errors: list[str] = []
    if not isinstance(obj, dict):
        return ["event must be a JSON object"]

    if obj.get("version") != PROTOCOL_VERSION:
        errors.append(f"version must be {PROTOCOL_VERSION}")

    for field in (
        "sessionId",
        "domain",
        "category",
        "eventName",
        "correlationId",
    ):
        val = obj.get(field)
        if not isinstance(val, str) or not val:
            errors.append(f"{field} must be a non-empty string")

    seq = obj.get("sequence")
    if not isinstance(seq, int) or isinstance(seq, bool) or seq < 0:
        errors.append("sequence must be an integer >= 0")

    severity = obj.get("severity")
    if severity not in SEVERITIES:
        errors.append(f"severity must be one of {list(SEVERITIES)}")

    has_utc = isinstance(obj.get("timestampUtc"), str) and bool(obj.get("timestampUtc"))
    has_mono = isinstance(obj.get("monotonicMs"), (int, float)) and not isinstance(
        obj.get("monotonicMs"), bool
    )
    if not has_utc and not has_mono:
        errors.append("timestampUtc or monotonicMs is required")

    if "payload" in obj and obj["payload"] is not None and not isinstance(
        obj["payload"], dict
    ):
        errors.append("payload must be an object when present")

    return errors


def category_enabled(config: dict[str, Any], domain: str, category: str) -> bool:
    """Return True if domain/category passes config allow-lists (ignores profile)."""
    domains = config.get("enabledDomains") or []
    if domain not in domains:
        return False
    by_domain = config.get("enabledCategoriesByDomain") or {}
    allowed = by_domain.get(domain)
    if allowed is None:
        return False
    if "*" in allowed:
        return True
    return category in allowed


def event_matches_config(event: dict[str, Any], config: dict[str, Any]) -> bool:
    """Whether a producer should emit this event under the given config."""
    if not config.get("enabled"):
        return False

    profile = config.get("profile")
    if profile == "Off":
        return False

    domain = event.get("domain", "")
    category = event.get("category", "")
    severity = event.get("severity", "")

    if not category_enabled(config, domain, category):
        return False

    if profile == "ErrorsOnly":
        return severity in ERROR_SEVERITIES

    if profile == "GestureSummary":
        if severity in ERROR_SEVERITIES:
            return True
        return category.lower() in GESTURE_SUMMARY_CATEGORIES

    if profile == "Verbose":
        return True

    # Debug-only raw-cardinality stress: full allow-list match (not a Verbose alias).
    if profile == "ParityStress":
        return True

    return False


def protocol_document() -> dict[str, Any]:
    return {
        "status": "experimental_foundation",
        "not_a_wire_migration": True,
        "protocolVersion": PROTOCOL_VERSION,
        "ownership": {
            "unity": "lightweight producer — append JSONL, poll config at safe boundaries; never call MCP per event",
            "dochub": "protocol docs, config schema, session ingest, indexing, search, reporting",
        },
        "performanceRule": PERFORMANCE_RULE,
        "profiles": list(PROFILES),
        "severities": list(SEVERITIES),
        "eventSchema": load_schema("debug-trace-event.schema.json"),
        "configSchema": load_schema("debug-trace-config.schema.json"),
        "exampleEvent": {
            "version": 1,
            "sessionId": "sess-2026-08-01T12-00-00Z",
            "timestampUtc": "2026-08-01T12:00:01.250Z",
            "monotonicMs": 1250.0,
            "sequence": 42,
            "domain": "wire",
            "category": "gesture",
            "eventName": "KnotHit",
            "correlationId": "gesture-7f3a",
            "severity": "info",
            "payload": {"durationMs": 12.5, "targetId": "knot-12"},
        },
        "exampleConfig": {
            "version": 1,
            "enabled": True,
            "profile": "Verbose",
            "enabledDomains": ["wire", "graph", "persistence", "simulation"],
            "enabledCategoriesByDomain": {
                "wire": ["gesture", "lifecycle"],
                "graph": ["*"],
                "persistence": ["snapshot"],
                "simulation": ["*"],
            },
            "sampleEveryN": 1,
            "maxEventsPerSession": 50000,
            "captureUntilFirstAnomaly": False,
        },
        "lifecycle": [
            "MCP writes DebugTraceConfig JSON atomically to caller-supplied path",
            "Unity polls/applies config at safe boundaries (not per-frame busy loop)",
            "Producer appends native DTJ v1 bytes to a `.dtj` session (not JSONL)",
            "Agents read/search/analyze via *_dtj tools on store_dir / session_path",
            "No Wire Trace / JSONL import path on the production MCP surface",
        ],
    }
