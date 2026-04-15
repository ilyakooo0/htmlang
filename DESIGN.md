# htmlang (.hl)

A minimalist layout language inspired by elm-ui that compiles to static HTML.

## Principles

- `@` means structure, bare lines mean content
- Layout is explicit and compositional — no CSS cascade
- Every element declares its own layout role
- Output is a self-contained HTML file with embedded CSS (flexbox)

## Elements

| Element      | Output           | Purpose                        |
|--------------|------------------|--------------------------------|
| `@row`       | flex row         | Horizontal layout              |
| `@column`    | flex column      | Vertical layout                |
| `@el`        | div              | Generic container              |
| `@text`      | span             | Styled inline text             |
| `@paragraph` | p with spans     | Flowing/wrapping inline text   |
| `@image`     | img              | Image                          |
| `@link`      | a                | Anchor wrapping children       |
| `@raw`       | verbatim HTML    | Escape hatch                   |
| `@nav`       | nav              | Navigation landmark            |
| `@header`    | header           | Page/section header            |
| `@footer`    | footer           | Page/section footer            |
| `@main`      | main             | Main content area              |
| `@section`   | section          | Thematic section               |
| `@article`   | article          | Self-contained content         |
| `@aside`     | aside            | Sidebar/tangential content     |
| `@list`      | ul/ol            | List (`[ordered]` for ol)      |
| `@item`      | li               | List item                      |
| `@table`     | table            | Table                          |
| `@thead`     | thead            | Table head group               |
| `@tbody`     | tbody            | Table body group               |
| `@tr`        | tr               | Table row                      |
| `@td`        | td               | Table cell                     |
| `@th`        | th               | Table header cell              |
| `@video`     | video            | Video element                  |
| `@audio`     | audio            | Audio element                  |
| `@form`      | form             | Form container                 |
| `@details`   | details          | Disclosure widget              |
| `@summary`   | summary          | Summary for `@details`         |
| `@blockquote`| blockquote       | Block quotation                |
| `@cite`      | cite             | Citation/source reference      |
| `@code`      | code             | Inline code (monospace)        |
| `@pre`       | pre              | Preformatted text              |
| `@hr`        | hr               | Horizontal rule (self-closing) |
| `@figure`    | figure           | Figure with optional caption   |
| `@figcaption`| figcaption       | Caption for `@figure`          |
| `@progress`  | progress         | Progress bar                   |
| `@meter`     | meter            | Meter/gauge element            |

Bare lines (not starting with `@` or `[`) are text nodes.

## Syntax

### Basic structure

```
@element [attributes]
  children
```

Children are indented under their parent. Attributes are optional, comma-separated inside `[...]`. Attribute lists can span multiple lines:

```
@el [
  padding 20,
  background white,
  rounded 8,
  shadow 0 2px 4px rgba(0,0,0,0.1)
]
  Content
```

### Full example

```
-- My Site
@page My Site
@let primary #3b82f6
@let gap 20
@define card [
  padding 20,
  background white,
  rounded 8,
  border 1 #e5e7eb,
  shadow 0 2px 4px rgba(0,0,0,0.05)
]

@include header.hl

@column [max-width 800, center-x, padding 40, spacing $gap]
  @row [spacing $gap]
    @column [width fill, spacing 10]
      @text [bold, size 32] Welcome
      @paragraph [line-height 1.6]
        This is a page built with {@text [bold, color $primary] htmlang}.
        Read the {@link https://docs.example.com docs} to learn more.
    @image [width 80, height 80, rounded 40] avatar.png

  -- cards
  @row [wrap, spacing 10]
    [$card]
      First card
    [$card, background #f9fafb]
      Second card

  -- footer
  @row [spacing 10]
    @el [padding 16, background $primary, rounded 8] > @link https://example.com
      @text [color white] Get Started
    @el [width fill]
    @text [color #888, text-align right] © 2026

  @raw """
  <canvas id="chart"></canvas>
  """
```

## Directives

### `@page`

Sets the HTML `<title>` and generates boilerplate (`<!DOCTYPE>`, `<html>`, `<head>`, `<body>`).

```
@page My Site
```

### `@let`

Defines a variable. Referenced with `$name`.

```
@let primary #3b82f6
@let gap 20

@el [background $primary, spacing $gap]
```

Quoted values support string interpolation:

```
@let name World
@let greeting "Hello $name"   -- "Hello World"
@let base /api
@let url "$base/users"        -- "/api/users"
```

### `@include`

Inlines another `.hl` file. The path is resolved relative to the current file. Variables defined with `@let`, attribute bundles from `@define`, and functions from `@fn` in the included file are available after the `@include` line.

```
@include header.hl
@include components/card.hl
```

Variables can be used in the filename:

```
@let component card
@include $component.hl
```

Circular includes are detected and reported as errors. Nested includes are supported.

### `@import`

Like `@include`, but only imports definitions (`@let`, `@define`, `@fn`) without emitting DOM nodes. Use this for shared theme/component libraries.

```
@import theme.hl     -- imports variables, defines, functions only
@include header.hl   -- inlines everything including DOM nodes
```

### `@meta`

Adds a `<meta>` tag to the document `<head>`. Requires `@page`.

```
@meta description A portfolio site
@meta og:image https://example.com/preview.png
```

### `@head`

Adds raw content to the document `<head>`. Use for external fonts, favicons, or custom CSS/JS.

```
@head
  <link rel="icon" href="favicon.ico">
  <link rel="stylesheet" href="https://fonts.googleapis.com/css2?family=Inter">
```

### `@define`

Creates a named attribute bundle. Referenced with `$name` inside attribute lists.

```
@define card [padding 20, background white, rounded 8, border 1 #e5e7eb]

@el [$card]
  Content
@el [$card, background #f9fafb]
  Overridden background
```

Attributes listed after `$name` override those in the definition.

### `@fn`

Defines a pure function (reusable component). Parameters are prefixed with `$`. Default values can be specified with `=`.

```
@fn card $title $variant=primary
  @el [padding 20, background white, rounded 8, border 1 #e5e7eb]
    @text [bold, size 18] $title
    @children
```

Call it like any element, passing parameters in `[...]`:

```
@card [title Hello World]
  This is the card body.
  @text [italic] With styled text.

@card [title Warning, variant danger]
  Overridden variant.
```

Parameters with defaults can be omitted in calls — the default value is used.

### `@children`

A slot inside a function body that expands to the caller's indented children.

```
@fn layout $title
  @column [max-width 800, center-x, padding 40]
    @text [bold, size 32] $title
    @children

@layout [title My Page]
  @row [spacing 10]
    Content goes here
```

Functions can call other functions. `@let` and `@define` variables are available inside function bodies. Function parameters shadow `@let` variables of the same name within the body.

## Text

### Bare text

Any line not starting with `@` or `[` is a text node.

```
@column
  Hello world
  This is just text
```

### Styled text

Use `@text` when you need attributes.

```
@text [bold, size 24, color #333] Hello world
```

### Inline elements

Use `{...}` to embed elements inside text lines. Essential for mixed-style paragraphs.

```
@paragraph
  This is {@text [bold] important} and this is a {@link https://example.com link}.
```

## Comments

`--` starts a comment. The rest of the line is ignored.

```
-- this is a comment
@row [spacing 10]
  -- todo: add nav items
```

## Implicit `@el`

A line starting with `[` (attributes) that has indented children is an anonymous container (implicit `@el`).

```
[padding 20, background white]
  Hello

-- equivalent to:
@el [padding 20, background white]
  Hello
```

## Single-child chaining

Use `>` to chain single-child elements on one line, reducing nesting depth.

```
@el [padding 16, background blue, rounded 8] > @link https://example.com
  @text [color white] Get Started

-- equivalent to:
@el [padding 16, background blue, rounded 8]
  @link https://example.com
    @text [color white] Get Started
```

The last element in the chain takes the indented children.

## `@raw`

Triple-quoted block pasted verbatim into output. Use for arbitrary HTML, CSS, or JS.

```
@raw """
<style>
  @keyframes spin { to { transform: rotate(360deg); } }
</style>
<div class="custom-widget"></div>
"""
```

## Units

All numeric values default to pixels. You can use CSS units explicitly:

```
@el [width 50%, height 100vh, padding 2rem, max-width 80ch, size 1.2em]
```

Supported units: `%`, `rem`, `em`, `vh`, `vw`, `vmin`, `vmax`, `dvh`, `svh`, `ch`, `ex`, `cm`, `mm`, `in`, `pt`, `pc`, `fr`.

## New elements

### `@form`

Form container. Argument is the `action` URL.

```
@form [method post] /submit
  @label [for email] Email
  @input [type email, name email, required]
  @button [type submit] Send
```

### `@details` / `@summary`

Native disclosure widget. Use `[open]` to expand by default.

```
@details [open]
  @summary FAQ Question
  @text The answer is here.
```

### `@blockquote` / `@cite`

Semantic quotation with optional citation.

```
@blockquote [padding 20, border-left 4 #ccc]
  @text To be or not to be
  @cite Shakespeare
```

### `@code` / `@pre`

Code and preformatted text. `@code` renders as `<code>` with monospace font. `@pre` preserves whitespace.

```
@pre
  @code console.log("hello")
```

### `@hr`

Horizontal rule / divider (self-closing). Alias: `@divider`.

```
@hr [border-top 1 #e5e7eb]
```

### `@figure` / `@figcaption`

Figure with optional caption.

```
@figure
  @image [alt Sunset, width fill] sunset.jpg
  @figcaption A beautiful sunset
```

### `@progress` / `@meter`

Progress bar and meter elements. Use `value`, `max`, `min` attributes.

```
@progress [value 70, max 100]
@meter [value 6, min 0, max 10, low 3, high 8]
```

## `@each` destructuring

When `@each` has more than one variable and items contain spaces, values are destructured:

```
@each $name, $url in Home /, About /about, Contact /contact
  @link $url $name
```

`var()` and `calc()` expressions are also passed through as-is.

## Attributes reference

### Layout (set on parent)

| Attribute              | Effect                           |
|------------------------|----------------------------------|
| `spacing N`            | Gap between children             |
| `gap N`               | Alias for `spacing`              |
| `gap-x N`             | Horizontal gap (column-gap)      |
| `gap-y N`             | Vertical gap (row-gap)           |
| `padding N`            | Uniform padding                  |
| `padding Y X`         | Vertical + horizontal padding    |
| `padding T H B`       | Top + horizontal + bottom        |
| `padding T R B L`     | Per-side padding                 |
| `padding-x N`         | Horizontal padding               |
| `padding-y N`         | Vertical padding                 |

### Sizing (set on child)

| Attribute              | Effect                           |
|------------------------|----------------------------------|
| `width fill`           | Take remaining space (flex: 1)   |
| `width N`              | Exact pixels                     |
| `width shrink`         | Fit to content (default)         |
| `height fill`          | Take remaining space             |
| `height N`             | Exact pixels                     |
| `height shrink`        | Fit to content (default)         |
| `min-width N`          | Minimum width                    |
| `max-width N`          | Maximum width                    |
| `min-height N`         | Minimum height                   |
| `max-height N`         | Maximum height                   |

### Alignment (set on child)

| Attribute              | Effect                           |
|------------------------|----------------------------------|
| `center-x`            | Center horizontally              |
| `center-y`            | Center vertically                |
| `align-left`          | Align to left                    |
| `align-right`         | Align to right                   |
| `align-top`           | Align to top                     |
| `align-bottom`        | Align to bottom                  |

### Style

| Attribute              | Effect                           |
|------------------------|----------------------------------|
| `background COLOR`     | Background color                 |
| `color COLOR`          | Text color                       |
| `border N COLOR`       | Border width and color           |
| `border-top N COLOR`   | Top border                       |
| `border-bottom N COLOR`| Bottom border                    |
| `border-left N COLOR`  | Left border                      |
| `border-right N COLOR` | Right border                     |
| `rounded N`            | Border radius                    |
| `shadow VALUE`         | Box shadow (CSS value)           |
| `bold`                 | Bold text                        |
| `italic`               | Italic text                      |
| `underline`            | Underlined text                  |
| `size N`               | Font size in px                  |
| `font NAME`            | Font family                      |
| `text-align VALUE`     | Text alignment (left/center/right/justify) |
| `line-height VALUE`    | Line height (unitless or px)     |
| `letter-spacing N`     | Letter spacing                   |
| `text-transform VALUE` | Transform (uppercase/lowercase/capitalize) |
| `white-space VALUE`    | White-space (nowrap/pre/normal)  |
| `transition VALUE`     | CSS transition                   |
| `cursor VALUE`         | Cursor style                     |
| `opacity VALUE`        | Opacity (0–1)                    |
| `overflow VALUE`       | Overflow (hidden/scroll/auto/visible) |
| `position VALUE`       | Position (relative/absolute/fixed/sticky) |
| `top N`                | Top offset (positioned elements) |
| `right N`              | Right offset                     |
| `bottom N`             | Bottom offset                    |
| `left N`               | Left offset                      |
| `z-index N`            | Stack order                      |
| `display VALUE`        | Display mode (none/block/flex/grid) |
| `visibility VALUE`     | Visibility (visible/hidden)      |
| `transform VALUE`      | CSS transform (e.g., rotate(45deg)) |
| `backdrop-filter VALUE`| Backdrop filter (e.g., blur(10px)) |

### Margin

| Attribute              | Effect                           |
|------------------------|----------------------------------|
| `margin N`             | Uniform margin                   |
| `margin Y X`          | Vertical + horizontal margin     |
| `margin T R B L`      | Per-side margin                  |
| `margin-x N`          | Horizontal margin                |
| `margin-y N`          | Vertical margin                  |

### Additional Style

| Attribute              | Effect                           |
|------------------------|----------------------------------|
| `filter VALUE`         | CSS filter (blur, brightness)    |
| `object-fit VALUE`     | Object fit (cover/contain/fill)  |
| `object-position VALUE`| Object position within container|
| `text-shadow VALUE`    | Text shadow                      |
| `text-overflow VALUE`  | Text overflow (ellipsis/clip)    |
| `pointer-events VALUE` | Pointer events (none/auto)      |
| `user-select VALUE`    | User selection (none/text/all)  |
| `justify-content VALUE`| Main axis alignment             |
| `align-items VALUE`    | Cross axis alignment            |
| `order N`              | Flex/grid item order            |
| `background-size VALUE`| Background size                 |
| `background-position VALUE`| Background position         |
| `background-repeat VALUE`| Background repeat             |
| `word-break VALUE`     | Word break (break-all/keep-all) |
| `overflow-wrap VALUE`  | Overflow wrap (break-word)      |

### Pseudo-states

Prefix any style attribute with `hover:`, `active:`, or `focus:` to apply it on that state.

```
@el [padding 16, background #3b82f6, hover:background #2563eb, active:background #1d4ed8, rounded 8, transition all 0.15s ease]
  @text [color white] Click me
```

All style attributes support state prefixes: `hover:color`, `active:rounded`, `focus:border`, etc.

### Dark mode

Prefix any style attribute with `dark:` to apply it when the user's system is in dark mode.

```
@el [background white, dark:background #1a1a2e, color #333, dark:color #eee]
  @text Theme-aware content
```

This generates a `@media (prefers-color-scheme: dark)` rule.

### Print styles

Prefix any style attribute with `print:` to apply it when printing.

```
@nav [print:display none]
  @text Navigation (hidden in print)
```

This generates a `@media print` rule.

### Flow & Grid

| Attribute              | Effect                           |
|------------------------|----------------------------------|
| `wrap`                 | Enable flex-wrap (on `@row`)     |
| `grid`                 | Enable CSS grid layout           |
| `grid-cols N`          | Grid template columns (N equal)  |
| `grid-rows N`          | Grid template rows (N equal)     |
| `col-span N`           | Span N columns in grid           |
| `row-span N`           | Span N rows in grid              |

### Identity

| Attribute              | Effect                           |
|------------------------|----------------------------------|
| `id NAME`              | HTML id attribute                |
| `class NAME`           | HTML class attribute             |

## Asset inlining

SVG images can be inlined directly into the HTML output using the `[inline]` attribute:

```
@image [inline, width 24, height 24] icon.svg
```

This reads the SVG file and embeds its content directly, keeping the output self-contained.

## CLI

```
htmlang init                 # scaffold a new project
htmlang init my-site         # scaffold in a new directory
htmlang page.hl              # compile page.hl → page.html
htmlang site/                # compile all .hl files in directory
htmlang --watch page.hl      # compile and watch for changes
htmlang -w page.hl           # short form
htmlang --serve site/        # serve a multi-page site with hot reload
```

Watch mode recompiles automatically when the source file or any `@include`d/`@import`ed files change.

## Compilation target

Each `.hl` file compiles to a single self-contained `.html` file:

- Elements map to `<div>`, `<span>`, `<p>`, `<a>`, `<img>` as appropriate
- Layout uses flexbox (`display: flex`, `flex-direction`, `gap`, etc.)
- Styles are scoped via generated class names in an embedded `<style>` block
- No external CSS, no JavaScript (unless injected via `@raw`)
- `@page` generates the HTML boilerplate; without it, output is an HTML fragment

## Editor support

A VS Code extension is available in `editors/vscode/` with:

- Syntax highlighting via TextMate grammar
- LSP integration via `htmlang-lsp` for diagnostics, completions, and hover documentation

The language server reports parse errors and warnings for unknown attributes as you type.
