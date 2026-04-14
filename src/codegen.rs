use crate::ast::*;

pub fn generate(doc: &Document) -> String {
    let mut body = String::new();
    for node in &doc.nodes {
        generate_node(node, None, &mut body);
    }

    match &doc.page_title {
        Some(title) => format!(
            "\
<!DOCTYPE html>
<html>
<head>
<meta charset=\"utf-8\">
<meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">
<title>{title}</title>
<style>
*, *::before, *::after {{ box-sizing: border-box; }}
body {{ margin: 0; font-family: system-ui, -apple-system, sans-serif; }}
img {{ display: block; }}
</style>
</head>
<body>
{body}\
</body>
</html>
",
            title = html_escape(title),
            body = body,
        ),
        None => body,
    }
}

fn generate_node(node: &Node, parent_kind: Option<&ElementKind>, out: &mut String) {
    match node {
        Node::Element(elem) => generate_element(elem, parent_kind, out),
        Node::Text(segments) => {
            let needs_wrap = matches!(
                parent_kind,
                Some(ElementKind::Row) | Some(ElementKind::Column) | Some(ElementKind::El)
            );
            if needs_wrap {
                out.push_str("<span>");
            }
            generate_text_segments(segments, out);
            if needs_wrap {
                out.push_str("</span>");
            }
        }
        Node::Raw(content) => {
            out.push_str(content);
            out.push('\n');
        }
    }
}

fn generate_element(elem: &Element, parent_kind: Option<&ElementKind>, out: &mut String) {
    // Image is a void element
    if elem.kind == ElementKind::Image {
        generate_image(elem, parent_kind, out);
        return;
    }

    let tag = match elem.kind {
        ElementKind::Row | ElementKind::Column | ElementKind::El => "div",
        ElementKind::Text => "span",
        ElementKind::Paragraph => "p",
        ElementKind::Link => "a",
        ElementKind::Image => unreachable!(),
    };

    let styles = attrs_to_css(&elem.attrs, &elem.kind, parent_kind);
    let (id, class) = extract_id_class(&elem.attrs);

    out.push('<');
    out.push_str(tag);

    if elem.kind == ElementKind::Link {
        if let Some(url) = &elem.argument {
            out.push_str(" href=\"");
            out.push_str(&html_escape(url));
            out.push('"');
        }
    }

    if !styles.is_empty() {
        out.push_str(" style=\"");
        out.push_str(&styles);
        out.push('"');
    }
    if let Some(id) = id {
        out.push_str(" id=\"");
        out.push_str(&html_escape(&id));
        out.push('"');
    }
    if let Some(class) = class {
        out.push_str(" class=\"");
        out.push_str(&html_escape(&class));
        out.push('"');
    }
    out.push('>');

    // Inline text argument (@text content)
    if elem.kind == ElementKind::Text {
        if let Some(text) = &elem.argument {
            out.push_str(&html_escape(text));
        }
    }

    // Children
    let is_paragraph = elem.kind == ElementKind::Paragraph;
    for (i, child) in elem.children.iter().enumerate() {
        generate_node(child, Some(&elem.kind), out);
        // In paragraphs, add newline between children so HTML renders a space
        if is_paragraph && i < elem.children.len() - 1 {
            out.push('\n');
        }
    }

    out.push_str("</");
    out.push_str(tag);
    out.push_str(">\n");
}

fn generate_image(elem: &Element, parent_kind: Option<&ElementKind>, out: &mut String) {
    let src = elem.argument.as_deref().unwrap_or("");
    let styles = attrs_to_css(&elem.attrs, &elem.kind, parent_kind);
    let (id, class) = extract_id_class(&elem.attrs);

    out.push_str("<img src=\"");
    out.push_str(&html_escape(src));
    out.push('"');

    if !styles.is_empty() {
        out.push_str(" style=\"");
        out.push_str(&styles);
        out.push('"');
    }
    if let Some(id) = id {
        out.push_str(" id=\"");
        out.push_str(&html_escape(&id));
        out.push('"');
    }
    if let Some(class) = class {
        out.push_str(" class=\"");
        out.push_str(&html_escape(&class));
        out.push('"');
    }
    out.push_str(">\n");
}

fn generate_text_segments(segments: &[TextSegment], out: &mut String) {
    for segment in segments {
        match segment {
            TextSegment::Plain(text) => out.push_str(&html_escape(text)),
            TextSegment::Inline(elem) => {
                // Render to buffer and trim trailing newlines for inline context
                let mut buf = String::new();
                generate_element(elem, None, &mut buf);
                out.push_str(buf.trim_end());
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Attribute → CSS mapping
// ---------------------------------------------------------------------------

fn attrs_to_css(
    attrs: &[Attribute],
    kind: &ElementKind,
    parent_kind: Option<&ElementKind>,
) -> String {
    let mut css = String::new();

    // Base styles from element kind
    match kind {
        ElementKind::Row => css.push_str("display:flex;flex-direction:row;"),
        ElementKind::Column => css.push_str("display:flex;flex-direction:column;"),
        ElementKind::El => css.push_str("display:flex;flex-direction:column;"),
        ElementKind::Paragraph => css.push_str("margin:0;"),
        _ => {}
    }

    for attr in attrs {
        let val = attr.value.as_deref();

        match attr.key.as_str() {
            // Layout
            "spacing" => {
                if let Some(v) = val {
                    push_css(&mut css, "gap", &format!("{}px", v));
                }
            }
            "padding" => {
                if let Some(v) = val {
                    let parts: Vec<&str> = v.split_whitespace().collect();
                    match parts.len() {
                        1 => push_css(&mut css, "padding", &format!("{}px", parts[0])),
                        2 => push_css(
                            &mut css,
                            "padding",
                            &format!("{}px {}px", parts[0], parts[1]),
                        ),
                        4 => push_css(
                            &mut css,
                            "padding",
                            &format!(
                                "{}px {}px {}px {}px",
                                parts[0], parts[1], parts[2], parts[3]
                            ),
                        ),
                        _ => {}
                    }
                }
            }
            "padding-x" => {
                if let Some(v) = val {
                    push_css(&mut css, "padding-left", &format!("{}px", v));
                    push_css(&mut css, "padding-right", &format!("{}px", v));
                }
            }
            "padding-y" => {
                if let Some(v) = val {
                    push_css(&mut css, "padding-top", &format!("{}px", v));
                    push_css(&mut css, "padding-bottom", &format!("{}px", v));
                }
            }

            // Sizing
            "width" => {
                if let Some(v) = val {
                    match v {
                        "fill" => match parent_kind {
                            Some(ElementKind::Row) => {
                                push_css(&mut css, "flex", "1");
                                push_css(&mut css, "min-width", "0");
                            }
                            _ => push_css(&mut css, "width", "100%"),
                        },
                        "shrink" => {
                            push_css(&mut css, "flex-shrink", "0");
                        }
                        _ => push_css(&mut css, "width", &format!("{}px", v)),
                    }
                }
            }
            "height" => {
                if let Some(v) = val {
                    match v {
                        "fill" => match parent_kind {
                            Some(ElementKind::Column) => {
                                push_css(&mut css, "flex", "1");
                                push_css(&mut css, "min-height", "0");
                            }
                            _ => push_css(&mut css, "height", "100%"),
                        },
                        "shrink" => {
                            push_css(&mut css, "flex-shrink", "0");
                        }
                        _ => push_css(&mut css, "height", &format!("{}px", v)),
                    }
                }
            }
            "min-width" => {
                if let Some(v) = val {
                    push_css(&mut css, "min-width", &format!("{}px", v));
                }
            }
            "max-width" => {
                if let Some(v) = val {
                    push_css(&mut css, "max-width", &format!("{}px", v));
                }
            }
            "min-height" => {
                if let Some(v) = val {
                    push_css(&mut css, "min-height", &format!("{}px", v));
                }
            }
            "max-height" => {
                if let Some(v) = val {
                    push_css(&mut css, "max-height", &format!("{}px", v));
                }
            }

            // Alignment
            "center-x" => match parent_kind {
                Some(ElementKind::Column) | Some(ElementKind::El) => {
                    push_css(&mut css, "align-self", "center");
                }
                _ => {
                    push_css(&mut css, "margin-left", "auto");
                    push_css(&mut css, "margin-right", "auto");
                }
            },
            "center-y" => match parent_kind {
                Some(ElementKind::Row) => {
                    push_css(&mut css, "align-self", "center");
                }
                _ => {
                    push_css(&mut css, "margin-top", "auto");
                    push_css(&mut css, "margin-bottom", "auto");
                }
            },
            "align-left" => match parent_kind {
                Some(ElementKind::Column) | Some(ElementKind::El) => {
                    push_css(&mut css, "align-self", "flex-start");
                }
                _ => {
                    push_css(&mut css, "margin-right", "auto");
                }
            },
            "align-right" => match parent_kind {
                Some(ElementKind::Column) | Some(ElementKind::El) => {
                    push_css(&mut css, "align-self", "flex-end");
                }
                _ => {
                    push_css(&mut css, "margin-left", "auto");
                }
            },
            "align-top" => match parent_kind {
                Some(ElementKind::Row) => {
                    push_css(&mut css, "align-self", "flex-start");
                }
                _ => {
                    push_css(&mut css, "margin-bottom", "auto");
                }
            },
            "align-bottom" => match parent_kind {
                Some(ElementKind::Row) => {
                    push_css(&mut css, "align-self", "flex-end");
                }
                _ => {
                    push_css(&mut css, "margin-top", "auto");
                }
            },

            // Style
            "background" => {
                if let Some(v) = val {
                    push_css(&mut css, "background", v);
                }
            }
            "color" => {
                if let Some(v) = val {
                    push_css(&mut css, "color", v);
                }
            }
            "border" => {
                if let Some(v) = val {
                    let parts: Vec<&str> = v.splitn(2, ' ').collect();
                    if parts.len() == 2 {
                        push_css(
                            &mut css,
                            "border",
                            &format!("{}px solid {}", parts[0], parts[1]),
                        );
                    } else {
                        push_css(
                            &mut css,
                            "border",
                            &format!("{}px solid currentColor", parts[0]),
                        );
                    }
                }
            }
            "rounded" => {
                if let Some(v) = val {
                    push_css(&mut css, "border-radius", &format!("{}px", v));
                }
            }
            "bold" => push_css(&mut css, "font-weight", "bold"),
            "italic" => push_css(&mut css, "font-style", "italic"),
            "underline" => push_css(&mut css, "text-decoration", "underline"),
            "size" => {
                if let Some(v) = val {
                    push_css(&mut css, "font-size", &format!("{}px", v));
                }
            }
            "font" => {
                if let Some(v) = val {
                    push_css(&mut css, "font-family", v);
                }
            }

            // Flow
            "wrap" => push_css(&mut css, "flex-wrap", "wrap"),

            // Identity — handled separately, not CSS
            "id" | "class" => {}

            // Unknown — ignore
            _ => {}
        }
    }

    css
}

fn push_css(css: &mut String, prop: &str, value: &str) {
    css.push_str(prop);
    css.push(':');
    css.push_str(value);
    css.push(';');
}

fn extract_id_class(attrs: &[Attribute]) -> (Option<String>, Option<String>) {
    let mut id = None;
    let mut class = None;
    for attr in attrs {
        match attr.key.as_str() {
            "id" => id = attr.value.clone(),
            "class" => class = attr.value.clone(),
            _ => {}
        }
    }
    (id, class)
}

fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}
