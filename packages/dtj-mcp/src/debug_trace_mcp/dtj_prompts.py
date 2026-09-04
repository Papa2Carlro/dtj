"""Debug Trace MCP prompts — ported from Wire Trace MCP wording/structure.

Legacy prompts referenced ``wire-trace.log``, linenos, and Wire Trace tool names.
These keep the same diagnostic plan shape but address native `.dtj` sessions via
DTJ tools (event_sequence / correlation / schema catalog).
"""

from __future__ import annotations

# Exact required caller inputs for every prompt in this family.
REQUIRED_INPUTS = ("session_path",)

PROMPT_TRIAGE = """\
Debug a native `.dtj` session using debug-trace MCP tools only \
(do not parse legacy `.log` / JSONL).
Required input: session_path (absolute path to one `.dtj` file).

Bounded query plan:
1. session_since_last_repro_dtj(session_path, top<=5) for a compact \
overview of the latest session segment.
2. Pick a domain preset if obvious via \
session_preset_report_dtj(session_path, report=…) where report ∈ \
{dangling, branch, gesture_route, tip_hold, commit_boundary, graph_undo} \
(top<=10), or session_persistence_mismatches_dtj(session_path, top<=10).
3. Drill in (bounds shown): \
session_unmatched_entities_dtj(session_path, kind, limit<=50); \
session_entity_cluster_dtj(session_path, entity_id, window<=20, limit<=30); \
session_causal_chain_dtj(session_path, event_sequence, hops<=5); \
session_snapshot_before_after_dtj(session_path, entity_id?, limit<=20).
4. session_context_dtj(session_path, event_sequence, before<=30, after<=30) \
for local context.

Evidence shape: cite event_sequence, correlation, entity ids (payload id/wire/…); \
include tool `text` excerpts. Do not invent wall-clock line numbers.
Optional catalog: resource debug-trace://event-catalog or \
event_catalog(registry_path=…).
"""

PROMPT_DANGLING = """\
Investigate a dangling that was not destroyed on a native `.dtj` session.
Required input: session_path.

Bounded query plan:
1. session_preset_report_dtj(session_path, report='dangling', top<=5)
2. session_unmatched_entities_dtj(session_path, kind='Dangling', limit<=50) \
and session_pair_latency_dtj(session_path, kind='Dangling')
3. For each survivor id: \
session_entity_cluster_dtj(session_path, entity_id, window<=20, limit<=30) \
then session_causal_chain_dtj(session_path, event_sequence, hops<=5) \
using the creation event_sequence from unmatched_entities.
4. session_sequence_gap_dtj(session_path, open_event='DanglingCreated', \
close_event='Destroyed|Consumed', max_lines<=100, limit<=50) for near misses.

Evidence shape: survivor entity id, creation event_sequence, last-seen tag, \
pair latency / unclosed flags. Do not cite legacy log line numbers.
"""

PROMPT_PERSISTENCE = """\
Investigate Snapshot / persistence sync mismatches on a native `.dtj` session.
Required input: session_path.

Bounded query plan:
1. session_persistence_mismatches_dtj(session_path, top<=10)
2. session_snapshot_diff_dtj(session_path, limit<=5) and \
session_snapshot_before_after_dtj(session_path, entity_id=…, limit<=20)
3. session_search_dtj(session_path, category='Snapshot', event='AfterSync', \
limit<=100, offset=0)
4. session_context_dtj(session_path, event_sequence, before<=30, after<=30) \
on the mismatch event_sequence.

Evidence shape: field-level before→after diffs, affected entity ids, \
Snapshot event_sequence values, mismatch markers from tool `text`.
"""

PROMPTS: dict[str, dict[str, str]] = {
    "triage": {
        "title": "Triage",
        "description": "Recommended first steps after reproducing a bug.",
        "text": PROMPT_TRIAGE,
    },
    "dangling": {
        "title": "Dangling survivor",
        "description": "Investigate a dangling that was not destroyed.",
        "text": PROMPT_DANGLING,
    },
    "persistence": {
        "title": "Persistence mismatch",
        "description": "Investigate Snapshot / persistence sync mismatches.",
        "text": PROMPT_PERSISTENCE,
    },
}


def prompt_text(name: str) -> str:
    entry = PROMPTS[name]
    return entry["text"]
