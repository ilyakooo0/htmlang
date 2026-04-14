use std::fs;
use std::path::Path;

fn compile(input: &str) -> String {
    let result = htmlang::parser::parse(input);
    assert!(
        result
            .diagnostics
            .iter()
            .all(|d| d.severity != htmlang::parser::Severity::Error),
        "unexpected parse errors: {:?}",
        result.diagnostics
    );
    htmlang::codegen::generate(&result.document)
}

fn parse_diagnostics(input: &str) -> Vec<htmlang::parser::Diagnostic> {
    htmlang::parser::parse(input).diagnostics
}

fn snapshot_test(name: &str) {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/snapshots");
    let hl_path = dir.join(format!("{}.hl", name));
    let html_path = dir.join(format!("{}.html", name));

    let input = fs::read_to_string(&hl_path)
        .unwrap_or_else(|e| panic!("failed to read {}: {}", hl_path.display(), e));
    let actual = compile(&input);

    if std::env::var("UPDATE_SNAPSHOTS").is_ok() {
        fs::write(&html_path, &actual).unwrap();
        return;
    }

    if !html_path.exists() {
        fs::write(&html_path, &actual).unwrap();
        eprintln!("created snapshot: {}", html_path.display());
        return;
    }

    let expected = fs::read_to_string(&html_path).unwrap();
    assert_eq!(actual, expected, "snapshot mismatch for {}", name);
}

#[test]
fn snapshot_basic_elements() {
    snapshot_test("basic_elements");
}

#[test]
fn snapshot_attributes() {
    snapshot_test("attributes");
}

#[test]
fn snapshot_alignment() {
    snapshot_test("alignment");
}

#[test]
fn snapshot_variables_defines() {
    snapshot_test("variables_defines");
}

#[test]
fn snapshot_functions() {
    snapshot_test("functions");
}

#[test]
fn snapshot_pseudo_states() {
    snapshot_test("pseudo_states");
}

#[test]
fn snapshot_chain_operator() {
    snapshot_test("chain_operator");
}

#[test]
fn snapshot_inline_text() {
    snapshot_test("inline_text");
}

#[test]
fn snapshot_raw_html() {
    snapshot_test("raw_html");
}

#[test]
fn snapshot_sizing() {
    snapshot_test("sizing");
}

#[test]
fn snapshot_no_page() {
    snapshot_test("no_page");
}

#[test]
fn snapshot_implicit_el() {
    snapshot_test("implicit_el");
}

#[test]
fn snapshot_responsive() {
    snapshot_test("responsive");
}

#[test]
fn snapshot_animations() {
    snapshot_test("animations");
}

#[test]
fn snapshot_css_vars() {
    snapshot_test("css_vars");
}

#[test]
fn snapshot_form_elements() {
    snapshot_test("form_elements");
}

#[test]
fn snapshot_conditionals() {
    snapshot_test("conditionals");
}

#[test]
fn snapshot_loops() {
    snapshot_test("loops");
}

#[test]
fn snapshot_accessibility() {
    snapshot_test("accessibility");
}

// ---------------------------------------------------------------------------
// Error case tests
// ---------------------------------------------------------------------------

#[test]
fn error_unknown_element() {
    let diags = parse_diagnostics("@unknown");
    assert!(
        diags
            .iter()
            .any(|d| d.message.contains("unknown element @unknown")),
        "expected unknown element error, got: {:?}",
        diags
    );
}

#[test]
fn error_unknown_element_suggestion() {
    let diags = parse_diagnostics("@ro");
    assert!(
        diags
            .iter()
            .any(|d| d.message.contains("did you mean @row")),
        "expected suggestion, got: {:?}",
        diags
    );
}

#[test]
fn error_unknown_attribute() {
    let diags = parse_diagnostics("@el [bakground red]");
    assert!(
        diags
            .iter()
            .any(|d| d.message.contains("unknown attribute") && d.message.contains("background")),
        "expected unknown attribute with suggestion, got: {:?}",
        diags
    );
}

#[test]
fn error_unclosed_bracket() {
    let diags = parse_diagnostics("@el [padding 10");
    assert!(
        diags.iter().any(|d| d.message.contains("unclosed")),
        "expected unclosed bracket error, got: {:?}",
        diags
    );
}

#[test]
fn error_recursive_function() {
    let input = "@fn loop\n  @loop\n@loop";
    let diags = parse_diagnostics(input);
    assert!(
        diags
            .iter()
            .any(|d| d.message.contains("recursive function call")),
        "expected recursive function error, got: {:?}",
        diags
    );
}

#[test]
fn error_else_without_if() {
    let diags = parse_diagnostics("@else");
    assert!(
        diags
            .iter()
            .any(|d| d.message.contains("@else without matching @if")),
        "expected @else error, got: {:?}",
        diags
    );
}

#[test]
fn error_each_bad_syntax() {
    let diags = parse_diagnostics("@each $x");
    assert!(
        diags
            .iter()
            .any(|d| d.message.contains("@each requires")),
        "expected @each syntax error, got: {:?}",
        diags
    );
}

#[test]
fn error_numeric_validation() {
    let diags = parse_diagnostics("@el [padding abc]");
    assert!(
        diags
            .iter()
            .any(|d| d.message.contains("expects a numeric value")),
        "expected numeric validation warning, got: {:?}",
        diags
    );
}

#[test]
fn error_opacity_range() {
    let diags = parse_diagnostics("@el [opacity 2.0]");
    assert!(
        diags
            .iter()
            .any(|d| d.message.contains("between 0 and 1")),
        "expected opacity range warning, got: {:?}",
        diags
    );
}

#[test]
fn warning_fill_outside_row() {
    let diags = parse_diagnostics("@column\n  @el [width fill]");
    assert!(
        diags
            .iter()
            .any(|d| d.message.contains("width fill") && d.message.contains("@row")),
        "expected fill context warning, got: {:?}",
        diags
    );
}

#[test]
fn warning_fill_inside_row_ok() {
    let diags = parse_diagnostics("@row\n  @el [width fill]");
    assert!(
        !diags.iter().any(|d| d.message.contains("width fill")),
        "should not warn about width fill inside @row, got: {:?}",
        diags
    );
}

#[test]
fn if_condition_truthy() {
    let output = compile("@let x hello\n@if $x\n  @text visible");
    assert!(output.contains("visible"));
}

#[test]
fn if_condition_falsy() {
    let output = compile("@let x false\n@if $x\n  @text hidden");
    assert!(!output.contains("hidden"));
}

#[test]
fn each_loop_expansion() {
    let output = compile("@each $n in a,b,c\n  @text $n");
    assert!(output.contains("a"));
    assert!(output.contains("b"));
    assert!(output.contains("c"));
}

#[test]
fn aria_data_attrs_accepted() {
    let diags = parse_diagnostics("@el [aria-label Test, data-id 42]");
    assert!(
        !diags.iter().any(|d| d.message.contains("unknown attribute")),
        "aria-*/data-* should not produce warnings, got: {:?}",
        diags
    );
}
