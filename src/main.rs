use std::collections::HashMap;
use std::env;
use std::fs;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::process;

fn compile(input_path: &str, dev: bool) -> (bool, Vec<PathBuf>) {
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

    if !has_errors {
        let html = if dev {
            htmlang::codegen::generate_dev(&result.document)
        } else {
            htmlang::codegen::generate(&result.document)
        };
        let out_path = Path::new(input_path).with_extension("html");
        match fs::write(&out_path, &html) {
            Ok(()) => eprintln!("wrote {}", out_path.display()),
            Err(e) => eprintln!("error: {}: {}", out_path.display(), e),
        }
    }

    (has_errors, result.included_files)
}

fn print_help() {
    eprintln!(
        "\
htmlang {} - a minimalist layout language that compiles to static HTML

Usage: htmlang [options] <file.hl>

Options:
  -w, --watch       Watch for changes and recompile
  -s, --serve       Start dev server with hot reload (implies --watch)
  -p, --port <N>    Port for dev server (default: 3000)
  -d, --dev         Development mode
  -h, --help        Show this help
  -V, --version     Show version

Examples:
  htmlang page.hl           Compile page.hl to page.html
  htmlang -w page.hl        Recompile on file changes
  htmlang -s page.hl        Start dev server with hot reload
  htmlang -s -p 8080 page.hl",
        env!("CARGO_PKG_VERSION")
    );
}

fn main() {
    let args: Vec<String> = env::args().collect();
    let mut watch = false;
    let mut serve = false;
    let mut dev = false;
    let mut port: u16 = 3000;
    let mut input_path = None;

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

    let (has_errors, included_files) = compile(&input_path, dev);

    if !watch {
        if has_errors {
            process::exit(1);
        }
        return;
    }

    // Start dev server if requested
    let reload_tx = if serve {
        let (tx, _) = tokio::sync::broadcast::channel::<()>(16);
        let out_path = Path::new(&input_path).with_extension("html");
        let server_tx = tx.clone();
        std::thread::spawn(move || {
            let rt = tokio::runtime::Runtime::new().expect("failed to create runtime");
            rt.block_on(htmlang::serve::run(port, out_path, server_tx));
        });
        Some(tx)
    } else {
        None
    };

    // Watch mode
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

    let input_canonical =
        fs::canonicalize(&input_path).unwrap_or_else(|_| PathBuf::from(&input_path));
    watcher
        .watch(&input_canonical, RecursiveMode::NonRecursive)
        .expect("failed to watch file");

    for inc in &included_files {
        let _ = watcher.watch(inc, RecursiveMode::NonRecursive);
    }

    eprintln!("watching for changes...");

    // Track content hashes for incremental rebuilds
    let mut content_hashes: HashMap<PathBuf, u64> = HashMap::new();

    fn hash_file(path: &Path) -> Option<u64> {
        let content = fs::read(path).ok()?;
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        content.hash(&mut hasher);
        Some(hasher.finish())
    }

    // Seed initial hashes
    if let Some(h) = hash_file(&input_canonical) {
        content_hashes.insert(input_canonical.clone(), h);
    }
    for inc in &included_files {
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

                if check_path(&input_canonical, &mut content_hashes) {
                    any_changed = true;
                }
                for inc in &included_files {
                    if check_path(inc, &mut content_hashes) {
                        any_changed = true;
                    }
                }

                if !any_changed {
                    continue;
                }

                eprintln!("\nrecompiling...");
                let (_, new_includes) = compile(&input_path, dev);

                for inc in &new_includes {
                    let _ = watcher.watch(inc, RecursiveMode::NonRecursive);
                    if let Some(h) = hash_file(inc) {
                        content_hashes.insert(inc.clone(), h);
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
