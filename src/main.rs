use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process;

fn compile(input_path: &str) -> (bool, Vec<PathBuf>) {
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
    }

    let has_errors = result
        .diagnostics
        .iter()
        .any(|d| d.severity == htmlang::parser::Severity::Error);

    if !has_errors {
        let html = htmlang::codegen::generate(&result.document);
        let out_path = Path::new(input_path).with_extension("html");
        match fs::write(&out_path, &html) {
            Ok(()) => eprintln!("wrote {}", out_path.display()),
            Err(e) => eprintln!("error: {}: {}", out_path.display(), e),
        }
    }

    (has_errors, result.included_files)
}

fn main() {
    let args: Vec<String> = env::args().collect();
    let mut watch = false;
    let mut serve = false;
    let mut port: u16 = 3000;
    let mut input_path = None;

    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--watch" | "-w" => watch = true,
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
            eprintln!("Usage: htmlang [--watch] [--serve [--port N]] <file.hl>");
            process::exit(1);
        }
    };

    let (has_errors, included_files) = compile(&input_path);

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

    loop {
        match rx.recv() {
            Ok(_) => {
                // Drain additional events (debounce)
                while rx.try_recv().is_ok() {}

                eprintln!("\nrecompiling...");
                let (_, new_includes) = compile(&input_path);

                for inc in &new_includes {
                    let _ = watcher.watch(inc, RecursiveMode::NonRecursive);
                }

                if let Some(ref tx) = reload_tx {
                    let _ = tx.send(());
                }
            }
            Err(_) => break,
        }
    }
}
