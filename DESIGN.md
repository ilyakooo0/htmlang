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

Bare lines (not starting with `@` or `[`) are text nodes.

## Syntax

### Basic structure

```
@element [attributes]
  children
```

Children are indented under their parent. Attributes are optional, comma-separated inside `[...]`.

### Full example

```
-- My Site
@page My Site
@let primary #3b82f6
@let gap 20
@define card [padding 20, background white, rounded 8, border 1 #e5e7eb]

@column [max-width 800, center-x, padding 40, spacing $gap]
  @row [spacing $gap]
    @column [width fill, spacing 10]
      @text [bold, size 32] Welcome
      @paragraph
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
    @text [color #888] © 2026

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

### `@include`

Inlines another `.hl` file.

```
@include header.hl
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

Defines a pure function (reusable component). Parameters are prefixed with `$`.

```
@fn card $title
  @el [padding 20, background white, rounded 8, border 1 #e5e7eb]
    @text [bold, size 18] $title
    @children
```

Call it like any element, passing parameters in `[...]`:

```
@card [title Hello World]
  This is the card body.
  @text [italic] With styled text.
```

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

## Attributes reference

### Layout (set on parent)

| Attribute              | Effect                           |
|------------------------|----------------------------------|
| `spacing N`            | Gap between children             |
| `padding N`            | Uniform padding                  |
| `padding-x N`         | Horizontal padding               |
| `padding-y N`         | Vertical padding                 |
| `padding T R B L`     | Per-side padding                 |

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
| `rounded N`            | Border radius                    |
| `bold`                 | Bold text                        |
| `italic`               | Italic text                      |
| `underline`            | Underlined text                  |
| `size N`               | Font size in px                  |
| `font NAME`            | Font family                      |

### Flow

| Attribute              | Effect                           |
|------------------------|----------------------------------|
| `wrap`                 | Enable flex-wrap (on `@row`)     |

### Identity

| Attribute              | Effect                           |
|------------------------|----------------------------------|
| `id NAME`              | HTML id attribute                |
| `class NAME`           | HTML class attribute             |

## Compilation target

Each `.hl` file compiles to a single self-contained `.html` file:

- Elements map to `<div>`, `<span>`, `<p>`, `<a>`, `<img>` as appropriate
- Layout uses flexbox (`display: flex`, `flex-direction`, `gap`, etc.)
- Styles are scoped via generated class names in an embedded `<style>` block
- No external CSS, no JavaScript (unless injected via `@raw`)
- `@page` generates the HTML boilerplate; without it, output is an HTML fragment
