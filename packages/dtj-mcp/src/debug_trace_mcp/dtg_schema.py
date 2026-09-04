"""DTG Event Schema Registry loader + strict/relaxed validation harness (ADR 0011).

Isolated from JSONL ``validate_event`` / ingest. Callers pass an explicit local
registry path (and optional SHA-256 digest). No network, no implicit discovery,
no redaction transforms, no graph edge emission.
"""

from __future__ import annotations

import hashlib
import json
from dataclasses import dataclass, field
from pathlib import Path
from typing import Any, Mapping

DEFAULT_MAX_REGISTRY_BYTES = 1_048_576  # 1 MiB

DTJ_VALUE_TYPES = frozenset(
    {
        "Bool",
        "I32",
        "I64",
        "U32",
        "U64",
        "F32",
        "F64",
        "Enum",
        "Vec2F32",
        "Vec3F32",
        "InternedString",
        "Bytes",
    }
)

SENSITIVITIES = frozenset(
    {"public", "internal", "sensitive", "secret_forbidden"}
)
REDACTION_ACTIONS = frozenset({"allow", "drop_field", "reject"})
GRAPH_ROLES = frozenset({"none", "event_reference", "span_reference"})
POLICIES = frozenset({"strict", "relaxed"})


class DtgSchemaError(Exception):
    """Fail-closed registry / validation error with a stable code (no secrets)."""

    def __init__(self, code: str, message: str) -> None:
        self.code = code
        super().__init__(message)


@dataclass(frozen=True)
class FieldDecl:
    name: str
    value_type: str
    required: bool
    sensitivity: str
    unit: str | None = None
    dimension: str | None = None
    semantic_role: str | None = None
    metric_eligible: bool = False
    graph_role: str = "none"
    redaction_action: str = "allow"


@dataclass(frozen=True)
class EventSchema:
    schema_id: str
    schema_version: int
    producer_profile_id: str
    compatibility_policy: str
    domain: str
    category: str
    event_name: str
    fields: tuple[FieldDecl, ...]

    def field_map(self) -> dict[str, FieldDecl]:
        return {f.name: f for f in self.fields}


@dataclass(frozen=True)
class LoadedRegistry:
    path: Path
    digest_sha256: str
    registry_version: int
    schemas: tuple[EventSchema, ...]

    def resolve(
        self,
        *,
        producer_profile_id: str,
        domain: str,
        category: str,
        event_name: str,
        schema_version: int,
    ) -> EventSchema | None:
        for schema in self.schemas:
            if (
                schema.producer_profile_id == producer_profile_id
                and schema.domain == domain
                and schema.category == category
                and schema.event_name == event_name
                and schema.schema_version == schema_version
            ):
                return schema
        return None


@dataclass
class ValidationResult:
    accepted: bool
    error_code: str | None = None
    error_message: str | None = None
    schema_id: str | None = None
    schema_version: int | None = None
    producer_profile_id: str | None = None
    compatibility_policy: str | None = None
    registry_digest: str | None = None
    approved_metric_fields: list[str] = field(default_factory=list)
    approved_graph_reference_fields: list[str] = field(default_factory=list)
    opaque_fields: list[str] = field(default_factory=list)
    dropped_fields: list[str] = field(default_factory=list)


def sha256_file(path: Path) -> str:
    h = hashlib.sha256()
    with path.open("rb") as fh:
        for chunk in iter(lambda: fh.read(65536), b""):
            h.update(chunk)
    return h.hexdigest()


def load_registry(
    path: Path | str,
    *,
    expected_digest: str | None = None,
    max_bytes: int = DEFAULT_MAX_REGISTRY_BYTES,
) -> LoadedRegistry:
    """Load a pinned local registry artifact. Fail-closed on size/digest/structure."""
    registry_path = Path(path)
    if not registry_path.is_file():
        raise DtgSchemaError("RegistryMissing", "registry artifact not found")

    size = registry_path.stat().st_size
    if size > max_bytes:
        raise DtgSchemaError(
            "RegistryTooLarge",
            f"registry exceeds max_bytes cap ({max_bytes})",
        )

    digest = sha256_file(registry_path)
    if expected_digest is not None:
        expected = expected_digest.strip().lower()
        if digest != expected:
            raise DtgSchemaError("DigestMismatch", "registry digest mismatch")

    try:
        raw = registry_path.read_bytes()
        data = json.loads(raw.decode("utf-8"))
    except (OSError, UnicodeDecodeError, json.JSONDecodeError) as exc:
        raise DtgSchemaError("CorruptRegistry", "registry is not valid JSON") from exc

    schemas = _parse_registry_document(data)
    return LoadedRegistry(
        path=registry_path.resolve(),
        digest_sha256=digest,
        registry_version=1,
        schemas=schemas,
    )


def validate_candidate(
    registry: LoadedRegistry,
    *,
    producer_profile_id: str,
    domain: str,
    category: str,
    event_name: str,
    schema_version: int,
    payload: Mapping[str, Any] | None,
) -> ValidationResult:
    """Validate a normalized in-memory event candidate against a resolved schema."""
    schema = registry.resolve(
        producer_profile_id=producer_profile_id,
        domain=domain,
        category=category,
        event_name=event_name,
        schema_version=schema_version,
    )
    if schema is None:
        return ValidationResult(
            accepted=False,
            error_code="UnknownEvent",
            error_message=(
                "no schema for producer_profile_id/domain/category/"
                "event_name/schema_version"
            ),
            registry_digest=registry.digest_sha256,
        )

    fields = schema.field_map()
    payload_map: dict[str, Any] = dict(payload or {})

    # secret_forbidden: reject if key present (value never echoed)
    for name, decl in fields.items():
        if decl.sensitivity == "secret_forbidden" and name in payload_map:
            return ValidationResult(
                accepted=False,
                error_code="SecretForbidden",
                error_message=f"field {name!r} is secret_forbidden",
                schema_id=schema.schema_id,
                schema_version=schema.schema_version,
                producer_profile_id=schema.producer_profile_id,
                compatibility_policy=schema.compatibility_policy,
                registry_digest=registry.digest_sha256,
            )

    for name, decl in fields.items():
        if decl.redaction_action == "reject" and name in payload_map:
            return ValidationResult(
                accepted=False,
                error_code="FieldRejected",
                error_message=f"field {name!r} rejected by redaction_action",
                schema_id=schema.schema_id,
                schema_version=schema.schema_version,
                producer_profile_id=schema.producer_profile_id,
                compatibility_policy=schema.compatibility_policy,
                registry_digest=registry.digest_sha256,
            )

    for name, decl in fields.items():
        if decl.required and name not in payload_map:
            return ValidationResult(
                accepted=False,
                error_code="RequiredFieldMissing",
                error_message=f"required field {name!r} is missing",
                schema_id=schema.schema_id,
                schema_version=schema.schema_version,
                producer_profile_id=schema.producer_profile_id,
                compatibility_policy=schema.compatibility_policy,
                registry_digest=registry.digest_sha256,
            )

    opaque: list[str] = []
    dropped: list[str] = []
    kept: dict[str, Any] = {}
    for name, value in payload_map.items():
        decl = fields.get(name)
        if decl is None:
            if schema.compatibility_policy == "strict":
                return ValidationResult(
                    accepted=False,
                    error_code="UnknownField",
                    error_message=f"unknown field {name!r} under strict policy",
                    schema_id=schema.schema_id,
                    schema_version=schema.schema_version,
                    producer_profile_id=schema.producer_profile_id,
                    compatibility_policy=schema.compatibility_policy,
                    registry_digest=registry.digest_sha256,
                )
            opaque.append(name)
            kept[name] = value
            continue
        if not _value_matches_type(decl.value_type, value):
            return ValidationResult(
                accepted=False,
                error_code="TypeMismatch",
                error_message=f"field {name!r} type mismatch for {decl.value_type}",
                schema_id=schema.schema_id,
                schema_version=schema.schema_version,
                producer_profile_id=schema.producer_profile_id,
                compatibility_policy=schema.compatibility_policy,
                registry_digest=registry.digest_sha256,
            )
        if decl.redaction_action == "drop_field":
            dropped.append(name)
            continue
        kept[name] = value

    metrics = sorted(
        f.name
        for f in schema.fields
        if f.metric_eligible and f.semantic_role == "metric" and f.name in kept
    )
    graph_refs = sorted(
        f.name
        for f in schema.fields
        if f.graph_role in {"event_reference", "span_reference"} and f.name in kept
    )

    return ValidationResult(
        accepted=True,
        schema_id=schema.schema_id,
        schema_version=schema.schema_version,
        producer_profile_id=schema.producer_profile_id,
        compatibility_policy=schema.compatibility_policy,
        registry_digest=registry.digest_sha256,
        approved_metric_fields=metrics,
        approved_graph_reference_fields=graph_refs,
        opaque_fields=sorted(opaque),
        dropped_fields=sorted(dropped),
    )


def _parse_registry_document(data: Any) -> tuple[EventSchema, ...]:
    if not isinstance(data, dict):
        raise DtgSchemaError("InvalidRegistry", "registry root must be an object")
    if data.get("registry_version") != 1:
        raise DtgSchemaError("UnsupportedRegistryVersion", "registry_version must be 1")
    schemas_raw = data.get("schemas")
    if not isinstance(schemas_raw, list) or not schemas_raw:
        raise DtgSchemaError("InvalidRegistry", "schemas must be a non-empty array")

    parsed: list[EventSchema] = []
    seen_keys: set[tuple[str, str, str, str, int]] = set()
    for entry in schemas_raw:
        schema = _parse_event_schema(entry)
        key = (
            schema.producer_profile_id,
            schema.domain,
            schema.category,
            schema.event_name,
            schema.schema_version,
        )
        if key in seen_keys:
            raise DtgSchemaError(
                "InvalidSchemaDeclaration",
                "duplicate schema identity in registry",
            )
        seen_keys.add(key)
        parsed.append(schema)
    return tuple(parsed)


def _parse_event_schema(entry: Any) -> EventSchema:
    if not isinstance(entry, dict):
        raise DtgSchemaError("InvalidSchemaDeclaration", "schema entry must be an object")
    try:
        schema_id = str(entry["schema_id"])
        schema_version = int(entry["schema_version"])
        producer_profile_id = str(entry["producer_profile_id"])
        compatibility_policy = str(entry["compatibility_policy"])
        domain = str(entry["domain"])
        category = str(entry["category"])
        event_name = str(entry["event_name"])
        fields_raw = entry["fields"]
    except (KeyError, TypeError, ValueError) as exc:
        raise DtgSchemaError(
            "InvalidSchemaDeclaration", "schema entry missing required keys"
        ) from exc

    if compatibility_policy not in POLICIES:
        raise DtgSchemaError(
            "InvalidSchemaDeclaration", "compatibility_policy must be strict or relaxed"
        )
    if schema_version < 1:
        raise DtgSchemaError("InvalidSchemaDeclaration", "schema_version must be >= 1")
    if not isinstance(fields_raw, list):
        raise DtgSchemaError("InvalidSchemaDeclaration", "fields must be an array")

    fields: list[FieldDecl] = []
    seen_names: set[str] = set()
    for raw in fields_raw:
        decl = _parse_field(raw)
        if decl.name in seen_names:
            raise DtgSchemaError(
                "InvalidSchemaDeclaration", "duplicate field name in schema"
            )
        seen_names.add(decl.name)
        fields.append(decl)

    return EventSchema(
        schema_id=schema_id,
        schema_version=schema_version,
        producer_profile_id=producer_profile_id,
        compatibility_policy=compatibility_policy,
        domain=domain,
        category=category,
        event_name=event_name,
        fields=tuple(fields),
    )


def _parse_field(raw: Any) -> FieldDecl:
    if not isinstance(raw, dict):
        raise DtgSchemaError("InvalidSchemaDeclaration", "field must be an object")
    try:
        name = str(raw["name"])
        value_type = str(raw["value_type"])
        required = bool(raw["required"])
        sensitivity = str(raw["sensitivity"])
    except (KeyError, TypeError, ValueError) as exc:
        raise DtgSchemaError(
            "InvalidSchemaDeclaration", "field missing required keys"
        ) from exc

    if value_type not in DTJ_VALUE_TYPES:
        raise DtgSchemaError("InvalidSchemaDeclaration", "unknown DTJ value_type")
    if sensitivity not in SENSITIVITIES:
        raise DtgSchemaError("InvalidSchemaDeclaration", "invalid sensitivity")

    metric_eligible = bool(raw.get("metric_eligible", False))
    semantic_role = raw.get("semantic_role")
    if semantic_role is not None:
        semantic_role = str(semantic_role)
    graph_role = str(raw.get("graph_role", "none"))
    if graph_role not in GRAPH_ROLES:
        raise DtgSchemaError("InvalidSchemaDeclaration", "invalid graph_role")

    if metric_eligible and semantic_role != "metric":
        raise DtgSchemaError(
            "InvalidSchemaDeclaration",
            "metric_eligible requires semantic_role=metric",
        )
    if graph_role in {"event_reference", "span_reference"}:
        if semantic_role is None:
            semantic_role = graph_role
        elif semantic_role != graph_role:
            raise DtgSchemaError(
                "InvalidSchemaDeclaration",
                "graph_role reference requires matching semantic_role",
            )

    if "redaction_action" in raw and raw["redaction_action"] is not None:
        redaction_action = str(raw["redaction_action"])
    else:
        redaction_action = _default_redaction_action(sensitivity)
    if redaction_action not in REDACTION_ACTIONS:
        raise DtgSchemaError("InvalidSchemaDeclaration", "invalid redaction_action")
    if redaction_action == "drop_field" and required:
        raise DtgSchemaError(
            "InvalidSchemaDeclaration",
            "required field cannot use redaction_action=drop_field",
        )
    if sensitivity == "secret_forbidden" and redaction_action != "reject":
        raise DtgSchemaError(
            "InvalidSchemaDeclaration",
            "secret_forbidden requires redaction_action=reject",
        )

    return FieldDecl(
        name=name,
        value_type=value_type,
        required=required,
        sensitivity=sensitivity,
        unit=str(raw["unit"]) if raw.get("unit") is not None else None,
        dimension=str(raw["dimension"]) if raw.get("dimension") is not None else None,
        semantic_role=semantic_role,
        metric_eligible=metric_eligible,
        graph_role=graph_role,
        redaction_action=redaction_action,
    )


def _default_redaction_action(sensitivity: str) -> str:
    if sensitivity == "secret_forbidden":
        return "reject"
    if sensitivity == "sensitive":
        return "drop_field"
    return "allow"


def _value_matches_type(value_type: str, value: Any) -> bool:
    if value_type == "Bool":
        return isinstance(value, bool)
    if value_type in {"I32", "I64"}:
        return isinstance(value, int) and not isinstance(value, bool)
    if value_type in {"U32", "U64", "Enum"}:
        return isinstance(value, int) and not isinstance(value, bool) and value >= 0
    if value_type in {"F32", "F64"}:
        return isinstance(value, (int, float)) and not isinstance(value, bool)
    if value_type == "InternedString":
        # decoded shape may carry resolved string or intern id
        return isinstance(value, str) or (
            isinstance(value, int) and not isinstance(value, bool) and value >= 1
        )
    if value_type == "Bytes":
        return isinstance(value, (bytes, bytearray)) or (
            isinstance(value, str) and value.startswith("0x")
        )
    if value_type == "Vec2F32":
        return (
            isinstance(value, (list, tuple))
            and len(value) == 2
            and all(isinstance(x, (int, float)) and not isinstance(x, bool) for x in value)
        )
    if value_type == "Vec3F32":
        return (
            isinstance(value, (list, tuple))
            and len(value) == 3
            and all(isinstance(x, (int, float)) and not isinstance(x, bool) for x in value)
        )
    return False
