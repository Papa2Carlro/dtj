import * as vscode from "vscode";

import { RUN_TRACEQL_COMMAND, RUN_TRACEQL_PICK_COMMAND } from "./constants";

/** CodeLenses on line 1: Run + pick journal override. */
export class TraceqlRunCodeLensProvider implements vscode.CodeLensProvider {
  provideCodeLenses(document: vscode.TextDocument): vscode.CodeLens[] {
    if (document.languageId !== "dtg-traceql") return [];
    const range = new vscode.Range(0, 0, 0, 0);
    return [
      new vscode.CodeLens(range, {
        title: "$(play) Run TraceQL",
        tooltip: "Run against sibling .dtj (or remembered / picked journal) in Session Explorer",
        command: RUN_TRACEQL_COMMAND,
      }),
      new vscode.CodeLens(range, {
        title: "Run against…",
        tooltip: "Pick a .dtj journal explicitly",
        command: RUN_TRACEQL_PICK_COMMAND,
      }),
    ];
  }
}
