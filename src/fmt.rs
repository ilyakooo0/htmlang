const MAX_LINE_WIDTH: usize = 100;
/// Once a line has already been wrapped over multiple attribute lines, we keep it
/// wrapped even after formatting as long as the attribute count stays >1. This
/// prevents format→format→format oscillation around the wrap threshold.
const WRAP_MIN_WIDTH: usize = 80;

/// Attribute category for sorting.
fn attr_category(key: &str) -> u8 {
    // Strip pseudo-state/responsive/media prefixes for categorization
    let base = key
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
        .or_else(|| key.strip_prefix("sm:"))
        .or_else(|| key.strip_prefix("md:"))
        .or_else(|| key.strip_prefix("lg:"))
        .or_else(|| key.strip_prefix("xl:"))
        .or_else(|| key.strip_prefix("dark:"))
        .or_else(|| key.strip_prefix("print:"))
        .or_else(|| key.strip_prefix("2xl:"))
        .or_else(|| key.strip_prefix("motion-safe:"))
        .or_else(|| key.strip_prefix("motion-reduce:"))
        .or_else(|| key.strip_prefix("landscape:"))
        .or_else(|| key.strip_prefix("portrait:"))
        .or_else(|| key.strip_prefix("visited:"))
        .or_else(|| key.strip_prefix("empty:"))
        .or_else(|| key.strip_prefix("target:"))
        .or_else(|| key.strip_prefix("valid:"))
        .or_else(|| key.strip_prefix("invalid:"))
        .unwrap_or(key);

    match base {
        // Layout (parent)
        "spacing" | "gap" | "gap-x" | "gap-y" | "wrap"
        | "grid" | "grid-cols" | "grid-rows"
        | "column-count" | "column-gap" | "column-width" | "column-rule" => 0,
        // Sizing
        "width" | "height" | "min-width" | "max-width" | "min-height" | "max-height"
        | "flex-grow" | "flex-shrink" | "flex-basis" => 1,
        // Padding & margin
        "padding" | "padding-x" | "padding-y" | "padding-inline" | "padding-block"
        | "margin" | "margin-x" | "margin-y" | "margin-inline" | "margin-block" => 2,
        // Alignment
        "center-x" | "center-y" | "align-left" | "align-right"
        | "align-top" | "align-bottom" | "justify-content" | "align-items"
        | "place-items" | "place-self" | "place-content" => 3,
        // Positioning
        "position" | "top" | "right" | "bottom" | "left" | "z-index" | "order" | "inset"
        | "col-span" | "row-span" | "display" | "visibility"
        | "overflow" | "overflow-x" | "overflow-y" | "hidden" => 4,
        // Visual style
        "background" | "background-size" | "background-position" | "background-repeat"
        | "color" | "opacity" | "accent-color" | "caret-color"
        | "background-image" | "background-blend-mode" | "mix-blend-mode" | "isolation" => 5,
        // Border & shape
        "border" | "border-top" | "border-bottom" | "border-left" | "border-right"
        | "rounded" | "outline" | "shadow" | "text-shadow"
        | "border-collapse" | "border-spacing" => 6,
        // Typography
        "bold" | "italic" | "underline" | "size" | "font"
        | "text-align" | "line-height" | "letter-spacing" | "text-transform"
        | "white-space" | "text-overflow" | "word-break" | "overflow-wrap"
        | "text-decoration" | "text-decoration-color" | "text-decoration-thickness"
        | "text-decoration-style" | "text-underline-offset" | "list-style"
        | "text-indent" | "hyphens" | "writing-mode" => 7,
        // Effects & interaction
        "transform" | "transition" | "animation" | "cursor" | "backdrop-filter"
        | "filter" | "pointer-events" | "user-select" | "aspect-ratio"
        | "object-fit" | "object-position" | "resize"
        | "clip-path" => 8,
        // Container queries & scroll
        "container" | "container-name" | "container-type"
        | "scroll-snap-type" | "scroll-snap-align" | "scroll-behavior" => 9,
        // Identity
        "id" | "class" => 10,
        // HTML passthrough / form / accessibility
        _ => 11,
    }
}

/// Sort a comma-separated attribute string by category.
fn sort_attrs(attrs_str: &str) -> String {
    let mut parts: Vec<&str> = attrs_str.split(',').collect();
    // Preserve order within same category (stable sort)
    parts.sort_by_key(|part| {
        let key = part.split_whitespace().next().unwrap_or("");
        attr_category(key)
    });
    parts.iter()
        .map(|p| p.trim())
        .filter(|p| !p.is_empty())
        .collect::<Vec<_>>()
        .join(", ")
}

/// Count unescaped `[` and `]` outside quoted strings. Handles both `"..."` and
/// `'...'` with `\"` / `\'` escapes. Returns (open_count, close_count).
fn count_brackets(s: &str) -> (i32, i32) {
    let mut open = 0i32;
    let mut close = 0i32;
    let mut chars = s.chars().peekable();
    let mut in_str: Option<char> = None;
    while let Some(c) = chars.next() {
        match in_str {
            Some(q) => {
                if c == '\\' {
                    // consume next char (escape)
                    chars.next();
                } else if c == q {
                    in_str = None;
                }
            }
            None => match c {
                '"' | '\'' => in_str = Some(c),
                '[' => open += 1,
                ']' => close += 1,
                _ => {}
            },
        }
    }
    (open, close)
}

/// Split a line into (code, trailing_comment). The comment starts at the first `--`
/// that lies outside quoted strings and outside `[...]` attribute lists. Returns
/// (whole_line, None) if there is no trailing comment.
fn split_trailing_comment(line: &str) -> (&str, Option<&str>) {
    let bytes = line.as_bytes();
    let mut in_str: Option<u8> = None;
    let mut bracket_depth = 0i32;
    let mut i = 0;
    while i + 1 < bytes.len() {
        let c = bytes[i];
        match in_str {
            Some(q) => {
                if c == b'\\' {
                    i += 2;
                    continue;
                } else if c == q {
                    in_str = None;
                }
            }
            None => match c {
                b'"' | b'\'' => in_str = Some(c),
                b'[' => bracket_depth += 1,
                b']' => bracket_depth -= 1,
                b'-' if bytes[i + 1] == b'-' && bracket_depth == 0 => {
                    // Require a space (or start-of-line) before the `--` so we
                    // don't split inside identifiers or CSS values like `a--b`.
                    if i == 0 || bytes[i - 1] == b' ' || bytes[i - 1] == b'\t' {
                        let (before, after) = line.split_at(i);
                        return (before.trim_end(), Some(after));
                    }
                }
                _ => {}
            },
        }
        i += 1;
    }
    (line, None)
}

/// Format an htmlang source file with normalized indentation (2 spaces per level),
/// sorted attributes, and cleaned-up whitespace.
pub fn format(input: &str) -> String {
    let mut output = String::new();
    let mut indent_stack: Vec<i32> = vec![-1]; // sentinel
    let mut bracket_depth: i32 = 0;
    let mut bracket_base_level: usize = 0;
    let mut bracket_content = String::new();
    let mut bracket_was_wrapped = false;
    let mut pending_comment: Option<String> = None;

    for line in input.lines() {
        let trimmed = line.trim();

        // Preserve blank lines
        if trimmed.is_empty() {
            output.push('\n');
            continue;
        }

        // Preserve comments at current indent level
        if trimmed.starts_with("--") {
            let raw_indent = (line.len() - line.trim_start().len()) as i32;
            while indent_stack.len() > 1 && *indent_stack.last().unwrap() >= raw_indent {
                indent_stack.pop();
            }
            let level = indent_stack.len() - 1;
            output.push_str(&"  ".repeat(level));
            output.push_str(trimmed);
            output.push('\n');
            indent_stack.push(raw_indent);
            continue;
        }

        // Inside multi-line bracket continuation — collect content (no comment
        // handling here; trailing comments inside a multi-line bracket block are
        // uncommon and intentionally preserved by the collector).
        if bracket_depth > 0 {
            bracket_content.push(' ');
            bracket_content.push_str(trimmed);
            let (open, close) = count_brackets(trimmed);
            bracket_depth += open - close;
            if bracket_depth <= 0 {
                // Bracket block is complete — format it
                let level = bracket_base_level;
                let full = bracket_content.clone();
                bracket_content.clear();
                let formatted = format_line_with_brackets(&full);
                let attrs_count = count_attrs(&formatted);
                let indented_len = level * 2 + formatted.len();
                // Stay wrapped if the original was wrapped (hysteresis).
                let should_wrap = (indented_len > MAX_LINE_WIDTH)
                    || (bracket_was_wrapped
                        && attrs_count > 1
                        && indented_len > WRAP_MIN_WIDTH);
                if should_wrap
                    && let Some(wrapped) = wrap_attrs(&formatted, level) {
                        output.push_str(&wrapped);
                        bracket_was_wrapped = false;
                        continue;
                    }
                output.push_str(&"  ".repeat(level));
                output.push_str(&formatted);
                if let Some(cmt) = pending_comment.take() {
                    output.push(' ');
                    output.push_str(&cmt);
                }
                output.push('\n');
                bracket_was_wrapped = false;
            }
            continue;
        }

        let raw_indent = (line.len() - line.trim_start().len()) as i32;

        // Pop stack to find parent
        while indent_stack.len() > 1 && *indent_stack.last().unwrap() >= raw_indent {
            indent_stack.pop();
        }

        let level = indent_stack.len() - 1;

        // Split off a trailing `-- comment` before doing bracket math / sorting.
        let (code, trailing) = split_trailing_comment(trimmed);
        let code = code.trim_end();
        let trailing = trailing.map(|s| s.trim().to_string());

        let (open, close) = count_brackets(code);
        bracket_depth = open - close;
        if bracket_depth > 0 {
            // Start of multi-line bracket
            bracket_base_level = level;
            bracket_content = code.to_string();
            bracket_was_wrapped = true;
            pending_comment = trailing;
            indent_stack.push(raw_indent);
            continue;
        }

        // Single-line: format brackets inline
        let formatted = format_line_with_brackets(code);
        let attrs_count = count_attrs(&formatted);

        let indented_len = level * 2 + formatted.len();
        // Single-line emission — only wrap when we strictly exceed the ceiling.
        if indented_len > MAX_LINE_WIDTH && formatted.contains('[')
            && let Some(mut wrapped) = wrap_attrs(&formatted, level) {
                if let Some(cmt) = &trailing {
                    // Reattach comment on the final `]` line.
                    if wrapped.ends_with('\n') {
                        wrapped.pop();
                    }
                    wrapped.push(' ');
                    wrapped.push_str(cmt);
                    wrapped.push('\n');
                }
                output.push_str(&wrapped);
                indent_stack.push(raw_indent);
                continue;
            }
        let _ = attrs_count;

        output.push_str(&"  ".repeat(level));
        output.push_str(&formatted);
        if let Some(cmt) = trailing {
            output.push(' ');
            output.push_str(&cmt);
        }
        output.push('\n');

        indent_stack.push(raw_indent);
    }

    output
}

/// Format a single line, sorting attributes inside [...] brackets.
fn format_line_with_brackets(line: &str) -> String {
    // Find bracket boundaries
    let Some(bracket_start) = line.find('[') else {
        return line.to_string();
    };
    let mut depth = 0;
    let mut bracket_end = None;
    let mut in_str: Option<char> = None;
    let mut prev = '\0';
    for (i, c) in line.char_indices() {
        if let Some(q) = in_str {
            if prev != '\\' && c == q {
                in_str = None;
            }
        } else {
            match c {
                '"' | '\'' => in_str = Some(c),
                '[' => depth += 1,
                ']' => {
                    depth -= 1;
                    if depth == 0 {
                        bracket_end = Some(i);
                        break;
                    }
                }
                _ => {}
            }
        }
        prev = c;
    }
    let Some(bracket_end) = bracket_end else {
        return line.to_string();
    };

    let before = &line[..bracket_start];
    let attrs_inner = &line[bracket_start + 1..bracket_end];
    let after = &line[bracket_end + 1..];

    let sorted = sort_attrs(attrs_inner);

    format!("{}[{}]{}", before, sorted, after)
}

fn count_attrs(line: &str) -> usize {
    let Some(start) = line.find('[') else { return 0 };
    let Some(end) = line.rfind(']') else { return 0 };
    if end <= start {
        return 0;
    }
    line[start + 1..end]
        .split(',')
        .map(|p| p.trim())
        .filter(|p| !p.is_empty())
        .count()
}

fn wrap_attrs(line: &str, indent_level: usize) -> Option<String> {
    let bracket_start = line.find('[')?;
    let mut depth = 0;
    let mut bracket_end = None;
    let mut in_str: Option<char> = None;
    let mut prev = '\0';
    for (i, c) in line.char_indices() {
        if let Some(q) = in_str {
            if prev != '\\' && c == q {
                in_str = None;
            }
        } else {
            match c {
                '"' | '\'' => in_str = Some(c),
                '[' => depth += 1,
                ']' => {
                    depth -= 1;
                    if depth == 0 {
                        bracket_end = Some(i);
                        break;
                    }
                }
                _ => {}
            }
        }
        prev = c;
    }
    let bracket_end = bracket_end?;

    let before = &line[..bracket_start];
    let attrs_inner = &line[bracket_start + 1..bracket_end];
    let after = &line[bracket_end + 1..];

    let parts: Vec<&str> = attrs_inner.split(',').map(|s| s.trim()).filter(|s| !s.is_empty()).collect();
    if parts.len() <= 1 {
        return None;
    }

    let base_indent = "  ".repeat(indent_level);
    let attr_indent = "  ".repeat(indent_level + 1);
    let mut result = String::new();
    result.push_str(&base_indent);
    result.push_str(before);
    result.push_str("[\n");
    for (i, part) in parts.iter().enumerate() {
        result.push_str(&attr_indent);
        result.push_str(part);
        if i < parts.len() - 1 {
            result.push(',');
        }
        result.push('\n');
    }
    result.push_str(&base_indent);
    result.push(']');
    result.push_str(after);
    result.push('\n');
    Some(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn idempotent_short_line() {
        let src = "@row [spacing 10]\n  Hi\n";
        assert_eq!(format(src), format(&format(src)));
    }

    #[test]
    fn idempotent_at_wrap_threshold() {
        // Build a line long enough to cross MAX_LINE_WIDTH after the first format.
        let src = "@row [padding 20, background white, rounded 8, border 1 #e5e7eb, color #333, shadow 0 2px 4px rgba(0,0,0,0.1)]\n  child\n";
        let a = format(src);
        let b = format(&a);
        assert_eq!(a, b, "formatter must be idempotent across runs");
    }

    #[test]
    fn preserves_trailing_comment() {
        let src = "@text [bold] hello -- greeting\n";
        let out = format(src);
        assert!(out.contains("-- greeting"), "trailing comment should survive format: {out}");
    }

    #[test]
    fn bracket_inside_string_is_ignored() {
        let src = "@text [content \"a [b] c\", bold] hi\n";
        let out = format(src);
        // Single-line emission — no multi-line bracket block should be triggered.
        assert_eq!(out.lines().count(), 1, "got:\n{out}");
    }

    #[test]
    fn comments_inside_brackets_are_not_split() {
        let src = "@el [content \"--not a comment\"]\n";
        let out = format(src);
        assert!(out.contains("\"--not a comment\""));
    }
}
