use std::collections::HashMap;
use std::sync::Arc;

use tokio::sync::RwLock;
use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::*;
use tower_lsp::{Client, LanguageServer, LspService, Server};

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
                };
                let line = d.line.saturating_sub(1) as u32;
                Diagnostic {
                    range: Range::new(Position::new(line, 0), Position::new(line, 1000)),
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
                code_action_provider: Some(CodeActionProviderCapability::Simple(true)),
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
                            token_modifiers: vec![],
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

        // Check if we're on an @include/@import line for path completions
        let lines: Vec<&str> = text.lines().collect();
        if let Some(line) = lines.get(pos.line as usize) {
            let trimmed = line.trim_start();
            if trimmed.starts_with("@include ") || trimmed.starts_with("@import ") {
                let items = path_completions(uri, pos);
                if !items.is_empty() {
                    return Ok(Some(CompletionResponse::Array(items)));
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
        let mut all_symbols = Vec::new();
        for (uri, text) in docs.iter() {
            let symbols = document_symbols(text);
            for mut sym in symbols {
                sym.location.uri = uri.clone();
                if query.is_empty()
                    || sym.name.to_lowercase().contains(&query)
                {
                    all_symbols.push(sym);
                }
            }
        }
        drop(docs);
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
}

// ---------------------------------------------------------------------------
// Completions
// ---------------------------------------------------------------------------

fn completions(text: &str, position: Position) -> Vec<CompletionItem> {
    let lines: Vec<&str> = text.lines().collect();
    let line = match lines.get(position.line as usize) {
        Some(l) => *l,
        None => return vec![],
    };

    let col = (position.character as usize).min(line.len());
    let before = &line[..col];

    let word_start = find_word_start(before);
    let edit_range = Range::new(
        Position::new(position.line, word_start as u32),
        position,
    );

    // Inside attribute brackets?
    if in_brackets(before) {
        let current_word = &before[word_start..];

        // $ variable/define reference
        if current_word.starts_with('$') {
            return variable_completions(text, edit_range);
        }

        // State prefix (hover:, active:, focus:) or media prefix (dark:, print:)
        if let Some(colon) = current_word.find(':') {
            let prefix = &current_word[..colon];
            if matches!(prefix, "hover" | "active" | "focus" | "focus-visible" | "focus-within" | "disabled" | "checked" | "placeholder" | "first" | "last" | "odd" | "even" | "dark" | "print" | "sm" | "md" | "lg" | "xl" | "2xl" | "motion-safe" | "motion-reduce" | "landscape" | "portrait") {
                return state_attr_completions(prefix, edit_range);
            }
        }

        return attr_completions(edit_range);
    }

    // $ variable reference outside brackets
    let current_word = &before[word_start..];
    if current_word.starts_with('$') {
        return variable_completions(text, edit_range);
    }

    // @ element/directive or start of line
    let trimmed = before.trim_start();
    if trimmed.is_empty() || trimmed.starts_with('@') {
        let mut items = element_completions(edit_range);
        items.extend(directive_completions(edit_range));
        items.extend(function_completions(text, edit_range));
        items.extend(snippet_completions(edit_range));
        return items;
    }

    vec![]
}

fn find_word_start(text: &str) -> usize {
    let bytes = text.as_bytes();
    let mut i = bytes.len();
    while i > 0 {
        let c = bytes[i - 1];
        if c.is_ascii_alphanumeric()
            || c == b'@'
            || c == b'$'
            || c == b'-'
            || c == b'_'
            || c == b':'
        {
            i -= 1;
        } else {
            break;
        }
    }
    i
}

fn in_brackets(text: &str) -> bool {
    let mut depth: i32 = 0;
    for ch in text.chars() {
        if ch == '[' {
            depth += 1;
        } else if ch == ']' {
            depth -= 1;
        }
    }
    depth > 0
}

fn item(
    label: &str,
    kind: CompletionItemKind,
    detail: &str,
    insert: &str,
    range: Range,
) -> CompletionItem {
    CompletionItem {
        label: label.to_string(),
        kind: Some(kind),
        detail: Some(detail.to_string()),
        text_edit: Some(CompletionTextEdit::Edit(TextEdit {
            range,
            new_text: insert.to_string(),
        })),
        ..Default::default()
    }
}

fn element_completions(range: Range) -> Vec<CompletionItem> {
    [
        ("@row", "Horizontal layout (flexbox row)"),
        ("@column", "Vertical layout (flexbox column)"),
        ("@col", "Vertical layout (short for @column)"),
        ("@el", "Generic container"),
        ("@text", "Styled inline text (span)"),
        ("@paragraph", "Flowing text block (p)"),
        ("@p", "Flowing text block (short for @paragraph)"),
        ("@image", "Image element"),
        ("@img", "Image element (short for @image)"),
        ("@link", "Anchor/link element"),
        ("@input", "Form input element (self-closing)"),
        ("@button", "Button element"),
        ("@btn", "Button element (short for @button)"),
        ("@select", "Select dropdown element"),
        ("@textarea", "Multi-line text input"),
        ("@option", "Option inside @select"),
        ("@opt", "Option (short for @option)"),
        ("@label", "Label element"),
        ("@raw", "Raw HTML escape hatch"),
        ("@children", "Slot for caller's children (inside @fn)"),
        ("@slot", "Named slot inside @fn (e.g., @slot header)"),
        // Semantic elements
        ("@nav", "Navigation container (nav)"),
        ("@header", "Page/section header (header)"),
        ("@footer", "Page/section footer (footer)"),
        ("@main", "Main content area (main)"),
        ("@section", "Thematic section (section)"),
        ("@article", "Self-contained content (article)"),
        ("@aside", "Sidebar/tangential content (aside)"),
        // List elements
        ("@list", "List container (ul/ol, use [ordered] for ol)"),
        ("@item", "List item (li)"),
        ("@li", "List item (short for @item)"),
        // Table elements
        ("@table", "Table element"),
        ("@thead", "Table head group"),
        ("@tbody", "Table body group"),
        ("@tr", "Table row"),
        ("@td", "Table cell"),
        ("@th", "Table header cell"),
        // Media elements
        ("@video", "Video element"),
        ("@audio", "Audio element"),
        // Additional elements
        ("@form", "Form container (form)"),
        ("@details", "Disclosure widget (details)"),
        ("@summary", "Summary for @details"),
        ("@blockquote", "Block quotation"),
        ("@cite", "Citation/source reference"),
        ("@code", "Inline code (monospace)"),
        ("@pre", "Preformatted text block"),
        ("@hr", "Horizontal rule/divider"),
        ("@divider", "Horizontal rule (alias for @hr)"),
        ("@figure", "Figure with optional caption"),
        ("@figcaption", "Caption for @figure"),
        ("@progress", "Progress bar (value, max attributes)"),
        ("@meter", "Meter/gauge element (value, min, max)"),
        ("@fragment", "Group children without a wrapper element"),
        // Dialog & interactive
        ("@dialog", "Dialog/modal element (dialog)"),
        // Definition lists
        ("@dl", "Description list (dl)"),
        ("@dt", "Description term (dt)"),
        ("@dd", "Description details (dd)"),
        // Form grouping
        ("@fieldset", "Fieldset group (fieldset)"),
        ("@legend", "Legend for @fieldset (legend)"),
        // Picture/responsive images
        ("@picture", "Responsive image container (picture)"),
        ("@source", "Media source for @picture/@video/@audio (source)"),
        // Inline semantics
        ("@time", "Date/time element (time)"),
        ("@mark", "Highlighted/marked text (mark)"),
        ("@kbd", "Keyboard input (kbd)"),
        ("@abbr", "Abbreviation (abbr)"),
        // Datalist
        ("@datalist", "Predefined options for @input (datalist)"),
    ]
    .iter()
    .map(|(name, detail)| item(name, CompletionItemKind::KEYWORD, detail, name, range))
    .collect()
}

fn directive_completions(range: Range) -> Vec<CompletionItem> {
    [
        ("@page", "Set HTML page title", "@page "),
        ("@let", "Define a variable", "@let "),
        ("@define", "Define an attribute bundle", "@define "),
        ("@fn", "Define a reusable function (supports $param=default)", "@fn "),
        ("@keyframes", "Define a CSS animation", "@keyframes "),
        ("@if", "Conditional rendering", "@if "),
        ("@else if", "Else-if branch", "@else if "),
        ("@else", "Else branch", "@else"),
        ("@each", "Loop over values (@each $var, $i in list)", "@each "),
        ("@include", "Include another .hl file (DOM + definitions)", "@include "),
        ("@import", "Import definitions only (no DOM nodes)", "@import "),
        ("@meta", "Add a <meta> tag to <head>", "@meta "),
        ("@head", "Add raw content to <head>", "@head"),
        ("@style", "Add raw CSS to stylesheet", "@style"),
        ("@slot", "Named slot in @fn for caller content", "@slot "),
        ("@match", "Pattern matching on a value", "@match "),
        ("@case", "Match case (inside @match)", "@case "),
        ("@default", "Default case (inside @match)", "@default"),
        ("@warn", "Emit a compile-time warning", "@warn "),
        ("@debug", "Print debug message during compilation", "@debug "),
        ("@lang", "Set document language (html lang attribute)", "@lang "),
        ("@favicon", "Set favicon (inlined as base64 data URI)", "@favicon "),
        ("@unless", "Inverse conditional (renders when false)", "@unless "),
        ("@og", "Add Open Graph meta tag", "@og "),
        ("@breakpoint", "Define custom responsive breakpoint", "@breakpoint "),
    ]
    .iter()
    .map(|(name, detail, insert)| {
        item(name, CompletionItemKind::SNIPPET, detail, insert, range)
    })
    .collect()
}

fn snippet_completions(range: Range) -> Vec<CompletionItem> {
    let snippets: &[(&str, &str, &str)] = &[
        (
            "card component",
            "Reusable card with title and content",
            "@fn card \\$title\n  @el [padding 20, background white, rounded 8]\n    @text [bold] \\$title\n    @children",
        ),
        (
            "responsive layout",
            "Centered responsive column layout",
            "@column [max-width 800, center-x, padding 40, spacing 20]",
        ),
        (
            "nav bar",
            "Navigation bar with horizontal items",
            "@nav [padding 16, background #1a1a2e]\n  @row [spacing 20, align-items center]",
        ),
        (
            "hero section",
            "Hero section with title and subtitle",
            "@column [padding 80, center-x, center-y, min-height 60vh]\n  @text [bold, size 48] ${1:Title}\n  @paragraph [size 18, color #666] ${2:Subtitle}",
        ),
    ];

    snippets
        .iter()
        .map(|(label, detail, insert)| CompletionItem {
            label: label.to_string(),
            kind: Some(CompletionItemKind::SNIPPET),
            detail: Some(detail.to_string()),
            insert_text_format: Some(InsertTextFormat::SNIPPET),
            text_edit: Some(CompletionTextEdit::Edit(TextEdit {
                range,
                new_text: insert.to_string(),
            })),
            sort_text: Some(format!("zz_{}", label)),
            ..Default::default()
        })
        .collect()
}

fn path_completions(uri: &Url, position: Position) -> Vec<CompletionItem> {
    let file_path = match uri.to_file_path() {
        Ok(p) => p,
        Err(_) => return vec![],
    };
    let dir = match file_path.parent() {
        Some(d) => d,
        None => return vec![],
    };

    let col = position.character as u32;
    let edit_range = Range::new(Position::new(position.line, col), Position::new(position.line, col));

    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return vec![],
    };

    let mut items = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) == Some("hl") {
            if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                // Skip the current file itself
                if Some(name) == file_path.file_name().and_then(|n| n.to_str()) {
                    continue;
                }
                items.push(CompletionItem {
                    label: name.to_string(),
                    kind: Some(CompletionItemKind::FILE),
                    detail: Some("htmlang file".to_string()),
                    text_edit: Some(CompletionTextEdit::Edit(TextEdit {
                        range: edit_range,
                        new_text: name.to_string(),
                    })),
                    ..Default::default()
                });
            }
        }
    }
    items
}

fn attr_completions(range: Range) -> Vec<CompletionItem> {
    [
        // Layout
        ("spacing", "Gap between children (supports CSS units)", true),
        ("gap", "Gap between children (alias for spacing)", true),
        ("padding", "Inner padding (1/2/3/4 values, supports CSS units)", true),
        ("padding-x", "Horizontal padding", true),
        ("padding-y", "Vertical padding", true),
        // Sizing
        ("width", "Width (px/% | fill | shrink)", true),
        ("height", "Height (px/% | fill | shrink)", true),
        ("min-width", "Minimum width", true),
        ("max-width", "Maximum width", true),
        ("min-height", "Minimum height", true),
        ("max-height", "Maximum height", true),
        // Alignment
        ("center-x", "Center horizontally", false),
        ("center-y", "Center vertically", false),
        ("align-left", "Align left", false),
        ("align-right", "Align right", false),
        ("align-top", "Align top", false),
        ("align-bottom", "Align bottom", false),
        // Style
        ("background", "Background color/value", true),
        ("color", "Text color", true),
        ("border", "Border (width [color])", true),
        ("border-top", "Top border (width [color])", true),
        ("border-bottom", "Bottom border (width [color])", true),
        ("border-left", "Left border (width [color])", true),
        ("border-right", "Right border (width [color])", true),
        ("rounded", "Border radius", true),
        ("bold", "Bold text", false),
        ("italic", "Italic text", false),
        ("underline", "Underlined text", false),
        ("size", "Font size", true),
        ("font", "Font family", true),
        ("transition", "CSS transition", true),
        ("cursor", "CSS cursor type", true),
        ("opacity", "Opacity (0-1)", true),
        // Typography
        ("text-align", "Text alignment (left/center/right/justify)", true),
        ("line-height", "Line height (unitless or px)", true),
        ("letter-spacing", "Letter spacing", true),
        ("text-transform", "Text transform (uppercase/lowercase/capitalize)", true),
        ("white-space", "White-space behavior (nowrap/pre/normal)", true),
        // Overflow & positioning
        ("overflow", "Overflow behavior (hidden/scroll/auto)", true),
        ("position", "Position type (relative/absolute/fixed/sticky)", true),
        ("top", "Top offset (for positioned elements)", true),
        ("right", "Right offset (for positioned elements)", true),
        ("bottom", "Bottom offset (for positioned elements)", true),
        ("left", "Left offset (for positioned elements)", true),
        ("z-index", "Stack order (integer)", true),
        // Display & visibility
        ("display", "Display mode (none/block/inline/flex/grid)", true),
        ("visibility", "Visibility (visible/hidden)", true),
        // Transform & filters
        ("transform", "CSS transform (e.g., rotate(45deg))", true),
        ("backdrop-filter", "Backdrop filter (e.g., blur(10px))", true),
        // Effects
        ("shadow", "Box shadow (CSS value)", true),
        // Flow
        ("wrap", "Enable flex-wrap", false),
        ("gap-x", "Horizontal gap between children", true),
        ("gap-y", "Vertical gap between children", true),
        // Grid
        ("grid", "Enable CSS grid layout", false),
        ("grid-cols", "Grid columns (number or template)", true),
        ("grid-rows", "Grid rows (number or template)", true),
        ("col-span", "Span columns in grid", true),
        ("row-span", "Span rows in grid", true),
        // Container queries
        ("container", "Enable container queries (inline-size)", false),
        ("container-name", "Container name for @container queries", true),
        ("container-type", "Container type (inline-size/size/normal)", true),
        // Identity
        ("id", "HTML id attribute", true),
        ("class", "HTML class attribute", true),
        // Animation
        ("animation", "CSS animation (e.g., name 0.3s ease)", true),
        // Form
        ("type", "Input type (text/email/password/submit/...)", true),
        ("placeholder", "Placeholder text", true),
        ("name", "Form field name", true),
        ("value", "Form field value", true),
        ("disabled", "Disable the element", false),
        ("required", "Mark field as required", false),
        ("checked", "Checkbox/radio checked state", false),
        ("for", "Label target (id of associated input)", true),
        ("action", "Form action URL", true),
        ("method", "Form method (get/post)", true),
        ("rows", "Textarea rows", true),
        ("cols", "Textarea columns", true),
        ("maxlength", "Maximum input length", true),
        ("multiple", "Allow multiple selections", false),
        // Accessibility
        ("alt", "Alternative text (for images)", true),
        ("role", "ARIA role", true),
        ("tabindex", "Tab order", true),
        ("title", "Tooltip text", true),
        ("aria-label", "Accessible label", true),
        ("aria-hidden", "Hide from assistive tech (true/false)", true),
        ("data-", "Custom data attribute", true),
        // CSS: aspect-ratio, outline, logical properties, scroll-snap
        ("aspect-ratio", "CSS aspect ratio (e.g., 16/9, 1)", true),
        ("outline", "Outline (width [color])", true),
        ("padding-inline", "Inline (horizontal) padding for i18n", true),
        ("padding-block", "Block (vertical) padding for i18n", true),
        ("margin-inline", "Inline (horizontal) margin for i18n", true),
        ("margin-block", "Block (vertical) margin for i18n", true),
        ("scroll-snap-type", "Scroll snap behavior (x/y mandatory/proximity)", true),
        ("scroll-snap-align", "Snap alignment (start/center/end)", true),
        // Media attributes
        ("controls", "Show media controls (for @video, @audio)", false),
        ("autoplay", "Auto-play media", false),
        ("loop", "Loop media playback", false),
        ("muted", "Mute media", false),
        ("poster", "Video poster image URL", true),
        ("preload", "Media preload hint (auto/metadata/none)", true),
        ("loading", "Loading behavior (lazy/eager)", true),
        ("decoding", "Image decoding (async/sync/auto)", true),
        // List
        ("ordered", "Use ordered list (ol instead of ul)", false),
        // Media src
        ("src", "Source URL for media elements", true),
        // Margin
        ("margin", "Outer margin (1/2/3/4 values)", true),
        ("margin-x", "Horizontal margin", true),
        ("margin-y", "Vertical margin", true),
        // Filter & object
        ("filter", "CSS filter (blur, brightness, grayscale, etc.)", true),
        ("object-fit", "Object fit for images (cover/contain/fill)", true),
        ("object-position", "Object position within container", true),
        // Text extras
        ("text-shadow", "Text shadow (CSS value)", true),
        ("text-overflow", "Text overflow (ellipsis/clip)", true),
        // Interaction
        ("pointer-events", "Pointer events (none/auto)", true),
        ("user-select", "User selection (none/text/all)", true),
        // Flexbox/grid alignment
        ("justify-content", "Main axis alignment (center/space-between/etc.)", true),
        ("align-items", "Cross axis alignment (center/baseline/etc.)", true),
        // Flex item
        ("order", "Flex/grid item order", true),
        // Background extras
        ("background-size", "Background size (cover/contain/auto)", true),
        ("background-position", "Background position (center/top/etc.)", true),
        ("background-repeat", "Background repeat (no-repeat/repeat/etc.)", true),
        // Text wrapping
        ("word-break", "Word break behavior (break-all/keep-all)", true),
        ("overflow-wrap", "Overflow wrap (break-word/anywhere)", true),
        // New element attrs
        ("open", "Details initially open", false),
        ("novalidate", "Disable form validation", false),
        ("low", "Meter low threshold", true),
        ("high", "Meter high threshold", true),
        ("optimum", "Meter optimum value", true),
        ("colspan", "Table cell column span", true),
        ("rowspan", "Table cell row span", true),
        ("scope", "Table header scope (col/row/colgroup/rowgroup)", true),
        ("inline", "Inline SVG images into output", false),
        // Hidden
        ("hidden", "Hide element (display:none)", false),
        // Overflow directional
        ("overflow-x", "Horizontal overflow (hidden/scroll/auto)", true),
        ("overflow-y", "Vertical overflow (hidden/scroll/auto)", true),
        // Inset
        ("inset", "Shorthand for top/right/bottom/left", true),
        // Modern form theming
        ("accent-color", "Accent color for form controls", true),
        ("caret-color", "Text cursor color", true),
        // List styling
        ("list-style", "List style type (disc/circle/square/none)", true),
        // Table styling
        ("border-collapse", "Border collapse mode (collapse/separate)", true),
        ("border-spacing", "Spacing between table cell borders", true),
        // Text decoration
        ("text-decoration", "Text decoration (underline/overline/line-through)", true),
        ("text-decoration-color", "Text decoration color", true),
        ("text-decoration-thickness", "Text decoration thickness", true),
        ("text-decoration-style", "Text decoration style (solid/dashed/dotted/wavy)", true),
        // Grid/flex placement
        ("place-items", "Shorthand for align-items + justify-items", true),
        ("place-self", "Shorthand for align-self + justify-self", true),
        // Scroll behavior
        ("scroll-behavior", "Scroll behavior (smooth/auto)", true),
        // Resize
        ("resize", "Resize behavior (none/both/horizontal/vertical)", true),
        // State prefixes
        ("hover:", "Style on hover", false),
        ("active:", "Style on active/click", false),
        ("focus:", "Style on focus", false),
        // New pseudo-state prefixes
        ("focus-visible:", "Style on keyboard focus", false),
        ("focus-within:", "Style when child has focus", false),
        ("disabled:", "Style when disabled", false),
        ("checked:", "Style when checked", false),
        ("placeholder:", "Style placeholder text", false),
        // Child selectors
        ("first:", "Style first child", false),
        ("last:", "Style last child", false),
        ("odd:", "Style odd children (1st, 3rd, ...)", false),
        ("even:", "Style even children (2nd, 4th, ...)", false),
        // Responsive prefixes
        ("sm:", "Style at 640px+ (small)", false),
        ("md:", "Style at 768px+ (medium)", false),
        ("lg:", "Style at 1024px+ (large)", false),
        ("xl:", "Style at 1280px+ (extra large)", false),
        // Additional responsive prefixes
        ("2xl:", "Style at 1536px+ (2x extra large)", false),
        // Motion prefixes
        ("motion-safe:", "Style when motion is allowed", false),
        ("motion-reduce:", "Style when reduced motion preferred", false),
        // Orientation prefixes
        ("landscape:", "Style in landscape orientation", false),
        ("portrait:", "Style in portrait orientation", false),
        // Media prefixes
        ("dark:", "Style in dark color scheme", false),
        ("print:", "Style for print media", false),
        // Clipping & blending
        ("clip-path", "Clip path (circle, polygon, etc.)", true),
        ("mix-blend-mode", "Blend mode (multiply, screen, overlay, etc.)", true),
        ("background-blend-mode", "Background blend mode", true),
        // Writing mode
        ("writing-mode", "Writing mode (horizontal-tb, vertical-rl, etc.)", true),
        // Multi-column layout
        ("column-count", "Number of columns in multi-column layout", true),
        ("column-gap", "Gap between columns", true),
        // Text
        ("text-indent", "First-line text indentation", true),
        ("hyphens", "Hyphenation behavior (none/manual/auto)", true),
        // Flex item sizing
        ("flex-grow", "Flex grow factor", true),
        ("flex-shrink", "Flex shrink factor", true),
        ("flex-basis", "Flex basis (initial main size)", true),
        // Stacking context
        ("isolation", "Create stacking context (isolate/auto)", true),
        // Grid/flex placement
        ("place-content", "Shorthand for align-content + justify-content", true),
        // Background image
        ("background-image", "Background image (url or gradient)", true),
    ]
    .iter()
    .map(|(name, detail, takes_value)| {
        let insert = if *takes_value {
            format!("{} ", name)
        } else {
            name.to_string()
        };
        item(name, CompletionItemKind::PROPERTY, detail, &insert, range)
    })
    .collect()
}

fn state_attr_completions(prefix: &str, range: Range) -> Vec<CompletionItem> {
    [
        ("background", "Background color/value", true),
        ("color", "Text color", true),
        ("border", "Border (width [color])", true),
        ("border-top", "Top border (width [color])", true),
        ("border-bottom", "Bottom border (width [color])", true),
        ("border-left", "Left border (width [color])", true),
        ("border-right", "Right border (width [color])", true),
        ("rounded", "Border radius", true),
        ("bold", "Bold text", false),
        ("italic", "Italic text", false),
        ("underline", "Underlined text", false),
        ("size", "Font size", true),
        ("opacity", "Opacity (0-1)", true),
        ("cursor", "CSS cursor type", true),
        ("shadow", "Box shadow (CSS value)", true),
        ("text-shadow", "Text shadow", true),
        ("transform", "CSS transform", true),
        ("filter", "CSS filter", true),
        ("display", "Display mode", true),
        ("visibility", "Visibility", true),
        ("pointer-events", "Pointer events", true),
        ("user-select", "User selection", true),
        // Layout attrs for pseudo-states
        ("width", "Width", true),
        ("height", "Height", true),
        ("padding", "Inner padding", true),
        ("padding-x", "Horizontal padding", true),
        ("padding-y", "Vertical padding", true),
        ("margin", "Outer margin", true),
        ("margin-x", "Horizontal margin", true),
        ("margin-y", "Vertical margin", true),
        ("outline", "Outline", true),
        ("text-decoration", "Text decoration", true),
        ("text-decoration-color", "Text decoration color", true),
    ]
    .iter()
    .map(|(name, detail, takes_value)| {
        let full = format!("{}:{}", prefix, name);
        let insert = if *takes_value {
            format!("{} ", full)
        } else {
            full.clone()
        };
        item(&full, CompletionItemKind::PROPERTY, detail, &insert, range)
    })
    .collect()
}

fn variable_completions(text: &str, range: Range) -> Vec<CompletionItem> {
    let mut items = Vec::new();

    for line in text.lines() {
        let trimmed = line.trim();
        if let Some(rest) = trimmed.strip_prefix("@let ") {
            if let Some((name, value)) = rest.trim().split_once(' ') {
                items.push(item(
                    &format!("${}", name),
                    CompletionItemKind::VARIABLE,
                    &format!("= {}", value.trim()),
                    &format!("${}", name),
                    range,
                ));
            }
        } else if let Some(rest) = trimmed.strip_prefix("@define ") {
            if let Some(bracket) = rest.find('[') {
                let name = rest[..bracket].trim();
                items.push(item(
                    &format!("${}", name),
                    CompletionItemKind::CONSTANT,
                    "Attribute bundle",
                    &format!("${}", name),
                    range,
                ));
            }
        }
    }

    items
}

fn function_completions(text: &str, range: Range) -> Vec<CompletionItem> {
    let mut items = Vec::new();

    for line in text.lines() {
        let trimmed = line.trim();
        if let Some(rest) = trimmed.strip_prefix("@fn ") {
            let parts: Vec<&str> = rest.split_whitespace().collect();
            if let Some(name) = parts.first() {
                let params = parts[1..].join(" ");
                let detail = if params.is_empty() {
                    "Function".to_string()
                } else {
                    format!("Function({})", params)
                };
                items.push(item(
                    &format!("@{}", name),
                    CompletionItemKind::FUNCTION,
                    &detail,
                    &format!("@{}", name),
                    range,
                ));
            }
        }
    }

    items
}

// ---------------------------------------------------------------------------
// Hover
// ---------------------------------------------------------------------------

fn hover_at(text: &str, position: Position) -> Option<Hover> {
    let lines: Vec<&str> = text.lines().collect();
    let line = lines.get(position.line as usize)?;
    let col = (position.character as usize).min(line.len());
    let word = word_at(line, col)?;

    let doc = if word.starts_with('$') {
        hover_variable(text, &word[1..])
    } else if word.starts_with('@') {
        hover_user_fn(text, &word[1..]).or_else(|| hover_builtin(&word))
    } else {
        hover_builtin(&word)
    }?;

    Some(Hover {
        contents: HoverContents::Markup(MarkupContent {
            kind: MarkupKind::Markdown,
            value: doc,
        }),
        range: None,
    })
}

fn word_at(line: &str, col: usize) -> Option<String> {
    let bytes = line.as_bytes();
    let mut start = col;
    while start > 0 && is_word_byte(bytes[start - 1]) {
        start -= 1;
    }
    let mut end = col;
    while end < bytes.len() && is_word_byte(bytes[end]) {
        end += 1;
    }
    if start == end {
        return None;
    }
    Some(line[start..end].to_string())
}

fn is_word_byte(c: u8) -> bool {
    c.is_ascii_alphanumeric() || c == b'@' || c == b'$' || c == b'-' || c == b'_' || c == b':'
}

fn hover_variable(text: &str, name: &str) -> Option<String> {
    for line in text.lines() {
        let trimmed = line.trim();
        if let Some(rest) = trimmed.strip_prefix("@let ") {
            if let Some((n, v)) = rest.trim().split_once(' ') {
                if n == name {
                    return Some(format!("**${}** = `{}`", name, v.trim()));
                }
            }
        }
    }

    for line in text.lines() {
        let trimmed = line.trim();
        if let Some(rest) = trimmed.strip_prefix("@define ") {
            let rest = rest.trim();
            if let Some(bracket) = rest.find('[') {
                if rest[..bracket].trim() == name {
                    return Some(format!(
                        "**${}** \u{2014} Attribute bundle\n\n`{}`",
                        name, trimmed
                    ));
                }
            }
        }
    }

    for line in text.lines() {
        let trimmed = line.trim();
        if let Some(rest) = trimmed.strip_prefix("@fn ") {
            let parts: Vec<&str> = rest.split_whitespace().collect();
            if let Some(fn_name) = parts.first() {
                for param in &parts[1..] {
                    let p = param.strip_prefix('$').unwrap_or(param);
                    if p == name {
                        return Some(format!(
                            "**${}** \u{2014} Parameter of `@{}`",
                            name, fn_name
                        ));
                    }
                }
            }
        }
    }

    None
}

fn hover_user_fn(text: &str, name: &str) -> Option<String> {
    for line in text.lines() {
        let trimmed = line.trim();
        if let Some(rest) = trimmed.strip_prefix("@fn ") {
            let parts: Vec<&str> = rest.split_whitespace().collect();
            if parts.first() == Some(&name) {
                let params = &parts[1..];
                let params_str = if params.is_empty() {
                    String::new()
                } else {
                    format!("\n\nParameters: {}", params.join(", "))
                };
                return Some(format!(
                    "**@{}** \u{2014} User function{}",
                    name, params_str
                ));
            }
        }
    }
    None
}

fn hover_builtin(word: &str) -> Option<String> {
    // Strip state prefix for attribute lookup
    let (state, base) = if let Some(rest) = word.strip_prefix("hover:") {
        (Some("hover"), rest)
    } else if let Some(rest) = word.strip_prefix("active:") {
        (Some("active"), rest)
    } else if let Some(rest) = word.strip_prefix("focus:") {
        (Some("focus"), rest)
    } else if let Some(rest) = word.strip_prefix("focus-visible:") {
        (Some("focus-visible"), rest)
    } else if let Some(rest) = word.strip_prefix("focus-within:") {
        (Some("focus-within"), rest)
    } else if let Some(rest) = word.strip_prefix("disabled:") {
        (Some("disabled"), rest)
    } else if let Some(rest) = word.strip_prefix("checked:") {
        (Some("checked"), rest)
    } else if let Some(rest) = word.strip_prefix("placeholder:") {
        (Some("placeholder"), rest)
    } else if let Some(rest) = word.strip_prefix("first:") {
        (Some("first"), rest)
    } else if let Some(rest) = word.strip_prefix("last:") {
        (Some("last"), rest)
    } else if let Some(rest) = word.strip_prefix("odd:") {
        (Some("odd"), rest)
    } else if let Some(rest) = word.strip_prefix("even:") {
        (Some("even"), rest)
    } else if let Some(rest) = word.strip_prefix("sm:") {
        (Some("sm"), rest)
    } else if let Some(rest) = word.strip_prefix("md:") {
        (Some("md"), rest)
    } else if let Some(rest) = word.strip_prefix("lg:") {
        (Some("lg"), rest)
    } else if let Some(rest) = word.strip_prefix("xl:") {
        (Some("xl"), rest)
    } else if let Some(rest) = word.strip_prefix("2xl:") {
        (Some("2xl"), rest)
    } else if let Some(rest) = word.strip_prefix("motion-safe:") {
        (Some("motion-safe"), rest)
    } else if let Some(rest) = word.strip_prefix("motion-reduce:") {
        (Some("motion-reduce"), rest)
    } else if let Some(rest) = word.strip_prefix("landscape:") {
        (Some("landscape"), rest)
    } else if let Some(rest) = word.strip_prefix("portrait:") {
        (Some("portrait"), rest)
    } else {
        (None, word)
    };

    let doc = match base {
        // Elements
        "@row" => "**@row** \u{2014} Horizontal layout\n\nRenders as `<div>` with `display: flex; flex-direction: row`.\n\nChildren are laid out left-to-right.",
        "@column" | "@col" => "**@column** \u{2014} Vertical layout\n\nRenders as `<div>` with `display: flex; flex-direction: column`.\n\nChildren are laid out top-to-bottom.",
        "@el" => "**@el** \u{2014} Generic container\n\nRenders as `<div>` with column flex layout.",
        "@text" => "**@text** \u{2014} Inline text\n\nRenders as `<span>`.\n\nUsage: `@text [bold, size 24] Hello world`",
        "@paragraph" | "@p" => "**@paragraph** \u{2014} Text block\n\nRenders as `<p>`.\n\nSupports inline elements: `{@text [bold] word}`",
        "@image" | "@img" => "**@image** \u{2014} Image\n\nRenders as `<img>`.\n\nUsage: `@image [width 200] https://example.com/photo.jpg`",
        "@link" => "**@link** \u{2014} Hyperlink\n\nRenders as `<a>`.\n\nUsage: `@link [color blue] https://example.com Link text`",
        "@raw" => "**@raw** \u{2014} Raw HTML\n\nPasses content through without processing.\n\nUsage: `@raw \"\"\"<div>custom html</div>\"\"\"`",
        "@page" => "**@page** \u{2014} Page title\n\nSets the HTML `<title>` and wraps output in a full document.\n\nUsage: `@page My Page Title`",
        "@let" => "**@let** \u{2014} Variable\n\nDefines a variable for `$name` substitution.\n\nUsage: `@let primary #3b82f6`",
        "@define" => "**@define** \u{2014} Attribute bundle\n\nDefines a reusable set of attributes.\n\nUsage: `@define card-style [padding 20, rounded 8]`\n\nApply with `$card-style` in attribute lists.",
        "@fn" => "**@fn** \u{2014} Function\n\nDefines a reusable component.\n\n```\n@fn card $title\n  @el [padding 20]\n    @text [bold] $title\n    @children\n```",
        "@keyframes" => "**@keyframes** \u{2014} CSS Animation\n\nDefines keyframes for CSS animations.\n\n```\n@keyframes fade-in\n  from{opacity:0}to{opacity:1}\n```\n\nUse with `animation` attribute: `[animation fade-in 0.3s ease]`",
        "@children" => "**@children** \u{2014} Children slot\n\nPlaceholder inside `@fn` body replaced with the caller's children.",
        "@input" => "**@input** \u{2014} Form input\n\nRenders as self-closing `<input>`.\n\nUsage: `@input [type text, placeholder Name, name user]`",
        "@button" | "@btn" => "**@button** \u{2014} Button\n\nRenders as `<button>`.\n\nUsage: `@button [type submit] Click me`",
        "@select" => "**@select** \u{2014} Select dropdown\n\nRenders as `<select>`. Use `@option` children.\n\nUsage: `@select [name color]`",
        "@textarea" => "**@textarea** \u{2014} Multi-line text input\n\nRenders as `<textarea>`.\n\nUsage: `@textarea [name bio, rows 4] Default text`",
        "@option" | "@opt" => "**@option** \u{2014} Select option\n\nRenders as `<option>`.\n\nUsage: `@option [value red] Red`",
        "@label" => "**@label** \u{2014} Form label\n\nRenders as `<label>`.\n\nUsage: `@label [for email] Email Address`",
        "@if" => "**@if** \u{2014} Conditional\n\nConditionally includes children at compile time.\n\n```\n@if $theme == dark\n  @el [background #333]\n@else if $theme == light\n  @el [background white]\n@else\n  @el [background gray]\n```",
        "@each" => "**@each** \u{2014} Loop\n\nRepeat children for each item in a comma-separated list.\nOptional index variable.\n\n```\n@each $color, $i in red,green,blue\n  @text $i: $color\n```",
        "@include" => "**@include** \u{2014} Include file\n\nIncludes another .hl file (DOM nodes + definitions).\n\nUsage: `@include header.hl`",
        "@import" => "**@import** \u{2014} Import definitions\n\nImports `@let`, `@define`, `@fn` from another .hl file without emitting DOM nodes.\n\nUsage: `@import theme.hl`",
        "@meta" => "**@meta** \u{2014} Meta tag\n\nAdds a `<meta>` tag to `<head>`.\n\nUsage: `@meta description A portfolio site`",
        "@head" => "**@head** \u{2014} Head content\n\nAdds raw content to `<head>` (fonts, icons, etc.).\n\n```\n@head\n  <link rel=\"icon\" href=\"favicon.ico\">\n```",
        "@style" => "**@style** \u{2014} Custom CSS\n\nAdds raw CSS to the stylesheet.\n\n```\n@style\n  .custom { border: 1px solid red; }\n  @container sidebar (min-width: 400px) { ... }\n```",
        "@slot" => "**@slot** \u{2014} Named slot\n\nDefines a named insertion point inside `@fn`. Callers fill it with `@slot name` + children.\n\n```\n@fn layout\n  @slot header\n  @children\n  @slot footer\n```",
        // Semantic elements
        "@nav" => "**@nav** \u{2014} Navigation\n\nRenders as `<nav>`. Semantic landmark for navigation links.",
        "@header" => "**@header** \u{2014} Header\n\nRenders as `<header>`. Page or section header.",
        "@footer" => "**@footer** \u{2014} Footer\n\nRenders as `<footer>`. Page or section footer.",
        "@main" => "**@main** \u{2014} Main content\n\nRenders as `<main>`. Primary content of the page.",
        "@section" => "**@section** \u{2014} Section\n\nRenders as `<section>`. Thematic grouping of content.",
        "@article" => "**@article** \u{2014} Article\n\nRenders as `<article>`. Self-contained, independently distributable content.",
        "@aside" => "**@aside** \u{2014} Aside\n\nRenders as `<aside>`. Content tangentially related to surrounding content.",
        // List elements
        "@list" => "**@list** \u{2014} List\n\nRenders as `<ul>` (or `<ol>` with `[ordered]`).\n\nUsage:\n```\n@list [ordered]\n  @item First\n  @item Second\n```",
        "@item" | "@li" => "**@item** \u{2014} List item\n\nRenders as `<li>`. Use inside `@list`.",
        // Table elements
        "@table" => "**@table** \u{2014} Table\n\nRenders as `<table>`.\n\n```\n@table\n  @thead\n    @tr\n      @th Name\n      @th Age\n  @tbody\n    @tr\n      @td Alice\n      @td 30\n```",
        "@thead" => "**@thead** \u{2014} Table head\n\nRenders as `<thead>`. Groups header rows.",
        "@tbody" => "**@tbody** \u{2014} Table body\n\nRenders as `<tbody>`. Groups body rows.",
        "@tr" => "**@tr** \u{2014} Table row\n\nRenders as `<tr>`.",
        "@td" => "**@td** \u{2014} Table cell\n\nRenders as `<td>`. Regular table data cell.",
        "@th" => "**@th** \u{2014} Table header cell\n\nRenders as `<th>`. Header cell (typically bold/centered).",
        // Media elements
        "@video" => "**@video** \u{2014} Video\n\nRenders as `<video>`.\n\nUsage: `@video [controls] demo.mp4`",
        "@audio" => "**@audio** \u{2014} Audio\n\nRenders as `<audio>`.\n\nUsage: `@audio [controls] song.mp3`",
        // Additional elements
        "@form" => "**@form** \u{2014} Form\n\nRenders as `<form>`. Container for form elements.\n\nUsage: `@form [method post] /submit`",
        "@details" => "**@details** \u{2014} Disclosure\n\nRenders as `<details>`. Use `[open]` for initially expanded.\n\nContains `@summary` for the toggle label.",
        "@summary" => "**@summary** \u{2014} Summary\n\nRenders as `<summary>`. Toggle label inside `@details`.\n\nUsage: `@summary Click to expand`",
        "@blockquote" => "**@blockquote** \u{2014} Block quotation\n\nRenders as `<blockquote>`. Semantic quotation container.",
        "@cite" => "**@cite** \u{2014} Citation\n\nRenders as `<cite>`. Source or reference for a quotation.\n\nUsage: `@cite The Great Gatsby`",
        "@code" => "**@code** \u{2014} Code\n\nRenders as `<code>` with monospace font.\n\nUsage: `@code console.log(\"hello\")`",
        "@pre" => "**@pre** \u{2014} Preformatted\n\nRenders as `<pre>` with preserved whitespace and monospace font.",
        "@hr" | "@divider" => "**@hr** \u{2014} Horizontal Rule\n\nRenders as self-closing `<hr>`. Visual divider.\n\nUsage: `@hr [border-top 1 #ccc]`",
        "@figure" => "**@figure** \u{2014} Figure\n\nRenders as `<figure>`. Container for media with optional `@figcaption`.\n\n```\n@figure\n  @image photo.jpg\n  @figcaption A nice photo\n```",
        "@figcaption" => "**@figcaption** \u{2014} Figure caption\n\nRenders as `<figcaption>`. Caption text inside `@figure`.",
        "@progress" => "**@progress** \u{2014} Progress bar\n\nRenders as `<progress>`.\n\nUsage: `@progress [value 70, max 100]`",
        "@meter" => "**@meter** \u{2014} Meter\n\nRenders as `<meter>`. Gauge for scalar measurement.\n\nUsage: `@meter [value 0.7, min 0, max 1, low 0.3, high 0.8]`",
        "@fragment" => "**@fragment** \u{2014} Fragment\n\nGroups children without emitting a wrapper element. Renders children directly in the parent.",
        // New elements
        "@dialog" => "**@dialog** \u{2014} Dialog\n\nRenders as `<dialog>`. Modal or non-modal dialog box.\n\nUsage: `@dialog [open] Dialog content`",
        "@dl" => "**@dl** \u{2014} Description list\n\nRenders as `<dl>`. Contains `@dt` and `@dd` pairs.",
        "@dt" => "**@dt** \u{2014} Description term\n\nRenders as `<dt>`. Term in a `@dl` description list.",
        "@dd" => "**@dd** \u{2014} Description details\n\nRenders as `<dd>`. Details for a `@dt` term.",
        "@fieldset" => "**@fieldset** \u{2014} Fieldset\n\nRenders as `<fieldset>`. Groups related form elements.\n\nUse `@legend` for a caption.",
        "@legend" => "**@legend** \u{2014} Legend\n\nRenders as `<legend>`. Caption for a `@fieldset`.",
        "@picture" => "**@picture** \u{2014} Picture\n\nRenders as `<picture>`. Container for responsive image sources.\n\nUse `@source` children for different media queries.",
        "@source" => "**@source** \u{2014} Source\n\nRenders as `<source>`. Media source for `@picture`, `@video`, or `@audio`.\n\nUsage: `@source [src image.webp, type image/webp]`",
        "@time" => "**@time** \u{2014} Time\n\nRenders as `<time>`. Machine-readable date/time.\n\nUsage: `@time [datetime 2024-01-15] January 15`",
        "@mark" => "**@mark** \u{2014} Mark\n\nRenders as `<mark>`. Highlighted or marked text.",
        "@kbd" => "**@kbd** \u{2014} Keyboard input\n\nRenders as `<kbd>`. Represents keyboard input.\n\nUsage: `@kbd Ctrl+C`",
        "@abbr" => "**@abbr** \u{2014} Abbreviation\n\nRenders as `<abbr>`. Abbreviation with optional title.\n\nUsage: `@abbr [title Hypertext Markup Language] HTML`",
        "@datalist" => "**@datalist** \u{2014} Datalist\n\nRenders as `<datalist>`. Provides predefined options for `@input`.\n\nUsage: `@datalist [id colors]`",
        // Directives
        "@match" => "**@match** \u{2014} Pattern matching\n\nMatch a value against cases.\n\n```\n@match $theme\n  @case dark\n    @el [background #333]\n  @case light\n    @el [background white]\n  @default\n    @el [background gray]\n```",
        "@case" => "**@case** \u{2014} Match case\n\nA case inside `@match`. Matches when the value equals the case value.",
        "@default" => "**@default** \u{2014} Default case\n\nFallback case inside `@match` when no other case matches.",
        "@warn" => "**@warn** \u{2014} Compile warning\n\nEmit a custom warning during compilation.\n\nUsage: `@warn This value is deprecated`",
        "@debug" => "**@debug** \u{2014} Debug message\n\nPrint a debug message to stderr during compilation.\n\nUsage: `@debug Theme is $theme`",
        "@lang" => "**@lang** \u{2014} Document language\n\nSets the `lang` attribute on the `<html>` element.\n\nUsage: `@lang en`",
        "@favicon" => "**@favicon** \u{2014} Favicon\n\nInlines a favicon as a base64 data URI in the `<head>`.\n\nUsage: `@favicon favicon.png`",
        "@unless" => "**@unless** \u{2014} Inverse conditional\n\nRenders children when the condition is false (opposite of `@if`).\n\nUsage: `@unless $debug`",
        "@og" => "**@og** \u{2014} Open Graph meta tag\n\nAdds an Open Graph `<meta>` tag to `<head>`.\n\nUsage: `@og title My Page Title`",
        "@breakpoint" => "**@breakpoint** \u{2014} Custom breakpoint\n\nDefines a custom responsive breakpoint.\n\nUsage: `@breakpoint tablet 600`",
        // Attributes
        "spacing" | "gap" => "**spacing** `<value>`\n\nGap between children. Supports CSS units (px, rem, em, %).\nMaps to CSS `gap`.",
        "padding" => "**padding** `<value>` | `<y> <x>` | `<t> <h> <b>` | `<t> <r> <b> <l>`\n\nInner padding. Supports CSS units. Accepts 1\u{2013}4 values.",
        "padding-x" => "**padding-x** `<value>`\n\nHorizontal padding (left + right). Supports CSS units.",
        "padding-y" => "**padding-y** `<value>`\n\nVertical padding (top + bottom). Supports CSS units.",
        "width" => "**width** `<value>` | `fill` | `shrink`\n\n- Number/unit: fixed width (e.g., `300`, `50%`, `80ch`)\n- `fill`: expand to fill parent\n- `shrink`: prevent flex shrinking",
        "height" => "**height** `<value>` | `fill` | `shrink`\n\n- Number/unit: fixed height (e.g., `300`, `100vh`)\n- `fill`: expand to fill parent\n- `shrink`: prevent flex shrinking",
        "min-width" => "**min-width** `<value>` \u{2014} Minimum width. Supports CSS units.",
        "max-width" => "**max-width** `<value>` \u{2014} Maximum width. Supports CSS units.",
        "min-height" => "**min-height** `<value>` \u{2014} Minimum height. Supports CSS units.",
        "max-height" => "**max-height** `<value>` \u{2014} Maximum height. Supports CSS units.",
        "center-x" => "**center-x**\n\nCenter horizontally.\n\nIn column parent: `align-self: center`\nOtherwise: auto margins.",
        "center-y" => "**center-y**\n\nCenter vertically.\n\nIn row parent: `align-self: center`\nOtherwise: auto margins.",
        "align-left" => "**align-left** \u{2014} Align to the left edge.",
        "align-right" => "**align-right** \u{2014} Align to the right edge.",
        "align-top" => "**align-top** \u{2014} Align to the top edge.",
        "align-bottom" => "**align-bottom** \u{2014} Align to the bottom edge.",
        "background" => "**background** `<color>` \u{2014} Background color or CSS background value.",
        "color" => "**color** `<color>` \u{2014} Text color.",
        "border" => "**border** `<width> [color]`\n\nBorder. Width in pixels, color defaults to `currentColor`.",
        "border-top" => "**border-top** `<width> [color]` \u{2014} Top border.",
        "border-bottom" => "**border-bottom** `<width> [color]` \u{2014} Bottom border.",
        "border-left" => "**border-left** `<width> [color]` \u{2014} Left border.",
        "border-right" => "**border-right** `<width> [color]` \u{2014} Right border.",
        "rounded" => "**rounded** `<value>` \u{2014} Border radius. Supports CSS units.",
        "bold" => "**bold** \u{2014} Bold text (`font-weight: bold`).",
        "italic" => "**italic** \u{2014} Italic text (`font-style: italic`).",
        "underline" => "**underline** \u{2014} Underlined text.",
        "size" => "**size** `<value>` \u{2014} Font size in pixels.",
        "font" => "**font** `<family>` \u{2014} Font family.",
        "transition" => "**transition** `<value>` \u{2014} CSS transition (e.g., `all 0.15s ease`).",
        "cursor" => "**cursor** `<value>` \u{2014} CSS cursor (e.g., `pointer`).",
        "opacity" => "**opacity** `<value>` \u{2014} Opacity from 0 to 1.",
        "text-align" => "**text-align** `<value>` \u{2014} Text alignment (`left`, `center`, `right`, `justify`).",
        "line-height" => "**line-height** `<value>` \u{2014} Line height. Unitless (e.g., `1.5`) or pixels.",
        "overflow" => "**overflow** `<value>` \u{2014} Overflow behavior (`hidden`, `scroll`, `auto`, `visible`).",
        "position" => "**position** `<value>` \u{2014} Position type (`relative`, `absolute`, `fixed`, `sticky`).",
        "top" => "**top** `<value>` \u{2014} Top offset for positioned elements.",
        "right" => "**right** `<value>` \u{2014} Right offset for positioned elements.",
        "bottom" => "**bottom** `<value>` \u{2014} Bottom offset for positioned elements.",
        "left" => "**left** `<value>` \u{2014} Left offset for positioned elements.",
        "z-index" => "**z-index** `<value>` \u{2014} Stack order (integer).",
        "display" => "**display** `<value>` \u{2014} Display mode (`none`, `block`, `inline`, `flex`, `grid`).",
        "visibility" => "**visibility** `<value>` \u{2014} Visibility (`visible`, `hidden`).",
        "transform" => "**transform** `<value>` \u{2014} CSS transform (e.g., `rotate(45deg)`, `scale(1.5)`).",
        "backdrop-filter" => "**backdrop-filter** `<value>` \u{2014} Backdrop filter (e.g., `blur(10px)`).",
        "letter-spacing" => "**letter-spacing** `<value>` \u{2014} Letter spacing. Supports CSS units.",
        "text-transform" => "**text-transform** `<value>` \u{2014} Text transform (`uppercase`, `lowercase`, `capitalize`).",
        "white-space" => "**white-space** `<value>` \u{2014} White-space behavior (`nowrap`, `pre`, `normal`).",
        "grid" => "**grid** \u{2014} Enable CSS grid layout on this element.",
        "grid-cols" => "**grid-cols** `<value>` \u{2014} Grid template columns. Number for equal columns, or CSS value.",
        "grid-rows" => "**grid-rows** `<value>` \u{2014} Grid template rows. Number for equal rows, or CSS value.",
        "col-span" => "**col-span** `<value>` \u{2014} Span N columns in a grid.",
        "row-span" => "**row-span** `<value>` \u{2014} Span N rows in a grid.",
        "shadow" => "**shadow** `<value>` \u{2014} Box shadow. Raw CSS value (e.g., `0 2px 4px rgba(0,0,0,0.1)`).",
        "gap-x" => "**gap-x** `<value>` \u{2014} Horizontal gap between children in pixels. Maps to `column-gap`.",
        "gap-y" => "**gap-y** `<value>` \u{2014} Vertical gap between children in pixels. Maps to `row-gap`.",
        "wrap" => "**wrap** \u{2014} Enable flex-wrap for children.",
        "id" => "**id** `<value>` \u{2014} HTML id attribute.",
        "class" => "**class** `<value>` \u{2014} HTML class attribute.",
        "animation" => "**animation** `<value>` \u{2014} CSS animation shorthand (e.g., `fade-in 0.3s ease`).\n\nDefine animations with `@keyframes`.",
        "container" => "**container** \u{2014} Enable container queries (`container-type: inline-size`).",
        "container-name" => "**container-name** `<value>` \u{2014} Name this container for `@container` queries.",
        "container-type" => "**container-type** `<value>` \u{2014} Container type (`inline-size`, `size`, `normal`).",
        // Form attributes
        "type" => "**type** `<value>` \u{2014} Input type (`text`, `email`, `password`, `submit`, etc.).",
        "placeholder" => "**placeholder** `<value>` \u{2014} Placeholder text for inputs.",
        "name" => "**name** `<value>` \u{2014} Form field name.",
        "value" => "**value** `<value>` \u{2014} Form field value.",
        "disabled" => "**disabled** \u{2014} Disable the element.",
        "required" => "**required** \u{2014} Mark field as required.",
        "checked" => "**checked** \u{2014} Checkbox/radio checked state.",
        "for" => "**for** `<id>` \u{2014} Label target (id of the associated input).",
        "rows" => "**rows** `<value>` \u{2014} Number of visible rows for textarea.",
        "cols" => "**cols** `<value>` \u{2014} Number of visible columns for textarea.",
        "maxlength" => "**maxlength** `<value>` \u{2014} Maximum input length.",
        // Accessibility
        "alt" => "**alt** `<value>` \u{2014} Alternative text for images.",
        "role" => "**role** `<value>` \u{2014} ARIA role (e.g., `navigation`, `banner`, `main`).",
        "tabindex" => "**tabindex** `<value>` \u{2014} Tab order. `0` = natural order, `-1` = skip.",
        "title" => "**title** `<value>` \u{2014} Tooltip text.",
        // New CSS attributes
        "aspect-ratio" => "**aspect-ratio** `<value>` \u{2014} CSS aspect ratio (e.g., `16/9`, `1`, `4/3`).",
        "outline" => "**outline** `<width> [color]` \u{2014} Outline (like border but doesn't affect layout).",
        "padding-inline" => "**padding-inline** `<value>` \u{2014} Horizontal padding (logical property, i18n-aware).",
        "padding-block" => "**padding-block** `<value>` \u{2014} Vertical padding (logical property, i18n-aware).",
        "margin-inline" => "**margin-inline** `<value>` \u{2014} Horizontal margin (logical property, i18n-aware).",
        "margin-block" => "**margin-block** `<value>` \u{2014} Vertical margin (logical property, i18n-aware).",
        "scroll-snap-type" => "**scroll-snap-type** `<value>` \u{2014} Scroll snap type (`x mandatory`, `y proximity`).",
        "scroll-snap-align" => "**scroll-snap-align** `<value>` \u{2014} Scroll snap alignment (`start`, `center`, `end`).",
        // Media/image attributes
        "loading" => "**loading** `<value>` \u{2014} Loading behavior for images (`lazy`, `eager`).",
        "decoding" => "**decoding** `<value>` \u{2014} Image decoding mode (`async`, `sync`, `auto`).",
        "controls" => "**controls** \u{2014} Show media controls (for @video, @audio).",
        "autoplay" => "**autoplay** \u{2014} Auto-play media.",
        "loop" => "**loop** \u{2014} Loop media playback.",
        "muted" => "**muted** \u{2014} Mute media.",
        "poster" => "**poster** `<url>` \u{2014} Poster image for video.",
        "preload" => "**preload** `<value>` \u{2014} Media preload hint (`auto`, `metadata`, `none`).",
        "ordered" => "**ordered** \u{2014} Use ordered list (`<ol>` instead of `<ul>`).",
        "src" => "**src** `<url>` \u{2014} Source URL for media elements.",
        // New CSS attributes
        "margin" => "**margin** `<value>` | `<y> <x>` | `<t> <h> <b>` | `<t> <r> <b> <l>`\n\nOuter margin. Supports CSS units. Accepts 1\u{2013}4 values.",
        "margin-x" => "**margin-x** `<value>` \u{2014} Horizontal margin (left + right).",
        "margin-y" => "**margin-y** `<value>` \u{2014} Vertical margin (top + bottom).",
        "filter" => "**filter** `<value>` \u{2014} CSS filter (e.g., `blur(5px)`, `brightness(1.2)`, `grayscale(1)`).",
        "object-fit" => "**object-fit** `<value>` \u{2014} How content fits its container (`cover`, `contain`, `fill`, `none`, `scale-down`).",
        "object-position" => "**object-position** `<value>` \u{2014} Position of content within container (e.g., `center`, `top left`).",
        "text-shadow" => "**text-shadow** `<value>` \u{2014} Text shadow. Raw CSS value (e.g., `1px 1px 2px rgba(0,0,0,0.3)`).",
        "text-overflow" => "**text-overflow** `<value>` \u{2014} Text overflow behavior (`ellipsis`, `clip`). Combine with `white-space nowrap` and `overflow hidden`.",
        "pointer-events" => "**pointer-events** `<value>` \u{2014} Pointer event behavior (`none`, `auto`).",
        "user-select" => "**user-select** `<value>` \u{2014} Text selection behavior (`none`, `text`, `all`, `auto`).",
        "justify-content" => "**justify-content** `<value>` \u{2014} Main axis alignment (`center`, `space-between`, `space-around`, `flex-start`, `flex-end`).",
        "align-items" => "**align-items** `<value>` \u{2014} Cross axis alignment (`center`, `flex-start`, `flex-end`, `stretch`, `baseline`).",
        "order" => "**order** `<value>` \u{2014} Flex/grid item order (integer).",
        "background-size" => "**background-size** `<value>` \u{2014} Background size (`cover`, `contain`, `auto`, or dimensions).",
        "background-position" => "**background-position** `<value>` \u{2014} Background position (`center`, `top`, `bottom left`, etc.).",
        "background-repeat" => "**background-repeat** `<value>` \u{2014} Background repeat (`no-repeat`, `repeat`, `repeat-x`, `repeat-y`).",
        "word-break" => "**word-break** `<value>` \u{2014} Word breaking behavior (`break-all`, `keep-all`, `normal`).",
        "overflow-wrap" => "**overflow-wrap** `<value>` \u{2014} Overflow wrapping (`break-word`, `anywhere`, `normal`).",
        // New element attributes
        "open" => "**open** \u{2014} Initially expand `@details` element.",
        "novalidate" => "**novalidate** \u{2014} Disable form validation.",
        "low" => "**low** `<value>` \u{2014} Low threshold for `@meter`.",
        "high" => "**high** `<value>` \u{2014} High threshold for `@meter`.",
        "optimum" => "**optimum** `<value>` \u{2014} Optimum value for `@meter`.",
        "colspan" => "**colspan** `<value>` \u{2014} Number of columns a cell spans.",
        "rowspan" => "**rowspan** `<value>` \u{2014} Number of rows a cell spans.",
        "scope" => "**scope** `<value>` \u{2014} Header scope (`col`, `row`, `colgroup`, `rowgroup`).",
        "inline" => "**inline** \u{2014} Inline SVG image content into the HTML output.",
        "hidden" => "**hidden** \u{2014} Hide element (`display: none`).",
        "overflow-x" => "**overflow-x** `<value>` \u{2014} Horizontal overflow (`hidden`, `scroll`, `auto`, `visible`).",
        "overflow-y" => "**overflow-y** `<value>` \u{2014} Vertical overflow (`hidden`, `scroll`, `auto`, `visible`).",
        "inset" => "**inset** `<value>` \u{2014} Shorthand for `top`, `right`, `bottom`, `left`. Maps to CSS `inset`.",
        "accent-color" => "**accent-color** `<color>` \u{2014} Accent color for form controls (checkboxes, radios, range).",
        "caret-color" => "**caret-color** `<color>` \u{2014} Color of the text input cursor.",
        "list-style" => "**list-style** `<value>` \u{2014} List style type (`disc`, `circle`, `square`, `decimal`, `none`).",
        "border-collapse" => "**border-collapse** `<value>` \u{2014} Table border model (`collapse`, `separate`).",
        "border-spacing" => "**border-spacing** `<value>` \u{2014} Spacing between table cell borders (when `border-collapse: separate`).",
        "text-decoration" => "**text-decoration** `<value>` \u{2014} Text decoration (`underline`, `overline`, `line-through`, `none`).",
        "text-decoration-color" => "**text-decoration-color** `<color>` \u{2014} Color of text decoration.",
        "text-decoration-thickness" => "**text-decoration-thickness** `<value>` \u{2014} Thickness of text decoration.",
        "text-decoration-style" => "**text-decoration-style** `<value>` \u{2014} Style of text decoration (`solid`, `dashed`, `dotted`, `wavy`, `double`).",
        "place-items" => "**place-items** `<value>` \u{2014} Shorthand for `align-items` and `justify-items`.",
        "place-self" => "**place-self** `<value>` \u{2014} Shorthand for `align-self` and `justify-self`.",
        "scroll-behavior" => "**scroll-behavior** `<value>` \u{2014} Scroll behavior (`smooth`, `auto`).",
        "resize" => "**resize** `<value>` \u{2014} Resize behavior (`none`, `both`, `horizontal`, `vertical`).",
        // New CSS attributes
        "clip-path" => "**clip-path** `<value>` \u{2014} Clip path (`circle()`, `polygon()`, `inset()`, `url()`).",
        "mix-blend-mode" => "**mix-blend-mode** `<value>` \u{2014} Blend mode (`multiply`, `screen`, `overlay`, `darken`, `lighten`).",
        "background-blend-mode" => "**background-blend-mode** `<value>` \u{2014} Background blend mode for layered backgrounds.",
        "writing-mode" => "**writing-mode** `<value>` \u{2014} Writing direction (`horizontal-tb`, `vertical-rl`, `vertical-lr`).",
        "column-count" => "**column-count** `<value>` \u{2014} Number of columns in multi-column layout.",
        "column-gap" => "**column-gap** `<value>` \u{2014} Gap between columns. Supports CSS units.",
        "text-indent" => "**text-indent** `<value>` \u{2014} Indentation of the first line of text.",
        "hyphens" => "**hyphens** `<value>` \u{2014} Hyphenation behavior (`none`, `manual`, `auto`).",
        "flex-grow" => "**flex-grow** `<value>` \u{2014} Flex grow factor (number). Controls how much an item grows.",
        "flex-shrink" => "**flex-shrink** `<value>` \u{2014} Flex shrink factor (number). Controls how much an item shrinks.",
        "flex-basis" => "**flex-basis** `<value>` \u{2014} Initial main size of a flex item (e.g., `200px`, `auto`, `0`).",
        "isolation" => "**isolation** `<value>` \u{2014} Creates a new stacking context (`isolate`, `auto`).",
        "place-content" => "**place-content** `<value>` \u{2014} Shorthand for `align-content` and `justify-content`.",
        "background-image" => "**background-image** `<value>` \u{2014} Background image (`url()` or gradient function).",
        _ => return None,
    };

    Some(if let Some(state) = state {
        format!("*({} state)* {}", state, doc)
    } else {
        doc.to_string()
    })
}

// ---------------------------------------------------------------------------
// Go to definition
// ---------------------------------------------------------------------------

fn definition_at(text: &str, position: Position, uri: &Url) -> Option<GotoDefinitionResponse> {
    let lines: Vec<&str> = text.lines().collect();
    let line = lines.get(position.line as usize)?;

    // Check for @include/@import file path navigation
    let trimmed = line.trim();
    if let Some(filename) = trimmed
        .strip_prefix("@include ")
        .or_else(|| trimmed.strip_prefix("@import "))
    {
        let filename = filename.trim();
        if !filename.is_empty() {
            let file_path = uri.to_file_path().ok()?;
            let dir = file_path.parent()?;
            let target = dir.join(filename);
            if target.exists() {
                let target_uri = Url::from_file_path(&target).ok()?;
                return Some(GotoDefinitionResponse::Scalar(Location {
                    uri: target_uri,
                    range: Range::new(Position::new(0, 0), Position::new(0, 0)),
                }));
            }
        }
    }

    let col = (position.character as usize).min(line.len());
    let word = word_at(line, col)?;

    // Find definition location
    let (def_line, def_col, def_len) = if word.starts_with('$') {
        let name = &word[1..];
        find_definition(text, name)?
    } else if word.starts_with('@') {
        let name = &word[1..];
        find_fn_definition(text, name)?
    } else {
        return None;
    };

    Some(GotoDefinitionResponse::Scalar(Location {
        uri: uri.clone(),
        range: Range::new(
            Position::new(def_line, def_col),
            Position::new(def_line, def_col + def_len),
        ),
    }))
}

/// Find @let, @define, or @fn parameter definition for a $name reference.
fn find_definition(text: &str, name: &str) -> Option<(u32, u32, u32)> {
    for (i, line) in text.lines().enumerate() {
        let trimmed = line.trim();
        let offset = (line.len() - trimmed.len()) as u32;

        if let Some(rest) = trimmed.strip_prefix("@let ") {
            if let Some((n, _)) = rest.trim().split_once(' ') {
                if n == name {
                    let col = offset + "@let ".len() as u32;
                    return Some((i as u32, col, n.len() as u32));
                }
            }
        }

        if let Some(rest) = trimmed.strip_prefix("@define ") {
            let rest = rest.trim();
            if let Some(bracket) = rest.find('[') {
                let n = rest[..bracket].trim();
                if n == name {
                    let col = offset + "@define ".len() as u32;
                    return Some((i as u32, col, n.len() as u32));
                }
            }
        }

        if let Some(rest) = trimmed.strip_prefix("@fn ") {
            let parts: Vec<&str> = rest.split_whitespace().collect();
            for param in &parts[1..] {
                let p = param.strip_prefix('$').unwrap_or(param);
                if p == name {
                    // Find the param position in the line
                    if let Some(pos) = line.find(param) {
                        return Some((i as u32, pos as u32, param.len() as u32));
                    }
                }
            }
        }
    }
    None
}

/// Find @fn definition for an @name function call.
fn find_fn_definition(text: &str, name: &str) -> Option<(u32, u32, u32)> {
    for (i, line) in text.lines().enumerate() {
        let trimmed = line.trim();
        let offset = (line.len() - trimmed.len()) as u32;

        if let Some(rest) = trimmed.strip_prefix("@fn ") {
            let parts: Vec<&str> = rest.split_whitespace().collect();
            if parts.first() == Some(&name) {
                let col = offset + "@fn ".len() as u32;
                return Some((i as u32, col, name.len() as u32));
            }
        }
    }
    None
}

// ---------------------------------------------------------------------------
// Rename
// ---------------------------------------------------------------------------

fn prepare_rename_at(text: &str, position: Position) -> Option<PrepareRenameResponse> {
    let lines: Vec<&str> = text.lines().collect();
    let line = lines.get(position.line as usize)?;
    let col = (position.character as usize).min(line.len());
    let word = word_at(line, col)?;

    // Only allow renaming $variables and @function calls/definitions
    if !word.starts_with('$') && !word.starts_with('@') {
        return None;
    }

    let name = &word[1..];

    // Check that the symbol actually has a definition
    if word.starts_with('$') {
        find_definition(text, name)?;
    } else {
        find_fn_definition(text, name)?;
    }

    // Find the range of the word in the line
    let bytes = line.as_bytes();
    let mut start = col;
    while start > 0 && is_word_byte(bytes[start - 1]) {
        start -= 1;
    }
    let mut end = col;
    while end < bytes.len() && is_word_byte(bytes[end]) {
        end += 1;
    }

    Some(PrepareRenameResponse::Range(Range::new(
        Position::new(position.line, start as u32),
        Position::new(position.line, end as u32),
    )))
}

fn rename_at(text: &str, position: Position, new_name: &str, uri: &Url) -> Option<WorkspaceEdit> {
    let lines: Vec<&str> = text.lines().collect();
    let line = lines.get(position.line as usize)?;
    let col = (position.character as usize).min(line.len());
    let word = word_at(line, col)?;

    let is_var = word.starts_with('$');
    let name = &word[1..]; // strip $ or @

    // Strip $ or @ from new_name if user included it
    let new_base = new_name
        .strip_prefix('$')
        .or_else(|| new_name.strip_prefix('@'))
        .unwrap_or(new_name);

    let mut edits = Vec::new();

    for (i, line) in text.lines().enumerate() {
        let line_num = i as u32;
        let trimmed = line.trim();

        if is_var {
            // Rename @let definition
            if let Some(rest) = trimmed.strip_prefix("@let ") {
                if let Some((n, _)) = rest.trim().split_once(' ') {
                    if n == name {
                        if let Some(pos) = line.find(n) {
                            edits.push(TextEdit {
                                range: Range::new(
                                    Position::new(line_num, pos as u32),
                                    Position::new(line_num, (pos + n.len()) as u32),
                                ),
                                new_text: new_base.to_string(),
                            });
                        }
                    }
                }
            }

            // Rename @define definition
            if let Some(rest) = trimmed.strip_prefix("@define ") {
                let rest_trimmed = rest.trim();
                if let Some(bracket) = rest_trimmed.find('[') {
                    let n = rest_trimmed[..bracket].trim();
                    if n == name {
                        if let Some(pos) = line.find(n) {
                            edits.push(TextEdit {
                                range: Range::new(
                                    Position::new(line_num, pos as u32),
                                    Position::new(line_num, (pos + n.len()) as u32),
                                ),
                                new_text: new_base.to_string(),
                            });
                            continue; // Don't also match $ references on this line
                        }
                    }
                }
            }

            // Rename @fn parameter definitions
            if let Some(rest) = trimmed.strip_prefix("@fn ") {
                let parts: Vec<&str> = rest.split_whitespace().collect();
                for param in &parts[1..] {
                    let p = param.strip_prefix('$').unwrap_or(param);
                    if p == name {
                        if let Some(pos) = line.find(param) {
                            let prefix = if param.starts_with('$') { "$" } else { "" };
                            edits.push(TextEdit {
                                range: Range::new(
                                    Position::new(line_num, pos as u32),
                                    Position::new(line_num, (pos + param.len()) as u32),
                                ),
                                new_text: format!("{}{}", prefix, new_base),
                            });
                        }
                    }
                }
            }

            // Rename all $name references
            let search = format!("${}", name);
            let replace = format!("${}", new_base);
            let mut offset = 0;
            while let Some(pos) = line[offset..].find(&search) {
                let abs_pos = offset + pos;
                // Check it's not part of a longer identifier
                let after = abs_pos + search.len();
                let is_end = after >= line.len()
                    || !line.as_bytes()[after].is_ascii_alphanumeric()
                        && line.as_bytes()[after] != b'-'
                        && line.as_bytes()[after] != b'_';
                if is_end {
                    edits.push(TextEdit {
                        range: Range::new(
                            Position::new(line_num, abs_pos as u32),
                            Position::new(line_num, (abs_pos + search.len()) as u32),
                        ),
                        new_text: replace.clone(),
                    });
                }
                offset = abs_pos + search.len();
            }
        } else {
            // Rename @fn definition
            if let Some(rest) = trimmed.strip_prefix("@fn ") {
                let parts: Vec<&str> = rest.split_whitespace().collect();
                if parts.first() == Some(&name) {
                    if let Some(pos) = line.find(&format!("@fn {}", name)) {
                        let start = pos + 4; // skip "@fn "
                        edits.push(TextEdit {
                            range: Range::new(
                                Position::new(line_num, start as u32),
                                Position::new(line_num, (start + name.len()) as u32),
                            ),
                            new_text: new_base.to_string(),
                        });
                    }
                }
            }

            // Rename @name function calls
            let search = format!("@{}", name);
            let replace = format!("@{}", new_base);
            let mut offset = 0;
            while let Some(pos) = line[offset..].find(&search) {
                let abs_pos = offset + pos;
                // Don't match @fn definition (handled above)
                if trimmed.starts_with("@fn ") {
                    offset = abs_pos + search.len();
                    continue;
                }
                // Check it's not part of a longer identifier
                let after = abs_pos + search.len();
                let is_end = after >= line.len()
                    || !line.as_bytes()[after].is_ascii_alphanumeric()
                        && line.as_bytes()[after] != b'-'
                        && line.as_bytes()[after] != b'_';
                if is_end {
                    edits.push(TextEdit {
                        range: Range::new(
                            Position::new(line_num, abs_pos as u32),
                            Position::new(line_num, (abs_pos + search.len()) as u32),
                        ),
                        new_text: replace.clone(),
                    });
                }
                offset = abs_pos + search.len();
            }
        }
    }

    if edits.is_empty() {
        return None;
    }

    let mut changes = HashMap::new();
    changes.insert(uri.clone(), edits);

    Some(WorkspaceEdit {
        changes: Some(changes),
        ..Default::default()
    })
}

// ---------------------------------------------------------------------------
// Document symbols (outline view)
// ---------------------------------------------------------------------------

#[allow(deprecated)] // SymbolInformation::deprecated is deprecated but needed for the struct
fn document_symbols(text: &str) -> Vec<SymbolInformation> {
    let mut symbols = Vec::new();

    for (i, line) in text.lines().enumerate() {
        let trimmed = line.trim();
        let line_num = i as u32;

        // @fn definitions
        if let Some(rest) = trimmed.strip_prefix("@fn ") {
            let parts: Vec<&str> = rest.split_whitespace().collect();
            if let Some(name) = parts.first() {
                let params = parts[1..].join(" ");
                let detail = if params.is_empty() { None } else { Some(format!("({})", params)) };
                symbols.push(SymbolInformation {
                    name: format!("@{}", name),
                    kind: SymbolKind::FUNCTION,
                    tags: None,
                    deprecated: None,
                    location: Location {
                        uri: Url::parse("file:///").unwrap(), // replaced by caller
                        range: Range::new(Position::new(line_num, 0), Position::new(line_num, line.len() as u32)),
                    },
                    container_name: detail,
                });
            }
        }

        // @let definitions
        if let Some(rest) = trimmed.strip_prefix("@let ") {
            if let Some((name, value)) = rest.trim().split_once(' ') {
                symbols.push(SymbolInformation {
                    name: format!("${}", name),
                    kind: SymbolKind::VARIABLE,
                    tags: None,
                    deprecated: None,
                    location: Location {
                        uri: Url::parse("file:///").unwrap(),
                        range: Range::new(Position::new(line_num, 0), Position::new(line_num, line.len() as u32)),
                    },
                    container_name: Some(format!("= {}", value.trim())),
                });
            }
        }

        // @define definitions
        if let Some(rest) = trimmed.strip_prefix("@define ") {
            if let Some(bracket) = rest.find('[') {
                let name = rest[..bracket].trim();
                symbols.push(SymbolInformation {
                    name: format!("${}", name),
                    kind: SymbolKind::CONSTANT,
                    tags: None,
                    deprecated: None,
                    location: Location {
                        uri: Url::parse("file:///").unwrap(),
                        range: Range::new(Position::new(line_num, 0), Position::new(line_num, line.len() as u32)),
                    },
                    container_name: Some("attribute bundle".to_string()),
                });
            }
        }

        // @keyframes definitions
        if let Some(rest) = trimmed.strip_prefix("@keyframes ") {
            let name = rest.trim();
            if !name.is_empty() {
                symbols.push(SymbolInformation {
                    name: format!("@keyframes {}", name),
                    kind: SymbolKind::EVENT,
                    tags: None,
                    deprecated: None,
                    location: Location {
                        uri: Url::parse("file:///").unwrap(),
                        range: Range::new(Position::new(line_num, 0), Position::new(line_num, line.len() as u32)),
                    },
                    container_name: Some("animation".to_string()),
                });
            }
        }
    }

    symbols
}

// ---------------------------------------------------------------------------
// Code actions (quick-fixes for typo suggestions)
// ---------------------------------------------------------------------------

fn code_actions(
    text: &str,
    _range: &Range,
    diagnostics: &[Diagnostic],
    uri: &Url,
) -> Vec<CodeActionOrCommand> {
    let mut actions = Vec::new();

    for diag in diagnostics {
        let msg = &diag.message;

        // Extract "did you mean 'X'?" or "did you mean @X?" suggestions
        if let Some(suggestion) = extract_suggestion(msg) {
            let line = diag.range.start.line as usize;
            let lines: Vec<&str> = text.lines().collect();
            if let Some(source_line) = lines.get(line) {
                // Determine what to replace
                let (old_text, new_text) = if msg.contains("unknown element") {
                    // Replace @wrong with @suggestion
                    let old = extract_between(msg, "unknown element @", ",")
                        .or_else(|| extract_between(msg, "unknown element @", ""));
                    if let Some(old) = old {
                        (format!("@{}", old), format!("@{}", suggestion))
                    } else {
                        continue;
                    }
                } else if msg.contains("unknown attribute") {
                    // Replace wrong with suggestion in attribute list
                    let old = extract_between(msg, "unknown attribute '", "'");
                    if let Some(old) = old {
                        (old.to_string(), suggestion.to_string())
                    } else {
                        continue;
                    }
                } else {
                    continue;
                };

                if let Some(col) = source_line.find(&old_text) {
                    let edit = TextEdit {
                        range: Range::new(
                            Position::new(diag.range.start.line, col as u32),
                            Position::new(diag.range.start.line, (col + old_text.len()) as u32),
                        ),
                        new_text: new_text.clone(),
                    };

                    let mut changes = HashMap::new();
                    changes.insert(uri.clone(), vec![edit]);

                    actions.push(CodeActionOrCommand::CodeAction(CodeAction {
                        title: format!("Replace with '{}'", new_text),
                        kind: Some(CodeActionKind::QUICKFIX),
                        diagnostics: Some(vec![diag.clone()]),
                        edit: Some(WorkspaceEdit {
                            changes: Some(changes),
                            ..Default::default()
                        }),
                        ..Default::default()
                    }));
                }
            }
        }
    }

    actions
}

fn extract_suggestion(msg: &str) -> Option<&str> {
    // "did you mean @X?" or "did you mean 'X'?"
    if let Some(idx) = msg.find("did you mean @") {
        let start = idx + "did you mean @".len();
        let rest = &msg[start..];
        let end = rest.find('?').unwrap_or(rest.len());
        return Some(&rest[..end]);
    }
    if let Some(idx) = msg.find("did you mean '") {
        let start = idx + "did you mean '".len();
        let rest = &msg[start..];
        let end = rest.find('\'')?;
        return Some(&rest[..end]);
    }
    None
}

fn extract_between<'a>(msg: &'a str, prefix: &str, suffix: &str) -> Option<&'a str> {
    let start = msg.find(prefix)? + prefix.len();
    let rest = &msg[start..];
    if suffix.is_empty() {
        Some(rest.trim())
    } else {
        let end = rest.find(suffix)?;
        Some(&rest[..end])
    }
}

// ---------------------------------------------------------------------------
// Color provider
// ---------------------------------------------------------------------------

fn find_colors(text: &str) -> Vec<ColorInformation> {
    let mut colors = Vec::new();
    for (line_idx, line) in text.lines().enumerate() {
        let mut start = 0;
        while let Some(pos) = line[start..].find('#') {
            let abs_pos = start + pos;
            let hex_start = abs_pos + 1;
            let hex_end = line[hex_start..]
                .find(|c: char| !c.is_ascii_hexdigit())
                .map(|p| hex_start + p)
                .unwrap_or(line.len());
            let hex = &line[hex_start..hex_end];
            let len = hex.len();
            if len == 3 || len == 6 || len == 8 {
                if let Some((r, g, b, a)) = parse_hex_color(hex) {
                    colors.push(ColorInformation {
                        range: Range::new(
                            Position::new(line_idx as u32, abs_pos as u32),
                            Position::new(line_idx as u32, hex_end as u32),
                        ),
                        color: Color {
                            red: r as f32 / 255.0,
                            green: g as f32 / 255.0,
                            blue: b as f32 / 255.0,
                            alpha: a as f32 / 255.0,
                        },
                    });
                }
            }
            start = hex_end;
        }
    }
    colors
}

fn parse_hex_color(hex: &str) -> Option<(u8, u8, u8, u8)> {
    match hex.len() {
        3 => {
            let r = u8::from_str_radix(&hex[0..1], 16).ok()? * 17;
            let g = u8::from_str_radix(&hex[1..2], 16).ok()? * 17;
            let b = u8::from_str_radix(&hex[2..3], 16).ok()? * 17;
            Some((r, g, b, 255))
        }
        6 => {
            let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
            let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
            let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
            Some((r, g, b, 255))
        }
        8 => {
            let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
            let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
            let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
            let a = u8::from_str_radix(&hex[6..8], 16).ok()?;
            Some((r, g, b, a))
        }
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// Folding ranges
// ---------------------------------------------------------------------------

fn folding_ranges(text: &str) -> Vec<FoldingRange> {
    let mut ranges = Vec::new();
    let lines: Vec<&str> = text.lines().collect();
    let mut i = 0;
    while i < lines.len() {
        let trimmed = lines[i].trim();
        // Fold blocks that start with @fn, @if, @else, @each, @match, @style, @head, @keyframes, @define
        if trimmed.starts_with("@fn ")
            || trimmed.starts_with("@if ")
            || trimmed == "@else"
            || trimmed.starts_with("@else if ")
            || trimmed.starts_with("@each ")
            || trimmed.starts_with("@match ")
            || trimmed == "@style"
            || trimmed == "@head"
            || trimmed.starts_with("@keyframes ")
        {
            let start_indent = lines[i].len() - lines[i].trim_start().len();
            let start_line = i;
            let mut end_line = i;
            let mut j = i + 1;
            while j < lines.len() {
                let l = lines[j];
                if l.trim().is_empty() {
                    j += 1;
                    continue;
                }
                let indent = l.len() - l.trim_start().len();
                if indent <= start_indent {
                    break;
                }
                end_line = j;
                j += 1;
            }
            if end_line > start_line {
                ranges.push(FoldingRange {
                    start_line: start_line as u32,
                    start_character: None,
                    end_line: end_line as u32,
                    end_character: None,
                    kind: Some(FoldingRangeKind::Region),
                    collapsed_text: None,
                });
            }
        }
        // Fold comment blocks (lines starting with --)
        if trimmed.starts_with("--") {
            let start_line = i;
            let mut end_line = i;
            let mut j = i + 1;
            while j < lines.len() && lines[j].trim().starts_with("--") {
                end_line = j;
                j += 1;
            }
            if end_line > start_line {
                ranges.push(FoldingRange {
                    start_line: start_line as u32,
                    start_character: None,
                    end_line: end_line as u32,
                    end_character: None,
                    kind: Some(FoldingRangeKind::Comment),
                    collapsed_text: None,
                });
            }
        }
        i += 1;
    }
    ranges
}

// ---------------------------------------------------------------------------
// Semantic tokens
// ---------------------------------------------------------------------------

fn semantic_tokens(text: &str) -> Vec<SemanticToken> {
    let mut tokens = Vec::new();
    let mut prev_line: u32 = 0;
    let mut prev_start: u32 = 0;

    for (line_idx, line) in text.lines().enumerate() {
        let trimmed = line.trim();
        let line_num = line_idx as u32;

        // Detect comments
        if trimmed.starts_with("--") {
            let col = (line.len() - trimmed.len()) as u32;
            push_token(&mut tokens, &mut prev_line, &mut prev_start, line_num, col, trimmed.len() as u32, 4);
            continue;
        }

        // Scan for @keywords
        let bytes = line.as_bytes();
        let mut i = 0;
        while i < bytes.len() {
            if bytes[i] == b'@' {
                let start = i;
                i += 1;
                while i < bytes.len() && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'-' || bytes[i] == b'_') {
                    i += 1;
                }
                let word = &line[start..i];
                let token_type = match word {
                    "@page" | "@let" | "@define" | "@fn" | "@if" | "@else" | "@each"
                    | "@include" | "@import" | "@meta" | "@head" | "@style" | "@keyframes"
                    | "@match" | "@case" | "@default" | "@slot" | "@children"
                    | "@warn" | "@debug" | "@lang" | "@favicon" | "@fragment"
                    | "@unless" | "@og" | "@breakpoint" => 0, // keyword
                    _ => {
                        // Check if it's a user function call (starts with @ but not a builtin element)
                        if is_builtin_element(word) { 0 } else { 2 } // function
                    }
                };
                push_token(&mut tokens, &mut prev_line, &mut prev_start, line_num, start as u32, (i - start) as u32, token_type);
            } else if bytes[i] == b'$' {
                let start = i;
                i += 1;
                while i < bytes.len() && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'-' || bytes[i] == b'_') {
                    i += 1;
                }
                if i > start + 1 {
                    push_token(&mut tokens, &mut prev_line, &mut prev_start, line_num, start as u32, (i - start) as u32, 1); // variable
                }
            } else {
                i += 1;
            }
        }
    }
    tokens
}

fn is_builtin_element(word: &str) -> bool {
    matches!(word,
        "@row" | "@column" | "@col" | "@el" | "@text" | "@paragraph" | "@p"
        | "@image" | "@img" | "@link" | "@input" | "@button" | "@btn"
        | "@select" | "@textarea" | "@option" | "@opt" | "@label" | "@raw"
        | "@nav" | "@header" | "@footer" | "@main" | "@section" | "@article" | "@aside"
        | "@list" | "@item" | "@li" | "@table" | "@thead" | "@tbody" | "@tr" | "@td" | "@th"
        | "@video" | "@audio" | "@form" | "@details" | "@summary"
        | "@blockquote" | "@cite" | "@code" | "@pre" | "@hr" | "@divider"
        | "@figure" | "@figcaption" | "@progress" | "@meter" | "@fragment"
        | "@dialog" | "@dl" | "@dt" | "@dd" | "@fieldset" | "@legend"
        | "@picture" | "@source" | "@time" | "@mark" | "@kbd" | "@abbr" | "@datalist"
    )
}

fn push_token(
    tokens: &mut Vec<SemanticToken>,
    prev_line: &mut u32,
    prev_start: &mut u32,
    line: u32,
    start: u32,
    length: u32,
    token_type: u32,
) {
    let delta_line = line - *prev_line;
    let delta_start = if delta_line == 0 {
        start - *prev_start
    } else {
        start
    };
    tokens.push(SemanticToken {
        delta_line,
        delta_start,
        length,
        token_type,
        token_modifiers_bitset: 0,
    });
    *prev_line = line;
    *prev_start = start;
}

// ---------------------------------------------------------------------------
// Inlay hints
// ---------------------------------------------------------------------------

fn inlay_hints(text: &str) -> Vec<InlayHint> {
    // Build variable map: name -> value
    let mut vars: HashMap<&str, &str> = HashMap::new();
    for line in text.lines() {
        let trimmed = line.trim();
        if let Some(rest) = trimmed.strip_prefix("@let ") {
            if let Some((name, value)) = rest.trim().split_once(' ') {
                vars.insert(name, value.trim());
            }
        }
    }

    let mut hints = Vec::new();
    for (line_idx, line) in text.lines().enumerate() {
        let trimmed = line.trim();
        // Skip @let definition lines — the value is already visible there
        if trimmed.starts_with("@let ") {
            continue;
        }
        let bytes = line.as_bytes();
        let mut i = 0;
        while i < bytes.len() {
            if bytes[i] == b'$' {
                let start = i;
                i += 1;
                while i < bytes.len()
                    && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'-' || bytes[i] == b'_')
                {
                    i += 1;
                }
                if i > start + 1 {
                    let var_name = &line[start + 1..i];
                    if let Some(value) = vars.get(var_name) {
                        hints.push(InlayHint {
                            position: Position::new(line_idx as u32, i as u32),
                            label: InlayHintLabel::String(format!(" \u{2192} {}", value)),
                            kind: None,
                            text_edits: None,
                            tooltip: None,
                            padding_left: Some(false),
                            padding_right: Some(true),
                            data: None,
                        });
                    }
                }
            } else {
                i += 1;
            }
        }
    }
    hints
}

// ---------------------------------------------------------------------------
// Linked editing ranges
// ---------------------------------------------------------------------------

fn linked_editing_ranges(text: &str, position: Position) -> Option<LinkedEditingRanges> {
    let lines: Vec<&str> = text.lines().collect();
    let line = lines.get(position.line as usize)?;
    let col = (position.character as usize).min(line.len());

    // Find the $variable at the cursor
    let bytes = line.as_bytes();
    let mut start = col;
    while start > 0 && (bytes[start - 1].is_ascii_alphanumeric() || bytes[start - 1] == b'$' || bytes[start - 1] == b'-' || bytes[start - 1] == b'_') {
        start -= 1;
    }
    let mut end = col;
    while end < bytes.len() && (bytes[end].is_ascii_alphanumeric() || bytes[end] == b'$' || bytes[end] == b'-' || bytes[end] == b'_') {
        end += 1;
    }
    if start == end {
        return None;
    }

    let word = &line[start..end];
    if !word.starts_with('$') {
        return None;
    }

    // Find all occurrences of this $variable in the document
    let mut ranges = Vec::new();
    for (line_idx, line) in text.lines().enumerate() {
        let line_bytes = line.as_bytes();
        let mut offset = 0;
        while let Some(pos) = line[offset..].find(word) {
            let abs_pos = offset + pos;
            // Check it's a whole word match
            let before_ok = abs_pos == 0 || {
                let c = line_bytes[abs_pos - 1];
                !c.is_ascii_alphanumeric() && c != b'-' && c != b'_'
            };
            let after_end = abs_pos + word.len();
            let after_ok = after_end >= line.len() || {
                let c = line_bytes[after_end];
                !c.is_ascii_alphanumeric() && c != b'-' && c != b'_'
            };
            if before_ok && after_ok {
                ranges.push(Range::new(
                    Position::new(line_idx as u32, abs_pos as u32),
                    Position::new(line_idx as u32, after_end as u32),
                ));
            }
            offset = abs_pos + word.len();
        }
    }

    if ranges.len() < 2 {
        return None;
    }

    Some(LinkedEditingRanges {
        ranges,
        word_pattern: None,
    })
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
