# htmlang

A minimalist layout language inspired by [elm-ui](https://package.elm-lang.org/packages/mdgriffith/elm-ui/latest/) that compiles to static HTML.

`@` means structure. Bare lines mean content. No CSS required.

## Example

```
@page My Site
@let primary #3b82f6

@fn card $title
  @el [padding 20, background white, rounded 8, border 1 #e5e7eb, hover:border 1 $primary, transition all 0.15s ease]
    @text [bold] $title
    @children

@column [max-width 800, center-x, padding 40, spacing 20]
  @text [bold, size 32] Hello

  @paragraph
    Built with {@text [bold, color $primary] htmlang}.

  @row [wrap, spacing 10]
    @card [title Simple]
      Write layouts without CSS
    @card [title Fast]
      Compiles to a single HTML file
```

Compiles to a self-contained `.html` file with flexbox layout and generated CSS classes. No JavaScript, no external dependencies.

## Install

```
cargo install --path .
```

## Usage

```
htmlang init                 # scaffold a new project
htmlang page.hl              # writes page.html
htmlang --watch page.hl      # recompile on change
htmlang fmt page.hl          # format a file
```

## Editor support

A VS Code extension with syntax highlighting and LSP integration is available in [`editors/vscode`](editors/vscode). The language server (`htmlang-lsp`) provides diagnostics, completions, and hover documentation.

## Language overview

### Elements

| Element      | Purpose                      |
|--------------|------------------------------|
| `@row`       | Horizontal flex layout       |
| `@column`    | Vertical flex layout         |
| `@el`        | Generic container            |
| `@text`      | Styled inline text           |
| `@paragraph` | Flowing text with inline elements |
| `@image`     | Image                        |
| `@link`      | Anchor                       |
| `@raw`       | Verbatim HTML escape hatch   |
| `@form`      | Form container               |
| `@details`   | Disclosure widget            |
| `@summary`   | Summary for `@details`       |
| `@blockquote`| Block quotation              |
| `@cite`      | Citation reference           |
| `@code`      | Inline code (monospace)      |
| `@pre`       | Preformatted text            |
| `@hr`        | Horizontal rule / divider    |
| `@figure`    | Figure with caption          |
| `@figcaption`| Caption for `@figure`        |
| `@progress`  | Progress bar                 |
| `@meter`     | Meter/gauge                  |

### Layout attributes

```
@row [spacing 20]                  -- gap between children
@row [gap-x 10, gap-y 20]         -- separate horizontal/vertical gaps
@el [padding 20]                   -- uniform padding (also padding-x, padding-y)
@el [width fill]                   -- take remaining space (also width 200, width shrink)
@el [center-x]                     -- center horizontally (also align-left, align-right)
@el [overflow hidden]              -- overflow behavior (hidden, scroll, auto)
```

### Style attributes

```
@el [background #3b82f6, color white, rounded 8, border 1 #e5e7eb]
@text [bold, italic, underline, size 18, font Inter]
@el [opacity 0.5, cursor pointer, transition all 0.15s ease]
@el [shadow 0 2px 4px rgba(0,0,0,0.1)]
@paragraph [text-align center, line-height 1.5]
```

### Positioning

```
@el [position relative]
  @el [position absolute, z-index 10]
    Overlay content
```

### Pseudo-states and media prefixes

Prefix any style attribute with `hover:`, `active:`, or `focus:`:

```
@el [background #3b82f6, hover:background #2563eb, active:background #1d4ed8]
```

Use `dark:` for dark mode and `print:` for print styles:

```
@el [background white, dark:background #1a1a2e, print:display none]
```

### Variables and defines

```
@let primary #3b82f6              -- simple variable, used as $primary
@let greeting "Hello $name"       -- quoted string interpolation
@define card [padding 20, rounded 8]   -- attribute bundle, used as [$card]
```

### Functions

```
@fn button $label
  @el [padding 12, background $primary, rounded 8]
    @text [color white, bold] $label
    @children                      -- slot for caller's children

@button [label Click me]
```

### File includes

```
@include header.hl                -- inline another .hl file
```

### Multi-line attributes

Attribute lists can span multiple lines:

```
@el [
  padding 20,
  background white,
  rounded 8,
  shadow 0 2px 4px rgba(0,0,0,0.1)
]
  Content here
```

### Other features

- `--` comments
- `{@text [bold] inline}` elements in text
- `@el [attrs] > @link url` single-child chaining
- `[padding 20]` bare attributes as implicit `@el`
- `@raw """..."""` for embedding arbitrary HTML/CSS/JS
- `@image [inline] logo.svg` inlines SVG content
- `@each $name, $url in Home /, About /about` destructuring

## Documentation

See [DESIGN.md](DESIGN.md) for the full language specification.
