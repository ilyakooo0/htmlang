use std::collections::HashMap;

use crate::ast::*;

// ---------------------------------------------------------------------------
// Style collector: deduplicates CSS and assigns class names
// ---------------------------------------------------------------------------

struct StyleEntry {
    class_name: String,
    base: String,
    hover: String,
    active: String,
    focus: String,
}

struct StyleCollector {
    entries: Vec<StyleEntry>,
    index: HashMap<String, usize>,
}

impl StyleCollector {
    fn new() -> Self {
        StyleCollector {
            entries: Vec::new(),
            index: HashMap::new(),
        }
    }

    /// Returns a class name for this style combination, or None if all empty.
    fn get_class(
        &mut self,
        base: String,
        hover: String,
        active: String,
        focus: String,
    ) -> Option<String> {
        if base.is_empty() && hover.is_empty() && active.is_empty() && focus.is_empty() {
            return None;
        }
        let key = format!("{}|{}|{}|{}", base, hover, active, focus);
        if let Some(&idx) = self.index.get(&key) {
            return Some(self.entries[idx].class_name.clone());
        }
        let name = format!("_{}", self.entries.len());
        let idx = self.entries.len();
        self.entries.push(StyleEntry {
            class_name: name.clone(),
            base,
            hover,
            active,
            focus,
        });
        self.index.insert(key, idx);
        Some(name)
    }

    fn to_css(&self) -> String {
        let mut css = String::new();
        for e in &self.entries {
            if !e.base.is_empty() {
                css.push_str(&format!(".{}{{{}}}", e.class_name, e.base));
            }
            if !e.hover.is_empty() {
                css.push_str(&format!(".{}:hover{{{}}}", e.class_name, e.hover));
            }
            if !e.active.is_empty() {
                css.push_str(&format!(".{}:active{{{}}}", e.class_name, e.active));
            }
            if !e.focus.is_empty() {
                css.push_str(&format!(".{}:focus{{{}}}", e.class_name, e.focus));
            }
        }
        css
    }
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

pub fn generate(doc: &Document) -> String {
    let mut styles = StyleCollector::new();
    let mut body = String::new();

    for node in &doc.nodes {
        generate_node(node, None, &mut body, &mut styles);
    }

    let element_css = styles.to_css();

    match &doc.page_title {
        Some(title) => format!(
            "<!DOCTYPE html><html><head><meta charset=\"utf-8\"><meta name=\"viewport\" content=\"width=device-width, initial-scale=1\"><title>{title}</title><style>*,*::before,*::after{{box-sizing:border-box}}body{{margin:0;font-family:system-ui,-apple-system,sans-serif}}img{{display:block}}{element_css}</style></head><body>{body}</body></html>",
            title = html_escape(title),
            element_css = element_css,
            body = body,
        ),
        None => {
            if element_css.is_empty() {
                body
            } else {
                format!("<style>{}</style>{}", element_css, body)
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Node generation
// ---------------------------------------------------------------------------

fn generate_node(
    node: &Node,
    parent_kind: Option<&ElementKind>,
    out: &mut String,
    styles: &mut StyleCollector,
) {
    match node {
        Node::Element(elem) => generate_element(elem, parent_kind, out, styles),
        Node::Text(segments) => {
            let needs_wrap = matches!(
                parent_kind,
                Some(ElementKind::Row) | Some(ElementKind::Column) | Some(ElementKind::El)
            );
            if needs_wrap {
                out.push_str("<span>");
            }
            generate_text_segments(segments, out, styles);
            if needs_wrap {
                out.push_str("</span>");
            }
        }
        Node::Raw(content) => {
            out.push_str(content);
        }
    }
}

fn generate_element(
    elem: &Element,
    parent_kind: Option<&ElementKind>,
    out: &mut String,
    styles: &mut StyleCollector,
) {
    if elem.kind == ElementKind::Image {
        generate_image(elem, parent_kind, out, styles);
        return;
    }
    if elem.kind == ElementKind::Children {
        return;
    }

    let tag = match elem.kind {
        ElementKind::Row | ElementKind::Column | ElementKind::El => "div",
        ElementKind::Text => "span",
        ElementKind::Paragraph => "p",
        ElementKind::Link => "a",
        ElementKind::Image | ElementKind::Children => unreachable!(),
    };

    // Compute CSS for each state and get a class name
    let gen_class = compute_class(&elem.attrs, &elem.kind, parent_kind, styles);
    let (id, user_class) = extract_id_class(&elem.attrs);

    out.push('<');
    out.push_str(tag);

    if elem.kind == ElementKind::Link {
        if let Some(url) = &elem.argument {
            out.push_str(" href=\"");
            out.push_str(&html_escape(url));
            out.push('"');
        }
    }

    emit_class_attr(out, gen_class.as_deref(), user_class.as_deref());

    if let Some(id) = id {
        out.push_str(" id=\"");
        out.push_str(&html_escape(&id));
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
        generate_node(child, Some(&elem.kind), out, styles);
        if is_paragraph && i < elem.children.len() - 1 {
            out.push(' ');
        }
    }

    out.push_str("</");
    out.push_str(tag);
    out.push('>');
}

fn generate_image(
    elem: &Element,
    parent_kind: Option<&ElementKind>,
    out: &mut String,
    styles: &mut StyleCollector,
) {
    let src = elem.argument.as_deref().unwrap_or("");
    let gen_class = compute_class(&elem.attrs, &elem.kind, parent_kind, styles);
    let (id, user_class) = extract_id_class(&elem.attrs);

    out.push_str("<img src=\"");
    out.push_str(&html_escape(src));
    out.push('"');

    emit_class_attr(out, gen_class.as_deref(), user_class.as_deref());

    if let Some(id) = id {
        out.push_str(" id=\"");
        out.push_str(&html_escape(&id));
        out.push('"');
    }
    out.push('>');
}

fn generate_text_segments(
    segments: &[TextSegment],
    out: &mut String,
    styles: &mut StyleCollector,
) {
    for segment in segments {
        match segment {
            TextSegment::Plain(text) => out.push_str(&html_escape(text)),
            TextSegment::Inline(elem) => {
                let mut buf = String::new();
                generate_element(elem, None, &mut buf, styles);
                out.push_str(buf.trim_end());
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Style helpers
// ---------------------------------------------------------------------------

fn compute_class(
    attrs: &[Attribute],
    kind: &ElementKind,
    parent_kind: Option<&ElementKind>,
    styles: &mut StyleCollector,
) -> Option<String> {
    let base = attrs_to_css(attrs, "", kind, parent_kind);
    let hover = attrs_to_css(attrs, "hover:", kind, parent_kind);
    let active = attrs_to_css(attrs, "active:", kind, parent_kind);
    let focus = attrs_to_css(attrs, "focus:", kind, parent_kind);
    styles.get_class(base, hover, active, focus)
}

fn emit_class_attr(out: &mut String, gen_class: Option<&str>, user_class: Option<&str>) {
    match (gen_class, user_class) {
        (Some(g), Some(u)) => {
            out.push_str(" class=\"");
            out.push_str(g);
            out.push(' ');
            out.push_str(&html_escape(u));
            out.push('"');
        }
        (Some(g), None) => {
            out.push_str(" class=\"");
            out.push_str(g);
            out.push('"');
        }
        (None, Some(u)) => {
            out.push_str(" class=\"");
            out.push_str(&html_escape(u));
            out.push('"');
        }
        (None, None) => {}
    }
}

// ---------------------------------------------------------------------------
// Attribute → CSS mapping
// ---------------------------------------------------------------------------

const STATE_PREFIXES: &[&str] = &["hover:", "active:", "focus:"];

fn is_state_attr(key: &str) -> bool {
    STATE_PREFIXES.iter().any(|p| key.starts_with(p))
}

fn attrs_to_css(
    attrs: &[Attribute],
    state_prefix: &str,
    kind: &ElementKind,
    parent_kind: Option<&ElementKind>,
) -> String {
    let mut css = String::new();

    // Base element styles only for the default (non-state) pass
    if state_prefix.is_empty() {
        match kind {
            ElementKind::Row => css.push_str("display:flex;flex-direction:row;"),
            ElementKind::Column => css.push_str("display:flex;flex-direction:column;"),
            ElementKind::El => css.push_str("display:flex;flex-direction:column;"),
            ElementKind::Paragraph => css.push_str("margin:0;"),
            _ => {}
        }
    }

    for attr in attrs {
        // Determine the effective key for this pass
        let effective_key = if state_prefix.is_empty() {
            if is_state_attr(&attr.key) {
                continue;
            }
            attr.key.as_str()
        } else {
            match attr.key.strip_prefix(state_prefix) {
                Some(k) => k,
                None => continue,
            }
        };

        let val = attr.value.as_deref();

        match effective_key {
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
                        3 => push_css(
                            &mut css,
                            "padding",
                            &format!(
                                "{}px {}px {}px",
                                parts[0], parts[1], parts[2]
                            ),
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
                        "shrink" => push_css(&mut css, "flex-shrink", "0"),
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
                        "shrink" => push_css(&mut css, "flex-shrink", "0"),
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
                Some(ElementKind::Row) => push_css(&mut css, "align-self", "center"),
                _ => {
                    push_css(&mut css, "margin-top", "auto");
                    push_css(&mut css, "margin-bottom", "auto");
                }
            },
            "align-left" => match parent_kind {
                Some(ElementKind::Column) | Some(ElementKind::El) => {
                    push_css(&mut css, "align-self", "flex-start");
                }
                _ => push_css(&mut css, "margin-right", "auto"),
            },
            "align-right" => match parent_kind {
                Some(ElementKind::Column) | Some(ElementKind::El) => {
                    push_css(&mut css, "align-self", "flex-end");
                }
                _ => push_css(&mut css, "margin-left", "auto"),
            },
            "align-top" => match parent_kind {
                Some(ElementKind::Row) => push_css(&mut css, "align-self", "flex-start"),
                _ => push_css(&mut css, "margin-bottom", "auto"),
            },
            "align-bottom" => match parent_kind {
                Some(ElementKind::Row) => push_css(&mut css, "align-self", "flex-end"),
                _ => push_css(&mut css, "margin-top", "auto"),
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
            "transition" => {
                if let Some(v) = val {
                    push_css(&mut css, "transition", v);
                }
            }
            "cursor" => {
                if let Some(v) = val {
                    push_css(&mut css, "cursor", v);
                }
            }
            "opacity" => {
                if let Some(v) = val {
                    push_css(&mut css, "opacity", v);
                }
            }

            // Typography
            "text-align" => {
                if let Some(v) = val {
                    push_css(&mut css, "text-align", v);
                }
            }
            "line-height" => {
                if let Some(v) = val {
                    push_css(&mut css, "line-height", v);
                }
            }

            // Overflow & positioning
            "overflow" => {
                if let Some(v) = val {
                    push_css(&mut css, "overflow", v);
                }
            }
            "position" => {
                if let Some(v) = val {
                    push_css(&mut css, "position", v);
                }
            }
            "z-index" => {
                if let Some(v) = val {
                    push_css(&mut css, "z-index", v);
                }
            }

            // Effects
            "shadow" => {
                if let Some(v) = val {
                    push_css(&mut css, "box-shadow", v);
                }
            }

            // Flow
            "wrap" => push_css(&mut css, "flex-wrap", "wrap"),
            "gap-x" => {
                if let Some(v) = val {
                    push_css(&mut css, "column-gap", &format!("{}px", v));
                }
            }
            "gap-y" => {
                if let Some(v) = val {
                    push_css(&mut css, "row-gap", &format!("{}px", v));
                }
            }

            // Identity — not CSS
            "id" | "class" => {}

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
