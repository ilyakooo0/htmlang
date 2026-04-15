use std::collections::{HashMap, HashSet};
use std::fmt;
use std::path::{Path, PathBuf};

use crate::ast::*;

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
pub enum Severity {
    Error,
    Warning,
}

#[derive(Debug, Clone)]
pub struct Diagnostic {
    pub line: usize,
    pub message: String,
    pub severity: Severity,
    pub source_line: Option<String>,
}

pub struct ParseResult {
    pub document: Document,
    pub diagnostics: Vec<Diagnostic>,
    pub included_files: Vec<PathBuf>,
}

#[derive(Debug)]
struct ParseError {
    line: usize,
    message: String,
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "line {}: {}", self.line, self.message)
    }
}

// ---------------------------------------------------------------------------
// Internal types
// ---------------------------------------------------------------------------

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
    defaults: HashMap<String, String>,
    body_lines: Vec<Line>,
}

struct ParseContext {
    page_title: Option<String>,
    lang: Option<String>,
    favicon: Option<String>,
    meta_tags: Vec<(String, String)>,
    head_blocks: Vec<String>,
    variables: HashMap<String, String>,
    defines: HashMap<String, Vec<Attribute>>,
    functions: HashMap<String, FnDef>,
    keyframes: Vec<(String, String)>,
    css_vars: Vec<(String, String)>,
    custom_css: Vec<String>,
    og_tags: Vec<(String, String)>,
    custom_breakpoints: Vec<(String, String)>,
    diagnostics: Vec<Diagnostic>,
    base_path: Option<PathBuf>,
    included_files: Vec<PathBuf>,
    include_stack: Vec<PathBuf>,
    fn_call_stack: Vec<String>,
    file_cache: HashMap<PathBuf, String>,
    /// Track which @let variables are referenced (for unused warnings)
    used_variables: HashSet<String>,
    /// Track which @fn functions are called (for unused warnings)
    used_functions: HashSet<String>,
    /// Track which @define bundles are referenced (for unused warnings)
    used_defines: HashSet<String>,
    /// Line numbers of @let definitions (name -> line)
    let_lines: HashMap<String, usize>,
    /// Line numbers of @fn definitions (name -> line)
    fn_lines: HashMap<String, usize>,
    /// Line numbers of @define definitions (name -> line)
    define_lines: HashMap<String, usize>,
}

struct Parser {
    lines: Vec<Line>,
    pos: usize,
}

pub fn parse(input: &str) -> ParseResult {
    parse_with_base(input, None)
}

pub fn parse_with_base(input: &str, base_path: Option<&Path>) -> ParseResult {
    let lines = preprocess(input);
    let mut parser = Parser { lines, pos: 0 };
    let mut ctx = ParseContext {
        page_title: None,
        lang: None,
        favicon: None,
        meta_tags: Vec::new(),
        head_blocks: Vec::new(),
        variables: HashMap::new(),
        defines: HashMap::new(),
        functions: HashMap::new(),
        keyframes: Vec::new(),
        css_vars: Vec::new(),
        custom_css: Vec::new(),
        og_tags: Vec::new(),
        custom_breakpoints: Vec::new(),
        diagnostics: Vec::new(),
        base_path: base_path.map(|p| p.to_path_buf()),
        included_files: Vec::new(),
        include_stack: Vec::new(),
        fn_call_stack: Vec::new(),
        file_cache: HashMap::new(),
        used_variables: HashSet::new(),
        used_functions: HashSet::new(),
        used_defines: HashSet::new(),
        let_lines: HashMap::new(),
        fn_lines: HashMap::new(),
        define_lines: HashMap::new(),
    };
    let nodes = parser.parse_children(0, &mut ctx);
    validate_tree(&nodes, None, &mut ctx.diagnostics);
    check_unused(&mut ctx);
    ParseResult {
        document: Document {
            page_title: ctx.page_title,
            lang: ctx.lang,
            favicon: ctx.favicon,
            meta_tags: ctx.meta_tags,
            head_blocks: ctx.head_blocks,
            variables: ctx.variables,
            defines: ctx.defines,
            keyframes: ctx.keyframes,
            css_vars: ctx.css_vars,
            custom_css: ctx.custom_css,
            og_tags: ctx.og_tags,
            custom_breakpoints: ctx.custom_breakpoints,
            nodes,
        },
        diagnostics: ctx.diagnostics,
        included_files: ctx.included_files,
    }
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

        // Join continuation lines for multi-line attribute brackets
        let first_line_num = i + 1;
        let mut full = trimmed.to_string();
        let mut depth: i32 = full.chars().filter(|&c| c == '[').count() as i32
            - full.chars().filter(|&c| c == ']').count() as i32;
        while depth > 0 && i + 1 < raw_lines.len() {
            i += 1;
            let next = raw_lines[i].trim();
            if next.is_empty() || next.starts_with("--") {
                continue;
            }
            full.push(' ');
            full.push_str(next);
            depth += next.chars().filter(|&c| c == '[').count() as i32;
            depth -= next.chars().filter(|&c| c == ']').count() as i32;
        }

        lines.push(Line {
            indent,
            content: LineContent::Normal(full),
            line_num: first_line_num,
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
    ) -> Vec<Node> {
        let mut nodes = Vec::new();

        while self.pos < self.lines.len() {
            let indent = self.lines[self.pos].indent;
            if indent < min_indent {
                break;
            }

            match self.parse_line(ctx) {
                Ok(Some(new_nodes)) => nodes.extend(new_nodes),
                Ok(None) => {}
                Err(e) => {
                    ctx.diagnostics.push(Diagnostic {
                        line: e.line,
                        message: e.message,
                        severity: Severity::Error,
                        source_line: None,
                    });
                    // Skip forward past any deeper-indented children of the errored line
                    while self.pos < self.lines.len()
                        && self.lines[self.pos].indent > indent
                    {
                        self.pos += 1;
                    }
                }
            }
        }

        nodes
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

        if let Some(rest) = content.strip_prefix("@lang ") {
            ctx.lang = Some(substitute_vars(rest.trim(), &ctx.variables));
            return Ok(None);
        }

        if let Some(rest) = content.strip_prefix("@favicon ") {
            ctx.favicon = Some(substitute_vars(rest.trim(), &ctx.variables));
            return Ok(None);
        }

        if let Some(rest) = content.strip_prefix("@let ") {
            let rest = rest.trim();
            if let Some((name, value)) = rest.split_once(' ') {
                let value = value.trim();
                // Support quoted string interpolation: @let greeting "Hello $name"
                let value = if value.starts_with('"') && value.ends_with('"') && value.len() >= 2 {
                    &value[1..value.len()-1]
                } else {
                    value
                };
                track_var_refs(value, &mut ctx.used_variables);
                let value = substitute_vars(value, &ctx.variables);
                let value = evaluate_arithmetic(&value);
                if name.starts_with("--") {
                    // CSS custom property
                    ctx.css_vars.push((name.to_string(), value.clone()));
                }
                ctx.variables.insert(name.to_string(), value);
                ctx.let_lines.entry(name.to_string()).or_insert(line_num);
            }
            return Ok(None);
        }

        if let Some(rest) = content.strip_prefix("@meta ") {
            let rest = rest.trim();
            if let Some((name, value)) = rest.split_once(' ') {
                let value = substitute_vars(value.trim(), &ctx.variables);
                ctx.meta_tags.push((name.trim().to_string(), value));
            }
            return Ok(None);
        }

        if let Some(rest) = content.strip_prefix("@og ") {
            let rest = rest.trim();
            let parts: Vec<&str> = rest.splitn(2, ' ').collect();
            if parts.len() == 2 {
                let value = substitute_vars(parts[1].trim(), &ctx.variables);
                let value = if value.starts_with('"') && value.ends_with('"') && value.len() >= 2 {
                    value[1..value.len()-1].to_string()
                } else {
                    value
                };
                ctx.og_tags.push((parts[0].to_string(), value));
            }
            return Ok(None);
        }

        if let Some(rest) = content.strip_prefix("@breakpoint ") {
            let rest = rest.trim();
            if let Some((name, value)) = rest.split_once(' ') {
                ctx.custom_breakpoints.push((name.trim().to_string(), substitute_vars(value.trim(), &ctx.variables)));
            }
            return Ok(None);
        }

        if content == "@head" || content.starts_with("@head ") {
            // Collect indented body lines as raw head content
            let mut head_content = String::new();
            while self.pos < self.lines.len() && self.lines[self.pos].indent > current_indent {
                match &self.lines[self.pos].content {
                    LineContent::Normal(s) => {
                        head_content.push_str(s.trim());
                        head_content.push('\n');
                    }
                    LineContent::Raw(s) => {
                        head_content.push_str(s);
                        head_content.push('\n');
                    }
                }
                self.pos += 1;
            }
            let trimmed = head_content.trim().to_string();
            if !trimmed.is_empty() {
                ctx.head_blocks.push(trimmed);
            }
            return Ok(None);
        }

        // --- @style block (raw CSS) ---
        if content.trim() == "@style" {
            let mut style_content = String::new();
            while self.pos < self.lines.len() && self.lines[self.pos].indent > current_indent {
                match &self.lines[self.pos].content {
                    LineContent::Normal(s) => {
                        style_content.push_str(s.trim());
                        style_content.push('\n');
                    }
                    LineContent::Raw(s) => {
                        style_content.push_str(s);
                        style_content.push('\n');
                    }
                }
                self.pos += 1;
            }
            let trimmed = style_content.trim().to_string();
            if !trimmed.is_empty() {
                ctx.custom_css.push(trimmed);
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
                ctx.define_lines.entry(name.to_string()).or_insert(line_num);
            }
            return Ok(None);
        }

        if let Some(rest) = content.strip_prefix("@include ") {
            let filename = substitute_vars(rest.trim(), &ctx.variables);
            let resolved = match &ctx.base_path {
                Some(base) => base.join(&filename),
                None => PathBuf::from(&filename),
            };

            // Circular include check
            if ctx.include_stack.contains(&resolved) {
                ctx.diagnostics.push(Diagnostic {
                    line: line_num,
                    message: format!("circular include '{}'", filename),
                    severity: Severity::Error,
                    source_line: Some(content.clone()),
                });
                return Ok(None);
            }

            // Use file cache to avoid redundant reads
            let included_text = if let Some(cached) = ctx.file_cache.get(&resolved) {
                cached.clone()
            } else {
                match std::fs::read_to_string(&resolved) {
                    Ok(text) => {
                        ctx.file_cache.insert(resolved.clone(), text.clone());
                        text
                    }
                    Err(e) => {
                        ctx.diagnostics.push(Diagnostic {
                            line: line_num,
                            message: format!("cannot include '{}': {}", filename, e),
                            severity: Severity::Error,
                            source_line: Some(content.clone()),
                        });
                        return Ok(None);
                    }
                }
            };

            ctx.included_files.push(resolved.clone());
            ctx.include_stack.push(resolved.clone());
            let saved_base = ctx.base_path.clone();
            ctx.base_path = resolved.parent().map(|p| p.to_path_buf());

            let included_lines = preprocess(&included_text);
            let mut included_parser = Parser {
                lines: included_lines,
                pos: 0,
            };
            let nodes = included_parser.parse_children(0, ctx);

            ctx.base_path = saved_base;
            ctx.include_stack.pop();
            return Ok(Some(nodes));
        }

        // --- @import (definitions only, no DOM nodes) ---

        if let Some(rest) = content.strip_prefix("@import ") {
            let filename = substitute_vars(rest.trim(), &ctx.variables);
            let resolved = match &ctx.base_path {
                Some(base) => base.join(&filename),
                None => PathBuf::from(&filename),
            };

            if ctx.include_stack.contains(&resolved) {
                ctx.diagnostics.push(Diagnostic {
                    line: line_num,
                    message: format!("circular import '{}'", filename),
                    severity: Severity::Error,
                    source_line: Some(content.clone()),
                });
                return Ok(None);
            }

            let imported_text = if let Some(cached) = ctx.file_cache.get(&resolved) {
                cached.clone()
            } else {
                match std::fs::read_to_string(&resolved) {
                    Ok(text) => {
                        ctx.file_cache.insert(resolved.clone(), text.clone());
                        text
                    }
                    Err(e) => {
                        ctx.diagnostics.push(Diagnostic {
                            line: line_num,
                            message: format!("cannot import '{}': {}", filename, e),
                            severity: Severity::Error,
                            source_line: Some(content.clone()),
                        });
                        return Ok(None);
                    }
                }
            };

            ctx.included_files.push(resolved.clone());
            ctx.include_stack.push(resolved.clone());
            let saved_base = ctx.base_path.clone();
            ctx.base_path = resolved.parent().map(|p| p.to_path_buf());

            // Parse the file but discard DOM nodes — only keep definitions
            let imported_lines = preprocess(&imported_text);
            let mut imported_parser = Parser {
                lines: imported_lines,
                pos: 0,
            };
            let _discarded_nodes = imported_parser.parse_children(0, ctx);

            ctx.base_path = saved_base;
            ctx.include_stack.pop();
            return Ok(None); // No nodes emitted
        }

        // --- @if / @else ---

        if let Some(rest) = content.strip_prefix("@if ") {
            let rest = rest.trim();
            let condition = substitute_vars(rest, &ctx.variables);
            let result = evaluate_condition(&condition);

            // Collect then-body lines
            let mut then_lines = Vec::new();
            while self.pos < self.lines.len() && self.lines[self.pos].indent > current_indent {
                then_lines.push(self.lines[self.pos].clone());
                self.pos += 1;
            }

            // Build branches: [(condition_result, body_lines), ...]
            let mut branches: Vec<(bool, Vec<Line>)> = vec![(result, then_lines)];

            // Check for @else if / @else chains at same indent
            loop {
                if self.pos >= self.lines.len() || self.lines[self.pos].indent != current_indent {
                    break;
                }
                if let LineContent::Normal(ref s) = self.lines[self.pos].content {
                    let trimmed = s.trim();
                    if let Some(else_if_cond) = trimmed.strip_prefix("@else if ") {
                        self.pos += 1; // consume @else if
                        let cond = substitute_vars(else_if_cond.trim(), &ctx.variables);
                        let cond_result = evaluate_condition(&cond);
                        let mut body = Vec::new();
                        while self.pos < self.lines.len()
                            && self.lines[self.pos].indent > current_indent
                        {
                            body.push(self.lines[self.pos].clone());
                            self.pos += 1;
                        }
                        branches.push((cond_result, body));
                    } else if trimmed == "@else" {
                        self.pos += 1; // consume @else
                        let mut body = Vec::new();
                        while self.pos < self.lines.len()
                            && self.lines[self.pos].indent > current_indent
                        {
                            body.push(self.lines[self.pos].clone());
                            self.pos += 1;
                        }
                        // @else is always true (fallback)
                        branches.push((true, body));
                        break;
                    } else {
                        break;
                    }
                } else {
                    break;
                }
            }

            // Pick the first branch whose condition is true
            let body_lines = branches
                .into_iter()
                .find(|(cond, _)| *cond)
                .map(|(_, lines)| lines)
                .unwrap_or_default();

            if body_lines.is_empty() {
                return Ok(None);
            }

            let min_indent = body_lines.iter().map(|l| l.indent).min().unwrap_or(0);
            let adjusted: Vec<Line> = body_lines
                .iter()
                .map(|l| Line {
                    indent: l.indent - min_indent,
                    content: l.content.clone(),
                    line_num: l.line_num,
                })
                .collect();

            // Scope variables: @let inside @if doesn't leak out
            let saved_vars = ctx.variables.clone();
            let mut body_parser = Parser {
                lines: adjusted,
                pos: 0,
            };
            let nodes = body_parser.parse_children(0, ctx);
            ctx.variables = saved_vars;
            return Ok(Some(nodes));
        }

        if let Some(rest) = content.strip_prefix("@unless ") {
            let rest = rest.trim();
            let condition = substitute_vars(rest, &ctx.variables);
            let result = !evaluate_condition(&condition);
            let mut body_lines = Vec::new();
            while self.pos < self.lines.len() && self.lines[self.pos].indent > current_indent {
                body_lines.push(self.lines[self.pos].clone());
                self.pos += 1;
            }
            if !result || body_lines.is_empty() {
                return Ok(None);
            }
            let min_indent = body_lines.iter().map(|l| l.indent).min().unwrap_or(0);
            let adjusted: Vec<Line> = body_lines.iter().map(|l| Line {
                indent: l.indent - min_indent,
                content: l.content.clone(),
                line_num: l.line_num,
            }).collect();
            let saved_vars = ctx.variables.clone();
            let mut body_parser = Parser { lines: adjusted, pos: 0 };
            let nodes = body_parser.parse_children(0, ctx);
            ctx.variables = saved_vars;
            return Ok(Some(nodes));
        }

        if content.trim() == "@else" || content.trim().starts_with("@else if ") {
            return Err(ParseError {
                line: line_num,
                message: "@else without matching @if".to_string(),
            });
        }

        // --- @each loop ---

        if let Some(rest) = content.strip_prefix("@each ") {
            let rest = rest.trim();
            // Support: @each $var in list  OR  @each $var, $index in list
            // OR  @each $name, $url in Alice /alice, Bob /bob (destructuring)
            let (var_names, list_str) = if let Some((before_in, after_in)) = rest.split_once(" in ") {
                let before_in = before_in.trim();
                let vars: Vec<String> = before_in.split(',')
                    .map(|v| v.trim().strip_prefix('$').unwrap_or(v.trim()).to_string())
                    .collect();
                (vars, after_in.trim().to_string())
            } else {
                return Err(ParseError {
                    line: line_num,
                    message: "@each requires: @each $var in list".to_string(),
                });
            };
            let var_name = var_names[0].clone();
            let index_var = var_names.get(1).cloned();

            let list_str = substitute_vars(&list_str, &ctx.variables);
            track_var_refs(&list_str, &mut ctx.used_variables);
            // Support range syntax: @each $i in 1..5
            let items: Vec<String> = if let Some((start_s, end_s)) = list_str.split_once("..") {
                if let (Ok(start), Ok(end)) = (start_s.trim().parse::<i64>(), end_s.trim().parse::<i64>()) {
                    if start <= end {
                        (start..=end).map(|n| n.to_string()).collect()
                    } else {
                        (end..=start).rev().map(|n| n.to_string()).collect()
                    }
                } else {
                    list_str.split(',').map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect()
                }
            } else {
                list_str.split(',').map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect()
            };

            // Collect body lines
            let mut body_lines = Vec::new();
            while self.pos < self.lines.len() && self.lines[self.pos].indent > current_indent {
                body_lines.push(self.lines[self.pos].clone());
                self.pos += 1;
            }

            // Check for @else block (empty-state fallback)
            let mut else_lines = Vec::new();
            if self.pos < self.lines.len() && self.lines[self.pos].indent == current_indent {
                if let LineContent::Normal(ref s) = self.lines[self.pos].content {
                    if s.trim() == "@else" {
                        self.pos += 1; // consume @else
                        while self.pos < self.lines.len() && self.lines[self.pos].indent > current_indent {
                            else_lines.push(self.lines[self.pos].clone());
                            self.pos += 1;
                        }
                    }
                }
            }

            if body_lines.is_empty() {
                return Ok(None);
            }

            // If list is empty, render @else body
            if items.is_empty() {
                if else_lines.is_empty() {
                    return Ok(None);
                }
                let min_indent = else_lines.iter().map(|l| l.indent).min().unwrap_or(0);
                let adjusted: Vec<Line> = else_lines
                    .iter()
                    .map(|l| Line {
                        indent: l.indent - min_indent,
                        content: l.content.clone(),
                        line_num: l.line_num,
                    })
                    .collect();
                let mut body_parser = Parser {
                    lines: adjusted,
                    pos: 0,
                };
                let nodes = body_parser.parse_children(0, ctx);
                return Ok(Some(nodes));
            }

            let min_indent = body_lines.iter().map(|l| l.indent).min().unwrap_or(0);
            let adjusted: Vec<Line> = body_lines
                .iter()
                .map(|l| Line {
                    indent: l.indent - min_indent,
                    content: l.content.clone(),
                    line_num: l.line_num,
                })
                .collect();

            let saved_vars = ctx.variables.clone();
            let mut all_nodes = Vec::new();

            let has_extra_vars = var_names.len() > 2
                || (var_names.len() == 2 && items.first().map_or(false, |it| it.contains(' ')));

            for (i, item) in items.iter().enumerate() {
                if has_extra_vars {
                    // Destructuring: split item by spaces and assign to each variable
                    let parts: Vec<&str> = item.splitn(var_names.len(), ' ').collect();
                    for (vi, vn) in var_names.iter().enumerate() {
                        let val = parts.get(vi).unwrap_or(&"").to_string();
                        ctx.variables.insert(vn.clone(), val);
                    }
                } else {
                    ctx.variables.insert(var_name.clone(), item.clone());
                    if let Some(ref idx_name) = index_var {
                        ctx.variables.insert(idx_name.clone(), i.to_string());
                    }
                }
                let mut body_parser = Parser {
                    lines: adjusted.clone(),
                    pos: 0,
                };
                let nodes = body_parser.parse_children(0, ctx);
                all_nodes.extend(nodes);
            }

            ctx.variables = saved_vars;
            return Ok(Some(all_nodes));
        }

        // --- @match ---

        if let Some(rest) = content.strip_prefix("@match ") {
            let match_val = substitute_vars(rest.trim(), &ctx.variables);
            track_var_refs(rest.trim(), &mut ctx.used_variables);

            // Collect all child lines
            let mut match_lines = Vec::new();
            while self.pos < self.lines.len() && self.lines[self.pos].indent > current_indent {
                match_lines.push(self.lines[self.pos].clone());
                self.pos += 1;
            }

            if match_lines.is_empty() {
                return Ok(None);
            }

            let case_indent = match_lines[0].indent;

            // Group into cases: (Some(value), body) or (None, body) for @default
            let mut cases: Vec<(Option<String>, Vec<Line>)> = Vec::new();
            let mut mi = 0;
            while mi < match_lines.len() {
                if match_lines[mi].indent == case_indent {
                    if let LineContent::Normal(ref s) = match_lines[mi].content {
                        let trimmed = s.trim();
                        if let Some(case_val) = trimmed.strip_prefix("@case ") {
                            let case_val = substitute_vars(case_val.trim(), &ctx.variables);
                            mi += 1;
                            let mut body = Vec::new();
                            while mi < match_lines.len() && match_lines[mi].indent > case_indent {
                                body.push(match_lines[mi].clone());
                                mi += 1;
                            }
                            cases.push((Some(case_val), body));
                        } else if trimmed == "@default" {
                            mi += 1;
                            let mut body = Vec::new();
                            while mi < match_lines.len() && match_lines[mi].indent > case_indent {
                                body.push(match_lines[mi].clone());
                                mi += 1;
                            }
                            cases.push((None, body));
                        } else {
                            mi += 1;
                        }
                    } else {
                        mi += 1;
                    }
                } else {
                    mi += 1;
                }
            }

            // Find first matching case or @default
            let body_lines = cases
                .into_iter()
                .find(|(case_val, _)| match case_val {
                    Some(v) => *v == match_val,
                    None => true,
                })
                .map(|(_, lines)| lines)
                .unwrap_or_default();

            if body_lines.is_empty() {
                return Ok(None);
            }

            let min_indent = body_lines.iter().map(|l| l.indent).min().unwrap_or(0);
            let adjusted: Vec<Line> = body_lines
                .iter()
                .map(|l| Line {
                    indent: l.indent - min_indent,
                    content: l.content.clone(),
                    line_num: l.line_num,
                })
                .collect();

            let saved_vars = ctx.variables.clone();
            let mut body_parser = Parser {
                lines: adjusted,
                pos: 0,
            };
            let nodes = body_parser.parse_children(0, ctx);
            ctx.variables = saved_vars;
            return Ok(Some(nodes));
        }

        // --- @warn / @debug ---

        if let Some(rest) = content.strip_prefix("@warn ") {
            let msg = substitute_vars(rest.trim(), &ctx.variables);
            ctx.diagnostics.push(Diagnostic {
                line: line_num,
                message: msg,
                severity: Severity::Warning,
                source_line: Some(content.clone()),
            });
            return Ok(None);
        }

        if let Some(rest) = content.strip_prefix("@debug ") {
            let msg = substitute_vars(rest.trim(), &ctx.variables);
            eprintln!("debug: line {}: {}", line_num, msg);
            return Ok(None);
        }

        // --- @keyframes directive ---

        if let Some(rest) = content.strip_prefix("@keyframes ") {
            let name = rest.trim().to_string();
            if name.is_empty() {
                return Err(ParseError {
                    line: line_num,
                    message: "@keyframes requires a name".to_string(),
                });
            }
            // Collect body lines (indented deeper)
            let mut body = String::new();
            while self.pos < self.lines.len() && self.lines[self.pos].indent > current_indent {
                if let LineContent::Normal(ref s) = self.lines[self.pos].content {
                    let trimmed = s.trim();
                    body.push_str(trimmed);
                }
                self.pos += 1;
            }
            ctx.keyframes.push((name, body));
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
            let mut params = Vec::new();
            let mut defaults = HashMap::new();
            for part in &parts[1..] {
                let part = part.strip_prefix('$').unwrap_or(part);
                if let Some((param_name, default_val)) = part.split_once('=') {
                    params.push(param_name.to_string());
                    defaults.insert(param_name.to_string(), default_val.to_string());
                } else {
                    params.push(part.to_string());
                }
            }

            // Collect body lines (all lines indented deeper than @fn)
            let mut body_lines = Vec::new();
            while self.pos < self.lines.len() && self.lines[self.pos].indent > current_indent {
                body_lines.push(self.lines[self.pos].clone());
                self.pos += 1;
            }

            ctx.fn_lines.entry(name.clone()).or_insert(line_num);
            ctx.functions.insert(name, FnDef { params, defaults, body_lines });
            return Ok(None);
        }

        // --- Function call ---

        if content.starts_with('@') {
            let name = extract_element_name(&content);
            if ctx.functions.contains_key(name) {
                ctx.used_functions.insert(name.to_string());
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
        // Recursive function cycle detection
        if ctx.fn_call_stack.contains(&name.to_string()) {
            return Err(ParseError {
                line: line_num,
                message: format!(
                    "recursive function call to @{} (call stack: {})",
                    name,
                    ctx.fn_call_stack.join(" -> ")
                ),
            });
        }
        ctx.fn_call_stack.push(name.to_string());

        // Parse [param value, ...] arguments
        let rest = &content[1 + name.len()..];
        let rest = rest.trim_start();

        let args = if rest.starts_with('[') {
            let (attrs, _) = parse_attr_brackets_no_validate(rest, line_num, ctx)?;
            attrs
        } else {
            Vec::new()
        };

        // Clone function definition (releases borrow on ctx)
        let fn_def = ctx.functions.get(name).unwrap().clone();

        // Parse caller's children, separating named slots from default children
        let all_caller_children = self.parse_children(current_indent + 1, ctx);
        let mut slot_contents: HashMap<String, Vec<Node>> = HashMap::new();
        let mut caller_children = Vec::new();
        for child in all_caller_children {
            if let Node::Element(ref elem) = child {
                if let ElementKind::Slot(ref slot_name) = elem.kind {
                    if !slot_name.is_empty() {
                        slot_contents.entry(slot_name.clone()).or_default().extend(elem.children.clone());
                        continue;
                    }
                }
            }
            caller_children.push(child);
        }

        // Save variable state, inject function parameters
        let saved_vars = ctx.variables.clone();
        for (i, param) in fn_def.params.iter().enumerate() {
            let value = args
                .iter()
                .find(|a| a.key == *param)
                .and_then(|a| a.value.clone())
                .or_else(|| args.get(i).and_then(|a| a.value.clone()))
                .or_else(|| fn_def.defaults.get(param).cloned())
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
        let body_nodes = body_parser.parse_children(0, ctx);

        // Restore variables and call stack
        ctx.variables = saved_vars;
        ctx.fn_call_stack.pop();

        // Replace @children with caller's children and @slot with slot content
        Ok(replace_children_and_slots(body_nodes, &caller_children, &slot_contents))
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
        let children = self.parse_children(current_indent + 1, ctx);

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

fn replace_children_and_slots(
    nodes: Vec<Node>,
    caller_children: &[Node],
    slot_contents: &HashMap<String, Vec<Node>>,
) -> Vec<Node> {
    let mut result = Vec::new();
    for node in nodes {
        match node {
            Node::Element(elem) if elem.kind == ElementKind::Children => {
                result.extend(caller_children.iter().cloned());
            }
            Node::Element(elem) if matches!(&elem.kind, ElementKind::Slot(name) if !name.is_empty()) => {
                if let ElementKind::Slot(ref name) = elem.kind {
                    if let Some(content) = slot_contents.get(name) {
                        result.extend(content.iter().cloned());
                    }
                    // If no content provided for this slot, use the slot's own children as default
                    else if !elem.children.is_empty() {
                        result.extend(elem.children);
                    }
                }
            }
            Node::Element(mut elem) => {
                elem.children = replace_children_and_slots(elem.children, caller_children, slot_contents);
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
    ctx: &mut ParseContext,
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

    // For @slot, the argument is the slot name
    let kind = if let ElementKind::Slot(_) = kind {
        ElementKind::Slot(argument.clone().unwrap_or_default())
    } else {
        kind
    };

    Ok(Element {
        kind,
        attrs,
        argument,
        children,
        line_num,
    })
}

const KNOWN_ELEMENTS: &[&str] = &[
    "row", "column", "col", "el", "text", "paragraph", "p", "image", "img", "link", "children",
    "input", "button", "select", "textarea", "option", "label", "slot",
    "nav", "header", "footer", "main", "section", "article", "aside",
    "list", "item", "li",
    "table", "thead", "tbody", "tr", "td", "th",
    "video", "audio",
    "form", "details", "summary", "blockquote", "cite", "code", "pre", "hr", "divider",
    "figure", "figcaption", "progress", "meter",
    "fragment",
    "btn", "ul", "dialog", "dl", "dt", "dd", "fieldset", "legend",
    "picture", "source", "time", "mark", "kbd", "abbr", "datalist",
    "iframe", "output", "canvas",
];

const KNOWN_DIRECTIVES: &[&str] = &[
    "page", "let", "define", "fn", "include", "import", "raw", "keyframes",
    "if", "else", "each", "meta", "head", "style",
    "match", "case", "default", "warn", "debug",
    "unless", "og", "breakpoint", "lang", "favicon",
];

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
        "input" => Ok(ElementKind::Input),
        "button" | "btn" => Ok(ElementKind::Button),
        "select" => Ok(ElementKind::Select),
        "textarea" => Ok(ElementKind::Textarea),
        "option" | "opt" => Ok(ElementKind::Option),
        "label" => Ok(ElementKind::Label),
        "slot" => Ok(ElementKind::Slot(String::new())), // slot name filled in by parse_single_element
        "nav" => Ok(ElementKind::Nav),
        "header" => Ok(ElementKind::Header),
        "footer" => Ok(ElementKind::Footer),
        "main" => Ok(ElementKind::Main),
        "section" => Ok(ElementKind::Section),
        "article" => Ok(ElementKind::Article),
        "aside" => Ok(ElementKind::Aside),
        "list" => Ok(ElementKind::List),
        "item" | "li" => Ok(ElementKind::ListItem),
        "table" => Ok(ElementKind::Table),
        "thead" => Ok(ElementKind::TableHead),
        "tbody" => Ok(ElementKind::TableBody),
        "tr" => Ok(ElementKind::TableRow),
        "td" => Ok(ElementKind::TableCell),
        "th" => Ok(ElementKind::TableHeaderCell),
        "video" => Ok(ElementKind::Video),
        "audio" => Ok(ElementKind::Audio),
        "form" => Ok(ElementKind::Form),
        "details" => Ok(ElementKind::Details),
        "summary" => Ok(ElementKind::Summary),
        "blockquote" => Ok(ElementKind::Blockquote),
        "cite" => Ok(ElementKind::Cite),
        "code" => Ok(ElementKind::Code),
        "pre" => Ok(ElementKind::Pre),
        "hr" | "divider" => Ok(ElementKind::HorizontalRule),
        "figure" => Ok(ElementKind::Figure),
        "figcaption" => Ok(ElementKind::FigCaption),
        "progress" => Ok(ElementKind::Progress),
        "meter" => Ok(ElementKind::Meter),
        "fragment" => Ok(ElementKind::Fragment),
        "dialog" => Ok(ElementKind::Dialog),
        "dl" => Ok(ElementKind::DefinitionList),
        "dt" => Ok(ElementKind::DefinitionTerm),
        "dd" => Ok(ElementKind::DefinitionDescription),
        "fieldset" => Ok(ElementKind::Fieldset),
        "legend" => Ok(ElementKind::Legend),
        "picture" => Ok(ElementKind::Picture),
        "source" => Ok(ElementKind::Source),
        "time" => Ok(ElementKind::Time),
        "mark" => Ok(ElementKind::Mark),
        "kbd" => Ok(ElementKind::Kbd),
        "abbr" => Ok(ElementKind::Abbr),
        "datalist" => Ok(ElementKind::Datalist),
        "iframe" => Ok(ElementKind::Iframe),
        "output" => Ok(ElementKind::Output),
        "canvas" => Ok(ElementKind::Canvas),
        "ul" => Ok(ElementKind::List),
        _ => {
            let all_known: Vec<&str> = KNOWN_ELEMENTS
                .iter()
                .chain(KNOWN_DIRECTIVES.iter())
                .copied()
                .collect();
            let suggestion = suggest_closest(s, &all_known);
            let msg = match suggestion {
                Some(closest) => format!("unknown element @{}, did you mean @{}?", s, closest),
                None => format!("unknown element @{}", s),
            };
            Err(ParseError {
                line: line_num,
                message: msg,
            })
        }
    }
}

fn levenshtein(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    let mut dp = vec![vec![0usize; b.len() + 1]; a.len() + 1];
    for i in 0..=a.len() {
        dp[i][0] = i;
    }
    for j in 0..=b.len() {
        dp[0][j] = j;
    }
    for i in 1..=a.len() {
        for j in 1..=b.len() {
            let cost = if a[i - 1] == b[j - 1] { 0 } else { 1 };
            dp[i][j] = (dp[i - 1][j] + 1)
                .min(dp[i][j - 1] + 1)
                .min(dp[i - 1][j - 1] + cost);
        }
    }
    dp[a.len()][b.len()]
}

fn suggest_closest<'a>(input: &str, candidates: &[&'a str]) -> Option<&'a str> {
    let mut best = None;
    let mut best_dist = usize::MAX;
    for &candidate in candidates {
        let dist = levenshtein(input, candidate);
        if dist < best_dist && dist <= 2 && dist < input.len() {
            best_dist = dist;
            best = Some(candidate);
        }
    }
    best
}

// ---------------------------------------------------------------------------
// Attribute parsing
// ---------------------------------------------------------------------------

const KNOWN_ATTRS: &[&str] = &[
    // Layout
    "spacing", "gap", "padding", "padding-x", "padding-y",
    "width", "height", "min-width", "max-width", "min-height", "max-height",
    "center-x", "center-y", "align-left", "align-right", "align-top", "align-bottom",
    // Style
    "background", "color", "border", "border-top", "border-bottom", "border-left", "border-right",
    "rounded", "bold", "italic", "underline",
    "size", "font", "transition", "cursor", "opacity",
    "text-align", "line-height", "letter-spacing", "text-transform", "white-space",
    "overflow", "position", "top", "right", "bottom", "left", "z-index", "shadow",
    "wrap", "gap-x", "gap-y",
    // Display & visibility
    "display", "visibility",
    // Transform & filters
    "transform", "backdrop-filter",
    // Grid
    "grid", "grid-cols", "grid-rows", "col-span", "row-span",
    // Container queries
    "container", "container-name", "container-type",
    // Identity
    "id", "class",
    // Animation
    "animation",
    // Form
    "type", "placeholder", "name", "value", "disabled", "required", "checked",
    "for", "action", "method", "autocomplete",
    "min", "max", "step", "pattern", "maxlength", "rows", "cols", "multiple",
    // Accessibility
    "alt", "role", "tabindex", "title",
    // CSS: aspect-ratio, outline, logical properties, scroll-snap
    "aspect-ratio", "outline",
    "padding-inline", "padding-block", "margin-inline", "margin-block",
    "scroll-snap-type", "scroll-snap-align",
    // Media attributes
    "controls", "autoplay", "loop", "muted", "poster", "preload",
    "loading", "decoding",
    // List
    "ordered",
    // Media src (explicit attribute form)
    "src",
    // New elements
    "open", "novalidate", "low", "high", "optimum",
    "colspan", "rowspan", "scope",
    // Margin
    "margin", "margin-x", "margin-y",
    // Filter & object
    "filter", "object-fit", "object-position",
    // Text extras
    "text-shadow", "text-overflow",
    // Interaction
    "pointer-events", "user-select",
    // Flexbox/grid alignment
    "justify-content", "align-items",
    // Flex item
    "order",
    // Background extras
    "background-size", "background-position", "background-repeat",
    // Text wrapping
    "word-break", "overflow-wrap",
    // Asset inlining
    "inline",
    // Hidden
    "hidden",
    // Overflow directional
    "overflow-x", "overflow-y",
    // Inset shorthand
    "inset",
    // Modern form theming
    "accent-color", "caret-color",
    // List styling
    "list-style",
    // Table styling
    "border-collapse", "border-spacing",
    // Text decoration variants
    "text-decoration", "text-decoration-color", "text-decoration-thickness", "text-decoration-style",
    // Grid/flex placement
    "place-items", "place-self",
    // Scroll behavior
    "scroll-behavior",
    // Resize
    "resize",
    // New CSS properties
    "clip-path", "mix-blend-mode", "background-blend-mode", "writing-mode",
    "column-count", "column-gap", "text-indent", "hyphens",
    "flex-grow", "flex-shrink", "flex-basis", "isolation",
    "place-content", "background-image", "datetime",
    // New CSS properties (batch 2)
    "font-weight", "font-style", "text-wrap", "will-change", "touch-action",
    "vertical-align", "contain", "content-visibility",
    "scroll-margin", "scroll-margin-top", "scroll-margin-bottom", "scroll-margin-left", "scroll-margin-right",
    "scroll-padding", "scroll-padding-top", "scroll-padding-bottom", "scroll-padding-left", "scroll-padding-right",
    // Iframe/output attrs
    "sandbox", "allow", "allowfullscreen", "referrerpolicy",
    "formaction", "formmethod", "formtarget", "target",
    // Pseudo-element content
    "content",
];

/// Attributes that expect purely numeric values (px-based) or values with CSS units.
const NUMERIC_ATTRS: &[&str] = &[
    "spacing", "gap", "padding", "padding-x", "padding-y",
    "min-width", "max-width", "min-height", "max-height",
    "rounded", "size", "gap-x", "gap-y",
    "top", "right", "bottom", "left", "letter-spacing",
];

const CSS_UNIT_SUFFIXES: &[&str] = &[
    "%", "rem", "em", "vh", "vw", "vmin", "vmax", "dvh", "svh", "lvh",
    "ch", "ex", "cm", "mm", "in", "pt", "pc", "fr",
];

fn has_css_unit(value: &str) -> bool {
    CSS_UNIT_SUFFIXES.iter().any(|u| value.ends_with(u))
        || value.starts_with("var(")
        || value.starts_with("calc(")
}

/// Attributes that accept numeric OR keyword values.
const NUMERIC_OR_KEYWORD_ATTRS: &[&str] = &["width", "height"];
const SIZE_KEYWORDS: &[&str] = &["fill", "shrink"];

fn validate_attr_value(attr: &Attribute, line_num: usize, ctx: &mut ParseContext) {
    let base_key = strip_all_prefixes(attr.key.as_str());

    if let Some(val) = &attr.value {
        if NUMERIC_ATTRS.contains(&base_key) {
            // All space-separated parts must be numeric or have a CSS unit
            for part in val.split_whitespace() {
                if part.parse::<f64>().is_err() && !has_css_unit(part) {
                    ctx.diagnostics.push(Diagnostic {
                        line: line_num,
                        message: format!(
                            "'{}' expects a numeric value (with optional unit), got '{}'",
                            attr.key, val
                        ),
                        severity: Severity::Warning,
                        source_line: None,
                    });
                    return;
                }
            }
        } else if NUMERIC_OR_KEYWORD_ATTRS.contains(&base_key) {
            let is_keyword = SIZE_KEYWORDS.contains(&val.as_str());
            let is_numeric = val.parse::<f64>().is_ok();
            let has_unit = has_css_unit(val);
            if !is_keyword && !is_numeric && !has_unit {
                ctx.diagnostics.push(Diagnostic {
                    line: line_num,
                    message: format!(
                        "'{}' expects a number or one of [{}], got '{}'",
                        attr.key,
                        SIZE_KEYWORDS.join(", "),
                        val
                    ),
                    severity: Severity::Warning,
                    source_line: None,
                });
            }
        } else if base_key == "opacity" {
            if let Ok(v) = val.parse::<f64>() {
                if !(0.0..=1.0).contains(&v) {
                    ctx.diagnostics.push(Diagnostic {
                        line: line_num,
                        message: format!("'opacity' should be between 0 and 1, got '{}'", val),
                        severity: Severity::Warning,
                        source_line: None,
                    });
                }
            } else {
                ctx.diagnostics.push(Diagnostic {
                    line: line_num,
                    message: format!("'opacity' expects a numeric value, got '{}'", val),
                    severity: Severity::Warning,
                    source_line: None,
                });
            }
        } else if base_key == "z-index" {
            if val.parse::<i32>().is_err() {
                ctx.diagnostics.push(Diagnostic {
                    line: line_num,
                    message: format!("'z-index' expects an integer, got '{}'", val),
                    severity: Severity::Warning,
                    source_line: None,
                });
            }
        }
    }
}

fn parse_attr_brackets(
    input: &str,
    line_num: usize,
    ctx: &mut ParseContext,
) -> Result<(Vec<Attribute>, String), ParseError> {
    parse_attr_brackets_inner(input, line_num, ctx, true)
}

fn parse_attr_brackets_no_validate(
    input: &str,
    line_num: usize,
    ctx: &mut ParseContext,
) -> Result<(Vec<Attribute>, String), ParseError> {
    parse_attr_brackets_inner(input, line_num, ctx, false)
}

fn parse_attr_brackets_inner(
    input: &str,
    line_num: usize,
    ctx: &mut ParseContext,
    validate: bool,
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
    let attrs = parse_attr_list(attrs_inner, line_num, ctx, validate);

    Ok((attrs, remaining))
}

fn is_valid_hex_color(s: &str) -> bool {
    if !s.starts_with('#') {
        return true; // Not a hex color, skip
    }
    let hex = &s[1..];
    matches!(hex.len(), 3 | 4 | 6 | 8) && hex.chars().all(|c| c.is_ascii_hexdigit())
}

fn parse_attr_list(input: &str, line_num: usize, ctx: &mut ParseContext, validate: bool) -> Vec<Attribute> {
    let mut attrs = Vec::new();
    let mut seen_keys: Vec<String> = Vec::new();

    for part in split_commas(input) {
        let part = part.trim();
        if part.is_empty() {
            continue;
        }

        // $define reference — expand
        if part.starts_with('$') {
            let name = &part[1..];
            if let Some(define_attrs) = ctx.defines.get(name) {
                ctx.used_defines.insert(name.to_string());
                attrs.extend(define_attrs.clone());
                continue;
            }
        }

        // Substitute variables in value
        track_var_refs(part, &mut ctx.used_variables);
        let part = substitute_vars(part, &ctx.variables);

        let attr = if let Some((key, value)) = part.split_once(' ') {
            let value = evaluate_if_expr(value.trim());
            Attribute {
                key: key.trim().to_string(),
                value: Some(value),
            }
        } else {
            Attribute {
                key: part.to_string(),
                value: None,
            }
        };

        // Warn on duplicate attributes
        if validate {
            let stripped = strip_all_prefixes(&attr.key).to_string();
            if seen_keys.contains(&stripped) {
                ctx.diagnostics.push(Diagnostic {
                    line: line_num,
                    message: format!("duplicate attribute '{}'", attr.key),
                    severity: Severity::Warning,
                    source_line: None,
                });
            } else {
                seen_keys.push(stripped);
            }

            // Color validation for hex colors
            if matches!(strip_all_prefixes(&attr.key), "background" | "color") {
                if let Some(ref val) = attr.value {
                    if val.starts_with('#') && !is_valid_hex_color(val) {
                        ctx.diagnostics.push(Diagnostic {
                            line: line_num,
                            message: format!("invalid hex color '{}'", val),
                            severity: Severity::Warning,
                            source_line: None,
                        });
                    }
                }
            }
        }

        // Warn on unknown attributes
        if validate {
            let base_key = strip_all_prefixes(attr.key.as_str());
            if !KNOWN_ATTRS.contains(&base_key)
                && !base_key.starts_with("aria-")
                && !base_key.starts_with("data-")
            {
                let suggestion = suggest_closest(base_key, KNOWN_ATTRS);
                let msg = match suggestion {
                    Some(closest) => {
                        format!("unknown attribute '{}', did you mean '{}'?", attr.key, closest)
                    }
                    None => format!("unknown attribute '{}'", attr.key),
                };
                ctx.diagnostics.push(Diagnostic {
                    line: line_num,
                    message: msg,
                    severity: Severity::Warning,
                    source_line: None,
                });
            } else {
                validate_attr_value(&attr, line_num, ctx);
            }
        }

        attrs.push(attr);
    }

    attrs
}

fn split_commas(input: &str) -> Vec<&str> {
    let mut parts = Vec::new();
    let mut start = 0;
    let mut depth = 0;

    for (i, c) in input.char_indices() {
        match c {
            '[' | '(' => depth += 1,
            ']' | ')' => depth -= 1,
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

fn parse_text_segments(input: &str, ctx: &mut ParseContext) -> Vec<TextSegment> {
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

// ---------------------------------------------------------------------------
// Condition evaluation for @if
// ---------------------------------------------------------------------------

fn evaluate_condition(condition: &str) -> bool {
    if let Some((left, right)) = condition.split_once("!=") {
        left.trim() != right.trim()
    } else if let Some((left, right)) = condition.split_once("==") {
        left.trim() == right.trim()
    } else {
        // Truthy check: non-empty, not "false", not "0"
        let trimmed = condition.trim();
        !trimmed.is_empty() && trimmed != "false" && trimmed != "0"
    }
}

/// Evaluate `if(condition, true_val, false_val)` expressions in attribute values.
fn evaluate_if_expr(input: &str) -> String {
    let trimmed = input.trim();
    if !trimmed.starts_with("if(") || !trimmed.ends_with(')') {
        return input.to_string();
    }
    let inner = &trimmed[3..trimmed.len() - 1];
    let parts = split_if_args(inner);
    if parts.len() != 3 {
        return input.to_string();
    }
    let condition = parts[0].trim();
    let true_val = parts[1].trim();
    let false_val = parts[2].trim();

    if evaluate_condition(condition) {
        true_val.to_string()
    } else {
        false_val.to_string()
    }
}

fn split_if_args(input: &str) -> Vec<&str> {
    let mut parts = Vec::new();
    let mut start = 0;
    let mut depth = 0;
    for (i, c) in input.char_indices() {
        match c {
            '(' => depth += 1,
            ')' => depth -= 1,
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

fn evaluate_arithmetic(input: &str) -> String {
    let input = input.trim();
    for op in &[" * ", " / ", " + ", " - "] {
        if let Some((left, right)) = input.split_once(op) {
            let left = left.trim();
            let right = right.trim();
            let op_char = op.trim().chars().next().unwrap();
            if let (Ok(l), Ok(r)) = (left.parse::<f64>(), right.parse::<f64>()) {
                let result = match op_char {
                    '+' => l + r,
                    '-' => l - r,
                    '*' => l * r,
                    '/' => if r != 0.0 { l / r } else { return input.to_string() },
                    _ => return input.to_string(),
                };
                if result == result.floor() && result.abs() < i64::MAX as f64 {
                    return format!("{}", result as i64);
                }
                return format!("{}", result);
            }
        }
    }
    input.to_string()
}

// ---------------------------------------------------------------------------
// Post-parse validation (context-dependent warnings)
// ---------------------------------------------------------------------------

fn strip_all_prefixes(key: &str) -> &str {
    // Strip pseudo-state prefixes
    let key = key
        .strip_prefix("hover:")
        .or_else(|| key.strip_prefix("active:"))
        .or_else(|| key.strip_prefix("focus:"))
        .or_else(|| key.strip_prefix("focus-visible:"))
        .or_else(|| key.strip_prefix("focus-within:"))
        .or_else(|| key.strip_prefix("disabled:"))
        .or_else(|| key.strip_prefix("checked:"))
        .or_else(|| key.strip_prefix("placeholder:"))
        .or_else(|| key.strip_prefix("first:"))
        .or_else(|| key.strip_prefix("last:"))
        .or_else(|| key.strip_prefix("odd:"))
        .or_else(|| key.strip_prefix("even:"))
        .unwrap_or(key);
    // Strip responsive prefixes
    let key = key.strip_prefix("sm:")
        .or_else(|| key.strip_prefix("md:"))
        .or_else(|| key.strip_prefix("lg:"))
        .or_else(|| key.strip_prefix("xl:"))
        .or_else(|| key.strip_prefix("2xl:"))
        .unwrap_or(key);
    // Strip media prefixes
    let key = key.strip_prefix("dark:")
        .or_else(|| key.strip_prefix("print:"))
        .unwrap_or(key);
    let key = key.strip_prefix("motion-safe:")
        .or_else(|| key.strip_prefix("motion-reduce:"))
        .or_else(|| key.strip_prefix("landscape:"))
        .or_else(|| key.strip_prefix("portrait:"))
        .unwrap_or(key);
    key
}

/// Attributes that only make sense on container elements (@row, @column, @el).
const CONTAINER_ONLY_ATTRS: &[&str] = &[
    "spacing", "gap", "gap-x", "gap-y", "wrap",
    "grid", "grid-cols", "grid-rows",
    "container", "container-name", "container-type",
];

fn element_kind_name(kind: &ElementKind) -> &'static str {
    match kind {
        ElementKind::Row => "@row",
        ElementKind::Column => "@column",
        ElementKind::El => "@el",
        ElementKind::Text => "@text",
        ElementKind::Paragraph => "@paragraph",
        ElementKind::Image => "@image",
        ElementKind::Link => "@link",
        ElementKind::Children => "@children",
        ElementKind::Input => "@input",
        ElementKind::Button => "@button",
        ElementKind::Select => "@select",
        ElementKind::Textarea => "@textarea",
        ElementKind::Option => "@option",
        ElementKind::Label => "@label",
        ElementKind::Slot(_) => "@slot",
        ElementKind::Nav => "@nav",
        ElementKind::Header => "@header",
        ElementKind::Footer => "@footer",
        ElementKind::Main => "@main",
        ElementKind::Section => "@section",
        ElementKind::Article => "@article",
        ElementKind::Aside => "@aside",
        ElementKind::List => "@list",
        ElementKind::ListItem => "@item",
        ElementKind::Table => "@table",
        ElementKind::TableHead => "@thead",
        ElementKind::TableBody => "@tbody",
        ElementKind::TableRow => "@tr",
        ElementKind::TableCell => "@td",
        ElementKind::TableHeaderCell => "@th",
        ElementKind::Video => "@video",
        ElementKind::Audio => "@audio",
        ElementKind::Form => "@form",
        ElementKind::Details => "@details",
        ElementKind::Summary => "@summary",
        ElementKind::Blockquote => "@blockquote",
        ElementKind::Cite => "@cite",
        ElementKind::Code => "@code",
        ElementKind::Pre => "@pre",
        ElementKind::HorizontalRule => "@hr",
        ElementKind::Figure => "@figure",
        ElementKind::FigCaption => "@figcaption",
        ElementKind::Progress => "@progress",
        ElementKind::Meter => "@meter",
        ElementKind::Fragment => "@fragment",
        ElementKind::Dialog => "@dialog",
        ElementKind::DefinitionList => "@dl",
        ElementKind::DefinitionTerm => "@dt",
        ElementKind::DefinitionDescription => "@dd",
        ElementKind::Fieldset => "@fieldset",
        ElementKind::Legend => "@legend",
        ElementKind::Picture => "@picture",
        ElementKind::Source => "@source",
        ElementKind::Time => "@time",
        ElementKind::Mark => "@mark",
        ElementKind::Kbd => "@kbd",
        ElementKind::Abbr => "@abbr",
        ElementKind::Datalist => "@datalist",
        ElementKind::Iframe => "@iframe",
        ElementKind::Output => "@output",
        ElementKind::Canvas => "@canvas",
    }
}

fn is_container(kind: &ElementKind) -> bool {
    matches!(kind,
        ElementKind::Row | ElementKind::Column | ElementKind::El
        | ElementKind::Nav | ElementKind::Header | ElementKind::Footer
        | ElementKind::Main | ElementKind::Section | ElementKind::Article
        | ElementKind::Aside | ElementKind::List | ElementKind::ListItem
        | ElementKind::Form | ElementKind::Details | ElementKind::Figure
        | ElementKind::Blockquote
        | ElementKind::Dialog | ElementKind::DefinitionList | ElementKind::DefinitionTerm
        | ElementKind::DefinitionDescription | ElementKind::Fieldset | ElementKind::Datalist
        | ElementKind::Iframe | ElementKind::Canvas
    )
}

fn validate_tree(
    nodes: &[Node],
    parent_kind: Option<&ElementKind>,
    diagnostics: &mut Vec<Diagnostic>,
) {
    for node in nodes {
        if let Node::Element(elem) = node {
            for attr in &elem.attrs {
                let base = strip_all_prefixes(&attr.key);
                if base == "width" && attr.value.as_deref() == Some("fill") {
                    if !matches!(parent_kind, Some(ElementKind::Row)) {
                        diagnostics.push(Diagnostic {
                            line: elem.line_num,
                            message: "'width fill' works best inside @row; using 100% as fallback"
                                .to_string(),
                            severity: Severity::Warning,
                            source_line: None,
                        });
                    }
                }
                if base == "height" && attr.value.as_deref() == Some("fill") {
                    if !matches!(parent_kind, Some(ElementKind::Column)) {
                        diagnostics.push(Diagnostic {
                            line: elem.line_num,
                            message:
                                "'height fill' works best inside @column; using 100% as fallback"
                                    .to_string(),
                            severity: Severity::Warning,
                            source_line: None,
                        });
                    }
                }

                // Container-only attributes on non-container elements
                if CONTAINER_ONLY_ATTRS.contains(&base) && !is_container(&elem.kind) {
                    diagnostics.push(Diagnostic {
                        line: elem.line_num,
                        message: format!(
                            "'{}' has no effect on {} (only works on @row, @column, @el)",
                            base, element_kind_name(&elem.kind)
                        ),
                        severity: Severity::Warning,
                        source_line: None,
                    });
                }

                // Form-specific: placeholder only on @input/@textarea
                if base == "placeholder"
                    && !matches!(elem.kind, ElementKind::Input | ElementKind::Textarea)
                {
                    diagnostics.push(Diagnostic {
                        line: elem.line_num,
                        message: format!(
                            "'placeholder' has no effect on {} (only works on @input, @textarea)",
                            element_kind_name(&elem.kind)
                        ),
                        severity: Severity::Warning,
                        source_line: None,
                    });
                }

                // 'for' only on @label
                if base == "for" && !matches!(elem.kind, ElementKind::Label) {
                    diagnostics.push(Diagnostic {
                        line: elem.line_num,
                        message: format!(
                            "'for' has no effect on {} (only works on @label)",
                            element_kind_name(&elem.kind)
                        ),
                        severity: Severity::Warning,
                        source_line: None,
                    });
                }

                // 'rows'/'cols' only on @textarea
                if (base == "rows" || base == "cols")
                    && !matches!(elem.kind, ElementKind::Textarea)
                {
                    diagnostics.push(Diagnostic {
                        line: elem.line_num,
                        message: format!(
                            "'{}' has no effect on {} (only works on @textarea)",
                            base, element_kind_name(&elem.kind)
                        ),
                        severity: Severity::Warning,
                        source_line: None,
                    });
                }

                // 'ordered' only on @list
                if base == "ordered" && !matches!(elem.kind, ElementKind::List) {
                    diagnostics.push(Diagnostic {
                        line: elem.line_num,
                        message: format!(
                            "'ordered' has no effect on {} (only works on @list)",
                            element_kind_name(&elem.kind)
                        ),
                        severity: Severity::Warning,
                        source_line: None,
                    });
                }

                // Media-specific attributes only on @video/@audio
                if matches!(base, "controls" | "autoplay" | "loop" | "muted" | "poster" | "preload")
                    && !matches!(elem.kind, ElementKind::Video | ElementKind::Audio)
                {
                    diagnostics.push(Diagnostic {
                        line: elem.line_num,
                        message: format!(
                            "'{}' has no effect on {} (only works on @video, @audio)",
                            base, element_kind_name(&elem.kind)
                        ),
                        severity: Severity::Warning,
                        source_line: None,
                    });
                }
            }
            // Missing alt text on @image
            if matches!(elem.kind, ElementKind::Image) {
                if !elem.attrs.iter().any(|a| a.key == "alt") {
                    diagnostics.push(Diagnostic {
                        line: elem.line_num,
                        message: "@image missing 'alt' attribute (accessibility)".to_string(),
                        severity: Severity::Warning,
                        source_line: None,
                    });
                }
            }
            if matches!(elem.kind, ElementKind::Input) {
                if !elem.attrs.iter().any(|a| a.key == "type") {
                    diagnostics.push(Diagnostic {
                        line: elem.line_num,
                        message: "@input missing 'type' attribute (defaults to 'text')".to_string(),
                        severity: Severity::Warning,
                        source_line: None,
                    });
                }
            }
            if matches!(elem.kind, ElementKind::Link) {
                // For @link, argument is the URL, not text content
                let has_text = !elem.children.is_empty();
                if !has_text && !elem.attrs.iter().any(|a| a.key == "aria-label" || a.key == "title") {
                    diagnostics.push(Diagnostic {
                        line: elem.line_num,
                        message: "@link has no visible text or aria-label (accessibility)".to_string(),
                        severity: Severity::Warning,
                        source_line: None,
                    });
                }
            }

            validate_tree(&elem.children, Some(&elem.kind), diagnostics);
        }
    }
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
            && (chars[i + 1].is_alphanumeric() || chars[i + 1] == '_' || chars[i + 1] == '-')
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

// ---------------------------------------------------------------------------
// Variable usage tracking
// ---------------------------------------------------------------------------

/// Scan a string for $name references and record them in the used set.
fn track_var_refs(input: &str, used: &mut HashSet<String>) {
    if !input.contains('$') {
        return;
    }
    let chars: Vec<char> = input.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        if chars[i] == '$'
            && i + 1 < chars.len()
            && (chars[i + 1].is_alphanumeric() || chars[i + 1] == '_' || chars[i + 1] == '-')
        {
            let start = i + 1;
            let mut end = start;
            while end < chars.len()
                && (chars[end].is_alphanumeric() || chars[end] == '-' || chars[end] == '_')
            {
                end += 1;
            }
            let name: String = chars[start..end].iter().collect();
            used.insert(name);
            i = end;
        } else {
            i += 1;
        }
    }
}

// ---------------------------------------------------------------------------
// Unused definition warnings
// ---------------------------------------------------------------------------

fn check_unused(ctx: &mut ParseContext) {
    // Check unused @let variables
    for (name, &line) in &ctx.let_lines {
        if name.starts_with("--") {
            continue; // CSS vars are always used
        }
        if !ctx.used_variables.contains(name) {
            ctx.diagnostics.push(Diagnostic {
                line,
                message: format!("unused variable '${}' (defined but never referenced)", name),
                severity: Severity::Warning,
                source_line: None,
            });
        }
    }

    // Check unused @define bundles
    for (name, &line) in &ctx.define_lines {
        if !ctx.used_defines.contains(name) {
            ctx.diagnostics.push(Diagnostic {
                line,
                message: format!("unused define '${}' (defined but never referenced)", name),
                severity: Severity::Warning,
                source_line: None,
            });
        }
    }

    // Check unused @fn functions
    for (name, &line) in &ctx.fn_lines {
        if !ctx.used_functions.contains(name) {
            ctx.diagnostics.push(Diagnostic {
                line,
                message: format!("unused function '@{}' (defined but never called)", name),
                severity: Severity::Warning,
                source_line: None,
            });
        }
    }
}
