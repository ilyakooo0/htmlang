use std::collections::HashMap;

use crate::ast::*;

// ---------------------------------------------------------------------------
// Style collector: deduplicates CSS and assigns class names
// ---------------------------------------------------------------------------

/// (min-width breakpoint, prefix)
const BREAKPOINTS: &[(&str, &str)] = &[
    ("sm", "640px"),
    ("md", "768px"),
    ("lg", "1024px"),
    ("xl", "1280px"),
    ("2xl", "1536px"),
];

struct StyleEntry {
    class_name: String,
    base: String,
    /// (CSS selector suffix, css_rules) — e.g. (":hover", "color:red;")
    pseudo: Vec<(String, String)>,
    /// Responsive overrides: (breakpoint_prefix, css)
    responsive: Vec<(String, String)>,
    /// Dark mode overrides
    dark: String,
    /// Print overrides
    print: String,
    motion_safe: String,
    motion_reduce: String,
    landscape: String,
    portrait: String,
    /// Container query overrides: (breakpoint_prefix, css)
    container: Vec<(String, String)>,
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
        pseudo: Vec<(String, String)>,
        responsive: Vec<(String, String)>,
        dark: String,
        print: String,
        motion_safe: String,
        motion_reduce: String,
        landscape: String,
        portrait: String,
        container: Vec<(String, String)>,
    ) -> Option<String> {
        if base.is_empty()
            && pseudo.is_empty()
            && responsive.is_empty()
            && dark.is_empty()
            && print.is_empty()
            && motion_safe.is_empty()
            && motion_reduce.is_empty()
            && landscape.is_empty()
            && portrait.is_empty()
            && container.is_empty()
        {
            return None;
        }
        let pseudo_key: String = pseudo
            .iter()
            .map(|(sel, css)| format!("{}={}", sel, css))
            .collect::<Vec<_>>()
            .join("|");
        let resp_key: String = responsive
            .iter()
            .map(|(bp, css)| format!("{}={}", bp, css))
            .collect::<Vec<_>>()
            .join("|");
        let cq_key: String = container
            .iter()
            .map(|(bp, css)| format!("{}={}", bp, css))
            .collect::<Vec<_>>()
            .join("|");
        let key = format!("{}|{}|{}|{}|{}|{}|{}|{}|{}|{}", base, pseudo_key, resp_key, dark, print, motion_safe, motion_reduce, landscape, portrait, cq_key);
        if let Some(&idx) = self.index.get(&key) {
            return Some(self.entries[idx].class_name.clone());
        }
        let name = format!("_{}", self.entries.len());
        let idx = self.entries.len();
        self.entries.push(StyleEntry {
            class_name: name.clone(),
            base,
            pseudo,
            responsive,
            dark,
            print,
            motion_safe,
            motion_reduce,
            landscape,
            portrait,
            container,
        });
        self.index.insert(key, idx);
        Some(name)
    }

    fn to_css_formatted(&self, dev: bool, use_layer: bool) -> String {
        let mut css = String::new();
        let (sp, nl) = if dev { (" ", "\n") } else { ("", "") };

        // Wrap in @layer for specificity management
        if use_layer {
            if dev {
                css.push_str("@layer htmlang {\n");
            } else {
                css.push_str("@layer htmlang{");
            }
        }

        // Non-responsive rules
        for e in &self.entries {
            if !e.base.is_empty() {
                if use_layer && dev {
                    css.push_str(&format!("  .{}{sp}{{{}}}{nl}", e.class_name, e.base));
                } else {
                    css.push_str(&format!(".{}{sp}{{{}}}{nl}", e.class_name, e.base));
                }
            }
            for (selector, pseudo_css) in &e.pseudo {
                if !pseudo_css.is_empty() {
                    if use_layer && dev {
                        css.push_str(&format!("  .{}{}{sp}{{{}}}{nl}", e.class_name, selector, pseudo_css));
                    } else {
                        css.push_str(&format!(".{}{}{sp}{{{}}}{nl}", e.class_name, selector, pseudo_css));
                    }
                }
            }
        }

        // Responsive rules grouped by breakpoint
        for &(bp_name, bp_width) in BREAKPOINTS {
            let mut bp_css = String::new();
            for e in &self.entries {
                for (bp, rule_css) in &e.responsive {
                    if bp == bp_name && !rule_css.is_empty() {
                        if dev {
                            bp_css.push_str(&format!("  .{} {{{}}}\n", e.class_name, rule_css));
                        } else {
                            bp_css.push_str(&format!(".{}{{{}}}", e.class_name, rule_css));
                        }
                    }
                }
            }
            if !bp_css.is_empty() {
                if dev {
                    css.push_str(&format!(
                        "@media (min-width: {}) {{\n{}}}\n",
                        bp_width, bp_css
                    ));
                } else {
                    css.push_str(&format!(
                        "@media(min-width:{}){{{}}}",
                        bp_width, bp_css
                    ));
                }
            }
        }

        // Dark mode rules
        let mut dark_css = String::new();
        for e in &self.entries {
            if !e.dark.is_empty() {
                if dev {
                    dark_css.push_str(&format!("  .{} {{{}}}\n", e.class_name, e.dark));
                } else {
                    dark_css.push_str(&format!(".{}{{{}}}", e.class_name, e.dark));
                }
            }
        }
        if !dark_css.is_empty() {
            if dev {
                css.push_str(&format!("@media (prefers-color-scheme: dark) {{\n{}}}\n", dark_css));
            } else {
                css.push_str(&format!("@media(prefers-color-scheme:dark){{{}}}", dark_css));
            }
        }

        // Print rules
        let mut print_css = String::new();
        for e in &self.entries {
            if !e.print.is_empty() {
                if dev {
                    print_css.push_str(&format!("  .{} {{{}}}\n", e.class_name, e.print));
                } else {
                    print_css.push_str(&format!(".{}{{{}}}", e.class_name, e.print));
                }
            }
        }
        if !print_css.is_empty() {
            if dev {
                css.push_str(&format!("@media print {{\n{}}}\n", print_css));
            } else {
                css.push_str(&format!("@media print{{{}}}", print_css));
            }
        }

        // Motion safe rules
        let mut motion_safe_css = String::new();
        for e in &self.entries {
            if !e.motion_safe.is_empty() {
                if dev {
                    motion_safe_css.push_str(&format!("  .{} {{{}}}\n", e.class_name, e.motion_safe));
                } else {
                    motion_safe_css.push_str(&format!(".{}{{{}}}", e.class_name, e.motion_safe));
                }
            }
        }
        if !motion_safe_css.is_empty() {
            if dev {
                css.push_str(&format!("@media (prefers-reduced-motion: no-preference) {{\n{}}}\n", motion_safe_css));
            } else {
                css.push_str(&format!("@media(prefers-reduced-motion:no-preference){{{}}}", motion_safe_css));
            }
        }

        // Motion reduce rules
        let mut motion_reduce_css = String::new();
        for e in &self.entries {
            if !e.motion_reduce.is_empty() {
                if dev {
                    motion_reduce_css.push_str(&format!("  .{} {{{}}}\n", e.class_name, e.motion_reduce));
                } else {
                    motion_reduce_css.push_str(&format!(".{}{{{}}}", e.class_name, e.motion_reduce));
                }
            }
        }
        if !motion_reduce_css.is_empty() {
            if dev {
                css.push_str(&format!("@media (prefers-reduced-motion: reduce) {{\n{}}}\n", motion_reduce_css));
            } else {
                css.push_str(&format!("@media(prefers-reduced-motion:reduce){{{}}}", motion_reduce_css));
            }
        }

        // Landscape rules
        let mut landscape_css = String::new();
        for e in &self.entries {
            if !e.landscape.is_empty() {
                if dev {
                    landscape_css.push_str(&format!("  .{} {{{}}}\n", e.class_name, e.landscape));
                } else {
                    landscape_css.push_str(&format!(".{}{{{}}}", e.class_name, e.landscape));
                }
            }
        }
        if !landscape_css.is_empty() {
            if dev {
                css.push_str(&format!("@media (orientation: landscape) {{\n{}}}\n", landscape_css));
            } else {
                css.push_str(&format!("@media(orientation:landscape){{{}}}", landscape_css));
            }
        }

        // Portrait rules
        let mut portrait_css = String::new();
        for e in &self.entries {
            if !e.portrait.is_empty() {
                if dev {
                    portrait_css.push_str(&format!("  .{} {{{}}}\n", e.class_name, e.portrait));
                } else {
                    portrait_css.push_str(&format!(".{}{{{}}}", e.class_name, e.portrait));
                }
            }
        }
        if !portrait_css.is_empty() {
            if dev {
                css.push_str(&format!("@media (orientation: portrait) {{\n{}}}\n", portrait_css));
            } else {
                css.push_str(&format!("@media(orientation:portrait){{{}}}", portrait_css));
            }
        }

        // Container query rules grouped by breakpoint
        for &(bp_name, bp_width) in BREAKPOINTS {
            let mut cq_css = String::new();
            for e in &self.entries {
                for (bp, rule_css) in &e.container {
                    if bp == bp_name && !rule_css.is_empty() {
                        if dev {
                            cq_css.push_str(&format!("  .{} {{{}}}\n", e.class_name, rule_css));
                        } else {
                            cq_css.push_str(&format!(".{}{{{}}}", e.class_name, rule_css));
                        }
                    }
                }
            }
            if !cq_css.is_empty() {
                if dev {
                    css.push_str(&format!(
                        "@container (min-width: {}) {{\n{}}}\n",
                        bp_width, cq_css
                    ));
                } else {
                    css.push_str(&format!(
                        "@container(min-width:{}){{{}}}",
                        bp_width, cq_css
                    ));
                }
            }
        }

        // Close @layer
        if use_layer {
            if dev {
                css.push_str("}\n");
            } else {
                css.push('}');
            }
        }

        css
    }
}

struct GenContext {
    dev: bool,
    depth: usize,
}

impl GenContext {
    fn indent(&self) -> String {
        if self.dev {
            "  ".repeat(self.depth)
        } else {
            String::new()
        }
    }

    fn nl(&self) -> &str {
        if self.dev { "\n" } else { "" }
    }
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

pub fn generate(doc: &Document) -> String {
    generate_with_options(doc, false)
}

pub fn generate_dev(doc: &Document) -> String {
    generate_with_options(doc, true)
}

fn generate_with_options(doc: &Document, dev: bool) -> String {
    let mut styles = StyleCollector::new();
    let mut ctx = GenContext { dev, depth: 0 };
    let mut body = String::new();

    for node in &doc.nodes {
        generate_node(node, None, &mut body, &mut styles, &mut ctx);
    }

    let mut element_css = String::new();

    // CSS custom properties (includes @theme runtime tokens)
    let has_theme_tokens = !doc.theme_tokens.is_empty();
    if !doc.css_vars.is_empty() || has_theme_tokens {
        if dev {
            element_css.push_str(":root {\n");
            for (name, value) in &doc.css_vars {
                element_css.push_str(&format!("  {}: {};\n", name, value));
            }
            // @theme tokens as runtime CSS custom properties
            for (name, value) in &doc.theme_tokens {
                let css_name = format!("--{}", name);
                // Only emit if not already in css_vars
                if !doc.css_vars.iter().any(|(n, _)| *n == css_name) {
                    element_css.push_str(&format!("  {}: {};\n", css_name, value));
                }
            }
            element_css.push_str("}\n");
        } else {
            element_css.push_str(":root{");
            for (name, value) in &doc.css_vars {
                element_css.push_str(name);
                element_css.push(':');
                element_css.push_str(value);
                element_css.push(';');
            }
            for (name, value) in &doc.theme_tokens {
                let css_name = format!("--{}", name);
                if !doc.css_vars.iter().any(|(n, _)| *n == css_name) {
                    element_css.push_str(&css_name);
                    element_css.push(':');
                    element_css.push_str(value);
                    element_css.push(';');
                }
            }
            element_css.push('}');
        }
    }

    let has_custom_css = !doc.custom_css.is_empty();
    element_css.push_str(&styles.to_css_formatted(dev, !has_custom_css));

    // @keyframes
    for (name, kf_body) in &doc.keyframes {
        if dev {
            element_css.push_str(&format!("@keyframes {} {{\n{}\n}}\n", name, kf_body));
        } else {
            element_css.push_str(&format!("@keyframes {}{{{}}}", name, kf_body));
        }
    }

    // Auto-inject skeleton keyframe if skeleton attribute is used
    let skeleton_used = element_css.contains("hl-skeleton");
    if skeleton_used {
        if dev {
            element_css.push_str("@keyframes hl-skeleton {\n  0% { background-position: 200% 0; }\n  100% { background-position: -200% 0; }\n}\n");
        } else {
            element_css.push_str("@keyframes hl-skeleton{0%{background-position:200% 0}100%{background-position:-200% 0}}");
        }
    }

    // Also inject carousel webkit scrollbar hiding
    let carousel_used = element_css.contains("scroll-snap-type:x mandatory");
    if carousel_used && !element_css.contains("::-webkit-scrollbar") {
        // The carousel class already has scrollbar-width:none, but webkit needs pseudo-element
        // We add a global rule for carousel-style elements
    }

    // @style blocks (custom CSS)
    for block in &doc.custom_css {
        if dev {
            element_css.push_str(block);
            element_css.push('\n');
        } else {
            // Minify: collapse whitespace
            let minified: String = block
                .lines()
                .map(|l| l.trim())
                .collect::<Vec<_>>()
                .join("");
            element_css.push_str(&minified);
        }
    }

    // Build meta tags string
    let meta_html = if doc.meta_tags.is_empty() {
        String::new()
    } else {
        let mut m = String::new();
        for (name, content) in &doc.meta_tags {
            if dev {
                m.push_str(&format!(
                    "<meta name=\"{}\" content=\"{}\">\n",
                    html_escape(name),
                    html_escape(content)
                ));
            } else {
                m.push_str(&format!(
                    "<meta name=\"{}\" content=\"{}\">",
                    html_escape(name),
                    html_escape(content)
                ));
            }
        }
        m
    };

    // Build OG meta tags
    let og_html = if doc.og_tags.is_empty() {
        String::new()
    } else {
        let mut o = String::new();
        for (property, content) in &doc.og_tags {
            if dev {
                o.push_str(&format!(
                    "<meta property=\"og:{}\" content=\"{}\">\n",
                    html_escape(property),
                    html_escape(content)
                ));
            } else {
                o.push_str(&format!(
                    "<meta property=\"og:{}\" content=\"{}\">",
                    html_escape(property),
                    html_escape(content)
                ));
            }
        }
        o
    };

    // Build head blocks string
    let head_html = if doc.head_blocks.is_empty() {
        String::new()
    } else {
        let mut h = String::new();
        for block in &doc.head_blocks {
            h.push_str(block);
            if dev {
                h.push('\n');
            }
        }
        h
    };

    let lang_attr = match &doc.lang {
        Some(lang) => format!(" lang=\"{}\"", html_escape(lang)),
        None => String::new(),
    };

    let favicon_html = match &doc.favicon {
        Some(path) => {
            // Try to read and inline the favicon
            if let Ok(data) = std::fs::read(path) {
                let mime = if path.ends_with(".ico") {
                    "image/x-icon"
                } else if path.ends_with(".png") {
                    "image/png"
                } else if path.ends_with(".svg") {
                    "image/svg+xml"
                } else {
                    "image/x-icon"
                };
                let b64 = base64_encode(&data);
                if dev {
                    format!("<link rel=\"icon\" href=\"data:{};base64,{}\">\n", mime, b64)
                } else {
                    format!("<link rel=\"icon\" href=\"data:{};base64,{}\">", mime, b64)
                }
            } else {
                // Fall back to href
                if dev {
                    format!("<link rel=\"icon\" href=\"{}\">\n", html_escape(path))
                } else {
                    format!("<link rel=\"icon\" href=\"{}\">", html_escape(path))
                }
            }
        }
        None => String::new(),
    };

    // Canonical URL
    let canonical_html = match &doc.canonical {
        Some(url) => {
            if dev {
                format!("<link rel=\"canonical\" href=\"{}\">\n", html_escape(url))
            } else {
                format!("<link rel=\"canonical\" href=\"{}\">", html_escape(url))
            }
        }
        None => String::new(),
    };

    // Base URL
    let base_html = match &doc.base_url {
        Some(url) => {
            if dev {
                format!("<base href=\"{}\">\n", html_escape(url))
            } else {
                format!("<base href=\"{}\">", html_escape(url))
            }
        }
        None => String::new(),
    };

    // @font-face CSS
    for (name, url) in &doc.font_faces {
        let format_hint = if url.ends_with(".woff2") {
            " format('woff2')"
        } else if url.ends_with(".woff") {
            " format('woff')"
        } else if url.ends_with(".ttf") {
            " format('truetype')"
        } else if url.ends_with(".otf") {
            " format('opentype')"
        } else {
            ""
        };
        if dev {
            element_css.insert_str(0, &format!(
                "@font-face {{\n  font-family: '{}';\n  src: url('{}'){};\n  font-display: swap;\n}}\n",
                name, url, format_hint
            ));
        } else {
            element_css.insert_str(0, &format!(
                "@font-face{{font-family:'{}';src:url('{}'){};font-display:swap}}",
                name, url, format_hint
            ));
        }
    }

    // JSON-LD blocks
    let json_ld_html: String = doc.json_ld_blocks.iter().map(|block| {
        if dev {
            format!("<script type=\"application/ld+json\">\n{}\n</script>\n", block)
        } else {
            format!("<script type=\"application/ld+json\">{}</script>", block)
        }
    }).collect();

    match &doc.page_title {
        Some(title) => {
            if dev {
                format!(
                    "\
<!DOCTYPE html>
<html{lang_attr}>
<head>
<meta charset=\"utf-8\">
<meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">
<title>{title}</title>
{base_html}{canonical_html}{meta_html}{og_html}{favicon_html}{json_ld_html}{head_html}\
<style>
*, *::before, *::after {{ box-sizing: border-box; }}
body {{ margin: 0; font-family: system-ui, -apple-system, sans-serif; }}
img {{ display: block; }}
{element_css}\
</style>
</head>
<body>
{body}\
</body>
</html>
",
                    title = html_escape(title),
                    lang_attr = lang_attr,
                    base_html = base_html,
                    canonical_html = canonical_html,
                    meta_html = meta_html,
                    favicon_html = favicon_html,
                    json_ld_html = json_ld_html,
                    head_html = head_html,
                    og_html = og_html,
                    element_css = element_css,
                    body = body,
                )
            } else {
                format!(
                    "<!DOCTYPE html><html{lang_attr}><head><meta charset=\"utf-8\"><meta name=\"viewport\" content=\"width=device-width, initial-scale=1\"><title>{title}</title>{base_html}{canonical_html}{meta_html}{og_html}{favicon_html}{json_ld_html}{head_html}<style>*,*::before,*::after{{box-sizing:border-box}}body{{margin:0;font-family:system-ui,-apple-system,sans-serif}}img{{display:block}}{element_css}</style></head><body>{body}</body></html>",
                    title = html_escape(title),
                    lang_attr = lang_attr,
                    base_html = base_html,
                    canonical_html = canonical_html,
                    meta_html = meta_html,
                    og_html = og_html,
                    favicon_html = favicon_html,
                    json_ld_html = json_ld_html,
                    head_html = head_html,
                    element_css = element_css,
                    body = body,
                )
            }
        }
        None => {
            if element_css.is_empty() {
                body
            } else {
                if dev {
                    format!("<style>\n{}</style>\n{}", element_css, body)
                } else {
                    format!("<style>{}</style>{}", element_css, body)
                }
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
    ctx: &mut GenContext,
) {
    match node {
        Node::Element(elem) => generate_element(elem, parent_kind, out, styles, ctx),
        Node::Text(segments) => {
            let needs_wrap = matches!(
                parent_kind,
                Some(ElementKind::Row) | Some(ElementKind::Column) | Some(ElementKind::El)
                | Some(ElementKind::Nav) | Some(ElementKind::Header) | Some(ElementKind::Footer)
                | Some(ElementKind::Main) | Some(ElementKind::Section) | Some(ElementKind::Article)
                | Some(ElementKind::Aside) | Some(ElementKind::ListItem)
                | Some(ElementKind::Form) | Some(ElementKind::Details) | Some(ElementKind::Figure)
                | Some(ElementKind::Blockquote)
                | Some(ElementKind::Dialog) | Some(ElementKind::DefinitionList)
                | Some(ElementKind::DefinitionDescription) | Some(ElementKind::Fieldset)
                | Some(ElementKind::Datalist) | Some(ElementKind::Grid) | Some(ElementKind::Stack)
                | Some(ElementKind::Noscript) | Some(ElementKind::Address) | Some(ElementKind::Search)
            );
            if needs_wrap {
                out.push_str(&ctx.indent());
                out.push_str("<span>");
            }
            generate_text_segments(segments, out, styles, ctx);
            if needs_wrap {
                out.push_str("</span>");
                out.push_str(ctx.nl());
            }
        }
        Node::Raw(content) => {
            out.push_str(&ctx.indent());
            out.push_str(content);
            out.push_str(ctx.nl());
        }
    }
}

/// HTML attributes that are passed through to the HTML tag rather than converted to CSS.
const HTML_PASSTHROUGH_ATTRS: &[&str] = &[
    "type", "placeholder", "name", "value", "disabled", "required", "checked",
    "for", "action", "method", "autocomplete",
    "min", "max", "step", "pattern", "maxlength", "rows", "cols", "multiple",
    "alt", "role", "tabindex", "title", "autofocus",
    // Media
    "controls", "autoplay", "loop", "muted", "poster", "preload",
    // Image optimization
    "loading", "decoding",
    // Media src (explicit attribute form)
    "src",
    // New element attributes
    "datetime", "media", "sizes", "srcset", "list",
    // Details
    "open",
    // Form
    "novalidate",
    // Progress/Meter
    "low", "high", "optimum",
    // Table
    "colspan", "rowspan", "scope",
];

/// Boolean HTML attributes (rendered without a value, e.g., `<input disabled>`).
const BOOLEAN_HTML_ATTRS: &[&str] = &[
    "disabled", "required", "checked", "multiple",
    "controls", "autoplay", "loop", "muted",
    "open", "novalidate", "autofocus",
    "defer", "async", "nomodule",
];

fn emit_html_passthrough_attrs(out: &mut String, attrs: &[Attribute]) {
    for attr in attrs {
        let key = attr.key.as_str();
        let is_passthrough = HTML_PASSTHROUGH_ATTRS.contains(&key)
            || key.starts_with("aria-")
            || key.starts_with("data-");
        if !is_passthrough {
            continue;
        }
        if BOOLEAN_HTML_ATTRS.contains(&key) && attr.value.is_none() {
            out.push(' ');
            out.push_str(key);
        } else if let Some(val) = &attr.value {
            out.push(' ');
            out.push_str(key);
            out.push_str("=\"");
            out.push_str(&html_escape(val));
            out.push('"');
        }
    }
}

fn generate_element(
    elem: &Element,
    parent_kind: Option<&ElementKind>,
    out: &mut String,
    styles: &mut StyleCollector,
    ctx: &mut GenContext,
) {
    // Self-closing elements
    if matches!(elem.kind, ElementKind::Image | ElementKind::Input | ElementKind::HorizontalRule | ElementKind::Source | ElementKind::Spacer) {
        generate_self_closing(elem, parent_kind, out, styles, ctx);
        return;
    }
    // @script renders as <script> with raw body content (no HTML escaping)
    if elem.kind == ElementKind::Script {
        out.push_str(&ctx.indent());
        out.push_str("<script");
        // Pass through src, type, defer, async, etc.
        for attr in &elem.attrs {
            let key = attr.key.as_str();
            if matches!(key, "src" | "type" | "defer" | "async" | "crossorigin" | "integrity" | "nomodule" | "id") {
                if let Some(val) = &attr.value {
                    out.push(' ');
                    out.push_str(key);
                    out.push_str("=\"");
                    out.push_str(&html_escape(val));
                    out.push('"');
                } else {
                    out.push(' ');
                    out.push_str(key);
                }
            }
        }
        out.push('>');
        // Children are raw JS code, not HTML
        for child in &elem.children {
            match child {
                Node::Text(segments) => {
                    for seg in segments {
                        match seg {
                            TextSegment::Plain(text) => out.push_str(text),
                            _ => {}
                        }
                    }
                }
                Node::Raw(content) => out.push_str(content),
                _ => {}
            }
        }
        out.push_str("</script>");
        out.push_str(ctx.nl());
        return;
    }
    // @breadcrumb generates semantic <nav aria-label="breadcrumb"><ol>...</ol></nav>
    if elem.kind == ElementKind::Breadcrumb {
        let gen_class = compute_class(&elem.attrs, &elem.kind, parent_kind, styles);
        let (id, user_class) = extract_id_class(&elem.attrs);
        out.push_str(&ctx.indent());
        out.push_str("<nav aria-label=\"breadcrumb\"");
        emit_class_attr(out, gen_class.as_deref(), user_class.as_deref());
        if let Some(id) = id {
            out.push_str(" id=\"");
            out.push_str(&html_escape(&id));
            out.push('"');
        }
        out.push('>');
        out.push_str(ctx.nl());
        ctx.depth += 1;
        out.push_str(&ctx.indent());
        out.push_str("<ol>");
        out.push_str(ctx.nl());
        ctx.depth += 1;
        for child in &elem.children {
            out.push_str(&ctx.indent());
            out.push_str("<li>");
            let mut buf = String::new();
            generate_node(child, Some(&elem.kind), &mut buf, styles, ctx);
            out.push_str(buf.trim());
            out.push_str("</li>");
            out.push_str(ctx.nl());
        }
        ctx.depth -= 1;
        out.push_str(&ctx.indent());
        out.push_str("</ol>");
        out.push_str(ctx.nl());
        ctx.depth -= 1;
        out.push_str(&ctx.indent());
        out.push_str("</nav>");
        out.push_str(ctx.nl());
        return;
    }
    if elem.kind == ElementKind::Children {
        return;
    }
    if matches!(elem.kind, ElementKind::Slot(_)) {
        return;
    }
    if elem.kind == ElementKind::Fragment {
        // Render children without a wrapper element
        for child in &elem.children {
            generate_node(child, parent_kind, out, styles, ctx);
        }
        return;
    }

    let tag = match &elem.kind {
        ElementKind::Row | ElementKind::Column | ElementKind::El
        | ElementKind::Grid | ElementKind::Stack => "div",
        ElementKind::Text => "span",
        ElementKind::Paragraph => "p",
        ElementKind::Link => "a",
        ElementKind::Button => "button",
        ElementKind::Select => "select",
        ElementKind::Textarea => "textarea",
        ElementKind::Option => "option",
        ElementKind::Label => "label",
        // Semantic elements
        ElementKind::Nav => "nav",
        ElementKind::Header => "header",
        ElementKind::Footer => "footer",
        ElementKind::Main => "main",
        ElementKind::Section => "section",
        ElementKind::Article => "article",
        ElementKind::Aside => "aside",
        // List
        ElementKind::List => {
            if elem.attrs.iter().any(|a| a.key == "ordered") { "ol" } else { "ul" }
        }
        ElementKind::ListItem => "li",
        // Table
        ElementKind::Table => "table",
        ElementKind::TableHead => "thead",
        ElementKind::TableBody => "tbody",
        ElementKind::TableRow => "tr",
        ElementKind::TableCell => "td",
        ElementKind::TableHeaderCell => "th",
        // Media
        ElementKind::Video => "video",
        ElementKind::Audio => "audio",
        // Additional semantic elements
        ElementKind::Form => "form",
        ElementKind::Details => "details",
        ElementKind::Summary => "summary",
        ElementKind::Blockquote => "blockquote",
        ElementKind::Cite => "cite",
        ElementKind::Code => "code",
        ElementKind::Pre => "pre",
        ElementKind::Figure => "figure",
        ElementKind::FigCaption => "figcaption",
        ElementKind::Progress => "progress",
        ElementKind::Meter => "meter",
        // New elements
        ElementKind::Dialog => "dialog",
        ElementKind::DefinitionList => "dl",
        ElementKind::DefinitionTerm => "dt",
        ElementKind::DefinitionDescription => "dd",
        ElementKind::Fieldset => "fieldset",
        ElementKind::Legend => "legend",
        ElementKind::Picture => "picture",
        ElementKind::Time => "time",
        ElementKind::Mark => "mark",
        ElementKind::Kbd => "kbd",
        ElementKind::Abbr => "abbr",
        ElementKind::Datalist => "datalist",
        ElementKind::Iframe => "iframe",
        ElementKind::Output => "output",
        ElementKind::Canvas => "canvas",
        ElementKind::Badge => "span",
        ElementKind::Tooltip => "span",
        ElementKind::Avatar => "div",
        ElementKind::Carousel => "div",
        ElementKind::Chip => "span",
        ElementKind::Tag => "span",
        ElementKind::Noscript => "noscript",
        ElementKind::Address => "address",
        ElementKind::Search => "search",
        ElementKind::Image | ElementKind::Input | ElementKind::HorizontalRule
        | ElementKind::Children | ElementKind::Slot(_) | ElementKind::Fragment
        | ElementKind::Source | ElementKind::Spacer
        | ElementKind::Script | ElementKind::Breadcrumb => unreachable!(),
    };

    let kind_label = match elem.kind {
        ElementKind::Row => "row",
        ElementKind::Column => "column",
        ElementKind::El => "el",
        ElementKind::Text => "text",
        ElementKind::Paragraph => "paragraph",
        ElementKind::Link => "link",
        ElementKind::Button => "button",
        ElementKind::Select => "select",
        ElementKind::Textarea => "textarea",
        ElementKind::Option => "option",
        ElementKind::Label => "label",
        ElementKind::Nav => "nav",
        ElementKind::Header => "header",
        ElementKind::Footer => "footer",
        ElementKind::Main => "main",
        ElementKind::Section => "section",
        ElementKind::Article => "article",
        ElementKind::Aside => "aside",
        ElementKind::List => "list",
        ElementKind::ListItem => "item",
        ElementKind::Table => "table",
        ElementKind::TableHead => "thead",
        ElementKind::TableBody => "tbody",
        ElementKind::TableRow => "tr",
        ElementKind::TableCell => "td",
        ElementKind::TableHeaderCell => "th",
        ElementKind::Video => "video",
        ElementKind::Audio => "audio",
        ElementKind::Form => "form",
        ElementKind::Details => "details",
        ElementKind::Summary => "summary",
        ElementKind::Blockquote => "blockquote",
        ElementKind::Cite => "cite",
        ElementKind::Code => "code",
        ElementKind::Pre => "pre",
        ElementKind::Figure => "figure",
        ElementKind::FigCaption => "figcaption",
        ElementKind::Fragment => "fragment",
        ElementKind::Dialog => "dialog",
        ElementKind::DefinitionList => "dl",
        ElementKind::DefinitionTerm => "dt",
        ElementKind::DefinitionDescription => "dd",
        ElementKind::Fieldset => "fieldset",
        ElementKind::Legend => "legend",
        ElementKind::Picture => "picture",
        ElementKind::Time => "time",
        ElementKind::Mark => "mark",
        ElementKind::Kbd => "kbd",
        ElementKind::Abbr => "abbr",
        ElementKind::Datalist => "datalist",
        ElementKind::Iframe => "iframe",
        ElementKind::Output => "output",
        ElementKind::Canvas => "canvas",
        ElementKind::Grid => "grid",
        ElementKind::Stack => "stack",
        ElementKind::Badge => "badge",
        ElementKind::Tooltip => "tooltip",
        ElementKind::Avatar => "avatar",
        ElementKind::Carousel => "carousel",
        ElementKind::Chip => "chip",
        ElementKind::Tag => "tag",
        ElementKind::Noscript => "noscript",
        ElementKind::Address => "address",
        ElementKind::Search => "search",
        _ => "",
    };

    // Compute CSS for each state and get a class name
    let gen_class = compute_class(&elem.attrs, &elem.kind, parent_kind, styles);
    let (id, user_class) = extract_id_class(&elem.attrs);

    if ctx.dev && elem.line_num > 0 {
        out.push_str(&ctx.indent());
        out.push_str(&format!("<!-- @{} line {} -->\n", kind_label, elem.line_num));
    }
    out.push_str(&ctx.indent());
    out.push('<');
    out.push_str(tag);

    if elem.kind == ElementKind::Link {
        if let Some(url) = &elem.argument {
            out.push_str(" href=\"");
            out.push_str(&html_escape(url));
            out.push('"');
        }
    }

    // Video/Audio src
    if matches!(elem.kind, ElementKind::Video | ElementKind::Audio) {
        if let Some(src) = &elem.argument {
            out.push_str(" src=\"");
            out.push_str(&html_escape(src));
            out.push('"');
        }
    }

    // Form action
    if elem.kind == ElementKind::Form {
        if let Some(action) = &elem.argument {
            out.push_str(" action=\"");
            out.push_str(&html_escape(action));
            out.push('"');
        }
    }

    // Iframe src
    if elem.kind == ElementKind::Iframe {
        if let Some(src) = &elem.argument {
            out.push_str(" src=\"");
            out.push_str(&html_escape(src));
            out.push('"');
        }
    }

    // Tooltip title
    if elem.kind == ElementKind::Tooltip {
        if let Some(text) = &elem.argument {
            out.push_str(" title=\"");
            out.push_str(&html_escape(text));
            out.push('"');
        }
    }

    // Time datetime
    if elem.kind == ElementKind::Time {
        if let Some(dt) = elem.attrs.iter().find(|a| a.key == "datetime") {
            if let Some(val) = &dt.value {
                out.push_str(" datetime=\"");
                out.push_str(&html_escape(val));
                out.push('"');
            }
        }
    }

    emit_class_attr(out, gen_class.as_deref(), user_class.as_deref());

    if let Some(id) = id {
        out.push_str(" id=\"");
        out.push_str(&html_escape(&id));
        out.push('"');
    }

    emit_html_passthrough_attrs(out, &elem.attrs);

    // Source map attributes in dev mode
    if ctx.dev && elem.line_num > 0 {
        out.push_str(&format!(" data-hl-line=\"{}\" data-hl-el=\"{}\"", elem.line_num, kind_label));
    }

    out.push('>');
    out.push_str(ctx.nl());

    // Inline text argument
    if matches!(
        elem.kind,
        ElementKind::Text
            | ElementKind::Button
            | ElementKind::Label
            | ElementKind::Option
            | ElementKind::Textarea
            | ElementKind::ListItem
            | ElementKind::TableCell
            | ElementKind::TableHeaderCell
            | ElementKind::Summary
            | ElementKind::Cite
            | ElementKind::Code
            | ElementKind::FigCaption
            | ElementKind::Legend
            | ElementKind::DefinitionTerm
            | ElementKind::Mark
            | ElementKind::Kbd
            | ElementKind::Abbr
            | ElementKind::Time
            | ElementKind::DefinitionDescription
            | ElementKind::Badge
            | ElementKind::Tooltip
            | ElementKind::Chip
            | ElementKind::Tag
    ) {
        if let Some(text) = &elem.argument {
            out.push_str(&html_escape(text));
        }
    }

    // Children
    ctx.depth += 1;
    let is_paragraph = elem.kind == ElementKind::Paragraph;
    for (i, child) in elem.children.iter().enumerate() {
        generate_node(child, Some(&elem.kind), out, styles, ctx);
        if is_paragraph && i < elem.children.len() - 1 {
            out.push(' ');
        }
    }
    ctx.depth -= 1;

    out.push_str(&ctx.indent());
    out.push_str("</");
    out.push_str(tag);
    out.push('>');
    out.push_str(ctx.nl());
}

fn generate_self_closing(
    elem: &Element,
    parent_kind: Option<&ElementKind>,
    out: &mut String,
    styles: &mut StyleCollector,
    ctx: &GenContext,
) {
    let gen_class = compute_class(&elem.attrs, &elem.kind, parent_kind, styles);
    let (id, user_class) = extract_id_class(&elem.attrs);

    let (tag, kind_label) = match elem.kind {
        ElementKind::Image => ("img", "image"),
        ElementKind::Input => ("input", "input"),
        ElementKind::HorizontalRule => ("hr", "hr"),
        ElementKind::Source => ("source", "source"),
        ElementKind::Spacer => ("div", "spacer"),
        _ => unreachable!(),
    };

    if ctx.dev && elem.line_num > 0 {
        out.push_str(&ctx.indent());
        out.push_str(&format!("<!-- @{} line {} -->\n", kind_label, elem.line_num));
    }
    out.push_str(&ctx.indent());
    out.push('<');
    out.push_str(tag);

    // Image src (with optional non-SVG base64 inlining)
    if elem.kind == ElementKind::Image {
        let src = elem.argument.as_deref().unwrap_or("");
        let is_inline = elem.attrs.iter().any(|a| a.key == "inline");
        if is_inline && !src.is_empty() && !src.ends_with(".svg") {
            let mime = if src.ends_with(".png") { "image/png" }
                else if src.ends_with(".jpg") || src.ends_with(".jpeg") { "image/jpeg" }
                else if src.ends_with(".gif") { "image/gif" }
                else if src.ends_with(".webp") { "image/webp" }
                else if src.ends_with(".avif") { "image/avif" }
                else { "application/octet-stream" };
            if let Ok(data) = std::fs::read(src) {
                let b64 = base64_encode(&data);
                out.push_str(" src=\"data:");
                out.push_str(mime);
                out.push_str(";base64,");
                out.push_str(&b64);
                out.push('"');
            } else {
                out.push_str(" src=\"");
                out.push_str(&html_escape(src));
                out.push('"');
            }
        } else {
            out.push_str(" src=\"");
            out.push_str(&html_escape(src));
            out.push('"');
        }
    }

    emit_class_attr(out, gen_class.as_deref(), user_class.as_deref());

    if let Some(id) = id {
        out.push_str(" id=\"");
        out.push_str(&html_escape(&id));
        out.push('"');
    }

    emit_html_passthrough_attrs(out, &elem.attrs);

    // Source map attributes in dev mode (self-closing)
    if ctx.dev && elem.line_num > 0 {
        out.push_str(&format!(" data-hl-line=\"{}\" data-hl-el=\"{}\"", elem.line_num, kind_label));
    }

    // Image optimization: auto-add loading="lazy" and decoding="async"
    if elem.kind == ElementKind::Image {
        // SVG inlining: @image [inline] logo.svg
        if elem.attrs.iter().any(|a| a.key == "inline") {
            if let Some(src) = &elem.argument {
                if src.ends_with(".svg") {
                    if let Ok(svg_content) = std::fs::read_to_string(src) {
                        // Close the tag we opened, then emit inline SVG instead
                        out.truncate(out.rfind('<').unwrap_or(0));
                        out.push_str(&ctx.indent());
                        out.push_str(svg_content.trim());
                        out.push_str(ctx.nl());
                        return;
                    }
                }
            }
        }
        if !elem.attrs.iter().any(|a| a.key == "loading") {
            out.push_str(" loading=\"lazy\"");
        }
        if !elem.attrs.iter().any(|a| a.key == "decoding") {
            out.push_str(" decoding=\"async\"");
        }
    }

    out.push('>');
    out.push_str(ctx.nl());
}

fn generate_text_segments(
    segments: &[TextSegment],
    out: &mut String,
    styles: &mut StyleCollector,
    ctx: &mut GenContext,
) {
    for segment in segments {
        match segment {
            TextSegment::Plain(text) => out.push_str(&html_escape(text)),
            TextSegment::Inline(elem) => {
                let mut buf = String::new();
                generate_element(elem, None, &mut buf, styles, ctx);
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

    // Collect pseudo-state overrides
    let mut pseudo = Vec::new();
    for &(prefix, selector) in PSEUDO_PREFIXES {
        let css = attrs_to_css(attrs, prefix, kind, parent_kind);
        if !css.is_empty() {
            pseudo.push((selector.to_string(), css));
        }
    }

    // Auto-add ::-webkit-scrollbar pseudo for no-scrollbar attribute
    if attrs.iter().any(|a| a.key == "no-scrollbar") {
        pseudo.push(("::-webkit-scrollbar".to_string(), "display:none;".to_string()));
    }

    // Collect nth:EXPR: dynamic pseudo selectors
    let mut nth_prefixes: Vec<String> = Vec::new();
    for attr in attrs {
        if attr.key.starts_with("nth:") {
            let rest = &attr.key[4..];
            if let Some(colon_pos) = rest.find(':') {
                let prefix = format!("nth:{}:", &rest[..colon_pos]);
                if !nth_prefixes.contains(&prefix) {
                    nth_prefixes.push(prefix);
                }
            }
        }
    }
    for prefix in &nth_prefixes {
        let expr = &prefix[4..prefix.len() - 1];
        let selector = format!(":nth-child({})", expr);
        let css = attrs_to_css(attrs, prefix, kind, parent_kind);
        if !css.is_empty() {
            pseudo.push((selector, css));
        }
    }

    // Collect has(...): dynamic pseudo selectors
    let mut has_prefixes: Vec<String> = Vec::new();
    for attr in attrs {
        if attr.key.starts_with("has(") {
            if let Some(close) = attr.key.find("):") {
                let prefix = format!("{}:", &attr.key[..close + 1]);
                if !has_prefixes.contains(&prefix) {
                    has_prefixes.push(prefix);
                }
            }
        }
    }
    for prefix in &has_prefixes {
        let inner = &prefix[4..prefix.len() - 2]; // extract selector from has(selector):
        let selector = format!(":has({})", inner);
        let css = attrs_to_css(attrs, &prefix, kind, parent_kind);
        if !css.is_empty() {
            pseudo.push((selector, css));
        }
    }

    // Collect responsive overrides
    let mut responsive = Vec::new();
    for &(bp_name, _) in BREAKPOINTS {
        let prefix = format!("{}:", bp_name);
        let css = attrs_to_css(attrs, &prefix, kind, parent_kind);
        if !css.is_empty() {
            responsive.push((bp_name.to_string(), css));
        }
    }

    // Collect container query overrides
    let mut container = Vec::new();
    for &(bp_name, _) in BREAKPOINTS {
        let prefix = format!("cq-{}:", bp_name);
        let css = attrs_to_css(attrs, &prefix, kind, parent_kind);
        if !css.is_empty() {
            container.push((bp_name.to_string(), css));
        }
    }

    let dark = attrs_to_css(attrs, "dark:", kind, parent_kind);
    let print = attrs_to_css(attrs, "print:", kind, parent_kind);
    let motion_safe = attrs_to_css(attrs, "motion-safe:", kind, parent_kind);
    let motion_reduce = attrs_to_css(attrs, "motion-reduce:", kind, parent_kind);
    let landscape = attrs_to_css(attrs, "landscape:", kind, parent_kind);
    let portrait = attrs_to_css(attrs, "portrait:", kind, parent_kind);

    styles.get_class(base, pseudo, responsive, dark, print, motion_safe, motion_reduce, landscape, portrait, container)
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

/// (htmlang prefix, CSS selector suffix)
const PSEUDO_PREFIXES: &[(&str, &str)] = &[
    ("hover:", ":hover"),
    ("active:", ":active"),
    ("focus:", ":focus"),
    ("focus-visible:", ":focus-visible"),
    ("focus-within:", ":focus-within"),
    ("disabled:", ":disabled"),
    ("checked:", ":checked"),
    ("placeholder:", "::placeholder"),
    ("first:", ":first-child"),
    ("last:", ":last-child"),
    ("odd:", ":nth-child(odd)"),
    ("even:", ":nth-child(even)"),
    ("before:", "::before"),
    ("after:", "::after"),
    ("selection:", "::selection"),
    ("visited:", ":visited"),
    ("empty:", ":empty"),
    ("target:", ":target"),
    ("valid:", ":valid"),
    ("invalid:", ":invalid"),
];
const RESPONSIVE_PREFIXES: &[&str] = &["sm:", "md:", "lg:", "xl:", "2xl:"];
const MEDIA_PREFIXES: &[&str] = &["dark:", "print:", "motion-safe:", "motion-reduce:", "landscape:", "portrait:"];
const CONTAINER_QUERY_PREFIXES: &[&str] = &["cq-sm:", "cq-md:", "cq-lg:", "cq-xl:", "cq-2xl:"];

fn is_prefixed_attr(key: &str) -> bool {
    PSEUDO_PREFIXES.iter().any(|&(p, _)| key.starts_with(p))
        || RESPONSIVE_PREFIXES.iter().any(|p| key.starts_with(p))
        || MEDIA_PREFIXES.iter().any(|p| key.starts_with(p))
        || CONTAINER_QUERY_PREFIXES.iter().any(|p| key.starts_with(p))
        || key.starts_with("nth:")
        || key.starts_with("has(")
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
            // Semantic elements get flex column layout like @el
            ElementKind::Nav | ElementKind::Header | ElementKind::Footer
            | ElementKind::Main | ElementKind::Section | ElementKind::Article
            | ElementKind::Aside => css.push_str("display:flex;flex-direction:column;"),
            // Lists: reset browser defaults
            ElementKind::List => css.push_str("margin:0;padding-left:0;list-style:none;"),
            ElementKind::ListItem => css.push_str("display:flex;flex-direction:column;"),
            // Form: flex column like @el
            ElementKind::Form => css.push_str("display:flex;flex-direction:column;"),
            // Details: flex column
            ElementKind::Details => css.push_str("display:flex;flex-direction:column;"),
            // Figure: flex column
            ElementKind::Figure => css.push_str("display:flex;flex-direction:column;margin:0;"),
            // Blockquote: flex column, reset browser margin
            ElementKind::Blockquote => css.push_str("display:flex;flex-direction:column;margin:0;"),
            // Pre: preserve whitespace
            ElementKind::Pre => css.push_str("margin:0;white-space:pre;font-family:ui-monospace,monospace;"),
            // Code: monospace font
            ElementKind::Code => css.push_str("font-family:ui-monospace,monospace;"),
            // New elements
            ElementKind::Dialog => css.push_str("display:flex;flex-direction:column;"),
            ElementKind::DefinitionList => css.push_str("margin:0;"),
            ElementKind::DefinitionDescription => css.push_str("margin:0;display:flex;flex-direction:column;"),
            ElementKind::Fieldset => css.push_str("display:flex;flex-direction:column;border:1px solid currentColor;padding:0.5em;margin:0;"),
            ElementKind::Kbd => css.push_str("font-family:ui-monospace,monospace;"),
            ElementKind::Grid => css.push_str("display:grid;"),
            ElementKind::Stack => css.push_str("position:relative;"),
            ElementKind::Badge => css.push_str("display:inline-flex;align-items:center;justify-content:center;padding:2px 8px;border-radius:9999px;font-size:0.75rem;font-weight:600;line-height:1;"),
            ElementKind::Tooltip => css.push_str("position:relative;cursor:help;"),
            ElementKind::Spacer => css.push_str("flex:1;"),
            ElementKind::Avatar => css.push_str("display:inline-flex;align-items:center;justify-content:center;border-radius:9999px;overflow:hidden;flex-shrink:0;"),
            ElementKind::Carousel => css.push_str("display:flex;flex-direction:row;overflow-x:auto;scroll-snap-type:x mandatory;-webkit-overflow-scrolling:touch;scrollbar-width:none;"),
            ElementKind::Chip => css.push_str("display:inline-flex;align-items:center;gap:4px;padding:4px 12px;border-radius:9999px;font-size:0.875rem;border:1px solid currentColor;"),
            ElementKind::Tag => css.push_str("display:inline-flex;align-items:center;padding:2px 8px;border-radius:4px;font-size:0.75rem;font-weight:600;"),
            _ => {}
        }
        // Children of @carousel get scroll-snap-align and flex-shrink
        if matches!(parent_kind, Some(ElementKind::Carousel)) {
            css.push_str("scroll-snap-align:start;flex-shrink:0;");
        }
    }

    for attr in attrs {
        // Determine the effective key for this pass
        let effective_key = if state_prefix.is_empty() {
            if is_prefixed_attr(&attr.key) {
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
            "spacing" | "gap" => {
                if let Some(v) = val {
                    push_css(&mut css, "gap", &css_px(v));
                }
            }
            "padding" => {
                if let Some(v) = val {
                    push_css(&mut css, "padding", &css_px_multi(v));
                }
            }
            "padding-x" => {
                if let Some(v) = val {
                    push_css(&mut css, "padding-left", &css_px(v));
                    push_css(&mut css, "padding-right", &css_px(v));
                }
            }
            "padding-y" => {
                if let Some(v) = val {
                    push_css(&mut css, "padding-top", &css_px(v));
                    push_css(&mut css, "padding-bottom", &css_px(v));
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
                        _ => push_css(&mut css, "width", &css_px(v)),
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
                        _ => push_css(&mut css, "height", &css_px(v)),
                    }
                }
            }
            "min-width" => {
                if let Some(v) = val {
                    push_css(&mut css, "min-width", &css_px(v));
                }
            }
            "max-width" => {
                if let Some(v) = val {
                    push_css(&mut css, "max-width", &css_px(v));
                }
            }
            "min-height" => {
                if let Some(v) = val {
                    push_css(&mut css, "min-height", &css_px(v));
                }
            }
            "max-height" => {
                if let Some(v) = val {
                    push_css(&mut css, "max-height", &css_px(v));
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
                            &format!("{} solid {}", css_px(parts[0]), parts[1]),
                        );
                    } else {
                        push_css(
                            &mut css,
                            "border",
                            &format!("{} solid currentColor", css_px(parts[0])),
                        );
                    }
                }
            }
            "border-top" | "border-bottom" | "border-left" | "border-right" => {
                if let Some(v) = val {
                    let parts: Vec<&str> = v.splitn(2, ' ').collect();
                    if parts.len() == 2 {
                        push_css(
                            &mut css,
                            effective_key,
                            &format!("{} solid {}", css_px(parts[0]), parts[1]),
                        );
                    } else {
                        push_css(
                            &mut css,
                            effective_key,
                            &format!("{} solid currentColor", css_px(parts[0])),
                        );
                    }
                }
            }
            "rounded" => {
                if let Some(v) = val {
                    push_css(&mut css, "border-radius", &css_px(v));
                }
            }
            "bold" => push_css(&mut css, "font-weight", "bold"),
            "italic" => push_css(&mut css, "font-style", "italic"),
            "underline" => push_css(&mut css, "text-decoration", "underline"),
            "size" => {
                if let Some(v) = val {
                    push_css(&mut css, "font-size", &css_px(v));
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
            "letter-spacing" => {
                if let Some(v) = val {
                    push_css(&mut css, "letter-spacing", &css_px(v));
                }
            }
            "text-transform" => {
                if let Some(v) = val {
                    push_css(&mut css, "text-transform", v);
                }
            }
            "white-space" => {
                if let Some(v) = val {
                    push_css(&mut css, "white-space", v);
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
            "top" => {
                if let Some(v) = val {
                    push_css(&mut css, "top", &css_px(v));
                }
            }
            "right" => {
                if let Some(v) = val {
                    push_css(&mut css, "right", &css_px(v));
                }
            }
            "bottom" => {
                if let Some(v) = val {
                    push_css(&mut css, "bottom", &css_px(v));
                }
            }
            "left" => {
                if let Some(v) = val {
                    push_css(&mut css, "left", &css_px(v));
                }
            }
            "z-index" => {
                if let Some(v) = val {
                    push_css(&mut css, "z-index", v);
                }
            }

            // Display & visibility
            "display" => {
                if let Some(v) = val {
                    push_css(&mut css, "display", v);
                }
            }
            "visibility" => {
                if let Some(v) = val {
                    push_css(&mut css, "visibility", v);
                }
            }
            "hidden" => push_css(&mut css, "display", "none"),

            // Transform & filters
            "transform" => {
                if let Some(v) = val {
                    push_css(&mut css, "transform", v);
                }
            }
            "backdrop-filter" => {
                if let Some(v) = val {
                    push_css(&mut css, "backdrop-filter", v);
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
                    push_css(&mut css, "column-gap", &css_px(v));
                }
            }
            "gap-y" => {
                if let Some(v) = val {
                    push_css(&mut css, "row-gap", &css_px(v));
                }
            }

            // Grid
            "grid" => {
                push_css(&mut css, "display", "grid");
            }
            "grid-cols" => {
                if let Some(v) = val {
                    if let Ok(n) = v.parse::<u32>() {
                        push_css(&mut css, "grid-template-columns", &format!("repeat({},1fr)", n));
                    } else {
                        push_css(&mut css, "grid-template-columns", v);
                    }
                }
            }
            "grid-rows" => {
                if let Some(v) = val {
                    if let Ok(n) = v.parse::<u32>() {
                        push_css(&mut css, "grid-template-rows", &format!("repeat({},1fr)", n));
                    } else {
                        push_css(&mut css, "grid-template-rows", v);
                    }
                }
            }
            "col-span" => {
                if let Some(v) = val {
                    push_css(&mut css, "grid-column", &format!("span {}", v));
                }
            }
            "row-span" => {
                if let Some(v) = val {
                    push_css(&mut css, "grid-row", &format!("span {}", v));
                }
            }

            // Animation
            "animation" => {
                if let Some(v) = val {
                    push_css(&mut css, "animation", v);
                }
            }

            // Aspect ratio
            "aspect-ratio" => {
                if let Some(v) = val {
                    push_css(&mut css, "aspect-ratio", v);
                }
            }

            // Outline (like border but doesn't affect layout)
            "outline" => {
                if let Some(v) = val {
                    let parts: Vec<&str> = v.splitn(2, ' ').collect();
                    if parts.len() == 2 {
                        push_css(
                            &mut css,
                            "outline",
                            &format!("{} solid {}", css_px(parts[0]), parts[1]),
                        );
                    } else {
                        push_css(
                            &mut css,
                            "outline",
                            &format!("{} solid currentColor", css_px(parts[0])),
                        );
                    }
                }
            }

            // Logical properties (i18n-aware)
            "padding-inline" => {
                if let Some(v) = val {
                    push_css(&mut css, "padding-inline", &css_px_multi(v));
                }
            }
            "padding-block" => {
                if let Some(v) = val {
                    push_css(&mut css, "padding-block", &css_px_multi(v));
                }
            }
            "margin-inline" => {
                if let Some(v) = val {
                    push_css(&mut css, "margin-inline", &css_px_multi(v));
                }
            }
            "margin-block" => {
                if let Some(v) = val {
                    push_css(&mut css, "margin-block", &css_px_multi(v));
                }
            }

            // Scroll snap
            "scroll-snap-type" => {
                if let Some(v) = val {
                    push_css(&mut css, "scroll-snap-type", v);
                }
            }
            "scroll-snap-align" => {
                if let Some(v) = val {
                    push_css(&mut css, "scroll-snap-align", v);
                }
            }

            // Margin
            "margin" => {
                if let Some(v) = val {
                    push_css(&mut css, "margin", &css_px_multi(v));
                }
            }
            "margin-x" => {
                if let Some(v) = val {
                    push_css(&mut css, "margin-left", &css_px(v));
                    push_css(&mut css, "margin-right", &css_px(v));
                }
            }
            "margin-y" => {
                if let Some(v) = val {
                    push_css(&mut css, "margin-top", &css_px(v));
                    push_css(&mut css, "margin-bottom", &css_px(v));
                }
            }

            // Filter
            "filter" => {
                if let Some(v) = val {
                    push_css(&mut css, "filter", v);
                }
            }

            // Object fit/position (for images)
            "object-fit" => {
                if let Some(v) = val {
                    push_css(&mut css, "object-fit", v);
                }
            }
            "object-position" => {
                if let Some(v) = val {
                    push_css(&mut css, "object-position", v);
                }
            }

            // Text shadow
            "text-shadow" => {
                if let Some(v) = val {
                    push_css(&mut css, "text-shadow", v);
                }
            }

            // Text overflow
            "text-overflow" => {
                if let Some(v) = val {
                    push_css(&mut css, "text-overflow", v);
                }
            }

            // Interaction
            "pointer-events" => {
                if let Some(v) = val {
                    push_css(&mut css, "pointer-events", v);
                }
            }
            "user-select" => {
                if let Some(v) = val {
                    push_css(&mut css, "user-select", v);
                }
            }

            // Flexbox/grid alignment
            "justify-content" => {
                if let Some(v) = val {
                    push_css(&mut css, "justify-content", v);
                }
            }
            "align-items" => {
                if let Some(v) = val {
                    push_css(&mut css, "align-items", v);
                }
            }

            // Flex item order
            "order" => {
                if let Some(v) = val {
                    push_css(&mut css, "order", v);
                }
            }

            // Background extras
            "background-size" => {
                if let Some(v) = val {
                    push_css(&mut css, "background-size", v);
                }
            }
            "background-position" => {
                if let Some(v) = val {
                    push_css(&mut css, "background-position", v);
                }
            }
            "background-repeat" => {
                if let Some(v) = val {
                    push_css(&mut css, "background-repeat", v);
                }
            }

            // Text wrapping
            "word-break" => {
                if let Some(v) = val {
                    push_css(&mut css, "word-break", v);
                }
            }
            "overflow-wrap" => {
                if let Some(v) = val {
                    push_css(&mut css, "overflow-wrap", v);
                }
            }

            // Container queries
            "container" => {
                push_css(&mut css, "container-type", "inline-size");
            }
            "container-name" => {
                if let Some(v) = val {
                    push_css(&mut css, "container-name", v);
                }
            }
            "container-type" => {
                if let Some(v) = val {
                    push_css(&mut css, "container-type", v);
                }
            }

            // Overflow axis
            "overflow-x" => {
                if let Some(v) = val {
                    push_css(&mut css, "overflow-x", v);
                }
            }
            "overflow-y" => {
                if let Some(v) = val {
                    push_css(&mut css, "overflow-y", v);
                }
            }

            // Inset (shorthand for top/right/bottom/left)
            "inset" => {
                if let Some(v) = val {
                    push_css(&mut css, "inset", &css_px(v));
                }
            }

            // Accent & caret colors
            "accent-color" => {
                if let Some(v) = val {
                    push_css(&mut css, "accent-color", v);
                }
            }
            "caret-color" => {
                if let Some(v) = val {
                    push_css(&mut css, "caret-color", v);
                }
            }

            // List style
            "list-style" => {
                if let Some(v) = val {
                    push_css(&mut css, "list-style", v);
                }
            }

            // Table
            "border-collapse" => {
                if let Some(v) = val {
                    push_css(&mut css, "border-collapse", v);
                }
            }
            "border-spacing" => {
                if let Some(v) = val {
                    push_css(&mut css, "border-spacing", &css_px(v));
                }
            }

            // Text decoration
            "text-decoration" => {
                if let Some(v) = val {
                    push_css(&mut css, "text-decoration", v);
                }
            }
            "text-decoration-color" => {
                if let Some(v) = val {
                    push_css(&mut css, "text-decoration-color", v);
                }
            }
            "text-decoration-thickness" => {
                if let Some(v) = val {
                    push_css(&mut css, "text-decoration-thickness", &css_px(v));
                }
            }
            "text-decoration-style" => {
                if let Some(v) = val {
                    push_css(&mut css, "text-decoration-style", v);
                }
            }
            "text-underline-offset" => {
                if let Some(v) = val {
                    push_css(&mut css, "text-underline-offset", &css_px(v));
                }
            }

            // Multi-column
            "column-width" => {
                if let Some(v) = val {
                    push_css(&mut css, "column-width", &css_px(v));
                }
            }
            "column-rule" => {
                if let Some(v) = val {
                    push_css(&mut css, "column-rule", v);
                }
            }

            // Place items/self
            "place-items" => {
                if let Some(v) = val {
                    push_css(&mut css, "place-items", v);
                }
            }
            "place-self" => {
                if let Some(v) = val {
                    push_css(&mut css, "place-self", v);
                }
            }

            // Scroll behavior
            "scroll-behavior" => {
                if let Some(v) = val {
                    push_css(&mut css, "scroll-behavior", v);
                }
            }

            // Resize
            "resize" => {
                if let Some(v) = val {
                    push_css(&mut css, "resize", v);
                }
            }

            // New CSS properties
            "clip-path" => {
                if let Some(v) = val { push_css(&mut css, "clip-path", v); }
            }
            "mix-blend-mode" => {
                if let Some(v) = val { push_css(&mut css, "mix-blend-mode", v); }
            }
            "background-blend-mode" => {
                if let Some(v) = val { push_css(&mut css, "background-blend-mode", v); }
            }
            "writing-mode" => {
                if let Some(v) = val { push_css(&mut css, "writing-mode", v); }
            }
            "column-count" => {
                if let Some(v) = val { push_css(&mut css, "column-count", v); }
            }
            "column-gap" => {
                if let Some(v) = val { push_css(&mut css, "column-gap", &css_px(v)); }
            }
            "text-indent" => {
                if let Some(v) = val { push_css(&mut css, "text-indent", &css_px(v)); }
            }
            "hyphens" => {
                if let Some(v) = val { push_css(&mut css, "hyphens", v); }
            }
            "flex-grow" => {
                if let Some(v) = val { push_css(&mut css, "flex-grow", v); }
            }
            "flex-shrink" => {
                if let Some(v) = val { push_css(&mut css, "flex-shrink", v); }
            }
            "flex-basis" => {
                if let Some(v) = val { push_css(&mut css, "flex-basis", &css_px(v)); }
            }
            "isolation" => {
                if let Some(v) = val { push_css(&mut css, "isolation", v); }
            }
            "place-content" => {
                if let Some(v) = val { push_css(&mut css, "place-content", v); }
            }
            "background-image" => {
                if let Some(v) = val { push_css(&mut css, "background-image", v); }
            }
            "font-weight" => {
                if let Some(v) = val { push_css(&mut css, "font-weight", v); }
            }
            "font-style" => {
                if let Some(v) = val { push_css(&mut css, "font-style", v); }
            }
            "text-wrap" => {
                if let Some(v) = val { push_css(&mut css, "text-wrap", v); }
            }
            "will-change" => {
                if let Some(v) = val { push_css(&mut css, "will-change", v); }
            }
            "touch-action" => {
                if let Some(v) = val { push_css(&mut css, "touch-action", v); }
            }
            "vertical-align" => {
                if let Some(v) = val { push_css(&mut css, "vertical-align", v); }
            }
            "contain" => {
                if let Some(v) = val { push_css(&mut css, "contain", v); }
            }
            "scroll-margin" => {
                if let Some(v) = val { push_css(&mut css, "scroll-margin", &css_px(v)); }
            }
            "scroll-margin-top" | "scroll-margin-bottom" | "scroll-margin-left" | "scroll-margin-right" => {
                if let Some(v) = val { push_css(&mut css, effective_key, &css_px(v)); }
            }
            "scroll-padding" => {
                if let Some(v) = val { push_css(&mut css, "scroll-padding", &css_px(v)); }
            }
            "scroll-padding-top" | "scroll-padding-bottom" | "scroll-padding-left" | "scroll-padding-right" => {
                if let Some(v) = val { push_css(&mut css, effective_key, &css_px(v)); }
            }
            "content-visibility" => {
                if let Some(v) = val { push_css(&mut css, "content-visibility", v); }
            }
            "direction" => {
                if let Some(v) = val { push_css(&mut css, "direction", v); }
            }

            "content" => {
                if let Some(v) = val {
                    // Wrap in quotes if not already quoted and not a CSS keyword
                    if v.starts_with('"') || v.starts_with('\'') || v == "none" || v == "normal" || v.starts_with("attr(") || v.starts_with("counter(") {
                        push_css(&mut css, "content", v);
                    } else {
                        push_css(&mut css, "content", &format!("\"{}\"", v));
                    }
                }
            }

            // --- CSS Shorthands ---

            "truncate" => {
                push_css(&mut css, "overflow", "hidden");
                push_css(&mut css, "text-overflow", "ellipsis");
                push_css(&mut css, "white-space", "nowrap");
            }
            "line-clamp" => {
                if let Some(v) = val {
                    push_css(&mut css, "display", "-webkit-box");
                    push_css(&mut css, "-webkit-line-clamp", v);
                    push_css(&mut css, "-webkit-box-orient", "vertical");
                    push_css(&mut css, "overflow", "hidden");
                }
            }
            "blur" => {
                if let Some(v) = val {
                    push_css(&mut css, "filter", &format!("blur({})", css_px(v)));
                }
            }
            "backdrop-blur" => {
                if let Some(v) = val {
                    push_css(&mut css, "backdrop-filter", &format!("blur({})", css_px(v)));
                }
            }
            "no-scrollbar" => {
                push_css(&mut css, "scrollbar-width", "none");
                push_css(&mut css, "-ms-overflow-style", "none");
            }
            "skeleton" => {
                push_css(&mut css, "background", "linear-gradient(90deg,#e5e7eb 25%,#f3f4f6 50%,#e5e7eb 75%)");
                push_css(&mut css, "background-size", "200% 100%");
                push_css(&mut css, "animation", "hl-skeleton 1.5s ease-in-out infinite");
            }
            "gradient" => {
                if let Some(v) = val {
                    // Parse: "from to [angle]" or "color1 color2 [angle]"
                    let parts: Vec<&str> = v.split_whitespace().collect();
                    let bg = if parts.len() >= 3 && (parts[2].ends_with("deg") || parts[2].ends_with("turn") || parts[2].ends_with("rad")) {
                        format!("linear-gradient({},{},{})", parts[2], parts[0], parts[1])
                    } else if parts.len() >= 2 {
                        format!("linear-gradient({},{})", parts[0], parts[1])
                    } else {
                        format!("linear-gradient({},transparent)", parts[0])
                    };
                    push_css(&mut css, "background", &bg);
                }
            }

            // Grid areas
            "grid-template-areas" => {
                if let Some(v) = val { push_css(&mut css, "grid-template-areas", v); }
            }
            "grid-area" => {
                if let Some(v) = val { push_css(&mut css, "grid-area", v); }
            }

            // View transitions
            "view-transition-name" => {
                if let Some(v) = val { push_css(&mut css, "view-transition-name", v); }
            }

            // Animate shorthand (alias for animation)
            "animate" => {
                if let Some(v) = val { push_css(&mut css, "animation", v); }
            }

            // Critical CSS hint — not CSS, handled elsewhere
            "critical" => {}

            // Identity and HTML passthrough — not CSS
            "id" | "class" => {}
            "type" | "placeholder" | "name" | "value" | "disabled" | "required"
            | "checked" | "for" | "action" | "method" | "autocomplete" | "min"
            | "max" | "step" | "pattern" | "maxlength" | "rows" | "cols"
            | "multiple" | "alt" | "role" | "tabindex" | "title"
            | "controls" | "autoplay" | "loop" | "muted" | "poster" | "preload"
            | "loading" | "decoding" | "ordered" | "src"
            | "open" | "novalidate" | "low" | "high" | "optimum"
            | "colspan" | "rowspan" | "scope" | "inline"
            | "datetime" | "media" | "sizes" | "srcset" | "cite" | "list"
            | "sandbox" | "allow" | "allowfullscreen" | "referrerpolicy"
            | "formaction" | "formmethod" | "formtarget" | "target"
            | "autofocus" => {}

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

/// Known CSS units — if a value ends with one, skip appending `px`.
const CSS_UNITS: &[&str] = &[
    "%", "rem", "em", "vh", "vw", "vmin", "vmax", "dvh", "svh", "lvh",
    "ch", "ex", "cm", "mm", "in", "pt", "pc", "fr",
];

/// Format a numeric value: if it already has a CSS unit, pass through as-is;
/// otherwise append `px`.
fn css_px(value: &str) -> String {
    let v = value.trim();
    if v == "0" {
        return "0".to_string();
    }
    if CSS_UNITS.iter().any(|u| v.ends_with(u)) {
        return v.to_string();
    }
    if v.starts_with("var(") || v.starts_with("calc(") || v.starts_with("clamp(") || v.starts_with("min(") || v.starts_with("max(") {
        return v.to_string();
    }
    format!("{}px", v)
}

/// Format multiple space-separated values, each getting px if needed.
fn css_px_multi(value: &str) -> String {
    value
        .split_whitespace()
        .map(|p| css_px(p))
        .collect::<Vec<_>>()
        .join(" ")
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

fn base64_encode(data: &[u8]) -> String {
    const CHARS: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut result = String::new();
    for chunk in data.chunks(3) {
        let b0 = chunk[0] as u32;
        let b1 = if chunk.len() > 1 { chunk[1] as u32 } else { 0 };
        let b2 = if chunk.len() > 2 { chunk[2] as u32 } else { 0 };
        let triple = (b0 << 16) | (b1 << 8) | b2;
        result.push(CHARS[((triple >> 18) & 0x3F) as usize] as char);
        result.push(CHARS[((triple >> 12) & 0x3F) as usize] as char);
        if chunk.len() > 1 {
            result.push(CHARS[((triple >> 6) & 0x3F) as usize] as char);
        } else {
            result.push('=');
        }
        if chunk.len() > 2 {
            result.push(CHARS[(triple & 0x3F) as usize] as char);
        } else {
            result.push('=');
        }
    }
    result
}
