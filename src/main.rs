use std::collections::HashMap;
use std::env;
use std::fs;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::process;
use std::sync::Mutex;

struct DiagnosticJson {
    file: String,
    line: usize,
    severity: String,
    message: String,
}

fn compile(input_path: &str, dev: bool, error_overlay: bool, check_only: bool, output_path: Option<&str>, format_json: bool, json_collector: Option<&Mutex<Vec<DiagnosticJson>>>) -> (bool, Vec<PathBuf>) {
    let input = match fs::read_to_string(input_path) {
        Ok(s) => s,
        Err(e) => {
            if format_json {
                if let Some(collector) = json_collector {
                    collector.lock().unwrap().push(DiagnosticJson {
                        file: input_path.to_string(),
                        line: 0,
                        severity: "error".to_string(),
                        message: format!("{}", e),
                    });
                }
            } else {
                eprintln!("error: {}: {}", input_path, e);
            }
            return (true, vec![]);
        }
    };

    let base = Path::new(input_path).parent();
    let result = htmlang::parser::parse_with_base(&input, base);

    if format_json {
        if let Some(collector) = json_collector {
            let mut collected = collector.lock().unwrap();
            for d in &result.diagnostics {
                let severity = match d.severity {
                    htmlang::parser::Severity::Error => "error",
                    htmlang::parser::Severity::Warning => "warning",
                };
                collected.push(DiagnosticJson {
                    file: input_path.to_string(),
                    line: d.line,
                    severity: severity.to_string(),
                    message: d.message.clone(),
                });
            }
        }
    } else {
        for d in &result.diagnostics {
            let prefix = match d.severity {
                htmlang::parser::Severity::Error => "error",
                htmlang::parser::Severity::Warning => "warning",
            };
            eprintln!("{}: line {}: {}", prefix, d.line, d.message);
            if let Some(ref src) = d.source_line {
                eprintln!("  | {}", src);
            }
        }
    }

    let has_errors = result
        .diagnostics
        .iter()
        .any(|d| d.severity == htmlang::parser::Severity::Error);

    let out_path = match output_path {
        Some(p) => PathBuf::from(p),
        None => Path::new(input_path).with_extension("html"),
    };

    if !check_only {
        if has_errors {
            if error_overlay {
                let error_html = generate_error_overlay(&result.diagnostics, input_path);
                let _ = fs::write(&out_path, &error_html);
            }
        } else {
            let html = if dev {
                htmlang::codegen::generate_dev(&result.document)
            } else {
                htmlang::codegen::generate(&result.document)
            };
            match fs::write(&out_path, &html) {
                Ok(()) => eprintln!("wrote {}", out_path.display()),
                Err(e) => eprintln!("error: {}: {}", out_path.display(), e),
            }
        }
    }

    (has_errors, result.included_files)
}

fn print_json_diagnostics(diagnostics: &[DiagnosticJson]) {
    let mut json = String::from("{\"diagnostics\":[");
    for (i, d) in diagnostics.iter().enumerate() {
        if i > 0 {
            json.push(',');
        }
        let escaped_file = d.file.replace('\\', "\\\\").replace('"', "\\\"");
        let escaped_msg = d.message.replace('\\', "\\\\").replace('"', "\\\"").replace('\n', "\\n");
        json.push_str(&format!(
            "{{\"file\":\"{}\",\"line\":{},\"severity\":\"{}\",\"message\":\"{}\"}}",
            escaped_file, d.line, d.severity, escaped_msg
        ));
    }
    json.push_str("]}");
    println!("{}", json);
}

fn kebab_to_title(s: &str) -> String {
    s.split('-')
        .map(|word| {
            let mut chars = word.chars();
            match chars.next() {
                Some(c) => {
                    let mut title = c.to_uppercase().to_string();
                    title.extend(chars);
                    title
                }
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn copy_non_hl_files(src_dir: &Path, out_dir: &Path) {
    copy_non_hl_recursive(src_dir, src_dir, out_dir);
}

fn copy_non_hl_recursive(base: &Path, dir: &Path, out_dir: &Path) {
    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                copy_non_hl_recursive(base, &path, out_dir);
            } else if path.is_file() && path.extension().map_or(true, |e| e != "hl") {
                let rel = path.strip_prefix(base).unwrap_or(&path);
                let dest = out_dir.join(rel);
                if let Some(parent) = dest.parent() {
                    let _ = fs::create_dir_all(parent);
                }
                match fs::copy(&path, &dest) {
                    Ok(_) => eprintln!("copied {}", dest.display()),
                    Err(e) => eprintln!("error: copy {}: {}", dest.display(), e),
                }
            }
        }
    }
}

fn generate_error_overlay(diagnostics: &[htmlang::parser::Diagnostic], file: &str) -> String {
    let mut errors = String::new();
    for d in diagnostics {
        let prefix = match d.severity {
            htmlang::parser::Severity::Error => "error",
            htmlang::parser::Severity::Warning => "warning",
        };
        let escaped = d.message
            .replace('&', "&amp;")
            .replace('<', "&lt;")
            .replace('>', "&gt;");
        errors.push_str(&format!(
            "<div class=\"entry\"><span class=\"badge {}\">{}</span> line {}: {}</div>",
            prefix, prefix, d.line, escaped
        ));
    }
    format!(
        r#"<!DOCTYPE html><html><head><meta charset="utf-8"><title>Build Error</title><style>
*{{margin:0;box-sizing:border-box}}
body{{background:#1a1a2e;color:#eee;font-family:ui-monospace,monospace;padding:2rem}}
h1{{color:#ff6b6b;margin-bottom:1rem;font-size:1.5rem}}
.file{{color:#888;margin-bottom:1.5rem;font-size:0.9rem}}
.entry{{padding:0.5rem 0;border-bottom:1px solid #333}}
.badge{{display:inline-block;padding:2px 8px;border-radius:4px;font-size:0.8rem;margin-right:8px}}
.badge.error{{background:#c0392b;color:white}}
.badge.warning{{background:#f39c12;color:white}}
</style></head><body>
<h1>Build Error</h1>
<div class="file">{file}</div>
{errors}
</body></html>"#,
        file = file,
        errors = errors,
    )
}

fn print_help() {
    eprintln!(
        "\
htmlang {} - a minimalist layout language that compiles to static HTML

Usage: htmlang [options] <file.hl | directory>

Commands:
  init [dir]            Create a new project (defaults to current directory)
  new <page-name>       Create a new .hl page from a template
  build <dir> [-o <out>]  Compile all .hl files recursively (parallel)
  serve [dir|file] [-p N] [--open]  Start dev server with hot reload
  watch [dir|file] [-o out]  Watch for changes and recompile
  check <file.hl | dir> Check for errors without writing output
  convert <file.html>   Convert an HTML file to .hl format (stdout)
  fmt <file.hl>         Format a .hl file (normalizes indentation)
  sitemap <dir>         Generate sitemap.xml from .hl files
  lint <file.hl | dir>  Stricter lint checks (accessibility, nesting, etc.)
  stats <file.hl | dir> Show file statistics (elements, CSS rules, colors)
  preview <file.hl>     Compile and open in browser (one-shot, no server)
  diff <a.hl> <b.hl>    Show differences between two .hl files
  export <dir> [-o f]   Compile and bundle into an archive
  repl                  Interactive REPL (type .hl, get HTML)
  feed <dir> [-b URL]   Generate RSS feed from @page metadata
  components <dir>      List all @fn definitions across a project

Options:
  -w, --watch       Watch for changes and recompile
  -s, --serve       Start dev server with hot reload (implies --watch)
  -p, --port <N>    Port for dev server (default: 3000)
  --open            Open browser automatically (with --serve)
  -o, --output <path>  Output file/directory path
  -d, --dev         Development mode
  -c, --check       Check for errors without writing output
  --format json     Output diagnostics as JSON to stdout
  -h, --help        Show this help
  -V, --version     Show version

Examples:
  htmlang init              Scaffold a new project
  htmlang init my-site      Scaffold in a new directory
  htmlang new about-us      Create about-us.hl from template
  htmlang page.hl           Compile page.hl to page.html
  htmlang site/             Compile all .hl files in directory
  htmlang -w page.hl        Recompile on file changes
  htmlang -s page.hl        Start dev server with hot reload
  htmlang -s --open site/   Serve and open browser
  htmlang -s site/          Serve a multi-page site
  htmlang -s -p 8080 page.hl
  htmlang -c page.hl        Lint without writing output
  htmlang check src/        Check all files in a directory
  htmlang convert page.html Convert HTML to .hl format
  htmlang --format json page.hl  Get diagnostics as JSON
  htmlang fmt page.hl       Format a file
  htmlang build src/ -o dist/  Compile all .hl files to dist/
  htmlang sitemap src/      Generate sitemap.xml
  htmlang lint src/         Lint all files in directory
  htmlang stats page.hl     Show file statistics",
        env!("CARGO_PKG_VERSION")
    );
}

fn init_project(dir: &str) {
    let dir = Path::new(dir);
    if dir.to_str() != Some(".") {
        if let Err(e) = fs::create_dir_all(dir) {
            eprintln!("error: cannot create directory '{}': {}", dir.display(), e);
            process::exit(1);
        }
    }

    let index_path = dir.join("index.hl");
    if index_path.exists() {
        eprintln!("error: {} already exists", index_path.display());
        process::exit(1);
    }

    let template = r#"@page My Site
@let primary #3b82f6

@column [max-width 800, center-x, padding 40, spacing 20]
  @text [bold, size 32] Hello, htmlang!

  @paragraph [line-height 1.6]
    Edit {@text [bold, color $primary] index.hl} and run
    {@text [font monospace, size 14] htmlang -s .} to get started.

  @row [spacing 10]
    @el [padding 12 24, background $primary, rounded 8, cursor pointer, hover:background #2563eb, transition all 0.15s ease] > @link https://github.com/nicholasgasior/htmlang
      @text [color white, bold] Documentation
"#;

    match fs::write(&index_path, template) {
        Ok(()) => eprintln!("created {}", index_path.display()),
        Err(e) => {
            eprintln!("error: {}: {}", index_path.display(), e);
            process::exit(1);
        }
    }
}

fn collect_hl_files_recursive(dir: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    collect_hl_recursive_inner(dir, &mut files);
    files.sort();
    files
}

fn collect_hl_recursive_inner(dir: &Path, files: &mut Vec<PathBuf>) {
    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                collect_hl_recursive_inner(&path, files);
            } else if path.is_file() && path.extension().map_or(false, |e| e == "hl") {
                files.push(path);
            }
        }
    }
}

fn collect_all_files_recursive(dir: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    fn walk(dir: &Path, files: &mut Vec<PathBuf>) {
        if let Ok(entries) = fs::read_dir(dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    walk(&path, files);
                } else if path.is_file() {
                    files.push(path);
                }
            }
        }
    }
    walk(dir, &mut files);
    files.sort();
    files
}

fn generate_sitemap(dir: &str, base_url: &str) {
    let dir = Path::new(dir);
    let hl_files = collect_hl_files_recursive(dir);
    if hl_files.is_empty() {
        eprintln!("no .hl files found in {}", dir.display());
        process::exit(1);
    }
    let mut xml = String::from("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<urlset xmlns=\"http://www.sitemaps.org/schemas/sitemap/0.9\">\n");
    for file in &hl_files {
        let rel = file.strip_prefix(dir).unwrap_or(file);
        let url_path = rel.with_extension("html").to_string_lossy().replace('\\', "/");
        let url = if url_path == "index.html" {
            format!("{}/", base_url.trim_end_matches('/'))
        } else {
            format!("{}/{}", base_url.trim_end_matches('/'), url_path)
        };
        xml.push_str(&format!("  <url><loc>{}</loc></url>\n", url));
    }
    xml.push_str("</urlset>\n");
    let out_path = dir.join("sitemap.xml");
    match fs::write(&out_path, &xml) {
        Ok(()) => eprintln!("wrote {}", out_path.display()),
        Err(e) => {
            eprintln!("error: {}: {}", out_path.display(), e);
            process::exit(1);
        }
    }
}

fn lint_file(path: &str) -> Vec<String> {
    let input = match fs::read_to_string(path) {
        Ok(s) => s,
        Err(e) => return vec![format!("error: {}: {}", path, e)],
    };
    let base = Path::new(path).parent();
    let result = htmlang::parser::parse_with_base(&input, base);
    let mut warnings = Vec::new();

    // Report parser diagnostics
    for d in &result.diagnostics {
        let prefix = match d.severity {
            htmlang::parser::Severity::Error => "error",
            htmlang::parser::Severity::Warning => "warning",
        };
        warnings.push(format!("{}:{}:{}: {}", path, d.line, prefix, d.message));
    }

    // Additional lint checks on the AST
    lint_nodes(&result.document.nodes, path, 0, &mut warnings);
    warnings
}

fn lint_nodes(nodes: &[htmlang::ast::Node], path: &str, depth: usize, warnings: &mut Vec<String>) {
    for node in nodes {
        if let htmlang::ast::Node::Element(elem) = node {
            // Deeply nested elements (>10 levels)
            if depth > 10 {
                warnings.push(format!("{}:{}:lint: deeply nested element ({} levels) — consider simplifying", path, elem.line_num, depth));
            }

            // @image without alt
            if elem.kind == htmlang::ast::ElementKind::Image {
                if !elem.attrs.iter().any(|a| a.key == "alt") {
                    warnings.push(format!("{}:{}:lint: @image missing 'alt' attribute (accessibility)", path, elem.line_num));
                }
            }

            // @link without content or aria-label
            if elem.kind == htmlang::ast::ElementKind::Link {
                let has_aria = elem.attrs.iter().any(|a| a.key == "aria-label");
                let has_children = !elem.children.is_empty();
                let has_arg_text = elem.argument.as_ref().map_or(false, |_| false);
                if !has_aria && !has_children && !has_arg_text {
                    warnings.push(format!("{}:{}:lint: @link has no visible text or aria-label (accessibility)", path, elem.line_num));
                }
            }

            // @input without type
            if elem.kind == htmlang::ast::ElementKind::Input {
                if !elem.attrs.iter().any(|a| a.key == "type") {
                    warnings.push(format!("{}:{}:lint: @input missing 'type' attribute", path, elem.line_num));
                }
            }

            // Empty containers (no children, no text)
            if matches!(elem.kind,
                htmlang::ast::ElementKind::Row | htmlang::ast::ElementKind::Column | htmlang::ast::ElementKind::El
            ) && elem.children.is_empty() {
                warnings.push(format!("{}:{}:lint: empty container (@{}) has no children", path, elem.line_num,
                    match elem.kind {
                        htmlang::ast::ElementKind::Row => "row",
                        htmlang::ast::ElementKind::Column => "column",
                        _ => "el",
                    }
                ));
            }

            // @button without type
            if elem.kind == htmlang::ast::ElementKind::Button {
                if !elem.attrs.iter().any(|a| a.key == "type") {
                    warnings.push(format!("{}:{}:lint: @button missing 'type' attribute (defaults to submit)", path, elem.line_num));
                }
            }

            lint_nodes(&elem.children, path, depth + 1, warnings);
        }
    }
}

fn stats_file(path: &str) {
    let input = match fs::read_to_string(path) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("error: {}: {}", path, e);
            process::exit(1);
        }
    };
    let base = Path::new(path).parent();
    let result = htmlang::parser::parse_with_base(&input, base);
    let html = htmlang::codegen::generate(&result.document);

    let mut element_count = 0;
    let mut colors = std::collections::HashSet::new();
    let mut fonts = std::collections::HashSet::new();
    count_elements(&result.document.nodes, &mut element_count, &mut colors, &mut fonts);

    // Count CSS rules (approximate from generated style block)
    let css_rules = html.matches('{').count().saturating_sub(1); // subtract the html/head/body structure

    let source_bytes = input.len();
    let output_bytes = html.len();

    eprintln!("--- {} ---", path);
    eprintln!("  source size:    {} bytes ({} lines)", source_bytes, input.lines().count());
    eprintln!("  output size:    {} bytes", output_bytes);
    eprintln!("  elements:       {}", element_count);
    eprintln!("  CSS rules:      ~{}", css_rules);
    eprintln!("  unique colors:  {}", colors.len());
    if !colors.is_empty() {
        let mut sorted: Vec<_> = colors.iter().collect();
        sorted.sort();
        eprintln!("    {}", sorted.iter().map(|s| s.as_str()).collect::<Vec<_>>().join(", "));
    }
    eprintln!("  unique fonts:   {}", fonts.len());
    if !fonts.is_empty() {
        let mut sorted: Vec<_> = fonts.iter().collect();
        sorted.sort();
        eprintln!("    {}", sorted.iter().map(|s| s.as_str()).collect::<Vec<_>>().join(", "));
    }
    if result.diagnostics.iter().any(|d| d.severity == htmlang::parser::Severity::Error) {
        eprintln!("  errors:         {}", result.diagnostics.iter().filter(|d| d.severity == htmlang::parser::Severity::Error).count());
    }
    let warn_count = result.diagnostics.iter().filter(|d| d.severity == htmlang::parser::Severity::Warning).count();
    if warn_count > 0 {
        eprintln!("  warnings:       {}", warn_count);
    }
}

fn count_elements(
    nodes: &[htmlang::ast::Node],
    count: &mut usize,
    colors: &mut std::collections::HashSet<String>,
    fonts: &mut std::collections::HashSet<String>,
) {
    for node in nodes {
        if let htmlang::ast::Node::Element(elem) = node {
            *count += 1;
            for attr in &elem.attrs {
                let key = attr.key.as_str();
                // Strip pseudo/media prefixes for color/font detection
                let base_key = key.split(':').last().unwrap_or(key);
                if matches!(base_key, "color" | "background") {
                    if let Some(ref v) = attr.value {
                        colors.insert(v.clone());
                    }
                }
                if base_key == "font" {
                    if let Some(ref v) = attr.value {
                        fonts.insert(v.clone());
                    }
                }
            }
            count_elements(&elem.children, count, colors, fonts);
        }
    }
}

fn open_in_browser(port: u16) {
    let url = format!("http://127.0.0.1:{}", port);
    let cmd = if cfg!(target_os = "macos") {
        "open"
    } else {
        "xdg-open"
    };
    let _ = std::process::Command::new(cmd).arg(&url).spawn();
}

// ---------------------------------------------------------------------------
// Config file support (htmlang.toml)
// ---------------------------------------------------------------------------

struct ProjectConfig {
    output: Option<String>,
    port: u16,
    variables: Vec<(String, String)>,
    breakpoints: Vec<(String, String)>,
}

fn load_config(target: &Path) -> ProjectConfig {
    let mut config = ProjectConfig {
        output: None,
        port: 3000,
        variables: Vec::new(),
        breakpoints: Vec::new(),
    };

    let config_path = if target.is_dir() {
        target.join("htmlang.toml")
    } else {
        target.parent().unwrap_or(Path::new(".")).join("htmlang.toml")
    };

    let content = match fs::read_to_string(&config_path) {
        Ok(s) => s,
        Err(_) => return config,
    };

    let mut section = "";
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        if trimmed.starts_with('[') && trimmed.ends_with(']') {
            section = &trimmed[1..trimmed.len() - 1];
            continue;
        }
        if let Some((key, value)) = trimmed.split_once('=') {
            let key = key.trim();
            let value = value.trim().trim_matches('"');
            match section {
                "" => match key {
                    "output" => config.output = Some(value.to_string()),
                    "port" => config.port = value.parse().unwrap_or(3000),
                    _ => {}
                },
                "variables" => {
                    config.variables.push((key.to_string(), value.to_string()));
                }
                "breakpoints" => {
                    config.breakpoints.push((key.to_string(), value.to_string()));
                }
                _ => {}
            }
        }
    }

    config
}

fn main() {
    let args: Vec<String> = env::args().collect();
    let mut watch = false;
    let mut serve = false;
    let mut dev = false;
    let mut check = false;
    let mut format_json = false;
    let mut open_browser = false;
    let mut port: u16 = 3000;
    let mut output_path: Option<String> = None;
    let mut input_path = None;

    // Handle "init" subcommand
    if args.len() >= 2 && args[1] == "init" {
        let dir = if args.len() >= 3 { &args[2] } else { "." };
        init_project(dir);
        return;
    }

    // Handle "fmt" subcommand
    if args.len() >= 3 && args[1] == "fmt" {
        let file = &args[2];
        match fs::read_to_string(file) {
            Ok(input) => {
                let formatted = htmlang::fmt::format(&input);
                match fs::write(file, &formatted) {
                    Ok(()) => eprintln!("formatted {}", file),
                    Err(e) => {
                        eprintln!("error: {}: {}", file, e);
                        process::exit(1);
                    }
                }
            }
            Err(e) => {
                eprintln!("error: {}: {}", file, e);
                process::exit(1);
            }
        }
        return;
    }

    // Handle "build" subcommand
    if args.len() >= 2 && args[1] == "build" {
        let mut src_dir = None;
        let mut out_dir = None;
        let mut i = 2;
        while i < args.len() {
            match args[i].as_str() {
                "-o" | "--output" => {
                    i += 1;
                    out_dir = args.get(i).map(|s| s.as_str());
                }
                _ if src_dir.is_none() => src_dir = Some(args[i].as_str()),
                _ => {
                    eprintln!("unknown argument: {}", args[i]);
                    process::exit(1);
                }
            }
            i += 1;
        }
        let src = src_dir.unwrap_or(".");
        let dir = Path::new(src);
        if !dir.is_dir() {
            eprintln!("error: '{}' is not a directory", src);
            process::exit(1);
        }
        let hl_files = collect_hl_files_recursive(dir);
        if hl_files.is_empty() {
            eprintln!("no .hl files found in {}", src);
            process::exit(1);
        }
        // Create output dir if needed
        if let Some(out) = out_dir {
            let _ = fs::create_dir_all(out);
        }
        // Pre-create output directories for each file (must be done before parallel compilation)
        let effective_outs: Vec<Option<String>> = hl_files.iter().map(|file| {
            out_dir.map(|o| {
                let rel = file.strip_prefix(dir).unwrap_or(file);
                let out_path = Path::new(o).join(rel).with_extension("html");
                if let Some(parent) = out_path.parent() {
                    let _ = fs::create_dir_all(parent);
                }
                out_path.to_string_lossy().to_string()
            })
        }).collect();

        // Compile files in parallel (with incremental skip for unchanged files)
        let any_errors = std::sync::atomic::AtomicBool::new(false);
        let skipped = std::sync::atomic::AtomicUsize::new(0);
        std::thread::scope(|s| {
            for (file, effective_out) in hl_files.iter().zip(effective_outs.iter()) {
                let any_errors = &any_errors;
                let skipped = &skipped;
                s.spawn(move || {
                    // Incremental: skip if output is newer than source
                    if let Some(out_path) = effective_out {
                        let out_p = Path::new(out_path.as_str());
                        if out_p.exists() {
                            if let (Ok(src_meta), Ok(out_meta)) = (file.metadata(), out_p.metadata()) {
                                if let (Ok(src_mod), Ok(out_mod)) = (src_meta.modified(), out_meta.modified()) {
                                    if out_mod > src_mod {
                                        skipped.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                                        return;
                                    }
                                }
                            }
                        }
                    }
                    let path_str = file.to_string_lossy().to_string();
                    let (has_errors, _) = compile(&path_str, false, false, false, effective_out.as_deref(), false, None);
                    if has_errors {
                        any_errors.store(true, std::sync::atomic::Ordering::Relaxed);
                    }
                });
            }
        });
        let skipped_count = skipped.load(std::sync::atomic::Ordering::Relaxed);
        if skipped_count > 0 {
            eprintln!("skipped {} unchanged files", skipped_count);
        }
        if any_errors.load(std::sync::atomic::Ordering::Relaxed) { process::exit(1); }

        // Copy non-.hl static assets to output directory
        if let Some(out) = out_dir {
            copy_non_hl_files(dir, Path::new(out));
        }
        return;
    }

    // Handle "sitemap" subcommand
    if args.len() >= 2 && args[1] == "sitemap" {
        let dir = if args.len() >= 3 { &args[2] } else { "." };
        let base_url = args.iter().position(|a| a == "--base-url" || a == "-b")
            .and_then(|i| args.get(i + 1))
            .map(|s| s.as_str())
            .unwrap_or("https://example.com");
        generate_sitemap(dir, base_url);
        return;
    }

    // Handle "lint" subcommand
    if args.len() >= 2 && args[1] == "lint" {
        let target = if args.len() >= 3 { &args[2] } else { "." };
        let path = Path::new(target);
        let mut all_warnings = Vec::new();
        if path.is_dir() {
            let hl_files = collect_hl_files_recursive(path);
            if hl_files.is_empty() {
                eprintln!("no .hl files found in {}", target);
                process::exit(1);
            }
            for file in &hl_files {
                let path_str = file.to_string_lossy().to_string();
                all_warnings.extend(lint_file(&path_str));
            }
        } else {
            all_warnings.extend(lint_file(target));
        }
        if all_warnings.is_empty() {
            eprintln!("no issues found");
        } else {
            for w in &all_warnings {
                eprintln!("{}", w);
            }
            process::exit(1);
        }
        return;
    }

    // Handle "stats" subcommand
    if args.len() >= 2 && args[1] == "stats" {
        let target = if args.len() >= 3 { &args[2] } else { "." };
        let path = Path::new(target);
        if path.is_dir() {
            let hl_files = collect_hl_files_recursive(path);
            if hl_files.is_empty() {
                eprintln!("no .hl files found in {}", target);
                process::exit(1);
            }
            for file in &hl_files {
                stats_file(&file.to_string_lossy());
            }
        } else {
            stats_file(target);
        }
        return;
    }

    // Handle "check" subcommand
    if args.len() >= 2 && args[1] == "check" {
        let mut check_target = None;
        let mut check_format_json = false;
        let mut ci = 2;
        while ci < args.len() {
            match args[ci].as_str() {
                "--format" => {
                    ci += 1;
                    if args.get(ci).map_or(false, |v| v == "json") {
                        check_format_json = true;
                    }
                }
                _ if check_target.is_none() => check_target = Some(args[ci].as_str()),
                _ => {
                    eprintln!("unknown argument: {}", args[ci]);
                    process::exit(1);
                }
            }
            ci += 1;
        }
        let target = check_target.unwrap_or(".");
        let json_collector = if check_format_json { Some(Mutex::new(Vec::new())) } else { None };
        let path = Path::new(target);
        let mut any_errors = false;
        if path.is_dir() {
            let hl_files = collect_hl_files_recursive(path);
            if hl_files.is_empty() {
                eprintln!("no .hl files found in {}", target);
                process::exit(1);
            }
            for file in &hl_files {
                let path_str = file.to_string_lossy().to_string();
                let (has_errors, _) = compile(&path_str, false, false, true, None, check_format_json, json_collector.as_ref());
                if has_errors { any_errors = true; }
            }
        } else {
            let (has_errors, _) = compile(target, false, false, true, None, check_format_json, json_collector.as_ref());
            if has_errors { any_errors = true; }
        }
        if check_format_json {
            if let Some(collector) = json_collector {
                print_json_diagnostics(&collector.lock().unwrap());
            }
        }
        if any_errors { process::exit(1); }
        return;
    }

    // Handle "new" subcommand
    if args.len() >= 2 && args[1] == "new" {
        if args.len() < 3 {
            eprintln!("usage: htmlang new <page-name>");
            process::exit(1);
        }
        let page_name = &args[2];
        let file_name = format!("{}.hl", page_name);
        let file_path = Path::new(&file_name);
        if file_path.exists() {
            eprintln!("error: {} already exists", file_name);
            process::exit(1);
        }
        let title = kebab_to_title(page_name);
        let template = format!(
            "@page {title}\n\
             @let primary #3b82f6\n\
             \n\
             @column [max-width 800, center-x, padding 40, spacing 20]\n\
             \x20\x20@text [bold, size 32] {title}\n\
             \x20\x20@paragraph [line-height 1.6]\n\
             \x20\x20\x20\x20Content goes here.\n",
            title = title,
        );
        match fs::write(file_path, &template) {
            Ok(()) => eprintln!("created {}", file_name),
            Err(e) => {
                eprintln!("error: {}: {}", file_name, e);
                process::exit(1);
            }
        }
        return;
    }

    // Handle "preview" subcommand
    if args.len() >= 2 && args[1] == "preview" {
        if args.len() < 3 {
            eprintln!("usage: htmlang preview <file.hl>");
            process::exit(1);
        }
        let file = &args[2];
        let tmp_dir = env::temp_dir().join("htmlang-preview");
        let _ = fs::create_dir_all(&tmp_dir);
        let out_path = tmp_dir.join("preview.html");
        let (has_errors, _) = compile(file, true, false, false, Some(&out_path.to_string_lossy()), false, None);
        if has_errors {
            process::exit(1);
        }
        let url = format!("file://{}", out_path.display());
        let cmd = if cfg!(target_os = "macos") { "open" } else { "xdg-open" };
        let _ = std::process::Command::new(cmd).arg(&url).spawn();
        eprintln!("opened {}", out_path.display());
        return;
    }

    // Handle "diff" subcommand
    if args.len() >= 2 && args[1] == "diff" {
        if args.len() < 4 {
            eprintln!("usage: htmlang diff <file1.hl> <file2.hl>");
            process::exit(1);
        }
        let file1 = &args[2];
        let file2 = &args[3];
        let compile_to_string = |path: &str| -> Result<String, String> {
            let input = fs::read_to_string(path).map_err(|e| format!("error: {}: {}", path, e))?;
            let base = Path::new(path).parent();
            let result = htmlang::parser::parse_with_base(&input, base);
            if result.diagnostics.iter().any(|d| d.severity == htmlang::parser::Severity::Error) {
                return Err(format!("error: {} has parse errors", path));
            }
            Ok(htmlang::codegen::generate_dev(&result.document))
        };
        let html1 = match compile_to_string(file1) {
            Ok(h) => h,
            Err(e) => { eprintln!("{}", e); process::exit(1); }
        };
        let html2 = match compile_to_string(file2) {
            Ok(h) => h,
            Err(e) => { eprintln!("{}", e); process::exit(1); }
        };
        let lines1: Vec<&str> = html1.lines().collect();
        let lines2: Vec<&str> = html2.lines().collect();
        let mut has_diff = false;
        let max_len = lines1.len().max(lines2.len());
        for i in 0..max_len {
            let l1 = lines1.get(i).unwrap_or(&"");
            let l2 = lines2.get(i).unwrap_or(&"");
            if l1 != l2 {
                has_diff = true;
                if !l1.is_empty() {
                    println!("- {}", l1);
                }
                if !l2.is_empty() {
                    println!("+ {}", l2);
                }
            }
        }
        if !has_diff {
            eprintln!("no differences");
        }
        return;
    }

    // Handle "export" subcommand
    if args.len() >= 2 && args[1] == "export" {
        let mut src_dir = None;
        let mut out_file = None;
        let mut ei = 2;
        while ei < args.len() {
            match args[ei].as_str() {
                "-o" | "--output" => {
                    ei += 1;
                    out_file = args.get(ei).map(|s| s.as_str());
                }
                _ if src_dir.is_none() => src_dir = Some(args[ei].as_str()),
                _ => {
                    eprintln!("unknown argument: {}", args[ei]);
                    process::exit(1);
                }
            }
            ei += 1;
        }
        let src = src_dir.unwrap_or(".");
        let dir = Path::new(src);
        if !dir.is_dir() {
            eprintln!("error: '{}' is not a directory", src);
            process::exit(1);
        }
        let hl_files = collect_hl_files_recursive(dir);
        if hl_files.is_empty() {
            eprintln!("no .hl files found in {}", src);
            process::exit(1);
        }
        let zip_path = out_file.unwrap_or("site.zip");
        // Build to temp directory, then create zip
        let tmp_dir = env::temp_dir().join("htmlang-export");
        let _ = fs::remove_dir_all(&tmp_dir);
        let _ = fs::create_dir_all(&tmp_dir);
        let mut any_errors = false;
        for file in &hl_files {
            let rel = file.strip_prefix(dir).unwrap_or(file);
            let out_path = tmp_dir.join(rel).with_extension("html");
            if let Some(parent) = out_path.parent() {
                let _ = fs::create_dir_all(parent);
            }
            let path_str = file.to_string_lossy().to_string();
            let (has_errors, _) = compile(&path_str, false, false, false, Some(&out_path.to_string_lossy()), false, None);
            if has_errors { any_errors = true; }
        }
        // Copy non-.hl assets
        copy_non_hl_files(dir, &tmp_dir);
        if any_errors {
            eprintln!("export completed with errors");
        }
        // Create a simple tar-like zip (concatenated files with headers)
        // Since we can't use external crates, we'll create a self-contained directory listing
        let export_files = collect_all_files_recursive(&tmp_dir);
        let zip_file = std::fs::File::create(zip_path);
        match zip_file {
            Ok(mut f) => {
                use std::io::Write;
                // Write a simple archive format: filename\0length\0content
                for file in &export_files {
                    let rel = file.strip_prefix(&tmp_dir).unwrap_or(file);
                    if let Ok(data) = fs::read(file) {
                        let _ = write!(f, "FILE:{}:{}:", rel.display(), data.len());
                        let _ = f.write_all(&data);
                    }
                }
                eprintln!("exported {} files to {}", export_files.len(), zip_path);
            }
            Err(e) => {
                eprintln!("error: {}: {}", zip_path, e);
                // Fallback: just report the built files in tmp_dir
                eprintln!("files available in {}", tmp_dir.display());
            }
        }
        return;
    }

    // Handle "serve" standalone subcommand
    if args.len() >= 2 && args[1] == "serve" {
        let mut serve_target = None;
        let mut serve_port: u16 = 3000;
        let mut serve_open = false;
        let mut si = 2;
        while si < args.len() {
            match args[si].as_str() {
                "-p" | "--port" => {
                    si += 1;
                    serve_port = args.get(si).and_then(|p| p.parse().ok()).unwrap_or(3000);
                }
                "--open" => serve_open = true,
                _ if serve_target.is_none() => serve_target = Some(args[si].clone()),
                _ => {
                    eprintln!("unknown argument: {}", args[si]);
                    process::exit(1);
                }
            }
            si += 1;
        }
        let target = serve_target.unwrap_or_else(|| ".".to_string());
        let target_path = Path::new(&target);
        // Load config
        let config = load_config(target_path);
        let effective_port = if serve_port != 3000 { serve_port } else { config.port };

        // Do initial compile
        if target_path.is_dir() {
            let hl_files = collect_hl_files_recursive(target_path);
            for file in &hl_files {
                let path_str = file.to_string_lossy().to_string();
                let effective_out = config.output.as_ref().map(|o| {
                    let rel = file.strip_prefix(target_path).unwrap_or(file);
                    let out_p = Path::new(o).join(rel).with_extension("html");
                    if let Some(parent) = out_p.parent() { let _ = fs::create_dir_all(parent); }
                    out_p.to_string_lossy().to_string()
                });
                compile(&path_str, true, true, false, effective_out.as_deref(), false, None);
            }
            let (tx, _) = tokio::sync::broadcast::channel::<()>(16);
            let serve_dir = config.output.as_ref().map(|o| PathBuf::from(o)).unwrap_or_else(|| target_path.to_path_buf());
            let index_path = serve_dir.join("index.html");
            let server_tx = tx.clone();
            std::thread::spawn(move || {
                let rt = tokio::runtime::Runtime::new().expect("failed to create runtime");
                rt.block_on(htmlang::serve::run(effective_port, index_path, server_tx));
            });
            if serve_open { open_in_browser(effective_port); }
            watch_loop(target_path, &hl_files, &[], true, true, Some(tx));
        } else {
            let (_, included) = compile(&target, true, true, false, None, false, None);
            let (tx, _) = tokio::sync::broadcast::channel::<()>(16);
            let out_path = Path::new(&target).with_extension("html");
            let server_tx = tx.clone();
            std::thread::spawn(move || {
                let rt = tokio::runtime::Runtime::new().expect("failed to create runtime");
                rt.block_on(htmlang::serve::run(effective_port, out_path, server_tx));
            });
            if serve_open { open_in_browser(effective_port); }
            let files = vec![PathBuf::from(&target)];
            watch_loop(Path::new(&target).parent().unwrap_or(Path::new(".")), &files, &included, true, true, Some(tx));
        }
        return;
    }

    // Handle "watch" standalone subcommand
    if args.len() >= 2 && args[1] == "watch" {
        let mut watch_target = None;
        let mut watch_output = None;
        let mut wi = 2;
        while wi < args.len() {
            match args[wi].as_str() {
                "-o" | "--output" => {
                    wi += 1;
                    watch_output = args.get(wi).cloned();
                }
                _ if watch_target.is_none() => watch_target = Some(args[wi].clone()),
                _ => {
                    eprintln!("unknown argument: {}", args[wi]);
                    process::exit(1);
                }
            }
            wi += 1;
        }
        let target = watch_target.unwrap_or_else(|| ".".to_string());
        let target_path = Path::new(&target);
        let config = load_config(target_path);
        let effective_output = watch_output.or(config.output);

        if target_path.is_dir() {
            let hl_files = collect_hl_files_recursive(target_path);
            if let Some(ref out) = effective_output { let _ = fs::create_dir_all(out); }
            let mut all_included = Vec::new();
            for file in &hl_files {
                let path_str = file.to_string_lossy().to_string();
                let effective_out = effective_output.as_ref().map(|o| {
                    let rel = file.strip_prefix(target_path).unwrap_or(file);
                    let out_p = Path::new(o).join(rel).with_extension("html");
                    if let Some(parent) = out_p.parent() { let _ = fs::create_dir_all(parent); }
                    out_p.to_string_lossy().to_string()
                });
                let (_, included) = compile(&path_str, false, false, false, effective_out.as_deref(), false, None);
                all_included.extend(included);
            }
            watch_loop(target_path, &hl_files, &all_included, false, false, None);
        } else {
            let (_, included) = compile(&target, false, false, false, effective_output.as_deref(), false, None);
            let files = vec![PathBuf::from(&target)];
            watch_loop(Path::new(&target).parent().unwrap_or(Path::new(".")), &files, &included, false, false, None);
        }
        return;
    }

    // Handle "repl" subcommand
    if args.len() >= 2 && args[1] == "repl" {
        use std::io::{self, BufRead, Write};
        eprintln!("htmlang repl — type .hl code, then a blank line to compile (Ctrl+D to exit)");
        let stdin = io::stdin();
        let stdout = io::stdout();
        loop {
            eprint!("> ");
            let _ = io::stderr().flush();
            let mut buffer = String::new();
            loop {
                let mut line = String::new();
                match stdin.lock().read_line(&mut line) {
                    Ok(0) => {
                        if buffer.is_empty() {
                            return;
                        }
                        break;
                    }
                    Ok(_) => {
                        if line.trim().is_empty() && !buffer.is_empty() {
                            break;
                        }
                        buffer.push_str(&line);
                    }
                    Err(_) => return,
                }
            }
            if buffer.trim().is_empty() {
                continue;
            }
            let result = htmlang::parser::parse(&buffer);
            for d in &result.diagnostics {
                let prefix = match d.severity {
                    htmlang::parser::Severity::Error => "error",
                    htmlang::parser::Severity::Warning => "warning",
                };
                eprintln!("{}: line {}: {}", prefix, d.line, d.message);
            }
            if !result.diagnostics.iter().any(|d| d.severity == htmlang::parser::Severity::Error) {
                let html = htmlang::codegen::generate_dev(&result.document);
                let _ = stdout.lock().write_all(html.as_bytes());
                let _ = stdout.lock().write_all(b"\n");
            }
        }
    }

    // Handle "feed" subcommand
    if args.len() >= 2 && args[1] == "feed" {
        let dir = if args.len() >= 3 && !args[2].starts_with('-') { &args[2] } else { "." };
        let base_url = args.iter().position(|a| a == "--base-url" || a == "-b")
            .and_then(|i| args.get(i + 1))
            .map(|s| s.as_str())
            .unwrap_or("https://example.com");
        let dir_path = Path::new(dir);
        let hl_files = collect_hl_files_recursive(dir_path);
        if hl_files.is_empty() {
            eprintln!("no .hl files found in {}", dir);
            process::exit(1);
        }
        let mut items = Vec::new();
        for file in &hl_files {
            let input = match fs::read_to_string(file) {
                Ok(s) => s,
                Err(_) => continue,
            };
            let base = file.parent();
            let result = htmlang::parser::parse_with_base(&input, base);
            if let Some(ref title) = result.document.page_title {
                let rel = file.strip_prefix(dir_path).unwrap_or(file);
                let url_path = rel.with_extension("html").to_string_lossy().replace('\\', "/");
                let url = if url_path == "index.html" {
                    format!("{}/", base_url.trim_end_matches('/'))
                } else {
                    format!("{}/{}", base_url.trim_end_matches('/'), url_path)
                };
                // Get description from meta tags if available
                let description = result.document.meta_tags.iter()
                    .find(|(k, _)| k == "description")
                    .map(|(_, v)| v.clone())
                    .unwrap_or_default();
                items.push((title.clone(), url, description));
            }
        }
        // Generate RSS 2.0 feed
        let site_title = items.first().map(|(t, _, _)| t.clone()).unwrap_or_else(|| "My Site".to_string());
        let mut rss = format!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
             <rss version=\"2.0\">\n\
             <channel>\n\
             <title>{}</title>\n\
             <link>{}</link>\n\
             <description>RSS feed generated by htmlang</description>\n",
            site_title, base_url
        );
        for (title, url, description) in &items {
            rss.push_str(&format!(
                "<item>\n<title>{}</title>\n<link>{}</link>\n<description>{}</description>\n</item>\n",
                title, url, description
            ));
        }
        rss.push_str("</channel>\n</rss>\n");
        let out_path = dir_path.join("feed.xml");
        match fs::write(&out_path, &rss) {
            Ok(()) => eprintln!("wrote {} ({} items)", out_path.display(), items.len()),
            Err(e) => {
                eprintln!("error: {}: {}", out_path.display(), e);
                process::exit(1);
            }
        }
        return;
    }

    // Handle "components" subcommand
    if args.len() >= 2 && args[1] == "components" {
        let target = if args.len() >= 3 { &args[2] } else { "." };
        let path = Path::new(target);
        let hl_files = if path.is_dir() {
            collect_hl_files_recursive(path)
        } else {
            vec![PathBuf::from(target)]
        };
        if hl_files.is_empty() {
            eprintln!("no .hl files found in {}", target);
            process::exit(1);
        }
        let mut total = 0usize;
        for file in &hl_files {
            let input = match fs::read_to_string(file) {
                Ok(s) => s,
                Err(_) => continue,
            };
            let mut found = Vec::new();
            for (line_num, line) in input.lines().enumerate() {
                let trimmed = line.trim();
                if let Some(rest) = trimmed.strip_prefix("@fn ") {
                    let parts: Vec<&str> = rest.split_whitespace().collect();
                    if let Some(&name) = parts.first() {
                        let params: Vec<&str> = parts[1..].iter()
                            .map(|p| p.strip_prefix('$').unwrap_or(p))
                            .collect();
                        let param_str = if params.is_empty() {
                            String::new()
                        } else {
                            format!(" ({})", params.join(", "))
                        };
                        found.push((line_num + 1, name.to_string(), param_str));
                    }
                }
            }
            if !found.is_empty() {
                let rel = file.strip_prefix(path).unwrap_or(file);
                for (line, name, params) in &found {
                    println!("  @{}{} — {}:{}", name, params, rel.display(), line);
                    total += 1;
                }
            }
        }
        if total == 0 {
            eprintln!("no @fn definitions found");
        } else {
            eprintln!("\n{} component(s) found", total);
        }
        return;
    }

    // Handle "convert" subcommand
    if args.len() >= 2 && args[1] == "convert" {
        if args.len() < 3 {
            eprintln!("usage: htmlang convert <file.html>");
            process::exit(1);
        }
        let html_file = &args[2];
        let html = match fs::read_to_string(html_file) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("error: {}: {}", html_file, e);
                process::exit(1);
            }
        };
        let hl_output = htmlang::convert::convert(&html);
        print!("{}", hl_output);
        return;
    }

    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--help" | "-h" => {
                print_help();
                process::exit(0);
            }
            "--version" | "-V" => {
                println!("htmlang {}", env!("CARGO_PKG_VERSION"));
                process::exit(0);
            }
            "--watch" | "-w" => watch = true,
            "--dev" | "-d" => dev = true,
            "--check" | "-c" => check = true,
            "--open" => open_browser = true,
            "--format" => {
                i += 1;
                match args.get(i) {
                    Some(f) if f == "json" => format_json = true,
                    Some(f) => {
                        eprintln!("unknown format: {}", f);
                        process::exit(1);
                    }
                    None => {
                        eprintln!("--format requires a value");
                        process::exit(1);
                    }
                }
            }
            "--serve" | "-s" => {
                serve = true;
                watch = true;
            }
            "--port" | "-p" => {
                i += 1;
                match args.get(i) {
                    Some(p) => {
                        port = p.parse().unwrap_or_else(|_| {
                            eprintln!("invalid port: {}", p);
                            process::exit(1);
                        });
                    }
                    None => {
                        eprintln!("--port requires a value");
                        process::exit(1);
                    }
                }
            }
            "--output" | "-o" => {
                i += 1;
                match args.get(i) {
                    Some(p) => output_path = Some(p.clone()),
                    None => {
                        eprintln!("--output requires a value");
                        process::exit(1);
                    }
                }
            }
            _ if input_path.is_none() => input_path = Some(args[i].clone()),
            _ => {
                eprintln!("unknown argument: {}", args[i]);
                process::exit(1);
            }
        }
        i += 1;
    }

    let input_path = match input_path {
        Some(p) => p,
        None => {
            print_help();
            process::exit(1);
        }
    };

    let is_dir = Path::new(&input_path).is_dir();

    // --- Directory mode: compile all .hl files ---
    if is_dir {
        let dir = Path::new(&input_path);
        let hl_files = collect_hl_files(dir);
        if hl_files.is_empty() {
            eprintln!("no .hl files found in {}", input_path);
            process::exit(1);
        }

        // Create output dir if needed
        if let Some(ref out) = output_path {
            let _ = fs::create_dir_all(out);
        }

        let json_collector = if format_json { Some(Mutex::new(Vec::new())) } else { None };
        let mut any_errors = false;
        let mut all_included: Vec<PathBuf> = Vec::new();
        for file in &hl_files {
            let path_str = file.to_string_lossy().to_string();
            let effective_out = output_path.as_ref().map(|o| {
                let rel = file.strip_prefix(dir).unwrap_or(file);
                let out_p = Path::new(o).join(rel).with_extension("html");
                if let Some(parent) = out_p.parent() {
                    let _ = fs::create_dir_all(parent);
                }
                out_p.to_string_lossy().to_string()
            });
            let (has_errors, included) = compile(&path_str, dev, serve, check, effective_out.as_deref(), format_json, json_collector.as_ref());
            if has_errors {
                any_errors = true;
            }
            all_included.extend(included);
        }

        if format_json {
            if let Some(collector) = json_collector {
                print_json_diagnostics(&collector.lock().unwrap());
            }
        }

        if !watch {
            if any_errors {
                process::exit(1);
            }
            return;
        }

        // For directory serve mode, serve the directory with index.html
        let reload_tx = if serve {
            let (tx, _) = tokio::sync::broadcast::channel::<()>(16);
            let index_path = dir.join("index.html");
            let server_tx = tx.clone();
            std::thread::spawn(move || {
                let rt = tokio::runtime::Runtime::new().expect("failed to create runtime");
                rt.block_on(htmlang::serve::run(port, index_path, server_tx));
            });
            if open_browser {
                open_in_browser(port);
            }
            Some(tx)
        } else {
            None
        };

        watch_loop(dir, &hl_files, &all_included, dev, serve, reload_tx);
        return;
    }

    // --- Single file mode ---
    let json_collector_single = if format_json { Some(Mutex::new(Vec::new())) } else { None };
    let (has_errors, included_files) = compile(&input_path, dev, serve, check, output_path.as_deref(), format_json, json_collector_single.as_ref());
    if format_json {
        if let Some(ref collector) = json_collector_single {
            print_json_diagnostics(&collector.lock().unwrap());
        }
    }

    if !watch {
        if has_errors {
            process::exit(1);
        }
        return;
    }

    // Start dev server if requested
    let reload_tx = if serve {
        let (tx, _) = tokio::sync::broadcast::channel::<()>(16);
        let out_path = match output_path {
            Some(ref p) => PathBuf::from(p),
            None => Path::new(&input_path).with_extension("html"),
        };
        let server_tx = tx.clone();
        std::thread::spawn(move || {
            let rt = tokio::runtime::Runtime::new().expect("failed to create runtime");
            rt.block_on(htmlang::serve::run(port, out_path, server_tx));
        });
        if open_browser {
            open_in_browser(port);
        }
        Some(tx)
    } else {
        None
    };

    let files = vec![PathBuf::from(&input_path)];
    watch_loop(
        Path::new(&input_path).parent().unwrap_or(Path::new(".")),
        &files,
        &included_files,
        dev,
        serve,
        reload_tx,
    );
}

fn collect_hl_files(dir: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_file() && path.extension().map_or(false, |e| e == "hl") {
                files.push(path);
            }
        }
    }
    files.sort();
    files
}

fn watch_loop(
    watch_dir: &Path,
    source_files: &[PathBuf],
    included_files: &[PathBuf],
    dev: bool,
    serve: bool,
    reload_tx: Option<tokio::sync::broadcast::Sender<()>>,
) {
    use notify::{Config, RecommendedWatcher, RecursiveMode, Watcher};
    use std::sync::mpsc;

    let (tx, rx) = mpsc::channel();
    let mut watcher = RecommendedWatcher::new(
        move |res| {
            if let Ok(event) = res {
                let _ = tx.send(event);
            }
        },
        Config::default(),
    )
    .expect("failed to create file watcher");

    // Watch all source files
    for file in source_files {
        let canonical = fs::canonicalize(file).unwrap_or_else(|_| file.clone());
        watcher
            .watch(&canonical, RecursiveMode::NonRecursive)
            .unwrap_or_else(|_| eprintln!("warning: could not watch {}", file.display()));
    }
    for inc in included_files {
        let _ = watcher.watch(inc, RecursiveMode::NonRecursive);
    }

    // Also watch the directory itself for new files
    let _ = watcher.watch(watch_dir, RecursiveMode::NonRecursive);

    if serve {
        eprintln!("watching for changes at http://127.0.0.1:3000");
    } else {
        eprintln!("watching for changes...");
    }

    // Track content hashes for incremental rebuilds
    let mut content_hashes: HashMap<PathBuf, u64> = HashMap::new();

    fn hash_file(path: &Path) -> Option<u64> {
        let content = fs::read(path).ok()?;
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        content.hash(&mut hasher);
        Some(hasher.finish())
    }

    // Seed initial hashes
    for file in source_files {
        let canonical = fs::canonicalize(file).unwrap_or_else(|_| file.clone());
        if let Some(h) = hash_file(&canonical) {
            content_hashes.insert(canonical, h);
        }
    }
    for inc in included_files {
        if let Some(h) = hash_file(inc) {
            content_hashes.insert(inc.clone(), h);
        }
    }

    loop {
        match rx.recv() {
            Ok(_) => {
                // Drain additional events (debounce)
                while rx.try_recv().is_ok() {}

                // Check if any file content actually changed
                let mut any_changed = false;
                let check_path = |path: &Path,
                                   hashes: &mut HashMap<PathBuf, u64>|
                 -> bool {
                    if let Some(h) = hash_file(path) {
                        if hashes.get(path) != Some(&h) {
                            hashes.insert(path.to_path_buf(), h);
                            return true;
                        }
                    }
                    false
                };

                for file in source_files {
                    let canonical = fs::canonicalize(file).unwrap_or_else(|_| file.clone());
                    if check_path(&canonical, &mut content_hashes) {
                        any_changed = true;
                    }
                }

                // Check for new .hl files in directory
                if watch_dir.is_dir() {
                    let current_files = collect_hl_files(watch_dir);
                    for file in &current_files {
                        let canonical = fs::canonicalize(file).unwrap_or_else(|_| file.clone());
                        if check_path(&canonical, &mut content_hashes) {
                            any_changed = true;
                            let _ = watcher.watch(&canonical, RecursiveMode::NonRecursive);
                        }
                    }
                }

                if !any_changed {
                    continue;
                }

                eprintln!("\nrecompiling...");

                // Recompile all source files
                for file in source_files {
                    let path_str = file.to_string_lossy().to_string();
                    let (_, new_includes) = compile(&path_str, dev, serve, false, None, false, None);
                    for inc in &new_includes {
                        let _ = watcher.watch(inc, RecursiveMode::NonRecursive);
                        if let Some(h) = hash_file(inc) {
                            content_hashes.insert(inc.clone(), h);
                        }
                    }
                }

                // Also compile any new .hl files in directory mode
                if watch_dir.is_dir() {
                    for file in &collect_hl_files(watch_dir) {
                        if !source_files.contains(file) {
                            let path_str = file.to_string_lossy().to_string();
                            let (_, new_includes) = compile(&path_str, dev, serve, false, None, false, None);
                            for inc in &new_includes {
                                let _ = watcher.watch(inc, RecursiveMode::NonRecursive);
                            }
                        }
                    }
                }

                if let Some(ref tx) = reload_tx {
                    let _ = tx.send(());
                }
            }
            Err(_) => break,
        }
    }
}
