import * as vscode from "vscode";

import {
  REFRESH_COMMAND,
  RUN_TRACEQL_COMMAND,
  RUN_TRACEQL_PICK_COMMAND,
} from "./constants";
import { SessionExplorerProvider } from "./sessionEditorProvider";
import { TraceqlRunCodeLensProvider } from "./traceqlCodeLens";
import { TraceqlDiagnosticsController } from "./traceqlDiagnostics";

export function activate(context: vscode.ExtensionContext): void {
  const provider = new SessionExplorerProvider(context);
  new TraceqlDiagnosticsController(context);
  context.subscriptions.push(
    vscode.window.registerCustomEditorProvider(
      SessionExplorerProvider.viewType,
      provider,
      {
        webviewOptions: { retainContextWhenHidden: true },
        supportsMultipleEditorsPerDocument: false,
      },
    ),
    vscode.commands.registerCommand(REFRESH_COMMAND, () => provider.refreshActive()),
    vscode.commands.registerCommand(RUN_TRACEQL_COMMAND, () => provider.runActiveTraceql()),
    vscode.commands.registerCommand(RUN_TRACEQL_PICK_COMMAND, () =>
      provider.runActiveTraceql({ forcePick: true }),
    ),
    vscode.languages.registerCodeLensProvider(
      { language: "dtg-traceql" },
      new TraceqlRunCodeLensProvider(),
    ),
  );
}

export function deactivate(): void {
  // Sessions dispose with their webview panels.
}
