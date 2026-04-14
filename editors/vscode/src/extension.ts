import { ExtensionContext, workspace } from "vscode";
import {
  LanguageClient,
  LanguageClientOptions,
  ServerOptions,
} from "vscode-languageclient/node";

let client: LanguageClient | undefined;

export function activate(context: ExtensionContext) {
  const config = workspace.getConfiguration("htmlang");
  const command = config.get<string>("server.path", "htmlang-lsp");

  const serverOptions: ServerOptions = { command };

  const clientOptions: LanguageClientOptions = {
    documentSelector: [{ scheme: "file", language: "htmlang" }],
  };

  client = new LanguageClient(
    "htmlang",
    "htmlang Language Server",
    serverOptions,
    clientOptions
  );

  client.start();
}

export function deactivate(): Thenable<void> | undefined {
  return client?.stop();
}
