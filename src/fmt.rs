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
        .unwrap_or(key);

    match base {
        // Layout (parent)
        "spacing" | "gap" | "gap-x" | "gap-y" | "wrap"
        | "grid" | "grid-cols" | "grid-rows" => 0,
        // Sizing
        "width" | "height" | "min-width" | "max-width" | "min-height" | "max-height" => 1,
        // Padding & margin
        "padding" | "padding-x" | "padding-y" | "padding-inline" | "padding-block"
        | "margin" | "margin-x" | "margin-y" | "margin-inline" | "margin-block" => 2,
        // Alignment
        "center-x" | "center-y" | "align-left" | "align-right"
        | "align-top" | "align-bottom" | "justify-content" | "align-items"
        | "place-items" | "place-self" => 3,
        // Positioning
        "position" | "top" | "right" | "bottom" | "left" | "z-index" | "order" | "inset"
        | "col-span" | "row-span" | "display" | "visibility"
        | "overflow" | "overflow-x" | "overflow-y" | "hidden" => 4,
        // Visual style
        "background" | "background-size" | "background-position" | "background-repeat"
        | "color" | "opacity" | "accent-color" | "caret-color" => 5,
        // Border & shape
        "border" | "border-top" | "border-bottom" | "border-left" | "border-right"
        | "rounded" | "outline" | "shadow" | "text-shadow"
        | "border-collapse" | "border-spacing" => 6,
        // Typography
        "bold" | "italic" | "underline" | "size" | "font"
        | "text-align" | "line-height" | "letter-spacing" | "text-transform"
        | "white-space" | "text-overflow" | "word-break" | "overflow-wrap"
        | "text-decoration" | "text-decoration-color" | "text-decoration-thickness"
        | "text-decoration-style" | "list-style" => 7,
        // Effects & interaction
        "transform" | "transition" | "animation" | "cursor" | "backdrop-filter"
        | "filter" | "pointer-events" | "user-select" | "aspect-ratio"
        | "object-fit" | "object-position" | "resize" => 8,
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
        let key = part.trim().split_whitespace().next().unwrap_or("");
        attr_category(key)
    });
    parts.iter()
        .map(|p| p.trim())
        .filter(|p| !p.is_empty())
        .collect::<Vec<_>>()
        .join(", ")
}

/// Format an htmlang source file with normalized indentation (2 spaces per level),
/// sorted attributes, and cleaned-up whitespace.
pub fn format(input: &str) -> String {
    let mut output = String::new();
    let mut indent_stack: Vec<i32> = vec![-1]; // sentinel
    let mut bracket_depth: i32 = 0;
    let mut bracket_base_level: usize = 0;
    let mut bracket_content = String::new();

    for line in input.lines() {
        let trimmed = line.trim();

        // Preserve blank lines
        if trimmed.is_empty() {
            output.push('\n');
            continue;
        }

        let open = trimmed.chars().filter(|&c| c == '[').count() as i32;
        let close = trimmed.chars().filter(|&c| c == ']').count() as i32;

        // Inside multi-line bracket continuation — collect content
        if bracket_depth > 0 {
            bracket_content.push(' ');
            bracket_content.push_str(trimmed);
            bracket_depth += open - close;
            if bracket_depth <= 0 {
                // Bracket block is complete — format it
                let level = bracket_base_level;
                // Find the bracket content within the full collected line
                let full = bracket_content.clone();
                bracket_content.clear();
                // Re-emit the line with sorted attrs
                let formatted = format_line_with_brackets(&full);
                output.push_str(&"  ".repeat(level));
                output.push_str(&formatted);
                output.push('\n');
            }
            continue;
        }

        let raw_indent = (line.len() - line.trim_start().len()) as i32;

        // Pop stack to find parent
        while indent_stack.len() > 1 && *indent_stack.last().unwrap() >= raw_indent {
            indent_stack.pop();
        }

        let level = indent_stack.len() - 1;

        bracket_depth = open - close;
        if bracket_depth > 0 {
            // Start of multi-line bracket
            bracket_base_level = level;
            bracket_content = trimmed.to_string();
            indent_stack.push(raw_indent);
            continue;
        }

        // Single-line: format brackets inline
        let formatted = format_line_with_brackets(trimmed);
        output.push_str(&"  ".repeat(level));
        output.push_str(&formatted);
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
    for (i, c) in line.char_indices() {
        match c {
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
    let Some(bracket_end) = bracket_end else {
        return line.to_string();
    };

    let before = &line[..bracket_start];
    let attrs_inner = &line[bracket_start + 1..bracket_end];
    let after = &line[bracket_end + 1..];

    let sorted = sort_attrs(attrs_inner);

    format!("{}[{}]{}", before, sorted, after)
}
