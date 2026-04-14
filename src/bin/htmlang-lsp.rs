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
        let diags = match htmlang::parser::parse(&text) {
            Ok(_) => vec![],
            Err(e) => {
                let line = e.line.saturating_sub(1) as u32;
                vec![Diagnostic {
                    range: Range::new(Position::new(line, 0), Position::new(line, 1000)),
                    severity: Some(DiagnosticSeverity::ERROR),
                    source: Some("htmlang".into()),
                    message: e.message,
                    ..Default::default()
                }]
            }
        };
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

        // State prefix (hover:, active:, focus:)
        if let Some(colon) = current_word.find(':') {
            let prefix = &current_word[..colon];
            if matches!(prefix, "hover" | "active" | "focus") {
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
        ("@raw", "Raw HTML escape hatch"),
        ("@children", "Slot for caller's children (inside @fn)"),
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
        ("@fn", "Define a reusable function", "@fn "),
    ]
    .iter()
    .map(|(name, detail, insert)| {
        item(name, CompletionItemKind::SNIPPET, detail, insert, range)
    })
    .collect()
}

fn attr_completions(range: Range) -> Vec<CompletionItem> {
    [
        // Layout
        ("spacing", "Gap between children (px)", true),
        ("padding", "Inner padding (1/2/4 values, px)", true),
        ("padding-x", "Horizontal padding (px)", true),
        ("padding-y", "Vertical padding (px)", true),
        // Sizing
        ("width", "Width (px | fill | shrink)", true),
        ("height", "Height (px | fill | shrink)", true),
        ("min-width", "Minimum width (px)", true),
        ("max-width", "Maximum width (px)", true),
        ("min-height", "Minimum height (px)", true),
        ("max-height", "Maximum height (px)", true),
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
        ("rounded", "Border radius (px)", true),
        ("bold", "Bold text", false),
        ("italic", "Italic text", false),
        ("underline", "Underlined text", false),
        ("size", "Font size (px)", true),
        ("font", "Font family", true),
        ("transition", "CSS transition", true),
        ("cursor", "CSS cursor type", true),
        ("opacity", "Opacity (0-1)", true),
        // Flow
        ("wrap", "Enable flex-wrap", false),
        // Identity
        ("id", "HTML id attribute", true),
        ("class", "HTML class attribute", true),
        // State prefixes
        ("hover:", "Style on hover", false),
        ("active:", "Style on active/click", false),
        ("focus:", "Style on focus", false),
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
        ("rounded", "Border radius (px)", true),
        ("bold", "Bold text", false),
        ("italic", "Italic text", false),
        ("underline", "Underlined text", false),
        ("size", "Font size (px)", true),
        ("opacity", "Opacity (0-1)", true),
        ("cursor", "CSS cursor type", true),
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
        "@children" => "**@children** \u{2014} Children slot\n\nPlaceholder inside `@fn` body replaced with the caller's children.",
        // Attributes
        "spacing" => "**spacing** `<value>`\n\nGap between children in pixels. Maps to CSS `gap`.",
        "padding" => "**padding** `<value>` | `<y> <x>` | `<t> <r> <b> <l>`\n\nInner padding in pixels. Accepts 1, 2, or 4 values.",
        "padding-x" => "**padding-x** `<value>`\n\nHorizontal padding (left + right) in pixels.",
        "padding-y" => "**padding-y** `<value>`\n\nVertical padding (top + bottom) in pixels.",
        "width" => "**width** `<px>` | `fill` | `shrink`\n\n- Number: fixed width in pixels\n- `fill`: expand to fill parent\n- `shrink`: prevent flex shrinking",
        "height" => "**height** `<px>` | `fill` | `shrink`\n\n- Number: fixed height in pixels\n- `fill`: expand to fill parent\n- `shrink`: prevent flex shrinking",
        "min-width" => "**min-width** `<value>` \u{2014} Minimum width in pixels.",
        "max-width" => "**max-width** `<value>` \u{2014} Maximum width in pixels.",
        "min-height" => "**min-height** `<value>` \u{2014} Minimum height in pixels.",
        "max-height" => "**max-height** `<value>` \u{2014} Maximum height in pixels.",
        "center-x" => "**center-x**\n\nCenter horizontally.\n\nIn column parent: `align-self: center`\nOtherwise: auto margins.",
        "center-y" => "**center-y**\n\nCenter vertically.\n\nIn row parent: `align-self: center`\nOtherwise: auto margins.",
        "align-left" => "**align-left** \u{2014} Align to the left edge.",
        "align-right" => "**align-right** \u{2014} Align to the right edge.",
        "align-top" => "**align-top** \u{2014} Align to the top edge.",
        "align-bottom" => "**align-bottom** \u{2014} Align to the bottom edge.",
        "background" => "**background** `<color>` \u{2014} Background color or CSS background value.",
        "color" => "**color** `<color>` \u{2014} Text color.",
        "border" => "**border** `<width> [color]`\n\nBorder. Width in pixels, color defaults to `currentColor`.",
        "rounded" => "**rounded** `<value>` \u{2014} Border radius in pixels.",
        "bold" => "**bold** \u{2014} Bold text (`font-weight: bold`).",
        "italic" => "**italic** \u{2014} Italic text (`font-style: italic`).",
        "underline" => "**underline** \u{2014} Underlined text.",
        "size" => "**size** `<value>` \u{2014} Font size in pixels.",
        "font" => "**font** `<family>` \u{2014} Font family.",
        "transition" => "**transition** `<value>` \u{2014} CSS transition (e.g., `all 0.15s ease`).",
        "cursor" => "**cursor** `<value>` \u{2014} CSS cursor (e.g., `pointer`).",
        "opacity" => "**opacity** `<value>` \u{2014} Opacity from 0 to 1.",
        "wrap" => "**wrap** \u{2014} Enable flex-wrap for children.",
        "id" => "**id** `<value>` \u{2014} HTML id attribute.",
        "class" => "**class** `<value>` \u{2014} HTML class attribute.",
        _ => return None,
    };

    Some(if let Some(state) = state {
        format!("*({} state)* {}", state, doc)
    } else {
        doc.to_string()
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
