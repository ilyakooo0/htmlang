use std::collections::HashMap;
use std::env;
use std::fs;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::process;

fn compile(input_path: &str, dev: bool, error_overlay: bool, check_only: bool, output_path: Option<&str>) -> (bool, Vec<PathBuf>) {
    let input = match fs::read_to_string(input_path) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("error: {}: {}", input_path, e);
            return (true, vec![]);
        }
    };

    let base = Path::new(input_path).parent();
    let result = htmlang::parser::parse_with_base(&input, base);

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
  init [dir]        Create a new project (defaults to current directory)
  build <dir> [-o <out>]  Compile all .hl files recursively
  fmt <file.hl>     Format a .hl file (normalizes indentation)
  sitemap <dir>     Generate sitemap.xml from .hl files

Options:
  -w, --watch       Watch for changes and recompile
  -s, --serve       Start dev server with hot reload (implies --watch)
  -p, --port <N>    Port for dev server (default: 3000)
  -o, --output <path>  Output file/directory path
  -d, --dev         Development mode
  -c, --check       Check for errors without writing output
  -h, --help        Show this help
  -V, --version     Show version

Examples:
  htmlang init              Scaffold a new project
  htmlang init my-site      Scaffold in a new directory
  htmlang page.hl           Compile page.hl to page.html
  htmlang site/             Compile all .hl files in directory
  htmlang -w page.hl        Recompile on file changes
  htmlang -s page.hl        Start dev server with hot reload
  htmlang -s site/          Serve a multi-page site
  htmlang -s -p 8080 page.hl
  htmlang -c page.hl        Lint without writing output
  htmlang fmt page.hl       Format a file
  htmlang build src/ -o dist/  Compile all .hl files to dist/
  htmlang sitemap src/      Generate sitemap.xml",
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

fn main() {
    let args: Vec<String> = env::args().collect();
    let mut watch = false;
    let mut serve = false;
    let mut dev = false;
    let mut check = false;
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
        let mut any_errors = false;
        for file in &hl_files {
            let path_str = file.to_string_lossy().to_string();
            let effective_out = out_dir.map(|o| {
                // Mirror directory structure
                let rel = file.strip_prefix(dir).unwrap_or(file);
                let out_path = Path::new(o).join(rel).with_extension("html");
                if let Some(parent) = out_path.parent() {
                    let _ = fs::create_dir_all(parent);
                }
                out_path.to_string_lossy().to_string()
            });
            let (has_errors, _) = compile(&path_str, false, false, false, effective_out.as_deref());
            if has_errors { any_errors = true; }
        }
        if any_errors { process::exit(1); }
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
            let (has_errors, included) = compile(&path_str, dev, serve, check, effective_out.as_deref());
            if has_errors {
                any_errors = true;
            }
            all_included.extend(included);
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
            Some(tx)
        } else {
            None
        };

        watch_loop(dir, &hl_files, &all_included, dev, serve, reload_tx);
        return;
    }

    // --- Single file mode ---
    let (has_errors, included_files) = compile(&input_path, dev, serve, check, output_path.as_deref());

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
                    let (_, new_includes) = compile(&path_str, dev, serve, false, None);
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
                            let (_, new_includes) = compile(&path_str, dev, serve, false, None);
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
