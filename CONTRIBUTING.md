# Contributing

Thanks for your interest in improving htmlang. This file covers the practical
bits: how to run the project locally, where the code lives, and what style of
changes land smoothly.

## Development

```
cargo build                     # build the CLI + LSP binaries
cargo test                      # run all tests (unit + snapshot + integration)
cargo run -- page.hl            # compile a .hl file to page.html
cargo run -- serve examples     # run the dev server at http://127.0.0.1:3000
```

CI runs `cargo test`, `cargo fmt --check`, and `cargo clippy -- -D warnings` on
Linux, macOS, and Windows. Your change should pass all three.

## Repository layout

- `crates/htmlang-core/` — parser, AST, code generator. No I/O lives here.
- `crates/htmlang-wasm/` — thin wrapper exposing `compile` to the web playground.
- `src/` — CLI, dev server, formatter, and HTML-to-hl converter.
- `src/bin/htmlang_lsp/` — language server binary (`htmlang-lsp`).
- `editors/vscode/` — VS Code extension.
- `tests/snapshots.rs` — integration / snapshot tests for the compiler.
- `examples/` — sample `.hl` files used as smoke tests and documentation.

## Adding a feature

1. Add a test first. Most language features fit as a new `#[test]` in
   `tests/snapshots.rs`; prefer integration tests that exercise the full
   parser-to-HTML pipeline. Pure parser / codegen helpers can live as unit
   tests alongside the code.
2. Thread the feature through the parser, then codegen, then the LSP (hover,
   completions, diagnostics).
3. Document it in `DESIGN.md`. If it's user-facing, also update `README.md`.
4. If it changes the CLI surface, update the `--help` output and the shell
   completions.

## Style

- Prefer enum-based ASTs and pattern matching over stringly-typed dispatch.
- Keep `@` prefixes and bracket-attribute syntax consistent with existing
  directives. Add `KNOWN_DIRECTIVES` entries when you add a new `@foo`.
- Diagnostics should include `line` and, when practical, `column` and a
  `source_line` excerpt. Use `Severity::Help` for suggestions, not `Warning`.
- No unwrap() on parsed user input. Use `Result<_, ParseError>` and record a
  diagnostic so the compiler keeps going.

## Reporting bugs

Bug reports are most useful when they include:

- The exact `.hl` input that reproduces the issue (minimized if possible).
- The command you ran and the output you saw.
- The output you expected instead.
- `htmlang --version` and your OS.

## License

By contributing, you agree that your changes will be licensed under the same
terms as the rest of the project.
