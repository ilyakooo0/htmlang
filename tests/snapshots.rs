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

#[test]
fn snapshot_css_units() {
    snapshot_test("css_units");
}

#[test]
fn snapshot_positional() {
    snapshot_test("positional");
}

#[test]
fn snapshot_else_if() {
    snapshot_test("else_if");
}

#[test]
fn snapshot_meta_head() {
    snapshot_test("meta_head");
}

#[test]
fn snapshot_extra_css() {
    snapshot_test("extra_css");
}

#[test]
fn snapshot_fn_defaults() {
    snapshot_test("fn_defaults");
}

#[test]
fn snapshot_each_index() {
    snapshot_test("each_index");
}

#[test]
fn snapshot_style_block() {
    snapshot_test("style_block");
}

#[test]
fn snapshot_named_slots() {
    snapshot_test("named_slots");
}

#[test]
fn snapshot_each_range() {
    snapshot_test("each_range");
}

#[test]
fn snapshot_container_queries() {
    snapshot_test("container_queries");
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
fn error_else_if_without_if() {
    let diags = parse_diagnostics("@else if $x == 1");
    assert!(
        diags
            .iter()
            .any(|d| d.message.contains("@else without matching @if")),
        "expected @else error, got: {:?}",
        diags
    );
}

#[test]
fn else_if_chain() {
    let output = compile("@let x 2\n@if $x == 1\n  @text one\n@else if $x == 2\n  @text two\n@else\n  @text other");
    assert!(output.contains("two"));
    assert!(!output.contains("one"));
    assert!(!output.contains("other"));
}

#[test]
fn fn_default_used() {
    let output = compile("@fn test $x=hello\n  @text $x\n@test");
    assert!(output.contains("hello"));
}

#[test]
fn fn_default_overridden() {
    let output = compile("@fn test $x=hello\n  @text $x\n@test [x world]");
    assert!(output.contains("world"));
    assert!(!output.contains("hello"));
}

#[test]
fn each_with_index() {
    let output = compile("@each $item, $i in a,b,c\n  @text $i");
    assert!(output.contains("0"));
    assert!(output.contains("1"));
    assert!(output.contains("2"));
}

#[test]
fn css_unit_passthrough() {
    let output = compile("@page T\n@el [width 50%, padding 2rem]");
    assert!(output.contains("width:50%"));
    assert!(output.contains("padding:2rem"));
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

// ---------------------------------------------------------------------------
// New feature tests
// ---------------------------------------------------------------------------

// --- Unused variable/function/define warnings ---

#[test]
fn warning_unused_variable() {
    let diags = parse_diagnostics("@let color red\n@el [padding 10]");
    assert!(
        diags.iter().any(|d| d.message.contains("unused variable") && d.message.contains("color")),
        "expected unused variable warning, got: {:?}",
        diags
    );
}

#[test]
fn no_warning_used_variable() {
    let diags = parse_diagnostics("@let color red\n@el [background $color]");
    assert!(
        !diags.iter().any(|d| d.message.contains("unused variable")),
        "should not warn about used variable, got: {:?}",
        diags
    );
}

#[test]
fn warning_unused_function() {
    let diags = parse_diagnostics("@fn card\n  @el [padding 10]\n@el");
    assert!(
        diags.iter().any(|d| d.message.contains("unused function") && d.message.contains("card")),
        "expected unused function warning, got: {:?}",
        diags
    );
}

#[test]
fn no_warning_used_function() {
    let diags = parse_diagnostics("@fn card\n  @el [padding 10]\n@card");
    assert!(
        !diags.iter().any(|d| d.message.contains("unused function")),
        "should not warn about used function, got: {:?}",
        diags
    );
}

#[test]
fn warning_unused_define() {
    let diags = parse_diagnostics("@define card-style [padding 10]\n@el");
    assert!(
        diags.iter().any(|d| d.message.contains("unused define")),
        "expected unused define warning, got: {:?}",
        diags
    );
}

#[test]
fn no_warning_used_define() {
    let diags = parse_diagnostics("@define card-style [padding 10]\n@el [$card-style]");
    assert!(
        !diags.iter().any(|d| d.message.contains("unused define")),
        "should not warn about used define, got: {:?}",
        diags
    );
}

// --- Element-specific attribute validation ---

#[test]
fn warning_spacing_on_text() {
    let diags = parse_diagnostics("@text [spacing 10] hello");
    assert!(
        diags.iter().any(|d| d.message.contains("spacing") && d.message.contains("no effect")),
        "expected spacing on @text warning, got: {:?}",
        diags
    );
}

#[test]
fn no_warning_spacing_on_row() {
    let diags = parse_diagnostics("@row [spacing 10]");
    assert!(
        !diags.iter().any(|d| d.message.contains("spacing") && d.message.contains("no effect")),
        "should not warn about spacing on @row, got: {:?}",
        diags
    );
}

#[test]
fn warning_placeholder_on_row() {
    let diags = parse_diagnostics("@row [placeholder test]");
    assert!(
        diags.iter().any(|d| d.message.contains("placeholder") && d.message.contains("no effect")),
        "expected placeholder on @row warning, got: {:?}",
        diags
    );
}

#[test]
fn warning_for_on_non_label() {
    let diags = parse_diagnostics("@el [for email]");
    assert!(
        diags.iter().any(|d| d.message.contains("'for'") && d.message.contains("@label")),
        "expected 'for' on non-label warning, got: {:?}",
        diags
    );
}

// --- @each ranges ---

#[test]
fn each_range_basic() {
    let output = compile("@each $i in 1..3\n  @text $i");
    assert!(output.contains("1"));
    assert!(output.contains("2"));
    assert!(output.contains("3"));
}

#[test]
fn each_range_with_index() {
    let output = compile("@each $n, $i in 1..3\n  @text $i");
    assert!(output.contains("0"));
    assert!(output.contains("1"));
    assert!(output.contains("2"));
}

// --- Named slots ---

#[test]
fn named_slot_basic() {
    let output = compile("@fn card\n  @el\n    @slot header\n    @children\n@card\n  @slot header\n    @text Title\n  @text Body");
    assert!(output.contains("Title"));
    assert!(output.contains("Body"));
}

#[test]
fn named_slot_default_content() {
    let output = compile("@fn card\n  @el\n    @slot header\n      @text Default\n    @children\n@card\n  @text Body");
    assert!(output.contains("Default"));
    assert!(output.contains("Body"));
}

// --- @style block ---

#[test]
fn style_block_output() {
    let output = compile("@page Test\n@style\n  .custom { color: red; }\n@el [class custom]\n  @text styled");
    assert!(output.contains(".custom{color:red;}") || output.contains(".custom { color: red; }"));
    assert!(output.contains("styled"));
}

// --- Container queries ---

#[test]
fn container_attr() {
    let output = compile("@page T\n@el [container]");
    assert!(output.contains("container-type:inline-size"));
}

#[test]
fn container_name_attr() {
    let output = compile("@page T\n@el [container-name sidebar]");
    assert!(output.contains("container-name:sidebar"));
}

// --- Variable scoping in @if ---

#[test]
fn if_block_scopes_variables() {
    let output = compile("@let x before\n@if true\n  @let x inside\n@text $x");
    // $x should be "before" outside the @if block
    assert!(output.contains("before"));
}

// --- CSS custom vars not warned as unused ---

#[test]
fn css_var_not_warned_unused() {
    let diags = parse_diagnostics("@let --primary blue");
    assert!(
        !diags.iter().any(|d| d.message.contains("unused")),
        "CSS vars should not be warned as unused, got: {:?}",
        diags
    );
}

// ---------------------------------------------------------------------------
// Snapshot tests for new features
// ---------------------------------------------------------------------------

#[test]
fn snapshot_semantic_elements() {
    snapshot_test("semantic_elements");
}

#[test]
fn snapshot_list_elements() {
    snapshot_test("list_elements");
}

#[test]
fn snapshot_table_elements() {
    snapshot_test("table_elements");
}

#[test]
fn snapshot_media_elements() {
    snapshot_test("media_elements");
}

#[test]
fn snapshot_match_directive() {
    snapshot_test("match_directive");
}

#[test]
fn snapshot_new_css_attrs() {
    snapshot_test("new_css_attrs");
}

// ---------------------------------------------------------------------------
// Feature tests: semantic elements
// ---------------------------------------------------------------------------

#[test]
fn semantic_nav_renders_nav_tag() {
    let output = compile("@page T\n@nav\n  @text Links");
    assert!(output.contains("<nav"));
    assert!(output.contains("</nav>"));
}

#[test]
fn semantic_header_renders_header_tag() {
    let output = compile("@page T\n@header\n  @text Title");
    assert!(output.contains("<header"));
    assert!(output.contains("</header>"));
}

#[test]
fn semantic_footer_renders_footer_tag() {
    let output = compile("@page T\n@footer\n  @text Footer");
    assert!(output.contains("<footer"));
    assert!(output.contains("</footer>"));
}

#[test]
fn semantic_main_renders_main_tag() {
    let output = compile("@page T\n@main\n  @text Content");
    assert!(output.contains("<main"));
    assert!(output.contains("</main>"));
}

#[test]
fn semantic_section_renders_section_tag() {
    let output = compile("@page T\n@section\n  @text Section");
    assert!(output.contains("<section"));
    assert!(output.contains("</section>"));
}

#[test]
fn semantic_article_renders_article_tag() {
    let output = compile("@page T\n@article\n  @text Article");
    assert!(output.contains("<article"));
    assert!(output.contains("</article>"));
}

#[test]
fn semantic_aside_renders_aside_tag() {
    let output = compile("@page T\n@aside\n  @text Sidebar");
    assert!(output.contains("<aside"));
    assert!(output.contains("</aside>"));
}

// ---------------------------------------------------------------------------
// Feature tests: list elements
// ---------------------------------------------------------------------------

#[test]
fn list_renders_ul_by_default() {
    let output = compile("@page T\n@list\n  @item Hello");
    assert!(output.contains("<ul"));
    assert!(output.contains("<li"));
    assert!(output.contains("Hello"));
}

#[test]
fn list_renders_ol_with_ordered() {
    let output = compile("@page T\n@list [ordered]\n  @item First\n  @item Second");
    assert!(output.contains("<ol"));
    assert!(output.contains("<li"));
}

#[test]
fn item_alias_li() {
    let output = compile("@page T\n@list\n  @li Works");
    assert!(output.contains("<li"));
    assert!(output.contains("Works"));
}

// ---------------------------------------------------------------------------
// Feature tests: table elements
// ---------------------------------------------------------------------------

#[test]
fn table_renders_proper_tags() {
    let output = compile("@page T\n@table\n  @thead\n    @tr\n      @th Header\n  @tbody\n    @tr\n      @td Cell");
    assert!(output.contains("<table"));
    assert!(output.contains("<thead"));
    assert!(output.contains("<tbody"));
    assert!(output.contains("<tr"));
    assert!(output.contains("<th"));
    assert!(output.contains("<td"));
    assert!(output.contains("Header"));
    assert!(output.contains("Cell"));
}

// ---------------------------------------------------------------------------
// Feature tests: media elements
// ---------------------------------------------------------------------------

#[test]
fn video_renders_with_src() {
    let output = compile("@page T\n@video [controls] demo.mp4");
    assert!(output.contains("<video"));
    assert!(output.contains("src=\"demo.mp4\""));
    assert!(output.contains("controls"));
    assert!(output.contains("</video>"));
}

#[test]
fn audio_renders_with_src() {
    let output = compile("@page T\n@audio [controls] song.mp3");
    assert!(output.contains("<audio"));
    assert!(output.contains("src=\"song.mp3\""));
    assert!(output.contains("controls"));
}

#[test]
fn video_with_multiple_attrs() {
    let output = compile("@page T\n@video [controls, muted, autoplay, loop] clip.mp4");
    assert!(output.contains("controls"));
    assert!(output.contains("muted"));
    assert!(output.contains("autoplay"));
    assert!(output.contains("loop"));
}

// ---------------------------------------------------------------------------
// Feature tests: @match directive
// ---------------------------------------------------------------------------

#[test]
fn match_selects_correct_case() {
    let output = compile("@let x b\n@match $x\n  @case a\n    @text A\n  @case b\n    @text B\n  @default\n    @text D");
    assert!(output.contains("B"));
    assert!(!output.contains(">A<"));
    assert!(!output.contains(">D<"));
}

#[test]
fn match_falls_to_default() {
    let output = compile("@let x z\n@match $x\n  @case a\n    @text A\n  @default\n    @text Default");
    assert!(output.contains("Default"));
    assert!(!output.contains(">A<"));
}

#[test]
fn match_no_match_no_default() {
    let output = compile("@let x z\n@match $x\n  @case a\n    @text A\n  @case b\n    @text B");
    assert!(!output.contains(">A<"));
    assert!(!output.contains(">B<"));
}

// ---------------------------------------------------------------------------
// Feature tests: new CSS attributes
// ---------------------------------------------------------------------------

#[test]
fn css_aspect_ratio() {
    let output = compile("@page T\n@el [aspect-ratio 16/9]");
    assert!(output.contains("aspect-ratio:16/9"));
}

#[test]
fn css_outline() {
    let output = compile("@page T\n@el [outline 2 red]");
    assert!(output.contains("outline:2px solid red"));
}

#[test]
fn css_outline_no_color() {
    let output = compile("@page T\n@el [outline 3]");
    assert!(output.contains("outline:3px solid currentColor"));
}

#[test]
fn css_padding_inline() {
    let output = compile("@page T\n@el [padding-inline 20]");
    assert!(output.contains("padding-inline:20px"));
}

#[test]
fn css_padding_block() {
    let output = compile("@page T\n@el [padding-block 10]");
    assert!(output.contains("padding-block:10px"));
}

#[test]
fn css_margin_inline() {
    let output = compile("@page T\n@el [margin-inline 20]");
    assert!(output.contains("margin-inline:20px"));
}

#[test]
fn css_margin_block() {
    let output = compile("@page T\n@el [margin-block 10]");
    assert!(output.contains("margin-block:10px"));
}

#[test]
fn css_scroll_snap_type() {
    let output = compile("@page T\n@el [scroll-snap-type x mandatory]");
    assert!(output.contains("scroll-snap-type:x mandatory"));
}

#[test]
fn css_scroll_snap_align() {
    let output = compile("@page T\n@el [scroll-snap-align center]");
    assert!(output.contains("scroll-snap-align:center"));
}

// ---------------------------------------------------------------------------
// Feature tests: @warn / @debug
// ---------------------------------------------------------------------------

#[test]
fn warn_produces_diagnostic() {
    let diags = parse_diagnostics("@warn Something is wrong");
    assert!(
        diags.iter().any(|d| d.message == "Something is wrong"
            && d.severity == htmlang::parser::Severity::Warning),
        "expected @warn diagnostic, got: {:?}",
        diags
    );
}

#[test]
fn warn_substitutes_variables() {
    let diags = parse_diagnostics("@let name test\n@warn Missing $name value");
    assert!(
        diags.iter().any(|d| d.message.contains("Missing test value")),
        "expected substituted @warn, got: {:?}",
        diags
    );
}

// ---------------------------------------------------------------------------
// Feature tests: image optimization hints
// ---------------------------------------------------------------------------

#[test]
fn image_auto_lazy_loading() {
    let output = compile("@page T\n@image https://example.com/photo.jpg");
    assert!(output.contains("loading=\"lazy\""));
    assert!(output.contains("decoding=\"async\""));
}

#[test]
fn image_explicit_loading_not_doubled() {
    let output = compile("@page T\n@image [loading eager] https://example.com/photo.jpg");
    assert!(output.contains("loading=\"eager\""));
    assert!(!output.contains("loading=\"lazy\""));
}

// ---------------------------------------------------------------------------
// Feature tests: element-specific attribute validation
// ---------------------------------------------------------------------------

#[test]
fn warning_ordered_on_non_list() {
    let diags = parse_diagnostics("@el [ordered]");
    assert!(
        diags.iter().any(|d| d.message.contains("ordered") && d.message.contains("@list")),
        "expected ordered on non-list warning, got: {:?}",
        diags
    );
}

#[test]
fn no_warning_ordered_on_list() {
    let diags = parse_diagnostics("@list [ordered]\n  @item x");
    assert!(
        !diags.iter().any(|d| d.message.contains("ordered") && d.message.contains("no effect")),
        "should not warn about ordered on @list, got: {:?}",
        diags
    );
}

#[test]
fn warning_controls_on_non_media() {
    let diags = parse_diagnostics("@el [controls]");
    assert!(
        diags.iter().any(|d| d.message.contains("controls") && d.message.contains("@video")),
        "expected controls on non-media warning, got: {:?}",
        diags
    );
}

// ---------------------------------------------------------------------------
// Feature tests: formatter
// ---------------------------------------------------------------------------

#[test]
fn fmt_normalizes_indentation() {
    let input = "@row\n      @col\n            @text hello\n      @col\n            @text world";
    let formatted = htmlang::fmt::format(input);
    assert_eq!(formatted, "@row\n  @col\n    @text hello\n  @col\n    @text world\n");
}

#[test]
fn fmt_preserves_blank_lines() {
    let input = "@page Test\n\n@row\n    @text hello";
    let formatted = htmlang::fmt::format(input);
    assert_eq!(formatted, "@page Test\n\n@row\n  @text hello\n");
}

// ---------------------------------------------------------------------------
// Feature tests: string interpolation (already exists, verify)
// ---------------------------------------------------------------------------

#[test]
fn string_interpolation_in_text() {
    let output = compile("@let name World\n@text Hello $name!");
    assert!(output.contains("Hello World!"));
}

#[test]
fn string_interpolation_in_bare_text() {
    let output = compile("@let greeting Hi\n@el\n  $greeting there");
    assert!(output.contains("Hi there"));
}
