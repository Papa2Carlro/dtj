import { DEFAULT_TRACEQL_QUERY } from "./constants";
import type { ExplorerUiState } from "./messages";

/** Build CSP-safe webview HTML. Script is inline with nonce; no external network. */
export function renderWebviewHtml(nonce: string, cspSource: string): string {
  return `<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="UTF-8" />
<meta http-equiv="Content-Security-Policy" content="default-src 'none'; style-src ${cspSource} 'nonce-${nonce}'; script-src 'nonce-${nonce}';" />
<meta name="viewport" content="width=device-width, initial-scale=1.0" />
<title>DTJ Session Explorer</title>
<style nonce="${nonce}">
  :root {
    color-scheme: light dark;
    --dh-gap: 8px;
    --dh-radius: 4px;
    --dh-pad: 10px;
    --dh-border: var(--vscode-panel-border, rgba(127,127,127,0.35));
    --dh-surface: var(--vscode-editorWidget-background, var(--vscode-sideBar-background, transparent));
    --dh-input-bg: var(--vscode-input-background, transparent);
    --dh-input-fg: var(--vscode-input-foreground, var(--vscode-foreground));
    --dh-input-border: var(--vscode-input-border, var(--dh-border));
    --dh-btn-bg: var(--vscode-button-background);
    --dh-btn-fg: var(--vscode-button-foreground);
    --dh-btn-hover: var(--vscode-button-hoverBackground);
    --dh-btn2-bg: var(--vscode-button-secondaryBackground, var(--vscode-input-background));
    --dh-btn2-fg: var(--vscode-button-secondaryForeground, var(--vscode-foreground));
    --dh-btn2-hover: var(--vscode-button-secondaryHoverBackground, var(--vscode-list-hoverBackground));
    --dh-muted: var(--vscode-descriptionForeground, inherit);
    --dh-select: var(--vscode-list-activeSelectionBackground, #094771);
    --dh-hover: var(--vscode-list-hoverBackground, rgba(127,127,127,0.12));
    --dh-warn-bg: var(--vscode-inputValidation-warningBackground, transparent);
    --dh-warn-bd: var(--vscode-inputValidation-warningBorder, #cca700);
    --dh-err-bd: var(--vscode-inputValidation-errorBorder, #f14c4c);
    --dh-stale-bd: var(--vscode-charts-orange, #d18616);
    --dh-modal-bg: var(--vscode-editorWidget-background, var(--vscode-editor-background));
    --dh-shadow: 0 12px 40px rgba(0,0,0,0.45);
  }
  * { box-sizing: border-box; }
  html, body { height: 100%; }
  body {
    margin: 0;
    font-family: var(--vscode-font-family);
    font-size: var(--vscode-font-size);
    color: var(--vscode-foreground);
    background: var(--vscode-editor-background);
    display: flex;
    flex-direction: column;
    min-height: 100%;
  }
  /* JetBrains-style single linear chrome */
  .chrome {
    display: flex;
    align-items: center;
    gap: 10px;
    flex-wrap: nowrap;
    height: 36px;
    padding: 0 10px;
    border-bottom: 1px solid var(--dh-border);
    background: var(--dh-surface);
    overflow: hidden;
  }
  .chrome .brand {
    flex: 0 0 auto;
    font-size: 12px;
    font-weight: 600;
    white-space: nowrap;
  }
  .chrome .sep { color: var(--dh-muted); opacity: 0.5; flex: 0 0 auto; }
  .chrome .meta {
    flex: 1 1 auto;
    min-width: 0;
    color: var(--dh-muted);
    font-size: 12px;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    font-variant-numeric: tabular-nums;
  }
  .chrome button { flex: 0 0 auto; height: 26px; padding: 0 10px; }
  .querybar {
    display: flex;
    align-items: center;
    gap: 8px;
    flex-wrap: nowrap;
    height: 36px;
    padding: 0 10px;
    border-bottom: 1px solid var(--dh-border);
    background: var(--vscode-editor-background);
  }
  .querybar .qlabel {
    flex: 0 0 auto;
    font-size: 11px;
    font-weight: 600;
    letter-spacing: 0.04em;
    text-transform: uppercase;
    color: var(--dh-muted);
    white-space: nowrap;
  }
  #query {
    flex: 1 1 auto;
    min-width: 0;
    height: 26px;
    resize: none;
    padding: 3px 8px;
    border: 1px solid var(--dh-input-border);
    border-radius: var(--dh-radius);
    background: var(--dh-input-bg);
    color: var(--dh-input-fg);
    font-family: var(--vscode-editor-font-family, ui-monospace, monospace);
    font-size: 12px;
    line-height: 18px;
    outline: none;
    white-space: nowrap;
    overflow-x: auto;
  }
  #query.expanded {
    position: absolute;
    left: 10px;
    right: 10px;
    top: 72px;
    height: 6.5em;
    z-index: 5;
    white-space: pre-wrap;
    resize: vertical;
    box-shadow: var(--dh-shadow);
  }
  #query:focus { border-color: var(--vscode-focusBorder, var(--dh-btn-bg)); }
  .querybar .actions { display: flex; gap: 6px; flex: 0 0 auto; }
  .content { padding: var(--dh-pad); display: flex; flex-direction: column; gap: 10px; flex: 1; min-height: 0; }
  .banner {
    padding: 7px 10px;
    border-radius: var(--dh-radius);
    border: 1px solid var(--dh-warn-bd);
    border-left-width: 3px;
    background: var(--dh-warn-bg);
    font-size: 12px;
    line-height: 1.4;
  }
  .banner.error { border-color: var(--dh-err-bd); border-left-color: var(--dh-err-bd); }
  .banner.stale { border-color: var(--dh-stale-bd); border-left-color: var(--dh-stale-bd); }
  .banner.info { border-color: var(--vscode-focusBorder, #3794ff); border-left-color: var(--vscode-focusBorder, #3794ff); }
  .busy-dim { opacity: 0.55; pointer-events: none; }
  .panel {
    border: 1px solid var(--dh-border);
    border-radius: var(--dh-radius);
    background: var(--dh-surface);
    overflow: hidden;
    display: flex;
    flex-direction: column;
    flex: 1;
    min-height: 0;
  }
  .pager {
    display: flex;
    flex-wrap: nowrap;
    align-items: center;
    gap: 6px;
    height: 34px;
    padding: 0 10px;
    border-bottom: 1px solid var(--dh-border);
    background: var(--vscode-editor-background);
  }
  .pager-info {
    flex: 1;
    min-width: 0;
    font-size: 12px;
    color: var(--dh-muted);
    font-variant-numeric: tabular-nums;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .pager-info strong { color: var(--vscode-foreground); font-weight: 600; }
  .table-wrap { overflow: auto; flex: 1; min-height: 120px; }
  table { width: 100%; border-collapse: collapse; font-size: 12px; }
  thead th {
    position: sticky; top: 0; z-index: 1;
    text-align: left; padding: 7px 10px;
    background: var(--dh-surface);
    border-bottom: 1px solid var(--dh-border);
    color: var(--dh-muted); font-weight: 600; font-size: 11px;
    text-transform: uppercase; letter-spacing: 0.03em;
    white-space: nowrap;
  }
  tbody td {
    padding: 6px 10px;
    border-bottom: 1px solid var(--dh-border);
    vertical-align: middle;
    font-variant-numeric: tabular-nums;
    white-space: nowrap;
  }
  tbody tr { cursor: pointer; }
  tbody tr:hover { background: var(--dh-hover); }
  tbody tr.selected { background: var(--dh-select); color: var(--vscode-list-activeSelectionForeground, inherit); }
  .sev, .chip {
    display: inline-block;
    padding: 1px 6px;
    border-radius: 999px;
    font-size: 11px;
    border: 1px solid var(--dh-border);
    background: var(--vscode-badge-background, transparent);
    color: var(--vscode-badge-foreground, inherit);
  }
  .time-mono { color: var(--dh-muted); font-size: 11px; margin-left: 6px; }
  button {
    font: inherit; font-size: 12px; border: none; border-radius: var(--dh-radius);
    padding: 4px 10px; cursor: pointer;
    background: var(--dh-btn2-bg); color: var(--dh-btn2-fg);
  }
  button:hover:not(:disabled) { background: var(--dh-btn2-hover); }
  button.primary { background: var(--dh-btn-bg); color: var(--dh-btn-fg); }
  button.primary:hover:not(:disabled) { background: var(--dh-btn-hover); }
  button:disabled { opacity: 0.45; cursor: default; }
  #setup {
    padding: 20px;
    border: 1px dashed var(--dh-border);
    border-radius: var(--dh-radius);
    color: var(--dh-muted);
  }
  .hidden { display: none !important; }

  /* Modal */
  .modal-root {
    position: fixed; inset: 0; z-index: 50;
    display: flex; align-items: center; justify-content: center;
    padding: 24px;
  }
  .modal-root[hidden] { display: none !important; }
  .modal-backdrop {
    position: absolute; inset: 0;
    background: rgba(0,0,0,0.45);
  }
  .modal {
    position: relative;
    width: min(720px, 100%);
    max-height: min(85vh, 820px);
    display: flex; flex-direction: column;
    border: 1px solid var(--dh-border);
    border-radius: 8px;
    background: var(--dh-modal-bg);
    box-shadow: var(--dh-shadow);
    overflow: hidden;
  }
  .modal-hd {
    display: flex; align-items: center; gap: 10px;
    height: 40px; padding: 0 12px;
    border-bottom: 1px solid var(--dh-border);
  }
  .modal-hd h2 {
    margin: 0; flex: 1; min-width: 0;
    font-size: 13px; font-weight: 600;
    white-space: nowrap; overflow: hidden; text-overflow: ellipsis;
  }
  .modal-bd { padding: 14px; overflow: auto; flex: 1; }
  .kv {
    display: grid;
    grid-template-columns: 140px 1fr;
    gap: 6px 12px;
    margin: 0 0 14px;
    font-size: 12px;
  }
  .kv dt { color: var(--dh-muted); margin: 0; }
  .kv dd { margin: 0; font-variant-numeric: tabular-nums; word-break: break-word; }
  .section-title {
    margin: 0 0 8px;
    font-size: 11px; font-weight: 600;
    letter-spacing: 0.04em; text-transform: uppercase;
    color: var(--dh-muted);
  }
  .payload-table { width: 100%; border-collapse: collapse; font-size: 12px; }
  .payload-table th, .payload-table td {
    text-align: left; padding: 6px 8px;
    border-bottom: 1px solid var(--dh-border);
    vertical-align: top;
  }
  .payload-table th {
    color: var(--dh-muted); font-size: 11px; font-weight: 600;
    text-transform: uppercase; letter-spacing: 0.03em;
  }
  .type-tag {
    font-family: var(--vscode-editor-font-family, ui-monospace, monospace);
    font-size: 11px; color: var(--dh-muted);
  }
  .val-mono {
    font-family: var(--vscode-editor-font-family, ui-monospace, monospace);
    font-variant-numeric: tabular-nums;
  }
</style>
</head>
<body>
  <header class="chrome">
    <span class="brand">DTJ Session Explorer</span>
    <span class="sep">|</span>
    <span id="meta" class="meta"></span>
    <button id="refresh" type="button">Refresh</button>
  </header>
  <div class="querybar">
    <span class="qlabel">TraceQL</span>
    <textarea id="query" spellcheck="false" rows="1" placeholder='FROM events WHERE domain = "wire" LIMIT 100'></textarea>
    <div class="actions">
      <button id="run" class="primary" type="button">Run</button>
      <button id="open-query" type="button">Open .traceql</button>
    </div>
  </div>
  <div class="content">
    <div id="banner" class="banner hidden"></div>
    <div id="setup" class="hidden"></div>
    <div id="browse" class="hidden panel">
      <div class="pager">
        <button id="first" type="button" title="First page">«</button>
        <button id="prev" type="button" title="Previous page">‹</button>
        <div id="pageinfo" class="pager-info"></div>
        <button id="next" type="button" title="Next page">›</button>
        <button id="last" type="button" title="Last page">»</button>
      </div>
      <div class="table-wrap">
        <table>
          <thead>
            <tr>
              <th>seq</th><th>time</th><th>severity</th><th>domain</th>
              <th>category</th><th>event</th><th>correlation</th><th>fields</th>
            </tr>
          </thead>
          <tbody id="rows"></tbody>
        </table>
      </div>
    </div>
  </div>

  <div id="modal-root" class="modal-root" hidden>
    <div class="modal-backdrop" id="modal-backdrop"></div>
    <div class="modal" role="dialog" aria-modal="true" aria-labelledby="modal-title">
      <div class="modal-hd">
        <h2 id="modal-title">Event detail</h2>
        <button id="modal-close" type="button">Close</button>
      </div>
      <div class="modal-bd" id="modal-body"></div>
    </div>
  </div>

<script nonce="${nonce}">
(function () {
  const vscode = acquireVsCodeApi();
  let state = null;
  let queryDirty = false;
  let anchors = { startUtcMs: null, monoOriginNs: null };
  const $ = (id) => document.getElementById(id);

  function text(el, value) {
    el.textContent = value == null ? "" : String(value);
  }

  function displayCount(value) {
    if (typeof value === "string" && /^-?\\d+$/.test(value)) return value;
    if (typeof value === "bigint") return value.toString();
    if (typeof value === "number" && Number.isFinite(value) && Number.isSafeInteger(value)) {
      return String(value);
    }
    return "?";
  }

  function isCanonicalU64(s) {
    return typeof s === "string" && /^(0|[1-9]\\d*)$/.test(s);
  }

  function isCanonicalI64(s) {
    return typeof s === "string" && /^-?(0|[1-9]\\d*)$/.test(s);
  }

  function addU64(a, b) {
    return (BigInt(a) + BigInt(b)).toString();
  }

  function subU64Sat(a, b) {
    const av = BigInt(a);
    const bv = BigInt(b);
    return av > bv ? (av - bv).toString() : "0";
  }

  function setBanner(kind, message) {
    const el = $("banner");
    const cls =
      kind === "error" ? " error" :
      kind === "stale" ? " stale" :
      kind === "info" ? " info" : "";
    el.className = "banner" + cls;
    if (!message) {
      el.classList.add("hidden");
      text(el, "");
      return;
    }
    el.classList.remove("hidden");
    text(el, message);
  }

  function pad2(n) { return String(n).padStart(2, "0"); }
  function pad3(n) { return String(n).padStart(3, "0"); }

  function formatUtcMs(msBig) {
    const ms = Number(msBig);
    if (!Number.isFinite(ms)) return null;
    const d = new Date(ms);
    if (Number.isNaN(d.getTime())) return null;
    return (
      d.getUTCFullYear() + "-" + pad2(d.getUTCMonth() + 1) + "-" + pad2(d.getUTCDate()) +
      " " + pad2(d.getUTCHours()) + ":" + pad2(d.getUTCMinutes()) + ":" + pad2(d.getUTCSeconds()) +
      "." + pad3(d.getUTCMilliseconds()) + "Z"
    );
  }

  function formatMonoNs(nsStr) {
    if (!isCanonicalU64(nsStr) && !(typeof nsStr === "string" && /^\\d+$/.test(nsStr))) return "—";
    const ns = BigInt(nsStr);
    const sec = ns / 1000000000n;
    const rem = ns % 1000000000n;
    const ms = rem / 1000000n;
    return sec.toString() + "." + pad3(Number(ms)) + "s mono";
  }

  /** Wall clock from session start_utc + (mono - origin). */
  function formatEventTime(monotonicNs) {
    const mono = displayCount(monotonicNs);
    if (!/^\\d+$/.test(mono)) {
      return { wall: "—", monoLabel: "—", title: "" };
    }
    const monoLabel = formatMonoNs(mono);
    if (anchors.startUtcMs == null || anchors.monoOriginNs == null) {
      return { wall: monoLabel, monoLabel, title: "monotonic_ns=" + mono };
    }
    try {
      const deltaNs = BigInt(mono) - BigInt(anchors.monoOriginNs);
      const start = BigInt(anchors.startUtcMs);
      // floor division toward -inf for negative delta
      const deltaMs = deltaNs >= 0n ? deltaNs / 1000000n : -((-deltaNs + 999999n) / 1000000n);
      const wallMs = start + deltaMs;
      const wall = formatUtcMs(wallMs) || monoLabel;
      return {
        wall,
        monoLabel,
        title: "utc≈" + wall + " · monotonic_ns=" + mono + " · Δns=" + deltaNs.toString()
      };
    } catch {
      return { wall: monoLabel, monoLabel, title: "monotonic_ns=" + mono };
    }
  }

  function readAnchors(summary) {
    const header = (summary && summary.header) || {};
    const start = header.start_utc_unix_ms;
    const origin = header.mono_origin_ns;
    anchors = {
      startUtcMs: (typeof start === "string" && isCanonicalI64(start)) ? start : null,
      monoOriginNs: (typeof origin === "string" && isCanonicalU64(origin)) ? origin : null
    };
  }

  function formatPayloadValue(field) {
    const typ = String(field.type ?? "?");
    if (typ === "i64" || typ === "u64" || typ === "enum") {
      return { text: displayCount(field.value), type: typ };
    }
    if (typ === "interned_string") {
      return {
        text: field.value == null ? "—" : String(field.value),
        type: typ + " #" + displayCount(field.id)
      };
    }
    if (typ === "bytes") {
      return { text: field.hex ? ("0x" + field.hex) : "—", type: typ };
    }
    if (typ === "bool") {
      return { text: field.value ? "true" : "false", type: typ };
    }
    if (Array.isArray(field.value)) {
      return { text: "[" + field.value.join(", ") + "]", type: typ };
    }
    if (field.value === null || field.value === undefined) {
      return { text: "null", type: typ };
    }
    return { text: String(field.value), type: typ };
  }

  let detailDismissed = false;
  let shownDetailSeq = null;

  function closeModal() {
    $("modal-root").hidden = true;
    detailDismissed = true;
  }

  function openDetailModal(event) {
    const time = formatEventTime(event.monotonic_ns);
    const title = "Event #" + displayCount(event.event_sequence) + " · " + String(event.event_name ?? "—");
    text($("modal-title"), title);

    const body = $("modal-body");
    body.replaceChildren();

    const hTime = document.createElement("div");
    hTime.className = "section-title";
    text(hTime, "Time");
    body.appendChild(hTime);

    const kv1 = document.createElement("dl");
    kv1.className = "kv";
    const rows1 = [
      ["Wall (UTC)", time.wall],
      ["Monotonic", time.monoLabel],
      ["monotonic_ns", displayCount(event.monotonic_ns)]
    ];
    for (const [k, v] of rows1) {
      const dt = document.createElement("dt"); text(dt, k);
      const dd = document.createElement("dd"); text(dd, v); dd.className = "val-mono";
      kv1.appendChild(dt); kv1.appendChild(dd);
    }
    body.appendChild(kv1);

    const hId = document.createElement("div");
    hId.className = "section-title";
    text(hId, "Identity");
    body.appendChild(hId);

    const kv2 = document.createElement("dl");
    kv2.className = "kv";
    const rows2 = [
      ["Sequence", displayCount(event.event_sequence)],
      ["Severity", String(event.severity ?? "—")],
      ["Domain", String(event.domain ?? "—") + "  ·  id " + displayCount(event.domain_id)],
      ["Category", String(event.category ?? "—") + "  ·  id " + displayCount(event.category_id)],
      ["Event", String(event.event_name ?? "—") + "  ·  id " + displayCount(event.event_name_id)],
      ["Correlation", String(event.correlation ?? "—") + "  ·  id " + displayCount(event.correlation_id)]
    ];
    for (const [k, v] of rows2) {
      const dt = document.createElement("dt"); text(dt, k);
      const dd = document.createElement("dd"); text(dd, v);
      kv2.appendChild(dt); kv2.appendChild(dd);
    }
    body.appendChild(kv2);

    const hPay = document.createElement("div");
    hPay.className = "section-title";
    text(hPay, "Payload");
    body.appendChild(hPay);

    const table = document.createElement("table");
    table.className = "payload-table";
    const thead = document.createElement("thead");
    const hr = document.createElement("tr");
    for (const h of ["Name", "Value", "Type"]) {
      const th = document.createElement("th"); text(th, h); hr.appendChild(th);
    }
    thead.appendChild(hr);
    table.appendChild(thead);
    const tbody = document.createElement("tbody");
    const payload = Array.isArray(event.payload) ? event.payload : [];
    if (payload.length === 0) {
      const tr = document.createElement("tr");
      const td = document.createElement("td");
      td.colSpan = 3;
      text(td, "No payload fields");
      td.className = "type-tag";
      tr.appendChild(td);
      tbody.appendChild(tr);
    } else {
      for (const field of payload) {
        const tr = document.createElement("tr");
        const rendered = formatPayloadValue(field);
        const tdName = document.createElement("td"); text(tdName, field.name == null ? "?" : String(field.name));
        const tdVal = document.createElement("td"); tdVal.className = "val-mono"; text(tdVal, rendered.text);
        const tdType = document.createElement("td"); tdType.className = "type-tag"; text(tdType, rendered.type);
        tr.appendChild(tdName); tr.appendChild(tdVal); tr.appendChild(tdType);
        tbody.appendChild(tr);
      }
    }
    table.appendChild(tbody);
    body.appendChild(table);
    shownDetailSeq = String(event.event_sequence ?? "");
    detailDismissed = false;
    $("modal-root").hidden = false;
  }

  function currentQueryText() {
    return $("query").value;
  }

  function currentOffset() {
    const o = state && state.offset;
    return isCanonicalU64(o) ? o : "0";
  }

  function pageStats(s) {
    const limit = BigInt(s.limit || 100);
    const offset = BigInt(isCanonicalU64(s.offset) ? s.offset : "0");
    const matchedStr = s.events && displayCount(s.events.matched_count);
    const returnedStr = s.events ? displayCount(s.events.returned_count) : "0";
    const matched = matchedStr && /^\\d+$/.test(matchedStr) ? BigInt(matchedStr) : null;
    const returned = /^\\d+$/.test(returnedStr) ? BigInt(returnedStr) : 0n;
    const page = limit === 0n ? 1n : offset / limit + 1n;
    let totalPages = 1n;
    if (matched !== null) {
      totalPages = matched === 0n ? 1n : (matched + limit - 1n) / limit;
    }
    const hasPrev = offset > 0n;
    const hasNext = matched !== null ? offset + returned < matched : returned === limit;
    const lastOffset = matched === null || matched === 0n
      ? "0"
      : ((totalPages - 1n) * limit).toString();
    return {
      page: page.toString(),
      totalPages: matched === null ? "?" : totalPages.toString(),
      matched: matchedStr || "?",
      returned: returnedStr,
      hasPrev,
      hasNext,
      lastOffset,
      limit: limit.toString()
    };
  }

  function loadPage(offset) {
    if (!state) return;
    if (queryDirty) {
      setBanner("warn", "Query edited — press Run before paging");
      return;
    }
    if (state.busy) return;
    vscode.postMessage({
      type: "loadEvents",
      offset,
      limit: state.limit || 100,
      filters: state.filters || {}
    });
  }

  function render(s) {
    state = s;
    const setup = $("setup");
    const browse = $("browse");
    const busy = Boolean(s.busy);
    // Keep Run enabled so a new query can cancel in-flight page/detail.
    $("refresh").disabled = s.busy === "reload";
    $("open-query").disabled = s.busy === "reload";
    if (!queryDirty && typeof s.queryText === "string") {
      $("query").value = s.queryText;
    }
    if (s.phase === "ready" || (s.phase === "stale" && s.summary)) {
      setup.classList.add("hidden");
      browse.classList.remove("hidden");
      browse.classList.toggle("busy-dim", busy && s.busy !== "event");
      readAnchors(s.summary);
      const summary = s.summary || {};
      const header = summary.header || {};
      const startLabel = anchors.startUtcMs ? (formatUtcMs(BigInt(anchors.startUtcMs)) || anchors.startUtcMs) : null;
      text($("meta"), [
        summary.session_file || "",
        "events " + displayCount(summary.event_count),
        "chunks " + displayCount(summary.chunks_committed),
        header.producer_name ? String(header.producer_name) : "",
        startLabel ? ("start " + startLabel) : ""
      ].filter(Boolean).join(" · "));

      const parts = [];
      let bannerKind = "";
      if (s.busy === "events") { parts.push("Loading events…"); bannerKind = "info"; }
      else if (s.busy === "event") { parts.push("Loading event detail…"); bannerKind = "info"; }
      else if (s.busy === "reload") { parts.push("Reloading session…"); bannerKind = "info"; }
      if (queryDirty) {
        parts.push("Query edited — press Run before paging");
        if (!bannerKind) bannerKind = "warn";
      }
      const softErrors = new Set([
        "TraceqlSubsetError", "EventNotFound", "NativeTimeout",
        "ResponseTooLarge", "NativeCancelled", "NativeProtocolError", "NativeProtocolMismatch"
      ]);
      if (s.errorKind && s.message && softErrors.has(s.errorKind)) {
        parts.push(s.message);
        bannerKind = "error";
      } else if (s.message && s.errorKind) {
        parts.push(s.message);
        bannerKind = "error";
      }
      if (s.stale) {
        parts.push("File changed on disk — refresh explicitly to reload.");
        if (bannerKind !== "error") bannerKind = "stale";
      }
      if (s.tornTail) {
        parts.push("Incomplete final tail (torn_tail). Prior committed events are browseable; session is not complete.");
        if (!bannerKind) bannerKind = "warn";
      }
      if (parts.length) setBanner(bannerKind || "warn", parts.join(" "));
      else setBanner("", "");

      const events = (s.events && s.events.events) || [];
      const tbody = $("rows");
      tbody.replaceChildren();
      for (const ev of events) {
        const tr = document.createElement("tr");
        const seq = displayCount(ev.event_sequence);
        if (s.selectedSequence && seq === String(s.selectedSequence)) tr.className = "selected";
        const time = formatEventTime(ev.monotonic_ns);

        const tdSeq = document.createElement("td"); text(tdSeq, seq); tr.appendChild(tdSeq);

        const tdTime = document.createElement("td");
        tdTime.title = time.title;
        const wall = document.createElement("span");
        text(wall, time.wall);
        tdTime.appendChild(wall);
        tr.appendChild(tdTime);

        const tdSev = document.createElement("td");
        const sev = document.createElement("span");
        sev.className = "sev";
        text(sev, ev.severity == null ? "" : ev.severity);
        tdSev.appendChild(sev);
        tr.appendChild(tdSev);

        for (const key of ["domain", "category", "event_name", "correlation"]) {
          const td = document.createElement("td");
          text(td, ev[key] == null ? "" : ev[key]);
          tr.appendChild(td);
        }
        const tdFields = document.createElement("td");
        text(tdFields, displayCount(ev.payload_field_count));
        tr.appendChild(tdFields);

        tr.addEventListener("click", () => {
          if (state && state.busy) return;
          detailDismissed = false;
          vscode.postMessage({ type: "selectEvent", sequence: seq });
        });
        tbody.appendChild(tr);
      }

      const stats = pageStats(s);
      const info = $("pageinfo");
      info.replaceChildren();
      const strongPage = document.createElement("strong");
      text(strongPage, "Page " + stats.page + " / " + stats.totalPages);
      info.appendChild(strongPage);
      info.appendChild(document.createTextNode(
        "  ·  showing " + stats.returned + "  ·  matched " + stats.matched + "  ·  limit " + stats.limit
      ));
      const pageLocked = busy || queryDirty;
      $("first").disabled = pageLocked || !stats.hasPrev;
      $("prev").disabled = pageLocked || !stats.hasPrev;
      $("next").disabled = pageLocked || !stats.hasNext;
      $("last").disabled = pageLocked || !stats.hasNext;

      if (s.detail && s.detail.event) {
        const seq = String(s.detail.event.event_sequence ?? "");
        if (!detailDismissed && (shownDetailSeq !== seq || $("modal-root").hidden)) {
          openDetailModal(s.detail.event);
        }
      } else if (s.errorKind === "EventNotFound") {
        $("modal-root").hidden = true;
        shownDetailSeq = null;
      }
      return;
    }

    browse.classList.add("hidden");
    setup.classList.remove("hidden");
    $("modal-root").hidden = true;
    shownDetailSeq = null;
    detailDismissed = false;
    const msg = s.message || s.phase;
    text(setup, msg);
    if (s.phase === "loading" || s.busy === "reload") setBanner("info", msg || "Loading…");
    else setBanner("error", msg);
  }

  $("query").addEventListener("input", () => {
    queryDirty = true;
    if (state && (state.phase === "ready" || (state.phase === "stale" && state.summary))) {
      render(state);
    }
  });
  $("query").addEventListener("focus", () => { $("query").classList.add("expanded"); });
  $("query").addEventListener("blur", () => { $("query").classList.remove("expanded"); });
  $("query").addEventListener("keydown", (ev) => {
    if ((ev.metaKey || ev.ctrlKey) && ev.key === "Enter") {
      ev.preventDefault();
      $("run").click();
    }
    if (ev.key === "Escape") {
      $("query").blur();
    }
  });
  $("refresh").addEventListener("click", () => vscode.postMessage({ type: "refresh" }));
  $("run").addEventListener("click", () => {
    queryDirty = false;
    closeModal();
    vscode.postMessage({ type: "runQuery", text: currentQueryText() });
  });
  $("open-query").addEventListener("click", () => {
    vscode.postMessage({ type: "openQueryFile", text: currentQueryText() });
  });
  $("first").addEventListener("click", () => loadPage("0"));
  $("prev").addEventListener("click", () => {
    if (!state) return;
    loadPage(subU64Sat(currentOffset(), String(state.limit || 100)));
  });
  $("next").addEventListener("click", () => {
    if (!state) return;
    loadPage(addU64(currentOffset(), String(state.limit || 100)));
  });
  $("last").addEventListener("click", () => {
    if (!state) return;
    loadPage(pageStats(state).lastOffset);
  });
  $("modal-close").addEventListener("click", closeModal);
  $("modal-backdrop").addEventListener("click", closeModal);
  window.addEventListener("keydown", (ev) => {
    if (ev.key === "Escape" && !$("modal-root").hidden) {
      closeModal();
    }
  });

  window.addEventListener("message", (event) => {
    const msg = event.data;
    if (msg && msg.type === "state") render(msg.state);
    if (msg && msg.type === "setQueryText" && typeof msg.text === "string") {
      $("query").value = msg.text;
      queryDirty = false;
    }
  });

  vscode.postMessage({ type: "ready" });
})();
</script>
</body>
</html>`;
}

export function initialExplorerState(
  partial?: Partial<ExplorerUiState>,
): ExplorerUiState {
  return {
    phase: "loading",
    stale: false,
    tornTail: false,
    busy: null,
    filters: {},
    offset: "0",
    limit: 100,
    selectedSequence: null,
    queryText: DEFAULT_TRACEQL_QUERY,
    ...partial,
  };
}
