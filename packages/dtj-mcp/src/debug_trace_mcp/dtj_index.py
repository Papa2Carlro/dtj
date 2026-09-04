"""Rebuildable JSON read index for one native DTJ session.

The `.dtj` source remains canonical. This sidecar is derived, never a source of
truth, and is never written next to ``session_path`` implicitly.
No embedded DB — plain JSON with inverted lookup maps.
Does not reuse JSONL ``SessionStore``.
"""

from __future__ import annotations

import json
import os
import uuid
from collections import defaultdict
from pathlib import Path
from typing import Any, Callable

from .dtj_read import ADAPTER_NAME, ADAPTER_VERSION, read_session_dtj
from .dtj_search import (
    DEFAULT_SEARCH_LIMIT,
    event_matches_native_filters,
    validate_mono_window,
    validate_search_limit,
    validate_search_offset,
)

INDEX_SCHEMA_VERSION = 1
INDEX_FORMAT = "dtj-json-index-v1"
ReadFn = Callable[..., dict[str, Any]]


def index_session_dtj(
    session_path: str | Path,
    index_path: str | Path,
    *,
    rebuild: bool = False,
    dtj_bin: str | Path | None = None,
    read_fn: ReadFn | None = None,
) -> dict[str, Any]:
    """Build or reuse a persistent JSON index for one explicit `.dtj` session.

    - ``rebuild=False``: reuse a valid matching index, otherwise structured error
      (never silently overwrite).
    - ``rebuild=True``: decode source and atomically create/replace ``index_path``.
    """
    session = Path(session_path).expanduser()
    index = Path(index_path).expanduser()
    session_str = str(session)
    index_str = str(index)
    reader = read_fn or read_session_dtj

    if not session.is_file():
        return _error(
            session_str,
            index_str,
            {
                "kind": "Io",
                "message": f"session file not found: {session_str}",
            },
        )

    if not rebuild:
        if not index.is_file():
            return _error(
                session_str,
                index_str,
                {
                    "kind": "IndexMissing",
                    "message": (
                        f"index file not found: {index_str}; "
                        "call with rebuild=true to build"
                    ),
                },
            )
        return _try_reuse_index(session, index)

    return _build_index_atomic(
        session=session,
        index=index,
        reader=reader,
        dtj_bin=dtj_bin,
    )


def load_validated_index_events(
    session_path: str | Path,
    index_path: str | Path,
    *,
    domain: str | None = None,
    category: str | None = None,
    event_name: str | None = None,
    correlation_id: str | None = None,
    severity: str | None = None,
) -> dict[str, Any]:
    """Read-only load of normalized events from a validated index (no source decode)."""
    session = Path(session_path).expanduser()
    index = Path(index_path).expanduser()
    session_str = str(session)
    index_str = str(index)

    opened = _open_validated_index(session, index)
    if not opened.get("ok"):
        return opened

    doc: dict[str, Any] = opened["doc"]
    events = _filter_events(
        doc.get("events") or [],
        domain=domain,
        category=category,
        event_name=event_name,
        correlation=correlation_id,
        severity=severity,
    )
    return {
        "ok": True,
        "adapter": {"name": ADAPTER_NAME, "version": ADAPTER_VERSION},
        "session_path": session_str,
        "index_path": index_str,
        "torn_tail": bool(doc.get("torn_tail", False)),
        "events": events,
    }


def search_session_dtj_index(
    session_path: str | Path,
    index_path: str | Path,
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
    """Search a previously built DTJ index without re-decoding the source."""
    session = Path(session_path).expanduser()
    index = Path(index_path).expanduser()
    session_str = str(session)
    index_str = str(index)

    limit_error = validate_search_limit(limit)
    if limit_error is not None:
        return _error(session_str, index_str, limit_error)
    offset_error = validate_search_offset(offset)
    if offset_error is not None:
        return _error(session_str, index_str, offset_error)
    mono_error = validate_mono_window(mono_from_ns, mono_to_ns)
    if mono_error is not None:
        return _error(session_str, index_str, mono_error)

    opened = _open_validated_index(session, index)
    if not opened.get("ok"):
        return opened

    doc: dict[str, Any] = opened["doc"]
    raw_events = doc.get("events")
    if not isinstance(raw_events, list):
        return _error(
            session_str,
            index_str,
            {
                "kind": "CorruptIndex",
                "message": "index events list missing or invalid",
            },
        )

    matched: list[dict[str, Any]] = []
    for event_obj in raw_events:
        if not isinstance(event_obj, dict):
            return _error(
                session_str,
                index_str,
                {
                    "kind": "CorruptIndex",
                    "message": "index event projection is not an object",
                },
            )
        if event_matches_native_filters(
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
        ):
            matched.append(event_obj)
    returned = matched[offset : offset + limit]
    return {
        "ok": True,
        "adapter": {"name": ADAPTER_NAME, "version": ADAPTER_VERSION},
        "session_path": session_str,
        "index_path": index_str,
        "torn_tail": bool(doc.get("torn_tail", False)),
        "matched_count": len(matched),
        "returned_count": len(returned),
        "limit": limit,
        "offset": offset,
        "events": returned,
    }


def _build_index_atomic(
    *,
    session: Path,
    index: Path,
    reader: ReadFn,
    dtj_bin: str | Path | None,
) -> dict[str, Any]:
    session_str = str(session)
    index_str = str(index)

    before = _source_identity(session)
    if before is None:
        return _error(
            session_str,
            index_str,
            {"kind": "Io", "message": f"session file not found: {session_str}"},
        )

    decoded = reader(session, dtj_bin=dtj_bin)
    if not decoded.get("ok"):
        return decoded

    after = _source_identity(session)
    if after is None or after != before:
        return _error(
            session_str,
            index_str,
            {
                "kind": "SourceChanged",
                "message": (
                    "session source identity changed during decode; "
                    "index was not published"
                ),
            },
        )

    events = decoded.get("events")
    if not isinstance(events, list):
        return _error(
            session_str,
            index_str,
            {
                "kind": "AdapterInvalidJson",
                "message": "native DTJ decode missing events list",
            },
        )

    doc = _build_document(before, decoded, events)
    index.parent.mkdir(parents=True, exist_ok=True)
    tmp = index.with_name(f".{index.name}.{uuid.uuid4().hex}.tmp")
    try:
        if tmp.exists():
            tmp.unlink()
        with tmp.open("w", encoding="utf-8") as fh:
            json.dump(doc, fh, separators=(",", ":"), ensure_ascii=False)
            fh.write("\n")
        # Validate temp before publish.
        with tmp.open(encoding="utf-8") as fh:
            check = json.load(fh)
        if check.get("schema_version") != INDEX_SCHEMA_VERSION:
            raise RuntimeError("temp index schema_version mismatch")
        if int(check.get("event_count", -1)) != len(events):
            raise RuntimeError("temp index event_count mismatch")
        os.replace(tmp, index)
    except Exception as exc:  # noqa: BLE001
        if tmp.exists():
            try:
                tmp.unlink()
            except OSError:
                pass
        return _error(
            session_str,
            index_str,
            {
                "kind": "IndexBuildFailed",
                "message": f"failed to publish index: {exc}",
            },
        )

    return {
        "ok": True,
        "adapter": decoded.get("adapter")
        or {"name": ADAPTER_NAME, "version": ADAPTER_VERSION},
        "session_path": session_str,
        "index": {
            "schema_version": INDEX_SCHEMA_VERSION,
            "path": index_str,
            "status": "built",
            "event_count": len(events),
            "chunks_committed": int(decoded.get("chunks_committed") or 0),
            "torn_tail": bool(decoded.get("torn_tail", False)),
        },
    }


def _try_reuse_index(session: Path, index: Path) -> dict[str, Any]:
    session_str = str(session)
    index_str = str(index)
    opened = _open_validated_index(session, index)
    if not opened.get("ok"):
        return opened
    doc: dict[str, Any] = opened["doc"]
    return {
        "ok": True,
        "adapter": {"name": ADAPTER_NAME, "version": ADAPTER_VERSION},
        "session_path": session_str,
        "index": {
            "schema_version": INDEX_SCHEMA_VERSION,
            "path": index_str,
            "status": "reused",
            "event_count": int(doc.get("event_count") or 0),
            "chunks_committed": int(doc.get("chunks_committed") or 0),
            "torn_tail": bool(doc.get("torn_tail", False)),
        },
    }


def _open_validated_index(session: Path, index: Path) -> dict[str, Any]:
    session_str = str(session)
    index_str = str(index)

    if not index.is_file():
        return _error(
            session_str,
            index_str,
            {
                "kind": "IndexMissing",
                "message": f"index file not found: {index_str}",
            },
        )

    identity = _source_identity(session)
    if identity is None:
        return _error(
            session_str,
            index_str,
            {"kind": "Io", "message": f"session file not found: {session_str}"},
        )

    try:
        with index.open(encoding="utf-8") as fh:
            doc = json.load(fh)
    except (OSError, UnicodeDecodeError, json.JSONDecodeError) as exc:
        return _error(
            session_str,
            index_str,
            {
                "kind": "CorruptIndex",
                "message": f"index is not readable JSON: {exc}",
            },
        )

    if not isinstance(doc, dict):
        return _error(
            session_str,
            index_str,
            {
                "kind": "CorruptIndex",
                "message": "index root must be a JSON object",
            },
        )

    if doc.get("format") != INDEX_FORMAT:
        return _error(
            session_str,
            index_str,
            {
                "kind": "CorruptIndex",
                "message": (
                    f"index format {doc.get('format')!r} is not "
                    f"{INDEX_FORMAT!r}"
                ),
            },
        )

    schema = doc.get("schema_version")
    if schema is None:
        return _error(
            session_str,
            index_str,
            {
                "kind": "CorruptIndex",
                "message": "index meta missing schema_version",
            },
        )
    if schema != INDEX_SCHEMA_VERSION:
        return _error(
            session_str,
            index_str,
            {
                "kind": "UnsupportedIndexSchema",
                "message": (
                    f"index schema_version {schema} unsupported; "
                    f"expected {INDEX_SCHEMA_VERSION}"
                ),
                "schema_version": schema,
                "expected": INDEX_SCHEMA_VERSION,
            },
        )

    if not _identity_matches(doc, identity):
        return _error(
            session_str,
            index_str,
            {
                "kind": "StaleIndex",
                "message": (
                    "index source identity does not match current session file "
                    "(path/size/mtime_ns + stored header metadata); rebuild required"
                ),
            },
        )

    return {"ok": True, "doc": doc}


def _build_document(
    identity: dict[str, Any],
    decoded: dict[str, Any],
    events: list[Any],
) -> dict[str, Any]:
    header = decoded.get("header") if isinstance(decoded.get("header"), dict) else {}
    by_domain: dict[str, list[int]] = defaultdict(list)
    by_category: dict[str, list[int]] = defaultdict(list)
    by_event_name: dict[str, list[int]] = defaultdict(list)
    by_correlation: dict[str, list[int]] = defaultdict(list)
    by_severity: dict[str, list[int]] = defaultdict(list)

    normalized: list[dict[str, Any]] = []
    for event in events:
        if not isinstance(event, dict):
            raise ValueError("event is not an object")
        seq = event.get("event_sequence")
        if not isinstance(seq, int):
            raise ValueError("event_sequence missing")
        normalized.append(event)
        if isinstance(event.get("domain"), str):
            by_domain[event["domain"]].append(seq)
        if isinstance(event.get("category"), str):
            by_category[event["category"]].append(seq)
        if isinstance(event.get("event_name"), str):
            by_event_name[event["event_name"]].append(seq)
        if isinstance(event.get("correlation"), str):
            by_correlation[event["correlation"]].append(seq)
        if isinstance(event.get("severity"), str):
            by_severity[event["severity"]].append(seq)

    return {
        "format": INDEX_FORMAT,
        "schema_version": INDEX_SCHEMA_VERSION,
        "adapter": {"name": ADAPTER_NAME, "version": ADAPTER_VERSION},
        "source_path": identity["source_path"],
        "source_size": identity["source_size"],
        "source_mtime_ns": identity["source_mtime_ns"],
        "header": header,
        "chunks_committed": int(decoded.get("chunks_committed") or 0),
        "torn_tail": bool(decoded.get("torn_tail", False)),
        "event_count": len(normalized),
        "indexes": {
            "by_domain": dict(by_domain),
            "by_category": dict(by_category),
            "by_event_name": dict(by_event_name),
            "by_correlation": dict(by_correlation),
            "by_severity": dict(by_severity),
        },
        "events": normalized,
    }


def _filter_events(
    events: list[Any],
    *,
    domain: str | None,
    category: str | None,
    event_name: str | None,
    correlation: str | None,
    severity: str | None,
) -> list[dict[str, Any]]:
    out: list[dict[str, Any]] = []
    for event in events:
        if not isinstance(event, dict):
            continue
        if domain is not None and event.get("domain") != domain:
            continue
        if category is not None and event.get("category") != category:
            continue
        if event_name is not None and event.get("event_name") != event_name:
            continue
        if correlation is not None and event.get("correlation") != correlation:
            continue
        if severity is not None and event.get("severity") != severity:
            continue
        out.append(event)
    out.sort(key=lambda e: int(e.get("event_sequence") or 0))
    return out


def _source_identity(session: Path) -> dict[str, Any] | None:
    try:
        st = session.stat()
    except OSError:
        return None
    mtime_ns = getattr(st, "st_mtime_ns", int(st.st_mtime * 1_000_000_000))
    return {
        "source_path": str(session.resolve()),
        "source_size": int(st.st_size),
        "source_mtime_ns": int(mtime_ns),
    }


def _identity_matches(doc: dict[str, Any], identity: dict[str, Any]) -> bool:
    return (
        doc.get("source_path") == identity["source_path"]
        and int(doc.get("source_size") or -1) == identity["source_size"]
        and int(doc.get("source_mtime_ns") or -1) == identity["source_mtime_ns"]
    )


def _error(
    session_path: str,
    index_path: str,
    error: dict[str, Any],
) -> dict[str, Any]:
    return {
        "ok": False,
        "adapter": {"name": ADAPTER_NAME, "version": ADAPTER_VERSION},
        "session_path": session_path,
        "index_path": index_path,
        "error": error,
    }
