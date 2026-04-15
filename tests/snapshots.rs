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

// ---------------------------------------------------------------------------
// Snapshot tests for batch 3 features
// ---------------------------------------------------------------------------

#[test]
fn snapshot_new_elements() {
    snapshot_test("new_elements");
}

#[test]
fn snapshot_dark_mode() {
    snapshot_test("dark_mode");
}

#[test]
fn snapshot_new_css() {
    snapshot_test("new_css");
}

#[test]
fn snapshot_string_interpolation() {
    snapshot_test("string_interpolation");
}

#[test]
fn snapshot_each_destructuring() {
    snapshot_test("each_destructuring");
}

// ---------------------------------------------------------------------------
// Feature tests: new elements
// ---------------------------------------------------------------------------

#[test]
fn form_renders_form_tag() {
    let output = compile("@page T\n@form [method post] /submit\n  @input [type text]");
    assert!(output.contains("<form"));
    assert!(output.contains("action=\"/submit\""));
    assert!(output.contains("method=\"post\""));
}

#[test]
fn details_summary_renders() {
    let output = compile("@page T\n@details [open]\n  @summary Click me\n  @text Content");
    assert!(output.contains("<details"));
    assert!(output.contains(" open"));
    assert!(output.contains("<summary"));
    assert!(output.contains("Click me"));
}

#[test]
fn blockquote_renders() {
    let output = compile("@page T\n@blockquote\n  @text A quote\n  @cite Source");
    assert!(output.contains("<blockquote"));
    assert!(output.contains("<cite"));
    assert!(output.contains("Source"));
}

#[test]
fn code_renders_monospace() {
    let output = compile("@page T\n@code hello");
    assert!(output.contains("<code"));
    assert!(output.contains("font-family:ui-monospace,monospace"));
}

#[test]
fn pre_renders_with_whitespace() {
    let output = compile("@page T\n@pre\n  @text formatted");
    assert!(output.contains("<pre"));
    assert!(output.contains("white-space:pre"));
}

#[test]
fn hr_renders_self_closing() {
    let output = compile("@page T\n@hr");
    assert!(output.contains("<hr"));
}

#[test]
fn divider_alias_for_hr() {
    let output = compile("@page T\n@divider");
    assert!(output.contains("<hr"));
}

#[test]
fn figure_figcaption_renders() {
    let output = compile("@page T\n@figure\n  @image [alt test] photo.jpg\n  @figcaption Caption");
    assert!(output.contains("<figure"));
    assert!(output.contains("<figcaption"));
    assert!(output.contains("Caption"));
}

#[test]
fn progress_renders() {
    let output = compile("@page T\n@progress [value 70, max 100]");
    assert!(output.contains("<progress"));
    assert!(output.contains("value=\"70\""));
    assert!(output.contains("max=\"100\""));
}

#[test]
fn meter_renders() {
    let output = compile("@page T\n@meter [value 6, min 0, max 10]");
    assert!(output.contains("<meter"));
    assert!(output.contains("value=\"6\""));
}

// ---------------------------------------------------------------------------
// Feature tests: dark mode
// ---------------------------------------------------------------------------

#[test]
fn dark_mode_generates_media_query() {
    let output = compile("@page T\n@el [background white, dark:background #333]");
    assert!(output.contains("prefers-color-scheme:dark"));
    assert!(output.contains("background:#333"));
}

#[test]
fn print_generates_media_query() {
    let output = compile("@page T\n@el [display flex, print:display none]");
    assert!(output.contains("@media print"));
    assert!(output.contains("display:none"));
}

// ---------------------------------------------------------------------------
// Feature tests: new CSS attributes
// ---------------------------------------------------------------------------

#[test]
fn css_margin() {
    let output = compile("@page T\n@el [margin 20]");
    assert!(output.contains("margin:20px"));
}

#[test]
fn css_margin_x() {
    let output = compile("@page T\n@el [margin-x 10]");
    assert!(output.contains("margin-left:10px"));
    assert!(output.contains("margin-right:10px"));
}

#[test]
fn css_margin_y() {
    let output = compile("@page T\n@el [margin-y 10]");
    assert!(output.contains("margin-top:10px"));
    assert!(output.contains("margin-bottom:10px"));
}

#[test]
fn css_filter() {
    let output = compile("@page T\n@el [filter blur(5px)]");
    assert!(output.contains("filter:blur(5px)"));
}

#[test]
fn css_object_fit() {
    let output = compile("@page T\n@image [object-fit cover, width 200] test.jpg");
    assert!(output.contains("object-fit:cover"));
}

#[test]
fn css_text_shadow() {
    let output = compile("@page T\n@text [text-shadow 1px 1px black] Hello");
    assert!(output.contains("text-shadow:1px 1px black"));
}

#[test]
fn css_text_overflow() {
    let output = compile("@page T\n@text [text-overflow ellipsis] Hello");
    assert!(output.contains("text-overflow:ellipsis"));
}

#[test]
fn css_pointer_events() {
    let output = compile("@page T\n@el [pointer-events none]");
    assert!(output.contains("pointer-events:none"));
}

#[test]
fn css_user_select() {
    let output = compile("@page T\n@el [user-select none]");
    assert!(output.contains("user-select:none"));
}

#[test]
fn css_justify_content() {
    let output = compile("@page T\n@row [justify-content space-between]");
    assert!(output.contains("justify-content:space-between"));
}

#[test]
fn css_align_items() {
    let output = compile("@page T\n@row [align-items center]");
    assert!(output.contains("align-items:center"));
}

#[test]
fn css_order() {
    let output = compile("@page T\n@el [order 2]");
    assert!(output.contains("order:2"));
}

#[test]
fn css_background_size() {
    let output = compile("@page T\n@el [background-size cover]");
    assert!(output.contains("background-size:cover"));
}

#[test]
fn css_word_break() {
    let output = compile("@page T\n@el [word-break break-all]");
    assert!(output.contains("word-break:break-all"));
}

#[test]
fn css_overflow_wrap() {
    let output = compile("@page T\n@el [overflow-wrap break-word]");
    assert!(output.contains("overflow-wrap:break-word"));
}

// ---------------------------------------------------------------------------
// Feature tests: string interpolation
// ---------------------------------------------------------------------------

#[test]
fn let_string_interpolation() {
    let output = compile("@let name World\n@let greeting \"Hello $name\"\n@text $greeting");
    assert!(output.contains("Hello World"));
}

// ---------------------------------------------------------------------------
// Feature tests: @each destructuring
// ---------------------------------------------------------------------------

#[test]
fn each_destructuring_pairs() {
    let output = compile("@each $name, $role in Alice Admin, Bob User\n  @text $name is $role");
    assert!(output.contains("Alice is Admin"));
    assert!(output.contains("Bob is User"));
}

// ---------------------------------------------------------------------------
// Feature tests: diagnostics
// ---------------------------------------------------------------------------

#[test]
fn warning_missing_alt_on_image() {
    let diags = parse_diagnostics("@image photo.jpg");
    assert!(
        diags.iter().any(|d| d.message.contains("alt") && d.message.contains("accessibility")),
        "expected missing alt warning, got: {:?}",
        diags
    );
}

#[test]
fn no_warning_with_alt_on_image() {
    let diags = parse_diagnostics("@image [alt A photo] photo.jpg");
    assert!(
        !diags.iter().any(|d| d.message.contains("missing") && d.message.contains("alt")),
        "should not warn when alt is present, got: {:?}",
        diags
    );
}

#[test]
fn warning_invalid_hex_color() {
    let diags = parse_diagnostics("@el [color #ggg]");
    assert!(
        diags.iter().any(|d| d.message.contains("invalid hex color")),
        "expected invalid hex color warning, got: {:?}",
        diags
    );
}

#[test]
fn no_warning_valid_hex_color() {
    let diags = parse_diagnostics("@el [color #ff0000]");
    assert!(
        !diags.iter().any(|d| d.message.contains("invalid hex color")),
        "should not warn on valid hex color, got: {:?}",
        diags
    );
}

#[test]
fn warning_duplicate_attribute() {
    let diags = parse_diagnostics("@el [padding 10, padding 20]");
    assert!(
        diags.iter().any(|d| d.message.contains("duplicate attribute")),
        "expected duplicate attribute warning, got: {:?}",
        diags
    );
}

#[test]
fn no_warning_different_attributes() {
    let diags = parse_diagnostics("@el [padding 10, margin 20]");
    assert!(
        !diags.iter().any(|d| d.message.contains("duplicate")),
        "should not warn on different attributes, got: {:?}",
        diags
    );
}

// ---------------------------------------------------------------------------
// Feature tests: formatter improvements
// ---------------------------------------------------------------------------

#[test]
fn fmt_sorts_attributes() {
    let formatted = htmlang::fmt::format("@el [color red, width 200, padding 10]");
    // width (sizing) should come before padding (spacing) which should come before color (visual)
    assert!(formatted.contains("[width 200, padding 10, color red]"));
}

#[test]
fn fmt_multiline_brackets() {
    let input = "@el [\n  color red,\n  width 200\n]\n  @text hello";
    let formatted = htmlang::fmt::format(input);
    assert!(formatted.contains("[width 200, color red]"));
    assert!(formatted.contains("  @text hello"));
}

// ---------------------------------------------------------------------------
// New features: pseudo-states, child selectors, fragment, hidden, CSS props
// ---------------------------------------------------------------------------

#[test]
fn snapshot_pseudo_states_extended() {
    snapshot_test("pseudo_states_extended");
}

#[test]
fn snapshot_fragment() {
    snapshot_test("fragment");
}

#[test]
fn snapshot_hidden_attr() {
    snapshot_test("hidden_attr");
}

#[test]
fn snapshot_new_css_properties() {
    snapshot_test("new_css_properties");
}

#[test]
fn snapshot_lang_directive() {
    snapshot_test("lang_directive");
}

// --- Assertion tests for new pseudo-states ---

#[test]
fn focus_visible_generates_pseudo() {
    let output = compile("@page T\n@el [focus-visible:border 2 blue]");
    assert!(output.contains(":focus-visible"));
    assert!(output.contains("border:2px solid blue"));
}

#[test]
fn focus_within_generates_pseudo() {
    let output = compile("@page T\n@el [focus-within:background #eee]");
    assert!(output.contains(":focus-within"));
    assert!(output.contains("background:#eee"));
}

#[test]
fn disabled_generates_pseudo() {
    let output = compile("@page T\n@el [disabled:opacity 0.5]");
    assert!(output.contains(":disabled"));
    assert!(output.contains("opacity:0.5"));
}

#[test]
fn checked_generates_pseudo() {
    let output = compile("@page T\n@el [checked:background green]");
    assert!(output.contains(":checked"));
    assert!(output.contains("background:green"));
}

#[test]
fn placeholder_generates_pseudo() {
    let output = compile("@page T\n@input [type text, placeholder:color #999]");
    assert!(output.contains("::placeholder"));
    assert!(output.contains("color:#999"));
}

#[test]
fn first_child_generates_pseudo() {
    let output = compile("@page T\n@el [first:border-top 0]");
    assert!(output.contains(":first-child"));
}

#[test]
fn last_child_generates_pseudo() {
    let output = compile("@page T\n@el [last:border-bottom 0]");
    assert!(output.contains(":last-child"));
}

#[test]
fn odd_generates_pseudo() {
    let output = compile("@page T\n@el [odd:background #f5f5f5]");
    assert!(output.contains(":nth-child(odd)"));
}

#[test]
fn even_generates_pseudo() {
    let output = compile("@page T\n@el [even:background white]");
    assert!(output.contains(":nth-child(even)"));
}

// --- Assertion tests for fragment ---

#[test]
fn fragment_no_wrapper() {
    let output = compile("@page T\n@column\n  @fragment\n    @text A\n    @text B");
    // Fragment should NOT add any div wrapper
    assert!(!output.contains("<div class=\"_1\"><span"));
    // But children should still be present
    assert!(output.contains("A"));
    assert!(output.contains("B"));
}

// --- Assertion tests for hidden ---

#[test]
fn hidden_generates_display_none() {
    let output = compile("@page T\n@el [hidden]");
    assert!(output.contains("display:none"));
}

// --- Assertion tests for new CSS properties ---

#[test]
fn css_overflow_x() {
    let output = compile("@page T\n@el [overflow-x hidden]");
    assert!(output.contains("overflow-x:hidden"));
}

#[test]
fn css_overflow_y() {
    let output = compile("@page T\n@el [overflow-y auto]");
    assert!(output.contains("overflow-y:auto"));
}

#[test]
fn css_inset() {
    let output = compile("@page T\n@el [inset 0]");
    assert!(output.contains("inset:0"));
}

#[test]
fn css_accent_color() {
    let output = compile("@page T\n@input [type checkbox, accent-color blue]");
    assert!(output.contains("accent-color:blue"));
}

#[test]
fn css_caret_color() {
    let output = compile("@page T\n@input [type text, caret-color red]");
    assert!(output.contains("caret-color:red"));
}

#[test]
fn css_list_style() {
    let output = compile("@page T\n@list [list-style disc]");
    assert!(output.contains("list-style:disc"));
}

#[test]
fn css_border_collapse() {
    let output = compile("@page T\n@table [border-collapse collapse]");
    assert!(output.contains("border-collapse:collapse"));
}

#[test]
fn css_text_decoration_full() {
    let output = compile("@page T\n@text [text-decoration underline, text-decoration-color red, text-decoration-style wavy] Hello");
    assert!(output.contains("text-decoration:underline"));
    assert!(output.contains("text-decoration-color:red"));
    assert!(output.contains("text-decoration-style:wavy"));
}

#[test]
fn css_place_items() {
    let output = compile("@page T\n@el [grid, place-items center]");
    assert!(output.contains("place-items:center"));
}

#[test]
fn css_place_self() {
    let output = compile("@page T\n@el [place-self center]");
    assert!(output.contains("place-self:center"));
}

#[test]
fn css_scroll_behavior() {
    let output = compile("@page T\n@el [scroll-behavior smooth]");
    assert!(output.contains("scroll-behavior:smooth"));
}

#[test]
fn css_resize() {
    let output = compile("@page T\n@textarea [resize vertical]");
    assert!(output.contains("resize:vertical"));
}

// --- Assertion tests for @lang ---

#[test]
fn lang_sets_html_attr() {
    let output = compile("@page T\n@lang en\n@text Hello");
    assert!(output.contains("<html lang=\"en\">"));
}

#[test]
fn lang_not_present_without_directive() {
    let output = compile("@page T\n@text Hello");
    assert!(output.contains("<html>"));
    assert!(!output.contains("lang="));
}

// --- Assertion test for @favicon ---

#[test]
fn favicon_fallback_href() {
    // Nonexistent file should fall back to href
    let output = compile("@page T\n@favicon nonexistent.png\n@text Hello");
    assert!(output.contains("<link rel=\"icon\" href=\"nonexistent.png\">"));
}

// --- No warnings for new attrs ---

#[test]
fn no_warning_new_pseudo_prefixes() {
    let diags = parse_diagnostics("@el [focus-visible:border 2 blue, disabled:opacity 0.5, checked:background green]");
    assert!(
        !diags.iter().any(|d| d.message.contains("unknown attribute")),
        "new pseudo-state prefixes should be recognized, got: {:?}",
        diags
    );
}

#[test]
fn no_warning_child_selectors() {
    let diags = parse_diagnostics("@el [first:padding 0, last:padding 0, odd:background #eee, even:background white]");
    assert!(
        !diags.iter().any(|d| d.message.contains("unknown attribute")),
        "child selectors should be recognized, got: {:?}",
        diags
    );
}

#[test]
fn no_warning_new_css_attrs() {
    let diags = parse_diagnostics("@el [overflow-x hidden, overflow-y auto, inset 0, accent-color blue, hidden]");
    assert!(
        !diags.iter().any(|d| d.message.contains("unknown attribute")),
        "new CSS attrs should be recognized, got: {:?}",
        diags
    );
}

// =========================================================================
// New elements snapshot
// =========================================================================

#[test]
fn snapshot_new_elements_2() {
    snapshot_test("new_elements_2");
}

// =========================================================================
// New CSS properties snapshot
// =========================================================================

#[test]
fn snapshot_new_css_properties_2() {
    snapshot_test("new_css_properties_2");
}

// =========================================================================
// Extended media queries snapshot
// =========================================================================

#[test]
fn snapshot_media_extended() {
    snapshot_test("media_extended");
}

// =========================================================================
// @unless directive snapshot
// =========================================================================

#[test]
fn snapshot_unless_directive() {
    snapshot_test("unless_directive");
}

// =========================================================================
// @og directive snapshot
// =========================================================================

#[test]
fn snapshot_og_directive() {
    snapshot_test("og_directive");
}

// =========================================================================
// Arithmetic in @let snapshot
// =========================================================================

#[test]
fn snapshot_arithmetic() {
    snapshot_test("arithmetic");
}

// =========================================================================
// New element assertion tests
// =========================================================================

#[test]
fn element_dialog() {
    let output = compile("@page T\n@dialog [id modal, open]\n  @text Hello");
    assert!(output.contains("<dialog"));
    assert!(output.contains("id=\"modal\""));
    assert!(output.contains("open"));
    assert!(output.contains("</dialog>"));
}

#[test]
fn element_definition_list() {
    let output = compile("@page T\n@dl\n  @dt Term\n  @dd Definition");
    assert!(output.contains("<dl"));
    assert!(output.contains("<dt>Term</dt>"));
    assert!(output.contains("<dd"));
    assert!(output.contains(">Definition</dd>"));
}

#[test]
fn element_fieldset_legend() {
    let output = compile("@page T\n@fieldset\n  @legend Info\n  @input [type text, name n]");
    assert!(output.contains("<fieldset"));
    assert!(output.contains("<legend>Info</legend>"));
    assert!(output.contains("</fieldset>"));
}

#[test]
fn element_picture_source() {
    let output = compile("@page T\n@picture\n  @source [srcset wide.jpg, media (min-width: 800px)]\n  @image [alt Photo] photo.jpg");
    assert!(output.contains("<picture"));
    assert!(output.contains("<source"));
    assert!(output.contains("srcset=\"wide.jpg\""));
    assert!(output.contains("</picture>"));
}

#[test]
fn element_time() {
    let output = compile("@page T\n@time [datetime 2026-04-15] April 15");
    assert!(output.contains("<time"));
    assert!(output.contains("datetime=\"2026-04-15\""));
    assert!(output.contains("April 15"));
    assert!(output.contains("</time>"));
}

#[test]
fn element_mark() {
    let output = compile("@page T\n@mark highlighted");
    assert!(output.contains("<mark>highlighted</mark>"));
}

#[test]
fn element_kbd() {
    let output = compile("@page T\n@kbd Ctrl+C");
    assert!(output.contains("<kbd"));
    assert!(output.contains("Ctrl+C</kbd>"));
}

#[test]
fn element_abbr() {
    let output = compile("@page T\n@abbr [title HyperText Markup Language] HTML");
    assert!(output.contains("<abbr"));
    assert!(output.contains("title=\"HyperText Markup Language\""));
    assert!(output.contains("HTML"));
}

#[test]
fn element_datalist() {
    let output = compile("@page T\n@datalist [id browsers]\n  @option Chrome\n  @option Firefox");
    assert!(output.contains("<datalist"));
    assert!(output.contains("id=\"browsers\""));
    assert!(output.contains("</datalist>"));
}

// =========================================================================
// New CSS property assertion tests
// =========================================================================

#[test]
fn css_clip_path() {
    let output = compile("@page T\n@el [clip-path circle(50%)]");
    assert!(output.contains("clip-path:circle(50%)"));
}

#[test]
fn css_mix_blend_mode() {
    let output = compile("@page T\n@el [mix-blend-mode multiply]");
    assert!(output.contains("mix-blend-mode:multiply"));
}

#[test]
fn css_background_blend_mode() {
    let output = compile("@page T\n@el [background-blend-mode overlay]");
    assert!(output.contains("background-blend-mode:overlay"));
}

#[test]
fn css_writing_mode() {
    let output = compile("@page T\n@el [writing-mode vertical-rl]");
    assert!(output.contains("writing-mode:vertical-rl"));
}

#[test]
fn css_column_count() {
    let output = compile("@page T\n@column [column-count 3]");
    assert!(output.contains("column-count:3"));
}

#[test]
fn css_column_gap() {
    let output = compile("@page T\n@column [column-gap 20]");
    assert!(output.contains("column-gap:20px"));
}

#[test]
fn css_text_indent() {
    let output = compile("@page T\n@paragraph [text-indent 2em]");
    assert!(output.contains("text-indent:2em"));
}

#[test]
fn css_hyphens() {
    let output = compile("@page T\n@paragraph [hyphens auto]");
    assert!(output.contains("hyphens:auto"));
}

#[test]
fn css_flex_grow() {
    let output = compile("@page T\n@el [flex-grow 2]");
    assert!(output.contains("flex-grow:2"));
}

#[test]
fn css_flex_shrink() {
    let output = compile("@page T\n@el [flex-shrink 0]");
    assert!(output.contains("flex-shrink:0"));
}

#[test]
fn css_flex_basis() {
    let output = compile("@page T\n@el [flex-basis 200]");
    assert!(output.contains("flex-basis:200px"));
}

#[test]
fn css_isolation() {
    let output = compile("@page T\n@el [isolation isolate]");
    assert!(output.contains("isolation:isolate"));
}

#[test]
fn css_place_content() {
    let output = compile("@page T\n@el [grid, place-content center]");
    assert!(output.contains("place-content:center"));
}

#[test]
fn css_background_image() {
    let output = compile("@page T\n@el [background-image linear-gradient(red, blue)]");
    assert!(output.contains("background-image:linear-gradient(red, blue)"));
}

// =========================================================================
// Media prefix assertion tests
// =========================================================================

#[test]
fn media_2xl_breakpoint() {
    let output = compile("@page T\n@el [2xl:padding 40]");
    assert!(output.contains("@media(min-width:1536px)"));
    assert!(output.contains("padding:40px"));
}

#[test]
fn media_motion_reduce() {
    let output = compile("@page T\n@el [motion-reduce:transition none]");
    assert!(output.contains("@media(prefers-reduced-motion:reduce)"));
    assert!(output.contains("transition:none"));
}

#[test]
fn media_motion_safe() {
    let output = compile("@page T\n@el [motion-safe:animation spin 1s]");
    assert!(output.contains("@media(prefers-reduced-motion:no-preference)"));
    assert!(output.contains("animation:spin 1s"));
}

#[test]
fn media_landscape() {
    let output = compile("@page T\n@el [landscape:padding 10]");
    assert!(output.contains("@media(orientation:landscape)"));
    assert!(output.contains("padding:10px"));
}

#[test]
fn media_portrait() {
    let output = compile("@page T\n@el [portrait:padding 40]");
    assert!(output.contains("@media(orientation:portrait)"));
    assert!(output.contains("padding:40px"));
}

// =========================================================================
// @unless assertion tests
// =========================================================================

#[test]
fn unless_false_shows_content() {
    let output = compile("@page T\n@let show false\n@unless $show\n  @text Visible");
    assert!(output.contains("Visible"));
}

#[test]
fn unless_true_hides_content() {
    let output = compile("@page T\n@let show true\n@unless $show\n  @text Hidden");
    assert!(!output.contains("Hidden"));
}

// =========================================================================
// @og assertion tests
// =========================================================================

#[test]
fn og_tags_in_output() {
    let output = compile("@page T\n@og title \"My Page\"\n@og image \"https://example.com/img.png\"\n@text Hello");
    assert!(output.contains("og:title"));
    assert!(output.contains("My Page"));
    assert!(output.contains("og:image"));
    assert!(output.contains("https://example.com/img.png"));
}

// =========================================================================
// Arithmetic assertion tests
// =========================================================================

#[test]
fn let_arithmetic_multiply() {
    let output = compile("@page T\n@let x 10\n@let y $x * 2\n@el [width $y]\n  @text test");
    assert!(output.contains("width:20px"));
}

#[test]
fn let_arithmetic_add() {
    let output = compile("@page T\n@let a 10\n@let b $a + 5\n@el [padding $b]\n  @text test");
    assert!(output.contains("padding:15px"));
}

#[test]
fn let_arithmetic_divide() {
    let output = compile("@page T\n@let x 200 / 4\n@el [height $x]\n  @text test");
    assert!(output.contains("height:50px"));
}

// =========================================================================
// New diagnostics assertion tests
// =========================================================================

#[test]
fn warning_missing_input_type() {
    let diags = parse_diagnostics("@input [name email]");
    assert!(
        diags.iter().any(|d| d.message.contains("missing 'type'")),
        "should warn about missing type on @input, got: {:?}",
        diags
    );
}

#[test]
fn warning_link_without_text() {
    let diags = parse_diagnostics("@link https://example.com");
    assert!(
        diags.iter().any(|d| d.message.contains("no visible text")),
        "should warn about @link without visible text, got: {:?}",
        diags
    );
}

#[test]
fn no_warning_link_with_text() {
    let diags = parse_diagnostics("@link https://example.com Click here");
    assert!(
        !diags.iter().any(|d| d.message.contains("no visible text")),
        "should not warn when @link has text, got: {:?}",
        diags
    );
}

// =========================================================================
// No warnings for new CSS attributes
// =========================================================================

#[test]
fn no_warning_new_css_properties_2() {
    let diags = parse_diagnostics(
        "@el [clip-path circle(50%), mix-blend-mode multiply, writing-mode vertical-rl, isolation isolate]"
    );
    assert!(
        !diags.iter().any(|d| d.message.contains("unknown attribute")),
        "new CSS properties should be recognized, got: {:?}",
        diags
    );
}

#[test]
fn no_warning_new_css_properties_3() {
    let diags = parse_diagnostics(
        "@el [column-count 3, column-gap 20, text-indent 2em, hyphens auto, flex-grow 1, flex-shrink 0, flex-basis 200, place-content center, background-image url(x)]"
    );
    assert!(
        !diags.iter().any(|d| d.message.contains("unknown attribute")),
        "new CSS properties should be recognized, got: {:?}",
        diags
    );
}

#[test]
fn no_warning_new_media_prefixes() {
    let diags = parse_diagnostics(
        "@el [2xl:padding 40, motion-safe:animation none, motion-reduce:transition none, landscape:width 100%, portrait:padding 20]"
    );
    assert!(
        !diags.iter().any(|d| d.message.contains("unknown attribute")),
        "new media prefixes should be recognized, got: {:?}",
        diags
    );
}

// ---------------------------------------------------------------------------
// Feature: if() conditional attribute values
// ---------------------------------------------------------------------------

#[test]
fn snapshot_if_expr() {
    snapshot_test("if_expr");
}

#[test]
fn if_expr_true_branch() {
    let output = compile("@let x true\n@el [background if($x, blue, gray)]\n  test");
    assert!(output.contains("blue"), "should use true branch, got: {}", output);
    assert!(!output.contains("gray"), "should not contain false branch");
}

#[test]
fn if_expr_false_branch() {
    let output = compile("@let x false\n@el [background if($x, blue, gray)]\n  test");
    assert!(output.contains("gray"), "should use false branch, got: {}", output);
    assert!(!output.contains("blue"), "should not contain true branch");
}

#[test]
fn if_expr_equality_condition() {
    let output = compile("@let theme dark\n@el [color if($theme == dark, white, black)]\n  test");
    assert!(output.contains("white"), "should match equality, got: {}", output);
}

#[test]
fn if_expr_inequality_condition() {
    let output = compile("@let mode light\n@el [color if($mode != dark, green, red)]\n  test");
    assert!(output.contains("green"), "should match inequality, got: {}", output);
}

#[test]
fn if_expr_passthrough_non_if() {
    // Regular values should not be affected
    let output = compile("@el [background blue]\n  test");
    assert!(output.contains("blue"));
}

// ---------------------------------------------------------------------------
// Feature: HTML-to-HL converter
// ---------------------------------------------------------------------------

#[test]
fn convert_basic_div() {
    let hl = htmlang::convert::convert("<div>Hello</div>");
    assert!(hl.contains("@el"), "div should become @el: {}", hl);
    assert!(hl.contains("Hello"), "text should be preserved: {}", hl);
}

#[test]
fn convert_paragraph() {
    let hl = htmlang::convert::convert("<p>Some text</p>");
    assert!(hl.contains("@paragraph"), "p should become @paragraph: {}", hl);
}

#[test]
fn convert_link() {
    let hl = htmlang::convert::convert("<a href=\"https://example.com\">Click</a>");
    assert!(hl.contains("@link"), "a should become @link: {}", hl);
    assert!(hl.contains("https://example.com"), "href preserved: {}", hl);
}

#[test]
fn convert_image() {
    let hl = htmlang::convert::convert("<img src=\"photo.jpg\" alt=\"A photo\">");
    assert!(hl.contains("@image"), "img should become @image: {}", hl);
    assert!(hl.contains("photo.jpg"), "src preserved: {}", hl);
    assert!(hl.contains("alt A photo"), "alt preserved: {}", hl);
}

#[test]
fn convert_nested() {
    let hl = htmlang::convert::convert("<div><span>inner</span></div>");
    assert!(hl.contains("@el"), "outer div: {}", hl);
    assert!(hl.contains("@text"), "span becomes @text: {}", hl);
    assert!(hl.contains("inner"), "text preserved: {}", hl);
}

#[test]
fn convert_semantic_elements() {
    let hl = htmlang::convert::convert("<nav><header>H</header></nav>");
    assert!(hl.contains("@nav"), "nav preserved: {}", hl);
    assert!(hl.contains("@header"), "header preserved: {}", hl);
}

#[test]
fn convert_list() {
    let hl = htmlang::convert::convert("<ul><li>A</li><li>B</li></ul>");
    assert!(hl.contains("@list"), "ul becomes @list: {}", hl);
    assert!(hl.contains("@item"), "li becomes @item: {}", hl);
}

#[test]
fn convert_ordered_list() {
    let hl = htmlang::convert::convert("<ol><li>First</li></ol>");
    assert!(hl.contains("@list [ordered]"), "ol becomes @list [ordered]: {}", hl);
}

#[test]
fn convert_form_elements() {
    let hl = htmlang::convert::convert("<form><input type=\"text\"><button>Go</button></form>");
    assert!(hl.contains("@form"), "form preserved: {}", hl);
    assert!(hl.contains("@input"), "input preserved: {}", hl);
    assert!(hl.contains("@button"), "button preserved: {}", hl);
}

#[test]
fn convert_table() {
    let hl = htmlang::convert::convert("<table><tr><td>Cell</td></tr></table>");
    assert!(hl.contains("@table"), "table: {}", hl);
    assert!(hl.contains("@tr"), "tr: {}", hl);
    assert!(hl.contains("@td"), "td: {}", hl);
}

#[test]
fn convert_strips_html_boilerplate() {
    let hl = htmlang::convert::convert("<!DOCTYPE html><html><head><title>T</title></head><body><p>Hi</p></body></html>");
    assert!(hl.contains("@paragraph"), "should extract body content: {}", hl);
    assert!(!hl.contains("DOCTYPE"), "should strip doctype: {}", hl);
}

#[test]
fn convert_inline_style() {
    let hl = htmlang::convert::convert("<div style=\"padding: 20px; background: red;\">X</div>");
    assert!(hl.contains("padding 20"), "padding converted: {}", hl);
    assert!(hl.contains("background red"), "background converted: {}", hl);
}

#[test]
fn convert_script_to_raw() {
    let hl = htmlang::convert::convert("<script>alert('hi')</script>");
    assert!(hl.contains("@raw"), "script becomes @raw: {}", hl);
}

// ---------------------------------------------------------------------------
// New improvement tests (batch 3)
// ---------------------------------------------------------------------------

#[test]
fn snapshot_new_elements_3() {
    snapshot_test("new_elements_3");
}

#[test]
fn snapshot_new_css_properties_3() {
    snapshot_test("new_css_properties_3");
}

#[test]
fn snapshot_pseudo_elements() {
    snapshot_test("pseudo_elements");
}

#[test]
fn snapshot_each_else() {
    snapshot_test("each_else");
}

#[test]
fn each_else_empty_list() {
    // Variable that resolves to empty string: @each produces no items → @else fires
    let output = compile("@let empty \"\"\n@each $item in $empty\n  @text $item\n@else\n  @text fallback");
    assert!(output.contains("fallback"), "should render @else block when list is empty: {}", output);
}

#[test]
fn each_else_non_empty_list() {
    let output = compile("@each $x in a,b\n  @text $x\n@else\n  @text empty");
    assert!(output.contains("a"), "should render loop items: {}", output);
    assert!(output.contains("b"), "should render loop items: {}", output);
    assert!(!output.contains("empty"), "should not render @else block when list is non-empty: {}", output);
}

#[test]
fn pseudo_element_before_content() {
    let output = compile("@el [before:content arrow, before:color red]\n  Hello");
    assert!(output.contains("::before"), "should generate ::before CSS: {}", output);
    assert!(output.contains("content:\"arrow\""), "should generate content property: {}", output);
    assert!(output.contains("color:red"), "should generate color in ::before: {}", output);
}

#[test]
fn pseudo_element_after_content() {
    let output = compile("@el [after:content ✓]\n  Done");
    assert!(output.contains("::after"), "should generate ::after CSS: {}", output);
    assert!(output.contains("content:\"✓\""), "should generate content: {}", output);
}

#[test]
fn css_font_weight_numeric() {
    let output = compile("@text [font-weight 300] Light text");
    assert!(output.contains("font-weight:300"), "should generate font-weight CSS: {}", output);
}

#[test]
fn css_text_wrap_balance() {
    let output = compile("@text [text-wrap balance] Balanced");
    assert!(output.contains("text-wrap:balance"), "should generate text-wrap CSS: {}", output);
}

#[test]
fn css_touch_action() {
    let output = compile("@el [touch-action none]\n  No touch");
    assert!(output.contains("touch-action:none"), "should generate touch-action CSS: {}", output);
}

#[test]
fn css_content_visibility() {
    let output = compile("@el [content-visibility auto]\n  Lazy");
    assert!(output.contains("content-visibility:auto"), "should generate content-visibility CSS: {}", output);
}

#[test]
fn css_scroll_margin() {
    let output = compile("@el [scroll-margin-top 80]\n  Offset");
    assert!(output.contains("scroll-margin-top:80px"), "should generate scroll-margin-top CSS: {}", output);
}

#[test]
fn element_iframe() {
    let output = compile("@iframe [width fill, height 400] https://example.com");
    assert!(output.contains("<iframe"), "should generate iframe tag: {}", output);
    assert!(output.contains("src=\"https://example.com\""), "should have src: {}", output);
}

#[test]
fn element_canvas() {
    let output = compile("@canvas [width 400, height 300, id myCanvas]");
    assert!(output.contains("<canvas"), "should generate canvas tag: {}", output);
    assert!(output.contains("id=\"myCanvas\""), "should have id: {}", output);
}

#[test]
fn element_output() {
    let output = compile("@output [for a b]\n  42");
    assert!(output.contains("<output"), "should generate output tag: {}", output);
}

// =========================================================================
// Batch 4: New elements, @each step, pseudo selectors, container queries
// =========================================================================

#[test]
fn snapshot_new_elements_4() {
    snapshot_test("new_elements_4");
}

#[test]
fn snapshot_each_step() {
    snapshot_test("each_step");
}

#[test]
fn snapshot_selection_pseudo() {
    snapshot_test("selection_pseudo");
}

#[test]
fn snapshot_nth_pseudo() {
    snapshot_test("nth_pseudo");
}

#[test]
fn snapshot_direction_attr() {
    snapshot_test("direction_attr");
}

// --- Grid element ---

#[test]
fn element_grid() {
    let output = compile("@page T\n@grid [grid-cols 3, gap 16]\n  @el\n    @text A");
    assert!(output.contains("display:grid"), "grid should have display:grid: {}", output);
    assert!(output.contains("grid-template-columns:repeat(3,1fr)"), "should have 3 cols: {}", output);
}

// --- Stack element ---

#[test]
fn element_stack() {
    let output = compile("@page T\n@stack [width 200, height 200]\n  @el\n    @text Layer");
    assert!(output.contains("position:relative"), "stack should have position:relative: {}", output);
}

// --- Spacer element ---

#[test]
fn element_spacer() {
    let output = compile("@page T\n@row\n  @text Left\n  @spacer\n  @text Right");
    assert!(output.contains("flex:1"), "spacer should have flex:1: {}", output);
}

// --- Badge element ---

#[test]
fn element_badge() {
    let output = compile("@page T\n@badge [background red, color white] 3");
    assert!(output.contains("<span"), "badge renders as span: {}", output);
    assert!(output.contains("border-radius:9999px"), "badge should be pill-shaped: {}", output);
    assert!(output.contains("3"), "badge content: {}", output);
}

// --- Tooltip element ---

#[test]
fn element_tooltip() {
    let output = compile("@page T\n@tooltip Hover for info\n  @text Help");
    assert!(output.contains("title=\"Hover for info\""), "tooltip should have title attr: {}", output);
    assert!(output.contains("cursor:help"), "tooltip should have cursor:help: {}", output);
}

// --- @each step ---

#[test]
fn each_step_basic() {
    let output = compile("@each $i in 0..20 step 5\n  @text $i");
    assert!(output.contains(">0<"), "should include 0: {}", output);
    assert!(output.contains(">5<"), "should include 5: {}", output);
    assert!(output.contains(">10<"), "should include 10: {}", output);
    assert!(output.contains(">15<"), "should include 15: {}", output);
    assert!(output.contains(">20<"), "should include 20: {}", output);
    assert!(!output.contains(">3<"), "should not include 3: {}", output);
}

#[test]
fn each_step_reverse() {
    let output = compile("@each $i in 10..1 step 3\n  @text $i");
    assert!(output.contains(">10<"), "should include 10: {}", output);
    assert!(output.contains(">7<"), "should include 7: {}", output);
    assert!(output.contains(">4<"), "should include 4: {}", output);
    assert!(output.contains(">1<"), "should include 1: {}", output);
}

// --- selection: pseudo ---

#[test]
fn selection_pseudo_generates_css() {
    let output = compile("@page T\n@text [selection:background blue, selection:color white] Select me");
    assert!(output.contains("::selection"), "should generate ::selection: {}", output);
    assert!(output.contains("background:blue"), "should have bg: {}", output);
}

// --- nth: pseudo ---

#[test]
fn nth_pseudo_generates_css() {
    let output = compile("@page T\n@el [nth:3:background red]\n  @text test");
    assert!(output.contains(":nth-child(3)"), "should generate :nth-child(3): {}", output);
    assert!(output.contains("background:red"), "should have bg: {}", output);
}

#[test]
fn nth_pseudo_formula() {
    let output = compile("@page T\n@el [nth:2n:background #eee]\n  @text test");
    assert!(output.contains(":nth-child(2n)"), "should generate :nth-child(2n): {}", output);
}

// --- container query prefix ---

#[test]
fn container_query_generates_css() {
    let output = compile("@page T\n@el [container]\n  @el [cq-sm:padding 20]\n    @text test");
    assert!(output.contains("@container(min-width:640px)"), "should generate container query: {}", output);
    assert!(output.contains("padding:20px"), "should have padding: {}", output);
}

// --- direction attribute ---

#[test]
fn direction_rtl() {
    let output = compile("@page T\n@el [direction rtl]\n  @text RTL text");
    assert!(output.contains("direction:rtl"), "should generate direction:rtl: {}", output);
}

// --- contrast checker ---

#[test]
fn warning_low_contrast() {
    let diags = parse_diagnostics("@el [background #ffffff, color #cccccc]\n  @text test");
    assert!(
        diags.iter().any(|d| d.message.contains("low contrast ratio")),
        "should warn about low contrast, got: {:?}",
        diags
    );
}

#[test]
fn no_warning_good_contrast() {
    let diags = parse_diagnostics("@el [background #ffffff, color #000000]\n  @text test");
    assert!(
        !diags.iter().any(|d| d.message.contains("low contrast ratio")),
        "should not warn about good contrast, got: {:?}",
        diags
    );
}

// --- no warnings for new features ---

#[test]
fn no_warning_selection_prefix() {
    let diags = parse_diagnostics("@el [selection:background blue]");
    assert!(
        !diags.iter().any(|d| d.message.contains("unknown attribute")),
        "selection: prefix should be recognized, got: {:?}",
        diags
    );
}

#[test]
fn no_warning_nth_prefix() {
    let diags = parse_diagnostics("@el [nth:3:background red]");
    assert!(
        !diags.iter().any(|d| d.message.contains("unknown attribute")),
        "nth: prefix should be recognized, got: {:?}",
        diags
    );
}

#[test]
fn no_warning_cq_prefix() {
    let diags = parse_diagnostics("@el [cq-sm:padding 20]");
    assert!(
        !diags.iter().any(|d| d.message.contains("unknown attribute")),
        "cq- prefix should be recognized, got: {:?}",
        diags
    );
}

#[test]
fn no_warning_direction_attr() {
    let diags = parse_diagnostics("@el [direction rtl]");
    assert!(
        !diags.iter().any(|d| d.message.contains("unknown attribute")),
        "direction should be recognized, got: {:?}",
        diags
    );
}

#[test]
fn no_warning_new_elements() {
    // Grid, stack, spacer, badge, tooltip should all parse without errors
    let diags = parse_diagnostics("@grid\n  @text A");
    assert!(
        !diags.iter().any(|d| d.severity == htmlang::parser::Severity::Error),
        "grid should parse, got: {:?}",
        diags
    );
    let diags = parse_diagnostics("@stack\n  @text A");
    assert!(
        !diags.iter().any(|d| d.severity == htmlang::parser::Severity::Error),
        "stack should parse, got: {:?}",
        diags
    );
    let diags = parse_diagnostics("@row\n  @spacer");
    assert!(
        !diags.iter().any(|d| d.severity == htmlang::parser::Severity::Error),
        "spacer should parse, got: {:?}",
        diags
    );
    let diags = parse_diagnostics("@badge [background red] 5");
    assert!(
        !diags.iter().any(|d| d.severity == htmlang::parser::Severity::Error),
        "badge should parse, got: {:?}",
        diags
    );
    let diags = parse_diagnostics("@tooltip Help text\n  @text Hover");
    assert!(
        !diags.iter().any(|d| d.severity == htmlang::parser::Severity::Error),
        "tooltip should parse, got: {:?}",
        diags
    );
}

// --- New feature tests ---

#[test]
fn snapshot_variable_filters() {
    snapshot_test("variable_filters");
}

#[test]
fn snapshot_new_elements_5() {
    snapshot_test("new_elements_5");
}

#[test]
fn snapshot_css_shorthands() {
    snapshot_test("css_shorthands");
}

#[test]
fn test_variable_filters() {
    let result = htmlang::parser::parse("@let name hello\n@text $name|uppercase");
    assert!(result.diagnostics.iter().all(|d| d.severity != htmlang::parser::Severity::Error));
    let html = htmlang::codegen::generate(&result.document);
    assert!(html.contains("HELLO"), "uppercase filter should work, got: {}", html);

    let result = htmlang::parser::parse("@let name HELLO\n@text $name|lowercase");
    let html = htmlang::codegen::generate(&result.document);
    assert!(html.contains("hello"), "lowercase filter should work, got: {}", html);

    let result = htmlang::parser::parse("@let name hello\n@text $name|capitalize");
    let html = htmlang::codegen::generate(&result.document);
    assert!(html.contains("Hello"), "capitalize filter should work, got: {}", html);

    let result = htmlang::parser::parse("@let name hello\n@text $name|length");
    let html = htmlang::codegen::generate(&result.document);
    assert!(html.contains("5"), "length filter should work, got: {}", html);

    let result = htmlang::parser::parse("@let name hello\n@text $name|reverse");
    let html = htmlang::codegen::generate(&result.document);
    assert!(html.contains("olleh"), "reverse filter should work, got: {}", html);

    let result = htmlang::parser::parse("@let name hello world\n@text $name|truncate:5");
    let html = htmlang::codegen::generate(&result.document);
    assert!(html.contains("hello..."), "truncate filter should work, got: {}", html);
}

#[test]
fn test_new_elements_parse() {
    // Avatar
    let diags = parse_diagnostics("@avatar [width 48, height 48]\n  @text AB");
    assert!(!diags.iter().any(|d| d.severity == htmlang::parser::Severity::Error), "avatar: {:?}", diags);

    // Carousel
    let diags = parse_diagnostics("@carousel [gap 16]\n  @el Slide 1");
    assert!(!diags.iter().any(|d| d.severity == htmlang::parser::Severity::Error), "carousel: {:?}", diags);

    // Chip
    let diags = parse_diagnostics("@chip [background #eee] Tag");
    assert!(!diags.iter().any(|d| d.severity == htmlang::parser::Severity::Error), "chip: {:?}", diags);

    // Tag
    let diags = parse_diagnostics("@tag [color blue] v1.0");
    assert!(!diags.iter().any(|d| d.severity == htmlang::parser::Severity::Error), "tag: {:?}", diags);
}

#[test]
fn test_css_shorthands_output() {
    let result = htmlang::parser::parse("@text [truncate] Hello");
    let html = htmlang::codegen::generate(&result.document);
    assert!(html.contains("text-overflow:ellipsis"), "truncate should add ellipsis, got: {}", html);
    assert!(html.contains("white-space:nowrap"), "truncate should add nowrap");

    let result = htmlang::parser::parse("@paragraph [line-clamp 3] Text");
    let html = htmlang::codegen::generate(&result.document);
    assert!(html.contains("-webkit-line-clamp:3"), "line-clamp should work, got: {}", html);

    let result = htmlang::parser::parse("@el [blur 4] Content");
    let html = htmlang::codegen::generate(&result.document);
    assert!(html.contains("filter:blur(4px)"), "blur should work, got: {}", html);

    let result = htmlang::parser::parse("@el [backdrop-blur 10] Content");
    let html = htmlang::codegen::generate(&result.document);
    assert!(html.contains("backdrop-filter:blur(10px)"), "backdrop-blur should work, got: {}", html);

    let result = htmlang::parser::parse("@el [no-scrollbar] Content");
    let html = htmlang::codegen::generate(&result.document);
    assert!(html.contains("scrollbar-width:none"), "no-scrollbar should work, got: {}", html);

    let result = htmlang::parser::parse("@el [skeleton, width 100, height 20] Content");
    let html = htmlang::codegen::generate(&result.document);
    assert!(html.contains("hl-skeleton"), "skeleton should add animation, got: {}", html);
    assert!(html.contains("@keyframes hl-skeleton"), "skeleton should add keyframes, got: {}", html);

    let result = htmlang::parser::parse("@el [gradient #fff #000] Content");
    let html = htmlang::codegen::generate(&result.document);
    assert!(html.contains("linear-gradient(#fff,#000)"), "gradient should work, got: {}", html);

    let result = htmlang::parser::parse("@el [gradient #fff #000 45deg] Content");
    let html = htmlang::codegen::generate(&result.document);
    assert!(html.contains("linear-gradient(45deg,#fff,#000)"), "gradient with angle should work, got: {}", html);
}

#[test]
fn test_carousel_children_snap() {
    let result = htmlang::parser::parse("@carousel\n  @el A\n  @el B");
    let html = htmlang::codegen::generate(&result.document);
    assert!(html.contains("scroll-snap-align:start"), "carousel children should have snap-align, got: {}", html);
}

#[test]
fn test_use_directive() {
    // We can't test @use with actual files in unit tests easily, but we can verify
    // the parser recognizes the directive without errors when it can't find the file
    let result = htmlang::parser::parse("@use nonexistent.hl card");
    let has_use_error = result.diagnostics.iter().any(|d|
        d.message.contains("cannot use") && d.severity == htmlang::parser::Severity::Error
    );
    assert!(has_use_error, "@use should report error for missing file, got: {:?}", result.diagnostics);
}

#[test]
fn test_enhanced_keyframes() {
    let result = htmlang::parser::parse("@keyframes fade-in\n  from [opacity 0]\n  to [opacity 1]\n@el [animation fade-in 0.3s] Content");
    let html = htmlang::codegen::generate(&result.document);
    assert!(html.contains("@keyframes fade-in{from{opacity:0;}to{opacity:1;}}"), "keyframes should parse htmlang syntax, got: {}", html);
}

#[test]
fn test_keyframes_percentage() {
    let result = htmlang::parser::parse("@keyframes slide\n  0% [transform translateX(-100%)]\n  100% [transform translateX(0)]");
    let html = htmlang::codegen::generate(&result.document);
    assert!(html.contains("0%{transform:translateX(-100%);}"), "keyframe percentage should work, got: {}", html);
}

#[test]
fn test_theme_directive() {
    let result = htmlang::parser::parse("@theme\n  primary #3b82f6\n  spacing-md 16\n\n@el [background $primary, padding $spacing-md] Content");
    let diags = &result.diagnostics;
    let errors: Vec<_> = diags.iter().filter(|d| d.severity == htmlang::parser::Severity::Error).collect();
    assert!(errors.is_empty(), "theme should not cause errors: {:?}", errors);
    let html = htmlang::codegen::generate(&result.document);
    assert!(html.contains("--primary:#3b82f6"), "theme should emit CSS vars, got: {}", html);
    assert!(html.contains("--spacing-md:16"), "theme should emit spacing var, got: {}", html);
    assert!(html.contains("background:#3b82f6"), "theme var should resolve in attrs, got: {}", html);
}

#[test]
fn test_deprecated_fn() {
    let result = htmlang::parser::parse("@deprecated Use @new-card instead\n@fn old-card $title\n  @text $title\n\n@old-card [title Hello]");
    let warnings: Vec<_> = result.diagnostics.iter()
        .filter(|d| d.message.contains("deprecated"))
        .collect();
    assert!(!warnings.is_empty(), "calling deprecated fn should warn, got: {:?}", result.diagnostics);
    assert!(warnings[0].message.contains("Use @new-card instead"), "deprecation message should be included");
}

#[test]
fn test_extends_directive() {
    // Can't test with actual files, but verify parse error for missing file
    let result = htmlang::parser::parse("@extends nonexistent.hl\n@slot content\n  Hello");
    let has_error = result.diagnostics.iter().any(|d|
        d.message.contains("cannot extend") && d.severity == htmlang::parser::Severity::Error
    );
    assert!(has_error, "@extends should report error for missing file, got: {:?}", result.diagnostics);
}

#[test]
fn test_color_filter_lighten() {
    let result = htmlang::parser::parse("@let primary #3b82f6\n@el [background $primary|lighten:20] Content");
    let html = htmlang::codegen::generate(&result.document);
    // Lighten #3b82f6 by 20% should produce a lighter blue
    assert!(html.contains("background:#"), "lighten filter should produce hex color, got: {}", html);
    // Verify it's not the original color
    assert!(!html.contains("background:#3b82f6"), "lighten should change the color");
}

#[test]
fn test_color_filter_darken() {
    let result = htmlang::parser::parse("@let primary #ffffff\n@el [background $primary|darken:50] Content");
    let html = htmlang::codegen::generate(&result.document);
    // Darken white by 50% should produce gray (#808080 approximately)
    assert!(html.contains("background:#"), "darken filter should produce hex color, got: {}", html);
    assert!(!html.contains("background:#ffffff"), "darken should change the color");
}

#[test]
fn test_color_filter_alpha() {
    let result = htmlang::parser::parse("@let primary #3b82f6\n@el [background $primary|alpha:0.5] Content");
    let html = htmlang::codegen::generate(&result.document);
    // Should produce 8-digit hex with alpha
    assert!(html.contains("background:#3b82f67f"), "alpha filter should add alpha channel, got: {}", html);
}

#[test]
fn test_color_filter_mix() {
    let result = htmlang::parser::parse("@let primary #000000\n@el [background $primary|mix:#ffffff:50] Content");
    let html = htmlang::codegen::generate(&result.document);
    // Mix black and white at 50% should produce gray
    assert!(html.contains("background:#808080") || html.contains("background:#7f7f7f"), "mix filter should blend colors, got: {}", html);
}

#[test]
fn test_autofocus_attribute() {
    let result = htmlang::parser::parse("@input [type text, autofocus]");
    let html = htmlang::codegen::generate(&result.document);
    assert!(html.contains("autofocus"), "autofocus should be in output, got: {}", html);
    // Should not produce unknown attribute warning
    let unknown_warnings: Vec<_> = result.diagnostics.iter()
        .filter(|d| d.message.contains("unknown attribute") && d.message.contains("autofocus"))
        .collect();
    assert!(unknown_warnings.is_empty(), "autofocus should not warn as unknown");
}

#[test]
fn test_repl_components_feed_subcommands_recognized() {
    // Just verify that the parser and codegen work for content that these commands would process
    let result = htmlang::parser::parse("@page Test Site\n@meta description A test\n@fn card $title\n  @text $title");
    assert!(!result.diagnostics.iter().any(|d| d.severity == htmlang::parser::Severity::Error));
}
