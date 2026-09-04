import * as vscode from "vscode";

import { parseTraceqlSubset } from "./traceqlSubset";

/** Live Problems panel diagnostics for explorer TraceQL subset. */
export class TraceqlDiagnosticsController {
  private readonly collection: vscode.DiagnosticCollection;

  constructor(context: vscode.ExtensionContext) {
    this.collection = vscode.languages.createDiagnosticCollection("dtg-traceql");
    context.subscriptions.push(this.collection);
    context.subscriptions.push(
      vscode.workspace.onDidChangeTextDocument((e) => {
        if (e.document.languageId === "dtg-traceql") this.refresh(e.document);
      }),
      vscode.workspace.onDidOpenTextDocument((doc) => {
        if (doc.languageId === "dtg-traceql") this.refresh(doc);
      }),
      vscode.workspace.onDidCloseTextDocument((doc) => {
        this.collection.delete(doc.uri);
      }),
    );
    for (const doc of vscode.workspace.textDocuments) {
      if (doc.languageId === "dtg-traceql") this.refresh(doc);
    }
  }

  refresh(doc: vscode.TextDocument): void {
    const parsed = parseTraceqlSubset(doc.getText());
    if (parsed.ok) {
      this.collection.set(doc.uri, []);
      return;
    }
    const range =
      doc.lineCount > 0
        ? doc.lineAt(0).range
        : new vscode.Range(0, 0, 0, 0);
    this.collection.set(doc.uri, [
      new vscode.Diagnostic(range, parsed.message, vscode.DiagnosticSeverity.Error),
    ]);
  }
}
