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
        ("@keyframes", "Define a CSS animation", "@keyframes "),
        ("@if", "Conditional rendering", "@if "),
        ("@each", "Loop over comma-separated values", "@each "),
        ("@include", "Include another .hl file", "@include "),
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
        ("padding", "Inner padding (1/2/3/4 values, px)", true),
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
        // Typography
        ("text-align", "Text alignment (left/center/right/justify)", true),
        ("line-height", "Line height (unitless or px)", true),
        // Overflow & positioning
        ("overflow", "Overflow behavior (hidden/scroll/auto)", true),
        ("position", "Position type (relative/absolute/fixed/sticky)", true),
        ("z-index", "Stack order (integer)", true),
        // Effects
        ("shadow", "Box shadow (CSS value)", true),
        // Flow
        ("wrap", "Enable flex-wrap", false),
        ("gap-x", "Horizontal gap between children (px)", true),
        ("gap-y", "Vertical gap between children (px)", true),
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
        // State prefixes
        ("hover:", "Style on hover", false),
        ("active:", "Style on active/click", false),
        ("focus:", "Style on focus", false),
        // Responsive prefixes
        ("sm:", "Style at 640px+ (small)", false),
        ("md:", "Style at 768px+ (medium)", false),
        ("lg:", "Style at 1024px+ (large)", false),
        ("xl:", "Style at 1280px+ (extra large)", false),
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
        ("shadow", "Box shadow (CSS value)", true),
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
        "@keyframes" => "**@keyframes** \u{2014} CSS Animation\n\nDefines keyframes for CSS animations.\n\n```\n@keyframes fade-in\n  from{opacity:0}to{opacity:1}\n```\n\nUse with `animation` attribute: `[animation fade-in 0.3s ease]`",
        "@children" => "**@children** \u{2014} Children slot\n\nPlaceholder inside `@fn` body replaced with the caller's children.",
        "@input" => "**@input** \u{2014} Form input\n\nRenders as self-closing `<input>`.\n\nUsage: `@input [type text, placeholder Name, name user]`",
        "@button" | "@btn" => "**@button** \u{2014} Button\n\nRenders as `<button>`.\n\nUsage: `@button [type submit] Click me`",
        "@select" => "**@select** \u{2014} Select dropdown\n\nRenders as `<select>`. Use `@option` children.\n\nUsage: `@select [name color]`",
        "@textarea" => "**@textarea** \u{2014} Multi-line text input\n\nRenders as `<textarea>`.\n\nUsage: `@textarea [name bio, rows 4] Default text`",
        "@option" | "@opt" => "**@option** \u{2014} Select option\n\nRenders as `<option>`.\n\nUsage: `@option [value red] Red`",
        "@label" => "**@label** \u{2014} Form label\n\nRenders as `<label>`.\n\nUsage: `@label [for email] Email Address`",
        "@if" => "**@if** \u{2014} Conditional\n\nConditionally includes children at compile time.\n\n```\n@if $theme == dark\n  @el [background #333]\n@else\n  @el [background white]\n```",
        "@each" => "**@each** \u{2014} Loop\n\nRepeat children for each item in a comma-separated list.\n\n```\n@each $color in red,green,blue\n  @el [background $color]\n```",
        "@include" => "**@include** \u{2014} Include file\n\nIncludes another .hl file.\n\nUsage: `@include header.hl`",
        // Attributes
        "spacing" => "**spacing** `<value>`\n\nGap between children in pixels. Maps to CSS `gap`.",
        "padding" => "**padding** `<value>` | `<y> <x>` | `<t> <h> <b>` | `<t> <r> <b> <l>`\n\nInner padding in pixels. Accepts 1, 2, 3, or 4 values.",
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
        "text-align" => "**text-align** `<value>` \u{2014} Text alignment (`left`, `center`, `right`, `justify`).",
        "line-height" => "**line-height** `<value>` \u{2014} Line height. Unitless (e.g., `1.5`) or pixels.",
        "overflow" => "**overflow** `<value>` \u{2014} Overflow behavior (`hidden`, `scroll`, `auto`, `visible`).",
        "position" => "**position** `<value>` \u{2014} Position type (`relative`, `absolute`, `fixed`, `sticky`).",
        "z-index" => "**z-index** `<value>` \u{2014} Stack order (integer).",
        "shadow" => "**shadow** `<value>` \u{2014} Box shadow. Raw CSS value (e.g., `0 2px 4px rgba(0,0,0,0.1)`).",
        "gap-x" => "**gap-x** `<value>` \u{2014} Horizontal gap between children in pixels. Maps to `column-gap`.",
        "gap-y" => "**gap-y** `<value>` \u{2014} Vertical gap between children in pixels. Maps to `row-gap`.",
        "wrap" => "**wrap** \u{2014} Enable flex-wrap for children.",
        "id" => "**id** `<value>` \u{2014} HTML id attribute.",
        "class" => "**class** `<value>` \u{2014} HTML class attribute.",
        "animation" => "**animation** `<value>` \u{2014} CSS animation shorthand (e.g., `fade-in 0.3s ease`).\n\nDefine animations with `@keyframes`.",
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
