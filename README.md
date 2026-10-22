# inkmd

A fast, minimal Markdown renderer for Linux and macOS terminals, written in Rust.

```bash
inkmd README.md
```

The document is rendered right in the terminal you are using, and the command
exits. There is no browser, server, pager or interactive UI. `inkmd` does one
thing: it turns Markdown into well-composed terminal text. When the terminal
supports modern features (true color, inline images, scaled headings), it uses
them. When it does not, the output degrades gracefully.

> **Ground rule:** the document always stays readable, whatever the terminal
> can do.

## Highlights

- **Fast**: about 0.6 ms end to end for a small document. Parsing and layout
  take a single pass with no intermediate tree.
- **Broad Markdown coverage**: CommonMark plus GFM tables, task lists,
  strikethrough, footnotes, alerts, autolinks, front matter, definition lists,
  math, wikilinks and safe embedded HTML.
- **Syntax highlighting** for ~220 languages, colored with your terminal's own
  palette.
- **Inline images** through the Kitty graphics protocol or the iTerm2 protocol
  (iTerm2, WezTerm), with a text fallback everywhere else.
- **Scaled headings** on terminals that implement Kitty's text sizing protocol
  (Kitty ≥ 0.40): H1 is shown at 2×, then 1.75×, 1.5× and 1.25×.
- **Graceful degradation**: true color, 256 or 16 colors, attributes only, or
  plain text. Unicode or ASCII symbols. The layout follows the terminal width.
- **Safe by design**: escape sequences from the document cannot reach your
  terminal, nothing is executed and nothing is downloaded.

## Installation

### From source

You need Rust 1.88 or newer and a C compiler. The C compiler builds
Oniguruma, the regex engine used for syntax highlighting. On Linux that is
`cc`/`gcc`; on macOS, the Xcode Command Line Tools (`xcode-select --install`).

```bash
cargo build --release
mkdir -p ~/.local/bin
cp target/release/inkmd ~/.local/bin/
```

Alternatively, `cargo install --path .` installs it into `~/.cargo/bin`.

`~/.local/bin` must be in your `PATH`. Many Linux distributions add it
automatically at login when the directory exists; macOS does not. If yours
does not, add this line to `~/.zshrc` (the default shell on macOS) or
`~/.bashrc` and open a new terminal:

```bash
export PATH="$HOME/.local/bin:$PATH"
```

Check the result with `command -v inkmd`.

### Prebuilt binaries

The [releases page](https://github.com/alejandrofonsecacuza/Inkmd/releases)
provides binaries with no runtime dependencies:

| Platform | Archive |
|---|---|
| Linux x86_64 (static, musl) | `inkmd-<version>-x86_64-unknown-linux-musl.tar.gz` |
| Linux ARM64 (static, musl) | `inkmd-<version>-aarch64-unknown-linux-musl.tar.gz` |
| macOS Apple Silicon | `inkmd-<version>-aarch64-apple-darwin.tar.gz` |
| macOS Intel | `inkmd-<version>-x86_64-apple-darwin.tar.gz` |

```bash
tar xzf inkmd-*.tar.gz
mkdir -p ~/.local/bin
cp inkmd-*/inkmd ~/.local/bin/
```

The macOS binaries are not signed by Apple. If the archive was downloaded
with a browser and macOS refuses to run it, remove the quarantine attribute:
`xattr -d com.apple.quarantine ~/.local/bin/inkmd`. Archives downloaded with
`curl` are not affected.

### Build options

The `release` profile already uses full LTO, a single codegen unit,
`panic = "abort"` and stripped symbols. The resulting binary is about 3.4 MB,
most of it syntax grammars. On Linux it only depends on the system glibc; on
macOS, on the system libraries.

```bash
# Optimize for the local CPU (the binary will not run on older CPUs):
RUSTFLAGS="-C target-cpu=native" cargo build --release

# Smaller binary (~0.9 MB): no syntax highlighting, no JPEG/GIF/WebP
# decoding, and no C compiler needed:
cargo build --release --no-default-features

# Fully static Linux binary:
rustup target add x86_64-unknown-linux-musl
cargo build --release --target x86_64-unknown-linux-musl
```

| Feature        | Default | Provides                                               |
|----------------|:-------:|--------------------------------------------------------|
| `highlight`    | yes     | Syntax highlighting (syntect with bat's grammar set)  |
| `image-decode` | yes     | JPEG, GIF and WebP for Kitty (PNG always works)        |

## Usage

```bash
inkmd README.md                     # render a file
inkmd README.md CONTRIBUTING.md     # several files, each with a header
cat README.md | inkmd               # read standard input
some-command | inkmd                # Markdown produced by another program
inkmd - < notes.md                  # '-' means standard input
inkmd --no-color README.md          # no colors
inkmd --width 100 README.md         # fixed width
inkmd --images never README.md      # never draw images
inkmd --color always README.md | less -R   # page it, keeping the styling
```

```
Usage: inkmd [OPTIONS] [FILE]...

Options:
  -w, --width <N>      Wrap at N columns [default: terminal width, at most 100]
      --color <WHEN>   Use colors and styles: auto, always, never [default: auto]
      --no-color       Same as --color never
      --images <MODE>  Inline images: auto, never, kitty, iterm2 [default: auto]
      --big-headings <WHEN>
                       Scale headings by level (Kitty text sizing):
                       auto, always, never [default: auto]
      --theme <NAME>   ansi (terminal palette), dark, light [default: ansi]
      --ascii          Use only ASCII symbols for markers and borders
  -h, --help           Print help
  -V, --version        Print version
```

Behavior:

- With no file arguments, `inkmd` reads standard input. If standard input is
  a terminal, it prints a short usage error instead of waiting for input.
- When stdout **is not a terminal** (a pipe or a file), no escape sequences
  are written by default. The output is plain, well-wrapped text that works
  with `grep`, `less` or a file. Use `--color always` (or `CLICOLOR_FORCE=1`)
  to force styling.
- The default width is the terminal width, capped at 100 columns for
  readability. An explicit `--width` (10–10000) is not capped. When stdout is
  not a terminal, `$COLUMNS` is used, or 80 if it is not set.
- Several files are rendered in order, each with a header. If one fails
  (missing, unreadable...), the error goes to stderr and the others still
  render.

Environment variables:

| Variable         | Effect                                                        |
|------------------|---------------------------------------------------------------|
| `NO_COLOR`       | Disables colors; bold and italic remain on a terminal          |
| `CLICOLOR_FORCE` | Forces styling even when stdout is not a terminal              |
| `COLORTERM`      | `truecolor`/`24bit` enables 24-bit color                       |
| `TERM`           | `dumb` disables styling; `*256color*` enables 256 colors       |
| `LANG`/`LC_*`    | A non-UTF-8 locale switches to ASCII symbols                   |
| `INKMD_THEME`   | Default theme (`ansi`, `dark`, `light`)                        |
| `INKMD_DEBUG`   | `1` prints capability detection and image decisions to stderr  |

Exit codes: `0` on success, `1` if some file could not be read, `2` for usage
errors. If the reading end of a pipe closes (`inkmd x.md | head`), the process
exits quietly, like any Unix filter.

## Markdown coverage

`inkmd` uses [pulldown-cmark] (CommonMark with GFM extensions). Each element
is rendered as follows:

| Element | Rendering | Without styling (pipe) |
|---|---|---|
| Headings 1–6 (ATX and Setext) | Bold, one color per level. H1 is underlined with `━`, H5 is italic and H6 is dimmed. Scaled 2×/1.75×/1.5×/1.25× on terminals with text sizing support. | `# Title` (markers kept) |
| Paragraphs, soft and hard breaks | Word wrap to the width; `\` and two trailing spaces force a break | |
| Italic, bold, combined | SGR 3 / 1 | plain text |
| Strikethrough (`~~x~~`, `~x~`) | SGR 9 | `~~x~~` (kept so the meaning is not lost) |
| Inline code | Own color | `` `x` `` (also with `NO_COLOR`) |
| Fenced (``` and ~~~) and indented code | 4-column indent, dimmed language label, highlighting | `[language]` label |
| Unknown language | Monospace text without colors | |
| Long code lines | Split with a `↪` marker; nothing is truncated | |
| Bullet lists | `•` `◦` `▪` per level, with a hanging indent | `*` `-` `+` with `--ascii` |
| Ordered lists | The start number is respected | |
| Tight and loose lists | Without / with blank lines between items | |
| Task lists | `☐` / `☑` (green) | `[ ]` / `[x]` with `--ascii` |
| Block quotes, nested | One `│` bar per level | `\|` with `--ascii` |
| GFM alerts (`> [!NOTE]`…) | Bar and title in the alert color | title as text |
| Thematic breaks | `─` line across the available width | |
| Inline, reference, collapsed and shortcut links | Underlined text followed by the dimmed URL in parentheses | `text (url)` |
| Autolinks `<…>`, e-mail, bare URLs (`https://`, `www.`) | Link style; the URL is not repeated | |
| Anchor links (`#section`) | Text only; the target is useless in a terminal | |
| Wikilinks `[[Page]]` | Rendered as links | |
| GFM tables | Aligned columns (left/center/right), bold header, `─┼─` separator | |
| Wide tables | Cells wrap inside their columns | |
| Tables that cannot fit even wrapped | One record per row: `header: value` | |
| Footnotes | `[n]` reference; the definition has a hanging indent | |
| Definition lists | Bold term, indented definitions | |
| YAML (`---`) / TOML (`+++`) front matter | Code block labeled `front matter` | |
| Images | Inline image (Kitty/iTerm2) or `[image: alt] (path)` | `[image: alt] (path)` |
| Images inside links (badges) | `[image: alt] (link target)` | |
| Unicode, CJK, emoji, combining marks | Real display width per grapheme; graphemes are never split | |
| Escapes and entities | Decoded (by the parser, and by inkmd inside raw HTML) | |
| Math `$…$`, `$$…$$`, ```` ```math ```` | Approximate TeX→Unicode: `\alpha^2` → `α²`, `\frac{a}{b}` → `a/b` | |
| Diagrams (```` ```mermaid ````, plantuml, dot) | Source shown as a code block labeled as a diagram | |
| Embedded HTML | Safe subset, see below | |

### Embedded HTML

HTML is **never executed or rendered as a document**. A small tokenizer
handles the tags that are common in READMEs:

- `<br>` becomes a line break and `<hr>` a rule. Block tags (`<p>`, `<div>`,
  `<h1>`…, `<li>`, `<tr>`…) start new lines.
- `<b>`, `<strong>`, `<i>`, `<em>`, `<u>`, `<s>`, `<del>`, `<code>`, `<kbd>`,
  `<mark>` and `<h1>`…`<h6>` map to the equivalent styles.
- `<img src alt>` is handled like a Markdown image, and `<a href>` like a link.
- `<details>`/`<summary>` become `▸ Summary` followed by the content, which
  is always shown.
- `<td>`/`<th>` cells are separated by `│` (no table layout).
- Comments and the contents of `<script>`, `<style>`, `<template>` and
  `<textarea>` are hidden.
- Any other tag is dropped and its text is kept.

## Terminal support and graceful degradation

Cheap checks run once at startup: `isatty`, `ioctl(TIOCGWINSZ)` and the
standard environment variables. The terminal itself is queried only when the
document needs it, that is, when it contains a heading or a local image.

### Colors and attributes

| Situation | Result |
|---|---|
| Output to a pipe or file (`--color auto`) | No escape sequences |
| `TERM=dumb` | No escape sequences |
| `NO_COLOR` or `--color never` on a terminal | No colors; bold, italic and underline remain |
| Regular terminal | 16 ANSI colors |
| `TERM=*256color*`, kitty, alacritty, ghostty | 256 colors |
| `COLORTERM=truecolor`/`24bit`, `TERM=*-direct` | 24-bit color |

The default `ansi` theme uses **only the 16 colors of the terminal palette**.
It therefore follows your color scheme, light or dark, at any color depth. The
`dark` and `light` themes define their own RGB colors, which are converted to
256 or 16 colors when the terminal supports fewer.

Color is never the only carrier of information. Headings keep their `#`
markers when unstyled, inline code gets backticks when there is no color,
strikethrough is kept as `~~` without styling, task states use different
symbols, and so on.

Color depth is read from `COLORTERM`/`TERM`. That is the de facto convention:
there is no reliable, universal query for it.

### Terminals

What each feature needs, and some terminals known to provide it:

| Feature | Requirement | Examples |
|---|---|---|
| Styled text, 16/256 colors | ANSI escape codes | Virtually every terminal, including macOS Terminal.app and the Linux console |
| 24-bit color | `COLORTERM=truecolor` | Kitty, WezTerm, Ghostty, iTerm2, Alacritty, GNOME Terminal, Konsole |
| Inline images | Kitty graphics or iTerm2 protocol | Kitty, Ghostty, WezTerm, iTerm2 |
| Scaled headings | Kitty text sizing protocol | Kitty ≥ 0.40 |

Everything is detected at run time. A missing feature falls back to text and
never affects readability. macOS Terminal.app, for example, gets 256 colors
and text placeholders for images.

### Unicode and ASCII

With a UTF-8 locale, discreet Unicode symbols are used (`•`, `│`, `─`, `☐`).
With a non-UTF-8 locale (e.g. `LANG=C`) or `--ascii`, ASCII equivalents are
used instead (`*`, `|`, `-`, `[ ]`). The document text itself is never changed.

### Width

- Text is wrapped at word boundaries. Each grapheme is measured with
  `unicode-width`: wide characters (CJK, emoji) count as 2 columns and
  combining marks as 0.
- Lines may break between CJK characters, which do not use spaces. They never
  break inside a grapheme cluster (ZWJ emoji, letters with combining marks).
- Words longer than a whole line are split at grapheme boundaries.
- The indentation of lists, quotes, footnotes and code is kept on every line.
- Tables adapt in three steps: natural width, then shrunk columns with
  wrapped cells, then a record layout.
- A test checks that no line of any fixture exceeds the requested width
  (20, 30, 45, 60 and 100 columns).

### Inline images

| Protocol | Detection | Formats |
|---|---|---|
| Kitty graphics | `OK` reply to an `a=q` query | PNG sent as is; JPEG/GIF/WebP decoded to RGBA (`image-decode` feature) |
| iTerm2 inline images (iTerm2, WezTerm) | `XTVERSION` reply | PNG, JPEG, GIF (the file as is) |
| None | — | `[image: alt] (path)` |

Detection writes a single query to `/dev/tty`. It contains a Kitty graphics
query, the text sizing probe, `XTVERSION`, a cell size request (`CSI 16 t`)
and **Device Attributes** (`CSI c`). Every terminal answers DA1, so its reply
marks the end of all the others and no fixed delay is needed (about 2 ms in
practice). A 300 ms timeout protects against terminals and multiplexers that
never answer. Nothing is decided from the terminal's name. When detection is
not possible (for example tmux with passthrough), `--images kitty|iterm2`
forces a protocol.

Policy:

- Only **local files** are displayed. Relative paths are resolved against the
  Markdown file, or the current directory for standard input.
- Remote resources (`http://`, `https://`, `//`, `data:`) are **never**
  downloaded. The alt text and the URL are shown instead.
- Files are identified by content (magic bytes), not by extension. SVG and
  unknown formats are shown as text.
- Files are limited to 32 MB and must be regular files (no devices,
  directories or FIFOs).
- Any failure (missing, unreadable, unsupported) prints the alt text with a
  note, such as `(logo.png: not found)`, and rendering continues.
- No temporary files are created. Image data is sent inline in base64.
- The size comes from the pixel dimensions and the real cell size. An image
  never exceeds the available width or about 2/3 of the terminal height.
- Inside tables, images always use the text form.

To diagnose, run `INKMD_DEBUG=1 inkmd doc.md`. It reports what the terminal
answered and, for each image, whether it was drawn (and at what size) or why
text was used. `tests/fixtures/showcase.md` includes a local PNG to try it.

### Scaled headings

On terminals that implement Kitty's *text sizing protocol* (Kitty 0.40 or
later), heading size decreases with the level:

| Level | Size | Sequence |
|---|---:|---|
| H1 | 2× | `OSC 66 ; s=2` |
| H2 | 1.75× | `s=2:n=7:d=8` |
| H3 | 1.5× | `s=2:n=3:d=4` |
| H4 | 1.25× | `s=2:n=5:d=8` |
| H5, H6 | normal | — |

H1–H4 take two rows and are centered vertically. With a fractional scale, all
the text in one escape code shares a block `2w` cells wide. The text is
therefore sent in chunks of 8 columns, which at 1.75×, 1.5× and 1.25× fill
exactly 14, 12 and 10 cells, so no gaps appear between chunks. Each heading
line wraps at the available width divided by its scale, and quote or list
prefixes are repeated on both rows.

Detection follows the protocol's own method. The capability query asks for the
cursor position (`CSI 6n`) before and after printing a space with `w=2` and
with `s=2`. The feature is enabled only if the cursor moves two cells each
time, and the probe space is erased immediately. Scaled headings are never
used for redirected or unstyled output, unless you pass
`--big-headings always`. `--big-headings never` turns them off.

### Syntax highlighting

The grammars (syntect with bat's extended set via two-face: about 220
languages, including Rust, Python, TypeScript, TOML, Dockerfile and YAML) are
loaded only the first time a code block with a language tag appears.

inkmd does not use a syntect color theme. Scopes are grouped into 9 semantic
categories (comment, keyword, string, number, function, type, inserted,
deleted, plain), and inkmd's theme colors them. Code therefore matches the
rest of the document and works with 16 colors.

A block is shown without colors when the language is unknown, highlighting
fails, the block is huge (over 256 KB, or lines over 4 KB), or the document
has already used its budget of 256 KB of highlighted code.

## Security

Documents are treated as untrusted input.

- **Control sequences**: all text that comes from the document, and file names
  in error messages, goes through a sanitizer. It replaces C0 controls, DEL and
  C1 controls with visible symbols (`␛`, `␇`, `�`, or `^[`, `^G`, `?` in ASCII
  mode). The only escape sequences in the output are the ones the renderer
  builds itself: SGR and, when enabled, images and scaled text. A document
  therefore cannot move the cursor, change the window title, redefine the
  palette, write to the clipboard (OSC 52) or create hidden hyperlinks. This
  covers inline code, code blocks, table cells, URLs, alt text, HTML and
  numeric entities such as `&#27;`. An integration test checks that every
  `ESC` in the output starts one of inkmd's own SGR sequences.
- **No execution**: no commands, scripts or HTML are run, and no external
  process is started.
- **No network**: nothing is downloaded.
- **Limits**: inputs over 256 MB are rejected, and reads go through `take()`
  so special files are bounded too. Images have their own limit. Deep nesting
  does not cause recursion in the renderer (it uses an explicit stack), and
  the content width never drops below 6 columns.
- **Errors**: missing files, directories, permission errors, invalid UTF-8
  (replaced with `�`), BOMs and CRLF line endings are handled without panics.
- **Terminal state**: the non-canonical mode used for the capability query is
  always restored before continuing.

See [SECURITY.md](SECURITY.md) to report a vulnerability.

## Architecture

```
src/
├── main.rs       CLI: arguments, input reading, initial detection, errors
├── lib.rs        Public API (Session, Options, render)
├── render.rs     pulldown-cmark events → blocks, containers, tables, code
├── wrap.rs       Styled lines, Unicode measurement and word wrapping
├── style.rs      Abstract styles, themes, color conversion and SGR
├── term.rs       Capabilities: tty, size, color, locale, terminal query
├── sanitize.rs   Neutralization of control characters
├── highlight.rs  Syntax highlighting (syntect), loaded on demand
├── image.rs      Loading, validation, Kitty/iTerm2 encoding
├── html.rs       Minimal HTML tokenizer and entities
└── math.rs       Approximate TeX → Unicode
```

Pipeline:

1. `pulldown-cmark` produces a stream of events.
2. `render` keeps a stack of containers (quotes, list items, footnotes...)
   that contribute a prefix to every line, and collects the inline content of
   each block.
3. `wrap` fits that content into the width left by the prefixes.
4. `style::Painter` turns abstract styles into SGR sequences for the detected
   color depth.
5. Output is buffered and written in 64 KB chunks.

Parsing, layout, terminal capabilities and output are kept separate, without
extra layers.

### Dependencies

| Crate | Why |
|---|---|
| `pulldown-cmark` | Mature and very fast CommonMark/GFM parser (over 300 MB/s). Event-based, so inkmd needs no tree of its own. Supports tables, tasks, footnotes, alerts, math, front matter, definition lists and wikilinks. |
| `unicode-width` | Display width according to the Unicode standard (UAX #11). |
| `unicode-segmentation` | Grapheme clusters, so a combined character is never split. |
| `lexopt` | Minimal argument parser: no macros, no startup cost. |
| `libc` | `isatty`, `ioctl`, `termios`, `select`, `SIGPIPE`: just what is needed to talk to the terminal. |
| `syntect` + `two-face` (optional) | Highlighting with Sublime/bat grammars: good quality and wide coverage. Oniguruma is used because it is faster than the pure-Rust engine. |
| `image` (optional) | Only the JPEG/GIF/WebP/PNG decoders, for Kitty. |
| `criterion` (dev) | Statistical benchmarks. |

Deliberately not used:

- `crossterm`: there is no interactive mode or event loop, and `libc` covers
  what is needed with less code.
- TUI frameworks: unnecessary for a non-interactive viewer.
- `clap`: more compile time and binary size for a handful of options.
- `viuer`/`ratatui-image`: heavy dependencies, while both image protocols fit
  in about 150 lines.
- A base64 crate: it is 15 lines.

### Design decisions

- **One pass over the events**, without building an AST.
- **Per-container prefixes** instead of recursion, so 200 nested quotes or 100
  nested lists cost no stack.
- **Abstract styles, converted to SGR at the end.** The renderer knows nothing
  about escape sequences, so switching modes (pipe, no color, 16/256/24-bit)
  never touches the layout.
- **Lazy initialization**: grammars and the terminal query are loaded only if
  the document needs them. A document without code or images pays nothing for
  them.
- **ANSI palette by default**, to respect the user's color scheme.
- **Detection by query, with DA1 as the end marker**, not by terminal name.
- **Chunked output** and default `SIGPIPE`, like a Unix filter.

## Testing

```bash
cargo test                                 # everything
cargo test --no-default-features           # without highlighting/decoding
UPDATE_GOLDEN=1 cargo test --test golden   # regenerate snapshots
```

- **Unit tests** (`src/`): SGR output and color conversion, sanitizing,
  wrapping (CJK, combining marks, ZWJ emoji, long words), HTML tokenizer and
  entities, TeX conversion, base64, image path resolution and geometry, column
  fitting, and parsing of terminal replies.
- **Rendering** (`tests/render.rs`): every Markdown element and extension.
  - Nested lists and combined formatting.
  - Tables: alignment, wrapping, record layout, CJK.
  - Links, images, HTML, math, footnotes and front matter.
  - Scaled headings.
  - Narrow widths (1–40 columns without panics), deep nesting and empty
    documents.
  - Styles that never leak across lines, and control characters neutralized
    in every context.
- **Golden snapshots** (`tests/golden.rs`): the 10 documents in
  `tests/fixtures/` are rendered as plain text, 16-color ANSI, narrow ASCII
  and true color. The results are compared with `tests/golden/`, where `ESC`
  is written as `\e`. Every line is also checked against the requested width.
- **CLI** (`tests/cli.rs`):
  - Inputs: one or several files, standard input, `-`, empty input, invalid
    UTF-8, BOM and CRLF.
  - Output: no escapes when redirected; `--color`, `NO_COLOR`,
    `CLICOLOR_FORCE` and color depth; `--width` and `COLUMNS`; ASCII mode and
    locale.
  - Options and errors: `--help`, `--version`, `--big-headings`, invalid
    arguments, and missing, directory or unreadable files.
  - Safety: file names with escapes and injected control sequences.
  - Images (Kitty, iTerm2, missing, fake), large documents and closed pipes.

## Performance

Reproducible benchmarks:

```bash
cargo bench          # criterion: parsing vs. rendering, in memory
scripts/bench.sh     # release binary: wall time and peak memory
```

The documents are generated deterministically (`benches/gen.rs`):

- **mixed**: headings, prose with emphasis and links, lists, quotes, Rust code
  and tables;
- **prose**: paragraphs only;
- **code**: code blocks only;
- **tables**: tables only.

The results below were measured on Linux, on an Intel Core Ultra 7 255U.
They are only indicative. **No comparisons with other tools have been made.**

In memory (`cargo bench`, median):

| Document | Parse only | Render, plain | Render, styled |
|---|---:|---:|---:|
| mixed 2 KB | 0.009 ms | 0.056 ms | 2.1 ms¹ |
| mixed 100 KB | 0.32 ms | 1.9 ms | 10.7 ms |
| mixed 5 MB | 17.7 ms | 98 ms | ~300 ms² |
| prose 100 KB | 0.20 ms | 1.0 ms | 1.1 ms |
| code 100 KB | 0.09 ms | 0.96 ms | 58 ms |
| tables 100 KB | 0.39 ms | 3.7 ms | 3.8 ms |

¹ Includes loading the grammars (~1.3 ms) and compiling the Rust grammar
(~2 ms). These costs are only paid by documents with tagged code blocks.
² Bounded by the highlighting budget: syntect processes about 1.7 MB/s.

Whole process (`scripts/bench.sh`; includes process startup and about 1 ms of
script overhead):

| Document | Plain | Styled | Peak RSS |
|---|---:|---:|---:|
| mixed 4 KB | 1.6 ms | 5.3 ms | 6.9 MB |
| mixed 104 KB | 4.4 ms | 16 ms | 7.4 MB |
| mixed 5.1 MB | 110 ms | 305 ms | 39 MB |
| prose 104 KB | 3.3 ms | 3.2 ms | 3.7 MB |

Measured without the script, startup plus rendering a small document with no
code takes about 0.6 ms.

## Limitations

- **Scaled headings** need a terminal with Kitty's text sizing protocol and do
  not survive `less`. When a heading mixes styles (for example `code` inside
  it), a gap of less than two cells may appear at each style change.
- **Sixel** is not implemented: it needs color quantization and an encoder of
  its own. Sixel-only terminals get the text form.
- **OSC 8 hyperlinks** are not emitted. They cannot be detected reliably, and
  terminals without support may print garbage. URLs are printed as text, which
  most terminals make clickable.
- **iTerm2/WezTerm** support was verified against a simulated terminal, not
  the real applications. Kitty is the reference protocol.
- **macOS** is built and tested in CI. The terminal query uses `select(2)`,
  because `poll(2)` does not work on terminal devices there. The release
  binaries are not signed or notarized by Apple.
- **Windows** is not supported (WSL works, as Linux).
- **Remote images** are never shown, by design. SVG is not rasterized.
- **Math** is converted to Unicode approximately. Matrices, alignments and
  complex nested fractions are flattened.
- **Diagrams** (Mermaid, PlantUML, Graphviz) are shown as source.
- **HTML** has no layout for HTML tables or `align`, and `<details>` is always
  expanded.
- **Color depth** comes from `COLORTERM`/`TERM`. If it is wrong, the default
  `ansi` theme still works.
- **Capability query**: if a terminal answers after the 300 ms timeout, the
  late reply may show up at the shell prompt. This can only happen for
  documents with headings or local images; `--images never
  --big-headings never` avoids the query entirely.
- **Ordered lists** with 10 or more items: two-digit numbers shift the text of
  those items by one column.
- **Bidirectional text** (Arabic, Hebrew) is written in logical order; visual
  reordering is up to the terminal.
- **Highlighting** is limited to 256 KB of code per document, for speed.
- There is no built-in pager, by design. Use
  `inkmd --color always x.md | less -R` (images do not survive `less`).
- Markdown superscript/subscript (`^x^`, `~x~`) is not enabled, because `~x~`
  conflicts with GFM strikethrough.

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md).

## License

Licensed under either of

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE))
- MIT license ([LICENSE-MIT](LICENSE-MIT))

at your option.

Unless you explicitly state otherwise, any contribution intentionally
submitted for inclusion in this work by you, as defined in the Apache-2.0
license, shall be dual licensed as above, without any additional terms or
conditions.

[pulldown-cmark]: https://github.com/pulldown-cmark/pulldown-cmark
