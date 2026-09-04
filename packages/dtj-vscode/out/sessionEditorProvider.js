"use strict";
var __createBinding = (this && this.__createBinding) || (Object.create ? (function(o, m, k, k2) {
    if (k2 === undefined) k2 = k;
    var desc = Object.getOwnPropertyDescriptor(m, k);
    if (!desc || ("get" in desc ? !m.__esModule : desc.writable || desc.configurable)) {
      desc = { enumerable: true, get: function() { return m[k]; } };
    }
    Object.defineProperty(o, k2, desc);
}) : (function(o, m, k, k2) {
    if (k2 === undefined) k2 = k;
    o[k2] = m[k];
}));
var __setModuleDefault = (this && this.__setModuleDefault) || (Object.create ? (function(o, v) {
    Object.defineProperty(o, "default", { enumerable: true, value: v });
}) : function(o, v) {
    o["default"] = v;
});
var __importStar = (this && this.__importStar) || (function () {
    var ownKeys = function(o) {
        ownKeys = Object.getOwnPropertyNames || function (o) {
            var ar = [];
            for (var k in o) if (Object.prototype.hasOwnProperty.call(o, k)) ar[ar.length] = k;
            return ar;
        };
        return ownKeys(o);
    };
    return function (mod) {
        if (mod && mod.__esModule) return mod;
        var result = {};
        if (mod != null) for (var k = ownKeys(mod), i = 0; i < k.length; i++) if (k[i] !== "default") __createBinding(result, mod, k[i]);
        __setModuleDefault(result, mod);
        return result;
    };
})();
Object.defineProperty(exports, "__esModule", { value: true });
exports.SessionExplorerProvider = void 0;
const fs = __importStar(require("node:fs/promises"));
const path = __importStar(require("node:path"));
const vscode = __importStar(require("vscode"));
const binary_1 = require("./binary");
const constants_1 = require("./constants");
const documentUri_1 = require("./documentUri");
const messages_1 = require("./messages");
const nativeErrors_1 = require("./nativeErrors");
const nonce_1 = require("./nonce");
const protocol_1 = require("./protocol");
const resolveSessionJournal_1 = require("./resolveSessionJournal");
const traceqlSubset_1 = require("./traceqlSubset");
const uiSessionClient_1 = require("./uiSessionClient");
const webviewHtml_1 = require("./webviewHtml");
class SessionExplorerProvider {
    context;
    static viewType = constants_1.VIEW_TYPE;
    sessions = new Map();
    runner;
    constructor(context, runner) {
        this.context = context;
        this.runner = runner;
        context.subscriptions.push(vscode.workspace.onDidSaveTextDocument((doc) => {
            void this.onQueryFileSaved(doc);
        }), vscode.workspace.onDidChangeConfiguration((e) => {
            if (e.affectsConfiguration(constants_1.BINARY_SETTING)) {
                void this.reloadAllSessions();
            }
        }));
    }
    async openCustomDocument(uri, _openContext, _token) {
        const validation = (0, documentUri_1.validateDtjDocumentUri)({
            scheme: uri.scheme,
            fsPath: uri.fsPath,
            path: uri.path,
        });
        if (!validation.ok) {
            throw new Error(validation.message);
        }
        return { uri, dispose: () => undefined };
    }
    async resolveCustomEditor(document, webviewPanel, _token) {
        const key = document.uri.toString();
        const navKey = `nav:${key}`;
        const persisted = (0, messages_1.sanitizePersistedNav)(this.context.workspaceState.get(navKey));
        if (!(0, messages_1.assertPersistedNavSafe)(persisted)) {
            await this.context.workspaceState.update(navKey, undefined);
        }
        const queryText = persisted.queryText.trim() ||
            (0, traceqlSubset_1.formatTraceqlSubset)(persisted.filters, constants_1.EVENTS_PAGE_DEFAULT) ||
            constants_1.DEFAULT_TRACEQL_QUERY;
        const session = {
            uri: document.uri,
            fsPath: document.uri.fsPath,
            panel: webviewPanel,
            client: null,
            state: (0, webviewHtml_1.initialExplorerState)({
                filters: persisted.filters,
                offset: persisted.offset,
                selectedSequence: persisted.selectedSequence,
                limit: constants_1.EVENTS_PAGE_DEFAULT,
                queryText,
            }),
            nav: { ...(0, messages_1.sanitizePersistedNav)(persisted), queryText },
            generation: 0,
            watcher: null,
            queryFileUri: null,
        };
        this.sessions.set(key, session);
        webviewPanel.webview.options = {
            enableScripts: true,
            localResourceRoots: [],
        };
        const nonce = (0, nonce_1.createCspNonce)();
        webviewPanel.webview.html = (0, webviewHtml_1.renderWebviewHtml)(nonce, webviewPanel.webview.cspSource);
        webviewPanel.webview.onDidReceiveMessage(async (raw) => {
            const msg = (0, messages_1.parseWebviewMessage)(raw);
            if (!msg)
                return;
            const current = this.sessions.get(key);
            if (!current)
                return;
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
    async refreshActive() {
        for (const [k, s] of this.sessions) {
            if (s.panel.active || s.panel.visible) {
                await this.reload(k);
            }
        }
    }
    async reloadAllSessions() {
        for (const key of [...this.sessions.keys()]) {
            await this.reload(key);
        }
    }
    /**
     * Run the active `.traceql` editor against its sibling `.dtj` (or a picked journal).
     * Opens Session Explorer and executes the explorer TraceQL subset via `dtj ui-session`.
     */
    async runActiveTraceql(opts) {
        const editor = vscode.window.activeTextEditor;
        if (!editor) {
            void vscode.window.showErrorMessage("Open a .traceql file to run");
            return;
        }
        await this.runTraceqlDocument(editor.document, opts);
    }
    async runTraceqlDocument(doc, opts) {
        const isTraceql = doc.languageId === "dtg-traceql" || doc.uri.path.toLowerCase().endsWith(".traceql");
        if (!isTraceql) {
            void vscode.window.showErrorMessage("Active editor is not a .traceql file");
            return;
        }
        const text = doc.getText();
        const journalUri = await this.resolveJournalForTraceql(doc, opts);
        if (!journalUri)
            return;
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
            void vscode.window.showErrorMessage(session.state.message ||
                `Session Explorer is not ready — set absolute ${constants_1.BINARY_SETTING} to a prebuilt dtj binary`);
            this.postState(session);
            return;
        }
        await this.runQuery(session.uri.toString(), text);
    }
    /**
     * Resolve order: sibling `.dtj` → linked open explorer → remembered pick → dialog.
     * `forcePick` skips auto-resolve and always asks.
     */
    async resolveJournalForTraceql(doc, opts) {
        if (!opts?.forcePick && doc.uri.scheme === "file") {
            const sibling = (0, resolveSessionJournal_1.siblingDtjFsPath)(doc.uri.fsPath);
            if (sibling) {
                try {
                    await fs.access(sibling);
                    return vscode.Uri.file(sibling);
                }
                catch {
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
            const remembered = this.context.workspaceState.get(journalPickStateKey(doc.uri.toString()));
            if (remembered && path.isAbsolute(remembered)) {
                try {
                    await fs.access(remembered);
                    return vscode.Uri.file(remembered);
                }
                catch {
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
            defaultUri: doc.uri.scheme === "file" ? vscode.Uri.file(path.dirname(doc.uri.fsPath)) : undefined,
        });
        const journal = picked?.[0] ?? null;
        if (journal?.scheme === "file") {
            await this.context.workspaceState.update(journalPickStateKey(doc.uri.toString()), journal.fsPath);
        }
        return journal;
    }
    async ensureExplorerSession(uri) {
        const key = uri.toString();
        if (!this.sessions.has(key)) {
            try {
                await vscode.commands.executeCommand("vscode.openWith", uri, constants_1.VIEW_TYPE);
            }
            catch (err) {
                void vscode.window.showErrorMessage(err instanceof Error ? err.message : "Failed to open Session Explorer");
                return null;
            }
        }
        const deadline = Date.now() + 12_000;
        while (Date.now() < deadline) {
            const s = this.sessions.get(key);
            if (s) {
                if (s.client)
                    return s;
                if (s.state.phase !== "loading")
                    return s;
            }
            await new Promise((r) => setTimeout(r, 40));
        }
        return this.sessions.get(key) ?? null;
    }
    postState(session) {
        void session.panel.webview.postMessage({ type: "state", state: session.state });
    }
    async persistNav(key, session) {
        const nav = {
            filters: session.state.filters,
            offset: session.state.offset,
            selectedSequence: session.state.selectedSequence,
            queryText: session.state.queryText,
        };
        if (!(0, messages_1.assertPersistedNavSafe)(nav))
            return;
        session.nav = nav;
        await this.context.workspaceState.update(`nav:${key}`, nav);
    }
    async runQuery(key, text) {
        const session = this.sessions.get(key);
        if (!session?.client)
            return;
        session.state.queryText = text;
        const parsed = (0, traceqlSubset_1.parseTraceqlSubset)(text);
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
    async openQueryFile(key, text) {
        const session = this.sessions.get(key);
        if (!session)
            return;
        const body = text.trim() || session.state.queryText || constants_1.DEFAULT_TRACEQL_QUERY;
        // Keep explorer bar as the text the user had; never clobber from disk on open.
        session.state.queryText = body;
        try {
            let uri;
            if (session.uri.scheme === "file") {
                const dir = path.dirname(session.uri.fsPath);
                const base = path.basename(session.uri.fsPath, path.extname(session.uri.fsPath));
                const sidecar = path.join(dir, `${base}.traceql`);
                uri = vscode.Uri.file(sidecar);
                try {
                    await fs.access(sidecar);
                }
                catch {
                    await fs.writeFile(sidecar, `${body}\n`, "utf8");
                }
            }
            else {
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
        }
        catch (err) {
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
    async onQueryFileSaved(doc) {
        for (const session of this.sessions.values()) {
            if (!session.queryFileUri)
                continue;
            if (session.queryFileUri.toString() !== doc.uri.toString())
                continue;
            const text = doc.getText();
            session.state.queryText = text;
            void session.panel.webview.postMessage({ type: "setQueryText", text });
            await this.persistNav(session.uri.toString(), session);
        }
    }
    async reload(key) {
        const session = this.sessions.get(key);
        if (!session)
            return;
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
        const validation = (0, documentUri_1.validateDtjDocumentUri)({
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
        const configured = vscode.workspace.getConfiguration().get(constants_1.BINARY_SETTING);
        const binary = (0, binary_1.resolveDtjBinaryPath)(configured);
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
        const client = new uiSessionClient_1.UiSessionClient(binary.executable, this.runner);
        session.client = client;
        const hello = await client.hello();
        if (gen !== session.generation)
            return;
        if (!hello.ok) {
            session.state = (0, nativeErrors_1.mapClientError)(hello.error, session.state);
            this.postState(session);
            return;
        }
        const summary = await client.summary(session.fsPath);
        if (gen !== session.generation)
            return;
        if (!summary.ok) {
            session.state = (0, nativeErrors_1.mapReaderOrNativeError)(summary.error, session.state);
            this.postState(session);
            return;
        }
        const summaryObj = summary.value;
        session.state.summary = summary.value;
        session.state.tornTail = Boolean(summaryObj.torn_tail);
        // Prefer last successful filters; optionally refresh from query text if parseable.
        let filters = session.state.filters;
        let limit = session.state.limit || constants_1.EVENTS_PAGE_DEFAULT;
        const parsed = (0, traceqlSubset_1.parseTraceqlSubset)(session.state.queryText || constants_1.DEFAULT_TRACEQL_QUERY);
        if (parsed.ok) {
            filters = parsed.filters;
            limit = parsed.limit;
            session.state.filters = filters;
            session.state.limit = limit;
        }
        const query = (0, protocol_1.defaultEventsQuery)({
            offset: session.state.offset,
            limit,
            filters,
        });
        const events = await client.events(session.fsPath, query);
        if (gen !== session.generation)
            return;
        if (!events.ok) {
            session.state = (0, nativeErrors_1.mapReaderOrNativeError)(events.error, {
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
    async loadEvents(key, offset, limit, filters, queryText) {
        const session = this.sessions.get(key);
        if (!session?.client)
            return;
        session.generation += 1;
        const gen = session.generation;
        session.state.filters = filters;
        session.state.offset = offset;
        session.state.limit = limit;
        if (queryText !== undefined) {
            session.state.queryText = queryText;
        }
        else {
            session.state.queryText = (0, traceqlSubset_1.formatTraceqlSubset)(filters, limit);
        }
        session.state = {
            ...session.state,
            busy: "events",
            errorKind: undefined,
            message: undefined,
        };
        this.postState(session);
        const events = await session.client.events(session.fsPath, (0, protocol_1.defaultEventsQuery)({ offset, limit, filters }));
        if (gen !== session.generation)
            return;
        if (!events.ok) {
            session.state = (0, nativeErrors_1.mapBrowseOpError)(events.error, {
                ...session.state,
                events: undefined,
                detail: undefined,
            });
            this.postState(session);
            return;
        }
        const torn = Boolean(events.value.torn_tail);
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
    async selectEvent(key, sequence) {
        const session = this.sessions.get(key);
        if (!session?.client)
            return;
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
        if (gen !== session.generation)
            return;
        if (!detail.ok) {
            session.state = (0, nativeErrors_1.mapBrowseOpError)(detail.error, {
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
    ensureWatcher(key, session) {
        session.watcher?.dispose();
        if (session.uri.scheme !== "file")
            return;
        const dir = vscode.Uri.file(path.dirname(session.uri.fsPath));
        const base = path.basename(session.uri.fsPath);
        const watcher = vscode.workspace.createFileSystemWatcher(new vscode.RelativePattern(dir, base));
        const markStale = () => {
            const s = this.sessions.get(key);
            if (!s)
                return;
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
exports.SessionExplorerProvider = SessionExplorerProvider;
function journalPickStateKey(traceqlUri) {
    return `journalPick:${traceqlUri}`;
}
//# sourceMappingURL=sessionEditorProvider.js.map