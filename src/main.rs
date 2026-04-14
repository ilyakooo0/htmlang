use std::env;
use std::fs;
use std::path::Path;
use std::process;

mod ast;
mod codegen;
mod parser;

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() < 2 {
        eprintln!("Usage: htmlang <file.hl>");
        process::exit(1);
    }

    let input_path = &args[1];
    let input = match fs::read_to_string(input_path) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("error: {}: {}", input_path, e);
            process::exit(1);
        }
    };

    let doc = match parser::parse(&input) {
        Ok(d) => d,
        Err(e) => {
            eprintln!("parse error: {}", e);
            process::exit(1);
        }
    };

    let html = codegen::generate(&doc);

    let out_path = Path::new(input_path).with_extension("html");
    match fs::write(&out_path, &html) {
        Ok(()) => eprintln!("wrote {}", out_path.display()),
        Err(e) => {
            eprintln!("error: {}: {}", out_path.display(), e);
            process::exit(1);
        }
    }
}
