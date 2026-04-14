use std::collections::HashMap;
use std::fmt;

use crate::ast::*;

#[derive(Debug)]
pub struct ParseError {
    pub line: usize,
    pub message: String,
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "line {}: {}", self.line, self.message)
    }
}

#[derive(Clone)]
enum LineContent {
    Normal(String),
    Raw(String),
}

#[derive(Clone)]
struct Line {
    indent: usize,
    content: LineContent,
    line_num: usize,
}

#[derive(Clone)]
struct FnDef {
    params: Vec<String>,
    body_lines: Vec<Line>,
}

struct ParseContext {
    page_title: Option<String>,
    variables: HashMap<String, String>,
    defines: HashMap<String, Vec<Attribute>>,
    functions: HashMap<String, FnDef>,
}

struct Parser {
    lines: Vec<Line>,
    pos: usize,
}

pub fn parse(input: &str) -> Result<Document, ParseError> {
    let lines = preprocess(input);
    let mut parser = Parser { lines, pos: 0 };
    let mut ctx = ParseContext {
        page_title: None,
        variables: HashMap::new(),
        defines: HashMap::new(),
        functions: HashMap::new(),
    };
    let nodes = parser.parse_children(0, &mut ctx)?;
    Ok(Document {
        page_title: ctx.page_title,
        variables: ctx.variables,
        defines: ctx.defines,
        nodes,
    })
}

// ---------------------------------------------------------------------------
// Preprocessing: strip comments/blanks, collapse @raw blocks
// ---------------------------------------------------------------------------

fn preprocess(input: &str) -> Vec<Line> {
    let raw_lines: Vec<&str> = input.lines().collect();
    let mut lines = Vec::new();
    let mut i = 0;

    while i < raw_lines.len() {
        let line = raw_lines[i];
        let trimmed = line.trim();

        if trimmed.is_empty() || trimmed.starts_with("--") {
            i += 1;
            continue;
        }

        let indent = line.len() - line.trim_start().len();

        // Handle @raw """..."""
        if trimmed.starts_with("@raw") {
            let after_raw = trimmed[4..].trim_start();
            if after_raw.starts_with("\"\"\"") {
                let after_open = &after_raw[3..];

                // Single-line: @raw """content"""
                if after_open.ends_with("\"\"\"") && after_open.len() >= 3 {
                    let content = &after_open[..after_open.len() - 3];
                    lines.push(Line {
                        indent,
                        content: LineContent::Raw(content.to_string()),
                        line_num: i + 1,
                    });
                    i += 1;
                    continue;
                }

                // Multiline: collect until closing """
                let mut raw_content = String::new();
                if !after_open.is_empty() {
                    raw_content.push_str(after_open);
                    raw_content.push('\n');
                }
                i += 1;
                while i < raw_lines.len() {
                    if raw_lines[i].trim() == "\"\"\"" {
                        i += 1;
                        break;
                    }
                    raw_content.push_str(raw_lines[i]);
                    raw_content.push('\n');
                    i += 1;
                }

                lines.push(Line {
                    indent,
                    content: LineContent::Raw(
                        raw_content.trim_end_matches('\n').to_string(),
                    ),
                    line_num: i,
                });
                continue;
            }
        }

        lines.push(Line {
            indent,
            content: LineContent::Normal(trimmed.to_string()),
            line_num: i + 1,
        });
        i += 1;
    }

    lines
}

// ---------------------------------------------------------------------------
// Parser
// ---------------------------------------------------------------------------

impl Parser {
    fn parse_children(
        &mut self,
        min_indent: usize,
        ctx: &mut ParseContext,
    ) -> Result<Vec<Node>, ParseError> {
        let mut nodes = Vec::new();

        while self.pos < self.lines.len() {
            let indent = self.lines[self.pos].indent;
            if indent < min_indent {
                break;
            }

            match self.parse_line(ctx)? {
                Some(new_nodes) => nodes.extend(new_nodes),
                None => {}
            }
        }

        Ok(nodes)
    }

    fn parse_line(
        &mut self,
        ctx: &mut ParseContext,
    ) -> Result<Option<Vec<Node>>, ParseError> {
        let line_num = self.lines[self.pos].line_num;
        let current_indent = self.lines[self.pos].indent;

        // Handle raw content
        if let LineContent::Raw(s) = &self.lines[self.pos].content {
            let content = s.clone();
            self.pos += 1;
            return Ok(Some(vec![Node::Raw(content)]));
        }

        // Normal content — clone to release borrow
        let content = match &self.lines[self.pos].content {
            LineContent::Normal(s) => s.clone(),
            _ => unreachable!(),
        };
        self.pos += 1;

        // --- Directives ---

        if let Some(rest) = content.strip_prefix("@page ") {
            ctx.page_title = Some(substitute_vars(rest, &ctx.variables));
            return Ok(None);
        }

        if let Some(rest) = content.strip_prefix("@let ") {
            let rest = rest.trim();
            if let Some((name, value)) = rest.split_once(' ') {
                ctx.variables
                    .insert(name.to_string(), substitute_vars(value.trim(), &ctx.variables));
            }
            return Ok(None);
        }

        if let Some(rest) = content.strip_prefix("@define ") {
            let rest = rest.trim();
            if let Some(bracket_start) = rest.find('[') {
                let name = rest[..bracket_start].trim();
                let attrs_str = &rest[bracket_start..];
                let (attrs, _) = parse_attr_brackets(attrs_str, line_num, ctx)?;
                ctx.defines.insert(name.to_string(), attrs);
            }
            return Ok(None);
        }

        if content.starts_with("@include ") {
            // TODO: implement file includes
            return Ok(None);
        }

        // --- Function definition ---

        if let Some(rest) = content.strip_prefix("@fn ") {
            let parts: Vec<&str> = rest.split_whitespace().collect();
            if parts.is_empty() {
                return Err(ParseError {
                    line: line_num,
                    message: "@fn requires a name".to_string(),
                });
            }
            let name = parts[0].to_string();
            let params: Vec<String> = parts[1..]
                .iter()
                .map(|p| p.strip_prefix('$').unwrap_or(p).to_string())
                .collect();

            // Collect body lines (all lines indented deeper than @fn)
            let mut body_lines = Vec::new();
            while self.pos < self.lines.len() && self.lines[self.pos].indent > current_indent {
                body_lines.push(self.lines[self.pos].clone());
                self.pos += 1;
            }

            ctx.functions.insert(name, FnDef { params, body_lines });
            return Ok(None);
        }

        // --- Function call ---

        if content.starts_with('@') {
            let name = extract_element_name(&content);
            if ctx.functions.contains_key(name) {
                let nodes =
                    self.expand_fn_call(name, &content, current_indent, line_num, ctx)?;
                return Ok(Some(nodes));
            }
        }

        // --- Elements ---

        if content.starts_with('@') || content.starts_with('[') {
            let node = self.parse_element_line(&content, current_indent, line_num, ctx)?;
            return Ok(Some(vec![node]));
        }

        // --- Bare text ---

        let segments = parse_text_segments(&content, ctx);
        Ok(Some(vec![Node::Text(segments)]))
    }

    fn expand_fn_call(
        &mut self,
        name: &str,
        content: &str,
        current_indent: usize,
        line_num: usize,
        ctx: &mut ParseContext,
    ) -> Result<Vec<Node>, ParseError> {
        // Parse [param value, ...] arguments
        let rest = &content[1 + name.len()..];
        let rest = rest.trim_start();

        let args = if rest.starts_with('[') {
            let (attrs, _) = parse_attr_brackets(rest, line_num, ctx)?;
            attrs
        } else {
            Vec::new()
        };

        // Clone function definition (releases borrow on ctx)
        let fn_def = ctx.functions.get(name).unwrap().clone();

        // Parse caller's children
        let caller_children = self.parse_children(current_indent + 1, ctx)?;

        // Save variable state, inject function parameters
        let saved_vars = ctx.variables.clone();
        for (i, param) in fn_def.params.iter().enumerate() {
            let value = args
                .iter()
                .find(|a| a.key == *param)
                .and_then(|a| a.value.clone())
                .or_else(|| args.get(i).and_then(|a| a.value.clone()))
                .unwrap_or_default();
            ctx.variables.insert(param.clone(), value);
        }

        // Normalize body indentation so it parses from indent 0
        let min_indent = fn_def
            .body_lines
            .iter()
            .map(|l| l.indent)
            .min()
            .unwrap_or(0);
        let adjusted: Vec<Line> = fn_def
            .body_lines
            .iter()
            .map(|l| Line {
                indent: l.indent - min_indent,
                content: l.content.clone(),
                line_num: l.line_num,
            })
            .collect();

        // Parse body with params in scope
        let mut body_parser = Parser {
            lines: adjusted,
            pos: 0,
        };
        let body_nodes = body_parser.parse_children(0, ctx)?;

        // Restore variables
        ctx.variables = saved_vars;

        // Replace @children with caller's children
        Ok(replace_children_nodes(body_nodes, &caller_children))
    }

    fn parse_element_line(
        &mut self,
        content: &str,
        current_indent: usize,
        line_num: usize,
        ctx: &mut ParseContext,
    ) -> Result<Node, ParseError> {
        let segments = split_chain(content);

        // Parse each segment into an Element
        let mut elements: Vec<Element> = Vec::new();
        for seg in &segments {
            let elem = parse_single_element(seg.trim(), line_num, ctx)?;
            elements.push(elem);
        }

        // Parse indented children (belong to the innermost element)
        let children = self.parse_children(current_indent + 1, ctx)?;

        // Build chain right-to-left: rightmost gets children, each wraps the next
        let mut current_children = children;
        for mut elem in elements.into_iter().rev() {
            elem.children.extend(current_children);
            current_children = vec![Node::Element(elem)];
        }

        Ok(current_children.into_iter().next().unwrap())
    }
}

// ---------------------------------------------------------------------------
// @children replacement
// ---------------------------------------------------------------------------

fn replace_children_nodes(nodes: Vec<Node>, caller_children: &[Node]) -> Vec<Node> {
    let mut result = Vec::new();
    for node in nodes {
        match node {
            Node::Element(elem) if elem.kind == ElementKind::Children => {
                result.extend(caller_children.iter().cloned());
            }
            Node::Element(mut elem) => {
                elem.children = replace_children_nodes(elem.children, caller_children);
                result.push(Node::Element(elem));
            }
            other => result.push(other),
        }
    }
    result
}

// ---------------------------------------------------------------------------
// Element parsing
// ---------------------------------------------------------------------------

fn extract_element_name(content: &str) -> &str {
    let without_at = &content[1..];
    match without_at.find(|c: char| c == ' ' || c == '[') {
        Some(i) => &without_at[..i],
        None => without_at,
    }
}

fn parse_single_element(
    content: &str,
    line_num: usize,
    ctx: &ParseContext,
) -> Result<Element, ParseError> {
    let (kind, rest) = if content.starts_with('[') {
        // Implicit @el
        (ElementKind::El, content.to_string())
    } else if content.starts_with('@') {
        let without_at = &content[1..];
        match without_at.find(|c: char| c == ' ' || c == '[') {
            Some(i) => {
                let kind_str = &without_at[..i];
                let rest = if without_at.as_bytes()[i] == b'[' {
                    without_at[i..].to_string()
                } else {
                    without_at[i + 1..].to_string()
                };
                (parse_element_kind(kind_str, line_num)?, rest)
            }
            None => (parse_element_kind(without_at, line_num)?, String::new()),
        }
    } else {
        return Err(ParseError {
            line: line_num,
            message: format!("expected @element or [attrs], got: {}", content),
        });
    };

    // Parse optional [attrs]
    let (attrs, rest) = if rest.starts_with('[') {
        parse_attr_brackets(&rest, line_num, ctx)?
    } else {
        (Vec::new(), rest)
    };

    let rest = rest.trim().to_string();

    // For @link, first token of rest is URL, remainder is inline text
    let mut children = Vec::new();
    let argument = if rest.is_empty() {
        None
    } else if kind == ElementKind::Link {
        let rest_sub = substitute_vars(&rest, &ctx.variables);
        if let Some((url, text)) = rest_sub.split_once(' ') {
            let text = text.trim();
            if !text.is_empty() {
                children.push(Node::Text(parse_text_segments(text, ctx)));
            }
            Some(url.to_string())
        } else {
            Some(rest_sub)
        }
    } else {
        Some(substitute_vars(&rest, &ctx.variables))
    };

    Ok(Element {
        kind,
        attrs,
        argument,
        children,
    })
}

fn parse_element_kind(s: &str, line_num: usize) -> Result<ElementKind, ParseError> {
    match s {
        "row" => Ok(ElementKind::Row),
        "column" | "col" => Ok(ElementKind::Column),
        "el" => Ok(ElementKind::El),
        "text" => Ok(ElementKind::Text),
        "paragraph" | "p" => Ok(ElementKind::Paragraph),
        "image" | "img" => Ok(ElementKind::Image),
        "link" => Ok(ElementKind::Link),
        "children" => Ok(ElementKind::Children),
        _ => Err(ParseError {
            line: line_num,
            message: format!("unknown element @{}", s),
        }),
    }
}

// ---------------------------------------------------------------------------
// Attribute parsing
// ---------------------------------------------------------------------------

fn parse_attr_brackets(
    input: &str,
    line_num: usize,
    ctx: &ParseContext,
) -> Result<(Vec<Attribute>, String), ParseError> {
    let mut depth = 0;
    let mut end_pos = 0;

    for (i, c) in input.char_indices() {
        match c {
            '[' => depth += 1,
            ']' => {
                depth -= 1;
                if depth == 0 {
                    end_pos = i;
                    break;
                }
            }
            _ => {}
        }
    }

    if depth != 0 {
        return Err(ParseError {
            line: line_num,
            message: "unclosed '[' in attribute list".to_string(),
        });
    }

    let attrs_inner = &input[1..end_pos];
    let remaining = input[end_pos + 1..].trim().to_string();
    let attrs = parse_attr_list(attrs_inner, ctx);

    Ok((attrs, remaining))
}

fn parse_attr_list(input: &str, ctx: &ParseContext) -> Vec<Attribute> {
    let mut attrs = Vec::new();

    for part in split_commas(input) {
        let part = part.trim();
        if part.is_empty() {
            continue;
        }

        // $define reference — expand
        if part.starts_with('$') {
            let name = &part[1..];
            if let Some(define_attrs) = ctx.defines.get(name) {
                attrs.extend(define_attrs.clone());
                continue;
            }
        }

        // Substitute variables in value
        let part = substitute_vars(part, &ctx.variables);

        if let Some((key, value)) = part.split_once(' ') {
            attrs.push(Attribute {
                key: key.trim().to_string(),
                value: Some(value.trim().to_string()),
            });
        } else {
            attrs.push(Attribute {
                key: part.to_string(),
                value: None,
            });
        }
    }

    attrs
}

fn split_commas(input: &str) -> Vec<&str> {
    let mut parts = Vec::new();
    let mut start = 0;
    let mut depth = 0;

    for (i, c) in input.char_indices() {
        match c {
            '[' => depth += 1,
            ']' => depth -= 1,
            ',' if depth == 0 => {
                parts.push(&input[start..i]);
                start = i + 1;
            }
            _ => {}
        }
    }
    parts.push(&input[start..]);
    parts
}

// ---------------------------------------------------------------------------
// Chain splitting (the > operator)
// ---------------------------------------------------------------------------

fn split_chain(content: &str) -> Vec<String> {
    let mut segments = Vec::new();
    let mut current = String::new();
    let chars: Vec<char> = content.chars().collect();
    let mut i = 0;
    let mut bracket_depth = 0;

    while i < chars.len() {
        if chars[i] == '[' {
            bracket_depth += 1;
        }
        if chars[i] == ']' {
            bracket_depth -= 1;
        }

        // Match " > @" outside brackets
        if bracket_depth == 0
            && i + 3 < chars.len()
            && chars[i] == ' '
            && chars[i + 1] == '>'
            && chars[i + 2] == ' '
            && chars[i + 3] == '@'
        {
            segments.push(current.trim().to_string());
            current = String::new();
            i += 3; // skip " > ", keep the "@"
            continue;
        }

        current.push(chars[i]);
        i += 1;
    }

    if !current.trim().is_empty() {
        segments.push(current.trim().to_string());
    }

    segments
}

// ---------------------------------------------------------------------------
// Text segment parsing (inline {...} elements)
// ---------------------------------------------------------------------------

fn parse_text_segments(input: &str, ctx: &ParseContext) -> Vec<TextSegment> {
    let mut segments = Vec::new();
    let mut current_text = String::new();
    let chars: Vec<char> = input.chars().collect();
    let mut i = 0;

    while i < chars.len() {
        if chars[i] == '{'
            && i + 1 < chars.len()
            && chars[i + 1] == '@'
        {
            // Flush accumulated plain text
            if !current_text.is_empty() {
                segments.push(TextSegment::Plain(substitute_vars(
                    &current_text,
                    &ctx.variables,
                )));
                current_text.clear();
            }

            // Find matching }
            let mut depth = 0;
            let start = i + 1; // after {
            i += 1;
            loop {
                if i >= chars.len() {
                    break;
                }
                if chars[i] == '{' {
                    depth += 1;
                } else if chars[i] == '}' {
                    if depth == 0 {
                        break;
                    }
                    depth -= 1;
                }
                i += 1;
            }

            let inner: String = chars[start..i].iter().collect();
            i += 1; // skip }

            if let Ok(elem) = parse_single_element(&inner, 0, ctx) {
                segments.push(TextSegment::Inline(elem));
            } else {
                current_text.push('{');
                current_text.push_str(&inner);
                current_text.push('}');
            }
        } else {
            current_text.push(chars[i]);
            i += 1;
        }
    }

    if !current_text.is_empty() {
        segments.push(TextSegment::Plain(substitute_vars(
            &current_text,
            &ctx.variables,
        )));
    }

    segments
}

// ---------------------------------------------------------------------------
// Variable substitution
// ---------------------------------------------------------------------------

fn substitute_vars(input: &str, vars: &HashMap<String, String>) -> String {
    if !input.contains('$') {
        return input.to_string();
    }

    let mut result = String::new();
    let chars: Vec<char> = input.chars().collect();
    let mut i = 0;

    while i < chars.len() {
        if chars[i] == '$'
            && i + 1 < chars.len()
            && (chars[i + 1].is_alphanumeric() || chars[i + 1] == '_')
        {
            let start = i + 1;
            let mut end = start;
            while end < chars.len()
                && (chars[end].is_alphanumeric() || chars[end] == '-' || chars[end] == '_')
            {
                end += 1;
            }
            let name: String = chars[start..end].iter().collect();
            if let Some(value) = vars.get(&name) {
                result.push_str(value);
            } else {
                result.push('$');
                result.push_str(&name);
            }
            i = end;
        } else {
            result.push(chars[i]);
            i += 1;
        }
    }

    result
}
