import * as fs from "node:fs/promises";
import * as path from "node:path";
import * as vscode from "vscode";

import { resolveDtjBinaryPath } from "./binary";
import {
  BINARY_SETTING,
  DEFAULT_TRACEQL_QUERY,
  EVENTS_PAGE_DEFAULT,
  VIEW_TYPE,
} from "./constants";
import { validateDtjDocumentUri } from "./documentUri";
import {
  assertPersistedNavSafe,
  parseWebviewMessage,
  sanitizePersistedNav,
  type ExplorerUiState,
  type PersistedNavState,
} from "./messages";
import { mapBrowseOpError, mapClientError, mapReaderOrNativeError } from "./nativeErrors";
import { createCspNonce } from "./nonce";
import { defaultEventsQuery, type EventFilters } from "./protocol";
import type { ProcessRunner } from "./processRunner";
import { siblingDtjFsPath } from "./resolveSessionJournal";
import { formatTraceqlSubset, parseTraceqlSubset } from "./traceqlSubset";
import { UiSessionClient } from "./uiSessionClient";
import { initialExplorerState, renderWebviewHtml } from "./webviewHtml";

type DocumentSession = {
  uri: vscode.Uri;
  fsPath: string;
  panel: vscode.WebviewPanel;
  client: UiSessionClient | null;
  state: ExplorerUiState;
  nav: PersistedNavState;
  generation: number;
  watcher: vscode.FileSystemWatcher | null;
  queryFileUri: vscode.Uri | null;
};

export class SessionExplorerProvider implements vscode.CustomReadonlyEditorProvider {
  public static readonly viewType = VIEW_TYPE;

  private readonly sessions = new Map<string, DocumentSession>();
  private readonly runner?: ProcessRunner;

  constructor(
    private readonly context: vscode.ExtensionContext,
    runner?: ProcessRunner,
  ) {
    this.runner = runner;
    context.subscriptions.push(
      vscode.workspace.onDidSaveTextDocument((doc) => {
        void this.onQueryFileSaved(doc);
      }),
      vscode.workspace.onDidChangeConfiguration((e) => {
        if (e.affectsConfiguration(BINARY_SETTING)) {
          void this.reloadAllSessions();
        }
      }),
    );
  }

  async openCustomDocument(
    uri: vscode.Uri,
    _openContext: vscode.CustomDocumentOpenContext,
    _token: vscode.CancellationToken,
  ): Promise<vscode.CustomDocument> {
    const validation = validateDtjDocumentUri({
      scheme: uri.scheme,
      fsPath: uri.fsPath,
      path: uri.path,
    });
    if (!validation.ok) {
      throw new Error(validation.message);
    }
    return { uri, dispose: () => undefined };
  }

  async resolveCustomEditor(
    document: vscode.CustomDocument,
    webviewPanel: vscode.WebviewPanel,
    _token: vscode.CancellationToken,
  ): Promise<void> {
    const key = document.uri.toString();
    const navKey = `nav:${key}`;
    const persisted = sanitizePersistedNav(this.context.workspaceState.get(navKey));
    if (!assertPersistedNavSafe(persisted)) {
      await this.context.workspaceState.update(navKey, undefined);
    }

    const queryText =
      persisted.queryText.trim() ||
      formatTraceqlSubset(persisted.filters, EVENTS_PAGE_DEFAULT) ||
      DEFAULT_TRACEQL_QUERY;

    const session: DocumentSession = {
      uri: document.uri,
      fsPath: document.uri.fsPath,
      panel: webviewPanel,
      client: null,
      state: initialExplorerState({
        filters: persisted.filters,
        offset: persisted.offset,
        selectedSequence: persisted.selectedSequence,
        limit: EVENTS_PAGE_DEFAULT,
        queryText,
      }),
      nav: { ...sanitizePersistedNav(persisted), queryText },
      generation: 0,
      watcher: null,
      queryFileUri: null,
    };
    this.sessions.set(key, session);

    webviewPanel.webview.options = {
      enableScripts: true,
      localResourceRoots: [],
    };
    const nonce = createCspNonce();
    webviewPanel.webview.html = renderWebviewHtml(nonce, webviewPanel.webview.cspSource);

    webviewPanel.webview.onDidReceiveMessage(async (raw) => {
      const msg = parseWebviewMessage(raw);
      if (!msg) return;
      const current = this.sessions.get(key);
      if (!current) return;
      switch (msg.type) {
        case "ready":
        case "refresh":
          await this.reload(key);
          break;
        case "runQuery":
          await this.runQuery(key, msg.text);
          break;
        case "openQueryFile":
          await this.openQueryFile(key, msg.text);
          break;
        case "loadEvents":
          await this.loadEvents(key, msg.offset, msg.limit, msg.filters);
          break;
        case "selectEvent":
          await this.selectEvent(key, msg.sequence);
          break;
      }
    });

    webviewPanel.onDidDispose(() => {
      const s = this.sessions.get(key);
      s?.client?.dispose();
      s?.watcher?.dispose();
      this.sessions.delete(key);
    });

    await this.reload(key);
  }

  async refreshActive(): Promise<void> {
    for (const [k, s] of this.sessions) {
      if (s.panel.active || s.panel.visible) {
        await this.reload(k);
      }
    }
  }

  private async reloadAllSessions(): Promise<void> {
    for (const key of [...this.sessions.keys()]) {
      await this.reload(key);
    }
  }

  /**
   * Run the active `.traceql` editor against its sibling `.dtj` (or a picked journal).
   * Opens Session Explorer and executes the explorer TraceQL subset via `dtj ui-session`.
   */
  async runActiveTraceql(opts?: { forcePick?: boolean }): Promise<void> {
    const editor = vscode.window.activeTextEditor;
    if (!editor) {
      void vscode.window.showErrorMessage("Open a .traceql file to run");
      return;
    }
    await this.runTraceqlDocument(editor.document, opts);
  }

  async runTraceqlDocument(
    doc: vscode.TextDocument,
    opts?: { forcePick?: boolean },
  ): Promise<void> {
    const isTraceql =
      doc.languageId === "dtg-traceql" || doc.uri.path.toLowerCase().endsWith(".traceql");
    if (!isTraceql) {
      void vscode.window.showErrorMessage("Active editor is not a .traceql file");
      return;
    }

    const text = doc.getText();
    const journalUri = await this.resolveJournalForTraceql(doc, opts);
    if (!journalUri) return;

    const session = await this.ensureExplorerSession(journalUri);
    if (!session) {
      void vscode.window.showErrorMessage("Could not open DTJ Session Explorer for the journal");
      return;
    }

    session.queryFileUri = doc.uri.scheme === "untitled" ? session.queryFileUri : doc.uri;
    session.state.queryText = text;
    void session.panel.webview.postMessage({ type: "setQueryText", text });
    session.panel.reveal(undefined, false);

    if (!session.client) {
      void vscode.window.showErrorMessage(
        session.state.message ||
          `Session Explorer is not ready — set absolute ${BINARY_SETTING} to a prebuilt dtj binary`,
      );
      this.postState(session);
      return;
    }

    await this.runQuery(session.uri.toString(), text);
  }

  /**
   * Resolve order: sibling `.dtj` → linked open explorer → remembered pick → dialog.
   * `forcePick` skips auto-resolve and always asks.
   */
  private async resolveJournalForTraceql(
    doc: vscode.TextDocument,
    opts?: { forcePick?: boolean },
  ): Promise<vscode.Uri | null> {
    if (!opts?.forcePick && doc.uri.scheme === "file") {
      const sibling = siblingDtjFsPath(doc.uri.fsPath);
      if (sibling) {
        try {
          await fs.access(sibling);
          return vscode.Uri.file(sibling);
        } catch {
          /* fall through */
        }
      }
    }

    if (!opts?.forcePick) {
      for (const s of this.sessions.values()) {
        if (s.queryFileUri?.toString() === doc.uri.toString()) {
          return s.uri;
        }
      }

      const remembered = this.context.workspaceState.get<string>(
        journalPickStateKey(doc.uri.toString()),
      );
      if (remembered && path.isAbsolute(remembered)) {
        try {
          await fs.access(remembered);
          return vscode.Uri.file(remembered);
        } catch {
          /* fall through */
        }
      }
    }

    const picked = await vscode.window.showOpenDialog({
      canSelectFiles: true,
      canSelectMany: false,
      canSelectFolders: false,
      filters: { "DTJ journal": ["dtj"] },
      openLabel: "Run against",
      title: "Select .dtj session for this TraceQL query",
      defaultUri:
        doc.uri.scheme === "file" ? vscode.Uri.file(path.dirname(doc.uri.fsPath)) : undefined,
    });
    const journal = picked?.[0] ?? null;
    if (journal?.scheme === "file") {
      await this.context.workspaceState.update(
        journalPickStateKey(doc.uri.toString()),
        journal.fsPath,
      );
    }
    return journal;
  }

  private async ensureExplorerSession(uri: vscode.Uri): Promise<DocumentSession | null> {
    const key = uri.toString();
    if (!this.sessions.has(key)) {
      try {
        await vscode.commands.executeCommand("vscode.openWith", uri, VIEW_TYPE);
      } catch (err) {
        void vscode.window.showErrorMessage(
          err instanceof Error ? err.message : "Failed to open Session Explorer",
        );
        return null;
      }
    }

    const deadline = Date.now() + 12_000;
    while (Date.now() < deadline) {
      const s = this.sessions.get(key);
      if (s) {
        if (s.client) return s;
        if (s.state.phase !== "loading") return s;
      }
      await new Promise((r) => setTimeout(r, 40));
    }
    return this.sessions.get(key) ?? null;
  }

  private postState(session: DocumentSession): void {
    void session.panel.webview.postMessage({ type: "state", state: session.state });
  }

  private async persistNav(key: string, session: DocumentSession): Promise<void> {
    const nav: PersistedNavState = {
      filters: session.state.filters,
      offset: session.state.offset,
      selectedSequence: session.state.selectedSequence,
      queryText: session.state.queryText,
    };
    if (!assertPersistedNavSafe(nav)) return;
    session.nav = nav;
    await this.context.workspaceState.update(`nav:${key}`, nav);
  }

  private async runQuery(key: string, text: string): Promise<void> {
    const session = this.sessions.get(key);
    if (!session?.client) return;
    session.state.queryText = text;
    const parsed = parseTraceqlSubset(text);
    if (!parsed.ok) {
      session.state = {
        ...session.state,
        busy: null,
        errorKind: parsed.kind,
        message: parsed.message,
        events: undefined,
        detail: undefined,
        selectedSequence: null,
        phase: session.state.summary
          ? session.state.stale
            ? "stale"
            : "ready"
          : session.state.phase,
      };
      this.postState(session);
      await this.persistNav(key, session);
      return;
    }
    await this.loadEvents(key, "0", parsed.limit, parsed.filters, text);
  }

  private async openQueryFile(key: string, text: string): Promise<void> {
    const session = this.sessions.get(key);
    if (!session) return;
    const body = text.trim() || session.state.queryText || DEFAULT_TRACEQL_QUERY;
    // Keep explorer bar as the text the user had; never clobber from disk on open.
    session.state.queryText = body;

    try {
      let uri: vscode.Uri;
      if (session.uri.scheme === "file") {
        const dir = path.dirname(session.uri.fsPath);
        const base = path.basename(session.uri.fsPath, path.extname(session.uri.fsPath));
        const sidecar = path.join(dir, `${base}.traceql`);
        uri = vscode.Uri.file(sidecar);
        try {
          await fs.access(sidecar);
        } catch {
          await fs.writeFile(sidecar, `${body}\n`, "utf8");
        }
      } else {
        const doc = await vscode.workspace.openTextDocument({
          content: `${body}\n`,
          language: "dtg-traceql",
        });
        session.queryFileUri = doc.uri;
        await vscode.window.showTextDocument(doc, {
          preview: false,
          viewColumn: vscode.ViewColumn.Beside,
        });
        await this.persistNav(key, session);
        return;
      }

      session.queryFileUri = uri;
      const doc = await vscode.workspace.openTextDocument(uri);
      await vscode.window.showTextDocument(doc, {
        preview: false,
        viewColumn: vscode.ViewColumn.Beside,
      });
      await this.persistNav(key, session);
    } catch (err) {
      session.state = {
        ...session.state,
        busy: null,
        errorKind: "TraceqlSubsetError",
        message: err instanceof Error ? err.message : "failed to open query file",
        phase: session.state.summary ? (session.state.stale ? "stale" : "ready") : session.state.phase,
      };
      this.postState(session);
    }
  }

  private async onQueryFileSaved(doc: vscode.TextDocument): Promise<void> {
    for (const session of this.sessions.values()) {
      if (!session.queryFileUri) continue;
      if (session.queryFileUri.toString() !== doc.uri.toString()) continue;
      const text = doc.getText();
      session.state.queryText = text;
      void session.panel.webview.postMessage({ type: "setQueryText", text });
      await this.persistNav(session.uri.toString(), session);
    }
  }

  private async reload(key: string): Promise<void> {
    const session = this.sessions.get(key);
    if (!session) return;
    session.generation += 1;
    const gen = session.generation;
    session.client?.dispose();
    session.client = null;
    session.state = {
      ...session.state,
      phase: "loading",
      busy: "reload",
      message: "Loading session…",
      errorKind: undefined,
      detail: undefined,
      events: undefined,
      summary: undefined,
      stale: false,
      tornTail: false,
    };
    this.postState(session);

    if (!vscode.workspace.isTrusted) {
      session.state = {
        ...session.state,
        busy: null,
        phase: "WorkspaceUntrusted",
        message: "Workspace Trust is required to run the local dtj binary",
      };
      this.postState(session);
      return;
    }

    const validation = validateDtjDocumentUri({
      scheme: session.uri.scheme,
      fsPath: session.uri.fsPath,
      path: session.uri.path,
    });
    if (!validation.ok) {
      session.state = {
        ...session.state,
        busy: null,
        phase: "UnsupportedDocument",
        message: validation.message,
        errorKind: validation.kind,
      };
      this.postState(session);
      return;
    }
    session.fsPath = validation.fsPath;

    const configured = vscode.workspace.getConfiguration().get<string>(BINARY_SETTING);
    const binary = resolveDtjBinaryPath(configured);
    if (!binary.ok) {
      session.state = {
        ...session.state,
        busy: null,
        phase: "NativeReaderUnavailable",
        message: binary.message,
        errorKind: binary.kind,
      };
      this.postState(session);
      return;
    }

    const client = new UiSessionClient(binary.executable, this.runner);
    session.client = client;

    const hello = await client.hello();
    if (gen !== session.generation) return;
    if (!hello.ok) {
      session.state = mapClientError(hello.error, session.state);
      this.postState(session);
      return;
    }

    const summary = await client.summary(session.fsPath);
    if (gen !== session.generation) return;
    if (!summary.ok) {
      session.state = mapReaderOrNativeError(summary.error, session.state);
      this.postState(session);
      return;
    }

    const summaryObj = summary.value as { torn_tail?: boolean };
    session.state.summary = summary.value;
    session.state.tornTail = Boolean(summaryObj.torn_tail);

    // Prefer last successful filters; optionally refresh from query text if parseable.
    let filters = session.state.filters;
    let limit = session.state.limit || EVENTS_PAGE_DEFAULT;
    const parsed = parseTraceqlSubset(session.state.queryText || DEFAULT_TRACEQL_QUERY);
    if (parsed.ok) {
      filters = parsed.filters;
      limit = parsed.limit;
      session.state.filters = filters;
      session.state.limit = limit;
    }

    const query = defaultEventsQuery({
      offset: session.state.offset,
      limit,
      filters,
    });
    const events = await client.events(session.fsPath, query);
    if (gen !== session.generation) return;
    if (!events.ok) {
      session.state = mapReaderOrNativeError(events.error, {
        ...session.state,
        events: undefined,
        detail: undefined,
      });
      this.postState(session);
      return;
    }

    session.state = {
      ...session.state,
      busy: null,
      phase: "ready",
      message: undefined,
      errorKind: undefined,
      events: events.value,
      stale: false,
    };
    this.ensureWatcher(key, session);
    this.postState(session);
    await this.persistNav(key, session);

    if (session.state.selectedSequence) {
      await this.selectEvent(key, session.state.selectedSequence);
    }
  }

  private async loadEvents(
    key: string,
    offset: string,
    limit: number,
    filters: EventFilters,
    queryText?: string,
  ): Promise<void> {
    const session = this.sessions.get(key);
    if (!session?.client) return;
    session.generation += 1;
    const gen = session.generation;
    session.state.filters = filters;
    session.state.offset = offset;
    session.state.limit = limit;
    if (queryText !== undefined) {
      session.state.queryText = queryText;
    } else {
      session.state.queryText = formatTraceqlSubset(filters, limit);
    }
    session.state = {
      ...session.state,
      busy: "events",
      errorKind: undefined,
      message: undefined,
    };
    this.postState(session);
    const events = await session.client.events(
      session.fsPath,
      defaultEventsQuery({ offset, limit, filters }),
    );
    if (gen !== session.generation) return;
    if (!events.ok) {
      session.state = mapBrowseOpError(events.error, {
        ...session.state,
        events: undefined,
        detail: undefined,
      });
      this.postState(session);
      return;
    }
    const torn = Boolean((events.value as { torn_tail?: boolean }).torn_tail);
    session.state = {
      ...session.state,
      busy: null,
      phase: session.state.stale ? "stale" : "ready",
      events: events.value,
      tornTail: torn,
      errorKind: undefined,
      message: undefined,
      detail: undefined,
      selectedSequence: null,
    };
    this.postState(session);
    await this.persistNav(key, session);
  }

  private async selectEvent(key: string, sequence: string): Promise<void> {
    const session = this.sessions.get(key);
    if (!session?.client) return;
    session.generation += 1;
    const gen = session.generation;
    session.state = {
      ...session.state,
      busy: "event",
      selectedSequence: sequence,
      errorKind: undefined,
      message: undefined,
    };
    this.postState(session);
    const detail = await session.client.event(session.fsPath, sequence);
    if (gen !== session.generation) return;
    if (!detail.ok) {
      session.state = mapBrowseOpError(detail.error, {
        ...session.state,
        detail: undefined,
      });
      this.postState(session);
      await this.persistNav(key, session);
      return;
    }
    session.state = {
      ...session.state,
      busy: null,
      detail: detail.value,
      errorKind: undefined,
      message: undefined,
      phase: session.state.stale ? "stale" : "ready",
    };
    this.postState(session);
    await this.persistNav(key, session);
  }

  private ensureWatcher(key: string, session: DocumentSession): void {
    session.watcher?.dispose();
    if (session.uri.scheme !== "file") return;
    const dir = vscode.Uri.file(path.dirname(session.uri.fsPath));
    const base = path.basename(session.uri.fsPath);
    const watcher = vscode.workspace.createFileSystemWatcher(
      new vscode.RelativePattern(dir, base),
    );
    const markStale = () => {
      const s = this.sessions.get(key);
      if (!s) return;
      s.state = {
        ...s.state,
        stale: true,
        phase: s.state.phase === "ready" || s.state.phase === "stale" ? "stale" : s.state.phase,
      };
      this.postState(s);
    };
    watcher.onDidChange(markStale);
    watcher.onDidDelete(markStale);
    session.watcher = watcher;
  }
}

function journalPickStateKey(traceqlUri: string): string {
  return `journalPick:${traceqlUri}`;
}
