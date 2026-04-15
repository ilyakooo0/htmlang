/// Format an htmlang source file with normalized indentation (2 spaces per level).
pub fn format(input: &str) -> String {
    let mut output = String::new();
    let mut indent_stack: Vec<i32> = vec![-1]; // sentinel
    let mut bracket_depth: i32 = 0;
    let mut bracket_base_level: usize = 0;

    for line in input.lines() {
        let trimmed = line.trim();

        // Preserve blank lines
        if trimmed.is_empty() {
            output.push('\n');
            continue;
        }

        let open = trimmed.chars().filter(|&c| c == '[').count() as i32;
        let close = trimmed.chars().filter(|&c| c == ']').count() as i32;

        // Inside multi-line bracket continuation
        if bracket_depth > 0 {
            let level = bracket_base_level + 1;
            output.push_str(&"  ".repeat(level));
            output.push_str(trimmed);
            output.push('\n');
            bracket_depth += open - close;
            continue;
        }

        let raw_indent = (line.len() - line.trim_start().len()) as i32;

        // Pop stack to find parent
        while indent_stack.len() > 1 && *indent_stack.last().unwrap() >= raw_indent {
            indent_stack.pop();
        }

        let level = indent_stack.len() - 1;
        output.push_str(&"  ".repeat(level));
        output.push_str(trimmed);
        output.push('\n');

        indent_stack.push(raw_indent);

        bracket_depth = open - close;
        if bracket_depth > 0 {
            bracket_base_level = level;
        }
    }

    output
}
