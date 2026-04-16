use std::collections::HashMap;
use std::sync::Arc;

use tokio::sync::RwLock;
use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::*;
use tower_lsp::{Client, LanguageServer, LspService, Server};

mod analysis;
mod completion;
mod hover;
mod navigation;

use analysis::{
    code_actions, document_symbols, find_colors, folding_ranges, get_signature_help, inlay_hints,
    semantic_tokens,
};
use completion::{completions, path_completions, use_symbol_completions};
use hover::hover_at;
use navigation::{
    definition_at, find_references, linked_editing_ranges, prepare_rename_at, rename_at,
};

// ---------------------------------------------------------------------------
// Backend
// ---------------------------------------------------------------------------

struct Backend {
    client: Client,
    documents: Arc<RwLock<HashMap<Url, String>>>,
}

impl Backend {
    async fn on_change(&self, uri: Url, text: String) {
        let result = htmlang::parser::parse(&text);
        let diags: Vec<Diagnostic> = result
            .diagnostics
            .iter()
            .map(|d| {
                let severity = match d.severity {
                    htmlang::parser::Severity::Error => DiagnosticSeverity::ERROR,
                    htmlang::parser::Severity::Warning => DiagnosticSeverity::WARNING,
                    htmlang::parser::Severity::Info => DiagnosticSeverity::INFORMATION,
                    htmlang::parser::Severity::Help => DiagnosticSeverity::HINT,
                };
                let line = d.line.saturating_sub(1) as u32;
                let col_start = d.column.unwrap_or(0) as u32;
                let col_end = if d.column.is_some() {
                    // Highlight a reasonable span from the column
                    let lines_vec: Vec<&str> = text.lines().collect();
                    lines_vec.get(line as usize)
                        .map(|l| l.len() as u32)
                        .unwrap_or(col_start + 1)
                } else {
                    1000 // Highlight entire line when no column info
                };
                Diagnostic {
                    range: Range::new(Position::new(line, col_start), Position::new(line, col_end)),
                    severity: Some(severity),
                    source: Some("htmlang".into()),
                    message: d.message.clone(),
                    ..Default::default()
                }
            })
            .collect();
        self.client
            .publish_diagnostics(uri.clone(), diags, None)
            .await;
        self.documents.write().await.insert(uri, text);
    }
}

#[tower_lsp::async_trait]
impl LanguageServer for Backend {
    async fn initialize(&self, _: InitializeParams) -> Result<InitializeResult> {
        Ok(InitializeResult {
            capabilities: ServerCapabilities {
                text_document_sync: Some(TextDocumentSyncCapability::Kind(
                    TextDocumentSyncKind::FULL,
                )),
                completion_provider: Some(CompletionOptions {
                    trigger_characters: Some(vec![
                        "@".into(),
                        "$".into(),
                        "[".into(),
                        ",".into(),
                    ]),
                    ..Default::default()
                }),
                hover_provider: Some(HoverProviderCapability::Simple(true)),
                definition_provider: Some(OneOf::Left(true)),
                rename_provider: Some(OneOf::Right(RenameOptions {
                    prepare_provider: Some(true),
                    work_done_progress_options: Default::default(),
                })),
                document_symbol_provider: Some(OneOf::Left(true)),
                code_action_provider: Some(CodeActionProviderCapability::Options(
                    CodeActionOptions {
                        code_action_kinds: Some(vec![
                            CodeActionKind::QUICKFIX,
                            CodeActionKind::REFACTOR_EXTRACT,
                        ]),
                        ..Default::default()
                    },
                )),
                color_provider: Some(ColorProviderCapability::Simple(true)),
                folding_range_provider: Some(FoldingRangeProviderCapability::Simple(true)),
                semantic_tokens_provider: Some(
                    SemanticTokensServerCapabilities::SemanticTokensOptions(SemanticTokensOptions {
                        legend: SemanticTokensLegend {
                            token_types: vec![
                                SemanticTokenType::KEYWORD,
                                SemanticTokenType::VARIABLE,
                                SemanticTokenType::FUNCTION,
                                SemanticTokenType::STRING,
                                SemanticTokenType::COMMENT,
                                SemanticTokenType::PROPERTY,
                            ],
                            token_modifiers: vec![
                                SemanticTokenModifier::new("deprecated"), // bit 0 = 1 -> dimmed/strikethrough
                            ],
                        },
                        full: Some(SemanticTokensFullOptions::Bool(true)),
                        range: None,
                        ..Default::default()
                    }),
                ),
                inlay_hint_provider: Some(OneOf::Left(true)),
                workspace_symbol_provider: Some(OneOf::Left(true)),
                linked_editing_range_provider: Some(
                    LinkedEditingRangeServerCapabilities::Simple(true),
                ),
                document_formatting_provider: Some(OneOf::Left(true)),
                document_range_formatting_provider: Some(OneOf::Left(true)),
                references_provider: Some(OneOf::Left(true)),
                signature_help_provider: Some(SignatureHelpOptions {
                    trigger_characters: Some(vec!["[".into(), ",".into()]),
                    retrigger_characters: Some(vec![",".into()]),
                    work_done_progress_options: Default::default(),
                }),
                document_link_provider: Some(DocumentLinkOptions {
                    resolve_provider: Some(false),
                    work_done_progress_options: Default::default(),
                }),
                code_lens_provider: Some(CodeLensOptions {
                    resolve_provider: Some(false),
                }),
                ..Default::default()
            },
            ..Default::default()
        })
    }

    async fn initialized(&self, _: InitializedParams) {}

    async fn shutdown(&self) -> Result<()> {
        Ok(())
    }

    async fn did_open(&self, params: DidOpenTextDocumentParams) {
        self.on_change(params.text_document.uri, params.text_document.text)
            .await;
    }

    async fn did_change(&self, params: DidChangeTextDocumentParams) {
        if let Some(change) = params.content_changes.into_iter().last() {
            self.on_change(params.text_document.uri, change.text).await;
        }
    }

    async fn did_close(&self, params: DidCloseTextDocumentParams) {
        self.documents
            .write()
            .await
            .remove(&params.text_document.uri);
        self.client
            .publish_diagnostics(params.text_document.uri, vec![], None)
            .await;
    }

    async fn completion(
        &self,
        params: CompletionParams,
    ) -> Result<Option<CompletionResponse>> {
        let uri = &params.text_document_position.text_document.uri;
        let pos = params.text_document_position.position;
        let docs = self.documents.read().await;
        let text = match docs.get(uri) {
            Some(t) => t.clone(),
            None => return Ok(None),
        };
        drop(docs);

        // Check if we're on an @include/@import/@use/@extends line for path/symbol completions
        let lines: Vec<&str> = text.lines().collect();
        if let Some(line) = lines.get(pos.line as usize) {
            let trimmed = line.trim_start();
            if trimmed.starts_with("@include ") || trimmed.starts_with("@import ")
                || trimmed.starts_with("@extends ") {
                let items = path_completions(uri, pos);
                if !items.is_empty() {
                    return Ok(Some(CompletionResponse::Array(items)));
                }
            }
            // @use "file.hl" fn1, fn2 -- after the filename, suggest exported @fn names
            if trimmed.starts_with("@use ") {
                let after_use = &trimmed[5..];
                // If we already have a filename (quoted or unquoted), suggest symbols from that file
                let has_file = after_use.contains(".hl");
                if has_file {
                    let items = use_symbol_completions(uri, trimmed, pos);
                    if !items.is_empty() {
                        return Ok(Some(CompletionResponse::Array(items)));
                    }
                } else {
                    let items = path_completions(uri, pos);
                    if !items.is_empty() {
                        return Ok(Some(CompletionResponse::Array(items)));
                    }
                }
            }
        }

        let items = completions(&text, pos);
        Ok(if items.is_empty() {
            None
        } else {
            Some(CompletionResponse::Array(items))
        })
    }

    async fn hover(&self, params: HoverParams) -> Result<Option<Hover>> {
        let uri = &params.text_document_position_params.text_document.uri;
        let pos = params.text_document_position_params.position;
        let docs = self.documents.read().await;
        let text = match docs.get(uri) {
            Some(t) => t.clone(),
            None => return Ok(None),
        };
        drop(docs);

        Ok(hover_at(&text, pos))
    }

    async fn goto_definition(
        &self,
        params: GotoDefinitionParams,
    ) -> Result<Option<GotoDefinitionResponse>> {
        let uri = params.text_document_position_params.text_document.uri.clone();
        let pos = params.text_document_position_params.position;
        let docs = self.documents.read().await;
        let text = match docs.get(&uri) {
            Some(t) => t.clone(),
            None => return Ok(None),
        };
        drop(docs);

        Ok(definition_at(&text, pos, &uri))
    }

    async fn prepare_rename(
        &self,
        params: TextDocumentPositionParams,
    ) -> Result<Option<PrepareRenameResponse>> {
        let uri = &params.text_document.uri;
        let pos = params.position;
        let docs = self.documents.read().await;
        let text = match docs.get(uri) {
            Some(t) => t.clone(),
            None => return Ok(None),
        };
        drop(docs);

        Ok(prepare_rename_at(&text, pos))
    }

    async fn rename(&self, params: RenameParams) -> Result<Option<WorkspaceEdit>> {
        let uri = params.text_document_position.text_document.uri.clone();
        let pos = params.text_document_position.position;
        let new_name = params.new_name;
        let docs = self.documents.read().await;
        let text = match docs.get(&uri) {
            Some(t) => t.clone(),
            None => return Ok(None),
        };
        drop(docs);

        Ok(rename_at(&text, pos, &new_name, &uri))
    }

    async fn document_symbol(
        &self,
        params: DocumentSymbolParams,
    ) -> Result<Option<DocumentSymbolResponse>> {
        let uri = &params.text_document.uri;
        let docs = self.documents.read().await;
        let text = match docs.get(uri) {
            Some(t) => t.clone(),
            None => return Ok(None),
        };
        drop(docs);

        let symbols = document_symbols(&text);
        Ok(if symbols.is_empty() {
            None
        } else {
            Some(DocumentSymbolResponse::Flat(symbols))
        })
    }

    async fn code_action(
        &self,
        params: CodeActionParams,
    ) -> Result<Option<CodeActionResponse>> {
        let uri = params.text_document.uri.clone();
        let docs = self.documents.read().await;
        let text = match docs.get(&uri) {
            Some(t) => t.clone(),
            None => return Ok(None),
        };
        drop(docs);

        let actions = code_actions(&text, &params.range, &params.context.diagnostics, &uri);
        Ok(if actions.is_empty() {
            None
        } else {
            Some(actions)
        })
    }

    async fn document_color(
        &self,
        params: DocumentColorParams,
    ) -> Result<Vec<ColorInformation>> {
        let uri = &params.text_document.uri;
        let docs = self.documents.read().await;
        let text = match docs.get(uri) {
            Some(t) => t.clone(),
            None => return Ok(vec![]),
        };
        drop(docs);
        Ok(find_colors(&text))
    }

    async fn color_presentation(
        &self,
        params: ColorPresentationParams,
    ) -> Result<Vec<ColorPresentation>> {
        let c = params.color;
        let r = (c.red * 255.0) as u8;
        let g = (c.green * 255.0) as u8;
        let b = (c.blue * 255.0) as u8;
        let hex = if c.alpha < 1.0 {
            let a = (c.alpha * 255.0) as u8;
            format!("#{:02x}{:02x}{:02x}{:02x}", r, g, b, a)
        } else {
            format!("#{:02x}{:02x}{:02x}", r, g, b)
        };
        Ok(vec![ColorPresentation {
            label: hex.clone(),
            text_edit: Some(TextEdit {
                range: params.range,
                new_text: hex,
            }),
            additional_text_edits: None,
        }])
    }

    async fn folding_range(
        &self,
        params: FoldingRangeParams,
    ) -> Result<Option<Vec<FoldingRange>>> {
        let uri = &params.text_document.uri;
        let docs = self.documents.read().await;
        let text = match docs.get(uri) {
            Some(t) => t.clone(),
            None => return Ok(None),
        };
        drop(docs);
        let ranges = folding_ranges(&text);
        Ok(if ranges.is_empty() { None } else { Some(ranges) })
    }

    async fn semantic_tokens_full(
        &self,
        params: SemanticTokensParams,
    ) -> Result<Option<SemanticTokensResult>> {
        let uri = &params.text_document.uri;
        let docs = self.documents.read().await;
        let text = match docs.get(uri) {
            Some(t) => t.clone(),
            None => return Ok(None),
        };
        drop(docs);
        let tokens = semantic_tokens(&text);
        Ok(Some(SemanticTokensResult::Tokens(SemanticTokens {
            result_id: None,
            data: tokens,
        })))
    }

    async fn inlay_hint(
        &self,
        params: InlayHintParams,
    ) -> Result<Option<Vec<InlayHint>>> {
        let uri = &params.text_document.uri;
        let docs = self.documents.read().await;
        let text = match docs.get(uri) {
            Some(t) => t.clone(),
            None => return Ok(None),
        };
        drop(docs);
        let hints = inlay_hints(&text);
        Ok(if hints.is_empty() { None } else { Some(hints) })
    }

    #[allow(deprecated)]
    async fn symbol(
        &self,
        params: WorkspaceSymbolParams,
    ) -> Result<Option<Vec<SymbolInformation>>> {
        let query = params.query.to_lowercase();
        let docs = self.documents.read().await;
        let mut all_symbols: Vec<SymbolInformation> = Vec::new();
        let mut covered_files: std::collections::HashSet<std::path::PathBuf> =
            std::collections::HashSet::new();

        // Open documents first — their in-memory text may be newer than what
        // is on disk.
        for (uri, text) in docs.iter() {
            if let Ok(path) = uri.to_file_path() {
                covered_files.insert(path);
            }
            for mut sym in document_symbols(text) {
                sym.location.uri = uri.clone();
                if query.is_empty() || sym.name.to_lowercase().contains(&query) {
                    all_symbols.push(sym);
                }
            }
        }
        drop(docs);

        // Extend search to .hl files on disk. The workspace root is not given
        // via a workspace folder, so derive it from the first open document's
        // path (common case: editor opened a folder containing one open file).
        let workspace_root: Option<std::path::PathBuf> = {
            let docs = self.documents.read().await;
            docs.keys()
                .next()
                .and_then(|u| u.to_file_path().ok())
                .and_then(|p| p.parent().map(|p| p.to_path_buf()))
        };
        if let Some(root) = workspace_root {
            let mut stack = vec![root];
            let max_files = 500; // safety cap for huge workspaces
            let mut scanned = 0usize;
            while let Some(dir) = stack.pop() {
                let Ok(entries) = std::fs::read_dir(&dir) else { continue };
                for entry in entries.flatten() {
                    if scanned >= max_files {
                        break;
                    }
                    let path = entry.path();
                    // Skip hidden / vendor / build directories.
                    if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                        if name.starts_with('.')
                            || name == "target"
                            || name == "node_modules"
                            || name == "dist"
                        {
                            continue;
                        }
                    }
                    if path.is_dir() {
                        stack.push(path);
                        continue;
                    }
                    if path.extension().map_or(false, |e| e == "hl")
                        && !covered_files.contains(&path)
                    {
                        scanned += 1;
                        let Ok(text) = std::fs::read_to_string(&path) else { continue };
                        let Ok(file_uri) = Url::from_file_path(&path) else { continue };
                        for mut sym in document_symbols(&text) {
                            sym.location.uri = file_uri.clone();
                            if query.is_empty()
                                || sym.name.to_lowercase().contains(&query)
                            {
                                all_symbols.push(sym);
                            }
                        }
                    }
                }
            }
        }

        Ok(if all_symbols.is_empty() {
            None
        } else {
            Some(all_symbols)
        })
    }

    async fn linked_editing_range(
        &self,
        params: LinkedEditingRangeParams,
    ) -> Result<Option<LinkedEditingRanges>> {
        let uri = &params.text_document_position_params.text_document.uri;
        let pos = params.text_document_position_params.position;
        let docs = self.documents.read().await;
        let text = match docs.get(uri) {
            Some(t) => t.clone(),
            None => return Ok(None),
        };
        drop(docs);
        Ok(linked_editing_ranges(&text, pos))
    }

    async fn formatting(
        &self,
        params: DocumentFormattingParams,
    ) -> Result<Option<Vec<TextEdit>>> {
        let uri = &params.text_document.uri;
        let docs = self.documents.read().await;
        let text = match docs.get(uri) {
            Some(t) => t.clone(),
            None => return Ok(None),
        };
        drop(docs);
        let formatted = htmlang::fmt::format(&text);
        if formatted == text {
            return Ok(None);
        }
        let last_line = text.lines().count().saturating_sub(1) as u32;
        let last_col = text.lines().last().map_or(0, |l| l.len()) as u32;
        Ok(Some(vec![TextEdit {
            range: Range::new(Position::new(0, 0), Position::new(last_line, last_col)),
            new_text: formatted,
        }]))
    }

    async fn range_formatting(
        &self,
        params: DocumentRangeFormattingParams,
    ) -> Result<Option<Vec<TextEdit>>> {
        let uri = &params.text_document.uri;
        let range = params.range;
        let docs = self.documents.read().await;
        let text = match docs.get(uri) {
            Some(t) => t.clone(),
            None => return Ok(None),
        };
        drop(docs);

        // Extract the selection (snapping to whole lines — htmlang is
        // indent-sensitive, so partial-line formatting is meaningless).
        let lines: Vec<&str> = text.lines().collect();
        let start_line = range.start.line as usize;
        let end_line = (range.end.line as usize).min(lines.len().saturating_sub(1));
        if start_line > end_line {
            return Ok(None);
        }

        let selection: String = lines[start_line..=end_line].join("\n");
        let formatted = htmlang::fmt::format(&selection);
        let formatted = formatted.trim_end_matches('\n').to_string();
        if formatted == selection {
            return Ok(None);
        }

        let last_col = lines[end_line].len() as u32;
        Ok(Some(vec![TextEdit {
            range: Range::new(
                Position::new(start_line as u32, 0),
                Position::new(end_line as u32, last_col),
            ),
            new_text: formatted,
        }]))
    }

    async fn document_link(
        &self,
        params: DocumentLinkParams,
    ) -> Result<Option<Vec<DocumentLink>>> {
        let uri = params.text_document.uri.clone();
        let docs = self.documents.read().await;
        let text = match docs.get(&uri) {
            Some(t) => t.clone(),
            None => return Ok(None),
        };
        drop(docs);

        let Ok(this_path) = uri.to_file_path() else {
            return Ok(None);
        };
        let Some(dir) = this_path.parent() else {
            return Ok(None);
        };

        let mut links = Vec::new();
        for (i, raw_line) in text.lines().enumerate() {
            let trimmed = raw_line.trim_start();
            let indent = raw_line.len() - trimmed.len();
            let (prefix, filename) = if let Some(rest) = trimmed.strip_prefix("@include ") {
                ("@include ", rest)
            } else if let Some(rest) = trimmed.strip_prefix("@import ") {
                ("@import ", rest)
            } else if let Some(rest) = trimmed.strip_prefix("@use ") {
                // @use "file.hl" fn1, fn2 — only the filename token is linkable.
                ("@use ", rest)
            } else if let Some(rest) = trimmed.strip_prefix("@extends ") {
                ("@extends ", rest)
            } else {
                continue;
            };

            // Pull the filename token (stop at whitespace or `,`).
            let name_token: &str = filename
                .trim_start_matches('"')
                .split(|c: char| c.is_whitespace() || c == ',')
                .next()
                .unwrap_or("")
                .trim_end_matches('"');
            if name_token.is_empty() {
                continue;
            }
            // Ignore glob patterns — they don't resolve to a single path.
            if name_token.contains('*') || name_token.contains('?') {
                continue;
            }

            let target = dir.join(name_token);
            if !target.exists() {
                continue;
            }
            let Ok(target_uri) = Url::from_file_path(&target) else {
                continue;
            };

            // Locate the token in the original line so the link highlights the
            // filename rather than the whole directive.
            let scan_from = indent + prefix.len();
            let Some(rel_start) = raw_line[scan_from..].find(name_token) else {
                continue;
            };
            let start_col = (scan_from + rel_start) as u32;
            let end_col = start_col + name_token.len() as u32;

            links.push(DocumentLink {
                range: Range::new(
                    Position::new(i as u32, start_col),
                    Position::new(i as u32, end_col),
                ),
                target: Some(target_uri),
                tooltip: Some(format!("Open {}", name_token)),
                data: None,
            });
        }

        Ok(if links.is_empty() { None } else { Some(links) })
    }

    async fn code_lens(
        &self,
        params: CodeLensParams,
    ) -> Result<Option<Vec<CodeLens>>> {
        let uri = params.text_document.uri.clone();
        let docs = self.documents.read().await;
        let text = match docs.get(&uri) {
            Some(t) => t.clone(),
            None => return Ok(None),
        };
        drop(docs);

        // Build a simple usage counter by scanning the whole document. For each
        // definition (@fn name / @let name / @define name) we emit a lens that
        // reports how many bare `@name` or `$name` call sites exist.
        let lines: Vec<&str> = text.lines().collect();

        #[derive(Clone)]
        struct Def { line: u32, name: String, kind: &'static str }
        let mut defs: Vec<Def> = Vec::new();

        for (i, line) in lines.iter().enumerate() {
            let trimmed = line.trim_start();
            if let Some(rest) = trimmed.strip_prefix("@fn ") {
                if let Some(n) = rest.split_whitespace().next() {
                    defs.push(Def { line: i as u32, name: n.to_string(), kind: "fn" });
                }
            } else if let Some(rest) = trimmed.strip_prefix("@let ") {
                if let Some((n, _)) = rest.trim().split_once(' ') {
                    defs.push(Def { line: i as u32, name: n.to_string(), kind: "let" });
                }
            } else if let Some(rest) = trimmed.strip_prefix("@define ") {
                if let Some(bracket) = rest.find('[') {
                    let n = rest[..bracket].trim();
                    if !n.is_empty() {
                        defs.push(Def { line: i as u32, name: n.to_string(), kind: "define" });
                    }
                }
            }
        }

        let mut lenses = Vec::with_capacity(defs.len());
        for def in &defs {
            // Count references: for @fn, look for `@name` at start-of-token;
            // for @let / @define, look for `$name`.
            let mut count: usize = 0;
            let needle = match def.kind {
                "fn" => format!("@{}", def.name),
                _ => format!("${}", def.name),
            };
            for (i, line) in lines.iter().enumerate() {
                if i as u32 == def.line {
                    continue;
                }
                let mut from = 0;
                while let Some(idx) = line[from..].find(&needle) {
                    let pos = from + idx;
                    // Guard: the char right after the match must not be a valid
                    // identifier continuation, so `$foo` doesn't match `$foobar`.
                    let after = line.as_bytes().get(pos + needle.len()).copied();
                    let ok = match after {
                        None => true,
                        Some(c) => !(c.is_ascii_alphanumeric() || c == b'_' || c == b'-'),
                    };
                    if ok {
                        count += 1;
                    }
                    from = pos + needle.len();
                }
            }

            let title = if count == 1 {
                "1 reference".to_string()
            } else {
                format!("{} references", count)
            };
            lenses.push(CodeLens {
                range: Range::new(
                    Position::new(def.line, 0),
                    Position::new(def.line, 0),
                ),
                // Non-executable lens: clients display the title without an
                // associated command action. Leaving `command` as None yields an
                // informational lens in most editors.
                command: Some(Command {
                    title,
                    command: String::new(),
                    arguments: None,
                }),
                data: None,
            });
        }

        Ok(if lenses.is_empty() { None } else { Some(lenses) })
    }

    async fn references(
        &self,
        params: ReferenceParams,
    ) -> Result<Option<Vec<Location>>> {
        let uri = params.text_document_position.text_document.uri.clone();
        let pos = params.text_document_position.position;
        let docs = self.documents.read().await;
        let text = match docs.get(&uri) {
            Some(t) => t.clone(),
            None => return Ok(None),
        };
        drop(docs);
        let refs = find_references(&text, pos, &uri);
        Ok(if refs.is_empty() { None } else { Some(refs) })
    }

    async fn signature_help(
        &self,
        params: SignatureHelpParams,
    ) -> Result<Option<SignatureHelp>> {
        let uri = &params.text_document_position_params.text_document.uri;
        let pos = params.text_document_position_params.position;
        let docs = self.documents.read().await;
        let text = match docs.get(uri) {
            Some(t) => t.clone(),
            None => return Ok(None),
        };
        drop(docs);
        Ok(get_signature_help(&text, pos))
    }
}

// ---------------------------------------------------------------------------
// Entry point
// ---------------------------------------------------------------------------

#[tokio::main]
async fn main() {
    let stdin = tokio::io::stdin();
    let stdout = tokio::io::stdout();
    let (service, socket) = LspService::new(|client| Backend {
        client,
        documents: Arc::new(RwLock::new(HashMap::new())),
    });
    Server::new(stdin, stdout, socket).serve(service).await;
}
