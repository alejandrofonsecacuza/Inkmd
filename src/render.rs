//! Markdown event stream → styled, wrapped terminal output.
//!
//! The renderer walks pulldown-cmark events once. Block containers
//! (quotes, list items, footnotes...) are kept on a stack and contribute a
//! prefix to every output line; inline content is accumulated per block and
//! word-wrapped to the width left by those prefixes.

use crate::highlight::Highlighter;
use crate::html::{self, Token};
use crate::image;
use crate::math;
use crate::sanitize::push_clean;
use crate::style::{Painter, Style, Theme, BOLD, ITALIC, STRIKE, UNDERLINE};
use crate::term::{self, Graphics, Protocol};
use crate::wrap::{self, hard_wrap, wrap, Inline, Line, Wrapped};
use pulldown_cmark::{
    Alignment, BlockQuoteKind, CodeBlockKind, Event, HeadingLevel, LinkType, MetadataBlockKind, Options as MdOptions,
    Parser, Tag, TagEnd,
};
use std::cell::OnceCell;
use std::fmt::Write as _;
use std::io::{self, Write};
use std::path::Path;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ImageMode {
    Never,
    /// Query the terminal the first time a local image is found.
    Auto,
    Force(Protocol),
}

#[derive(Clone, Debug)]
pub struct Options {
    /// Total output width in columns.
    pub width: usize,
    pub painter: Painter,
    pub theme: Theme,
    /// Use ASCII instead of Unicode symbols for markers and borders.
    pub ascii: bool,
    pub images: ImageMode,
    /// Terminal height, used to bound image size.
    pub term_rows: usize,
    /// Draw level-1 headings at double size (Kitty text sizing protocol).
    pub big_headings: BigHeadings,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BigHeadings {
    Never,
    /// Probe the terminal the first time a level-1 heading is found.
    Auto,
    Always,
}

impl Default for Options {
    fn default() -> Options {
        Options {
            width: 80,
            painter: Painter::plain(),
            theme: Theme::new(crate::style::ThemeName::Ansi),
            ascii: false,
            images: ImageMode::Never,
            term_rows: 24,
            big_headings: BigHeadings::Never,
        }
    }
}

/// The Markdown extensions inkmd enables.
pub fn parser_options() -> MdOptions {
    MdOptions::ENABLE_TABLES
        | MdOptions::ENABLE_FOOTNOTES
        | MdOptions::ENABLE_STRIKETHROUGH
        | MdOptions::ENABLE_TASKLISTS
        | MdOptions::ENABLE_HEADING_ATTRIBUTES
        | MdOptions::ENABLE_YAML_STYLE_METADATA_BLOCKS
        | MdOptions::ENABLE_PLUSES_DELIMITED_METADATA_BLOCKS
        | MdOptions::ENABLE_MATH
        | MdOptions::ENABLE_GFM
        | MdOptions::ENABLE_DEFINITION_LIST
        | MdOptions::ENABLE_WIKILINKS
}

/// Rendering session: options plus lazily initialized resources that are
/// shared by all documents rendered with it (syntax grammars, terminal
/// graphics capabilities).
pub struct Session {
    pub opts: Options,
    highlighter: OnceCell<Highlighter>,
    graphics: OnceCell<Graphics>,
}

impl Session {
    pub fn new(opts: Options) -> Session {
        Session { opts, highlighter: OnceCell::new(), graphics: OnceCell::new() }
    }

    /// Render `input` to `out`. `base_dir` is used to resolve relative image
    /// paths (the directory of the Markdown file).
    pub fn render(&self, input: &str, base_dir: Option<&Path>, out: &mut dyn Write) -> io::Result<()> {
        let mut r = Renderer::new(self, base_dir, out);
        for event in Parser::new_ext(input, parser_options()) {
            r.event(event);
            if r.containers.is_empty() && r.out.len() >= FLUSH_AT {
                r.flush()?;
            }
            if let Some(e) = r.error.take() {
                return Err(e);
            }
        }
        r.finish()
    }

    fn highlighter(&self) -> &Highlighter {
        self.highlighter.get_or_init(Highlighter::new)
    }

    /// Result of the terminal query, done at most once and only on a tty.
    fn caps(&self) -> &Graphics {
        self.graphics.get_or_init(|| {
            let g = if term::is_tty(1) { term::query_graphics() } else { Graphics::default() };
            debug(&format!("terminal capabilities: {g:?}"));
            g
        })
    }

    fn protocol(&self) -> Option<(Protocol, Option<(u16, u16)>)> {
        match self.opts.images {
            ImageMode::Never => None,
            ImageMode::Force(p) => Some((p, cell_from_ioctl())),
            ImageMode::Auto => {
                let g = self.caps();
                g.protocol.map(|p| (p, g.cell))
            }
        }
    }

    fn text_sizing(&self) -> bool {
        match self.opts.big_headings {
            BigHeadings::Never => false,
            BigHeadings::Always => true,
            BigHeadings::Auto => self.caps().text_sizing,
        }
    }
}

/// `INKMD_DEBUG=1` prints capability and image decisions to stderr.
pub(crate) fn debug(msg: &str) {
    if std::env::var_os("INKMD_DEBUG").is_some_and(|v| !v.is_empty() && v != "0") {
        eprintln!("inkmd: debug: {}", crate::sanitize::clean(msg, false));
    }
}

fn cell_from_ioctl() -> Option<(u16, u16)> {
    let ws = term::window_size(1)?;
    (ws.xpixel > 0 && ws.ypixel > 0).then(|| (ws.xpixel / ws.cols.max(1), ws.ypixel / ws.rows.max(1)))
}

/// Render a whole document with a one-off session.
pub fn render(input: &str, opts: &Options, out: &mut dyn Write) -> io::Result<()> {
    Session::new(opts.clone()).render(input, None, out)
}

/// Convenience for tests and benchmarks.
pub fn render_to_string(input: &str, opts: &Options) -> String {
    let mut v = Vec::new();
    render(input, opts, &mut v).expect("writing to a Vec cannot fail");
    String::from_utf8(v).expect("renderer produces UTF-8")
}

const FLUSH_AT: usize = 64 * 1024;
/// Bytes of code highlighted per document. Highlighting runs at a few MB/s,
/// so beyond this budget code blocks are shown unhighlighted to keep huge
/// documents fast.
const HIGHLIGHT_BUDGET: usize = 256 * 1024;
/// Never wrap content narrower than this, even when deeply nested.
const MIN_WIDTH: usize = 6;

struct Symbols {
    bullets: [&'static str; 3],
    quote: &'static str,
    rule: &'static str,
    task: [&'static str; 2],
    vbar: &'static str,
    hbar: &'static str,
    cross: &'static str,
    cont: &'static str,
    summary: &'static str,
}

const UNICODE: Symbols = Symbols {
    bullets: ["•", "◦", "▪"],
    quote: "│ ",
    rule: "─",
    task: ["☐", "☑"],
    vbar: "│",
    hbar: "─",
    cross: "┼",
    cont: "↪ ",
    summary: "▸ ",
};

const ASCII: Symbols = Symbols {
    bullets: ["*", "-", "+"],
    quote: "| ",
    rule: "-",
    task: ["[ ]", "[x]"],
    vbar: "|",
    hbar: "-",
    cross: "+",
    cont: "> ",
    summary: "> ",
};

struct Container {
    /// Prefix for the next line, until a line has been written.
    first: Option<Line>,
    rest: Line,
}

struct ListState {
    next: Option<u64>,
}

struct LinkState {
    url: String,
    start: usize,
    autolink: bool,
}

struct ImageState {
    url: String,
    start: usize,
    in_link: bool,
}

struct CodeState {
    lang: String,
    label: Option<String>,
    text: String,
}

struct TableState {
    aligns: Vec<Alignment>,
    rows: Vec<Vec<Vec<Inline>>>,
    header_rows: usize,
    row: Vec<Vec<Inline>>,
}

struct ImageBlock {
    img: image::Loaded,
    protocol: Protocol,
    cols: usize,
    rows: usize,
}

struct HtmlOpen {
    tag: String,
    depth: usize,
    href: Option<(String, usize)>,
}

struct Renderer<'s, 'w> {
    session: &'s Session,
    opts: &'s Options,
    th: &'s Theme,
    sym: &'static Symbols,
    base_dir: Option<&'s Path>,
    writer: &'w mut dyn Write,
    error: Option<io::Error>,
    out: String,

    containers: Vec<Container>,
    inline: Vec<Inline>,
    /// Text pushed at or after this index is never merged into earlier
    /// items, so link/image text can be located afterwards.
    merge_floor: usize,
    styles: Vec<Style>,
    pending_blank: bool,
    fresh: bool,
    lines_out: usize,

    lists: Vec<ListState>,
    item_para: Vec<bool>,
    links: Vec<LinkState>,
    image: Option<ImageState>,
    code: Option<CodeState>,
    table: Option<TableState>,
    blocks: Vec<Option<ImageBlock>>,
    html: html::Tokenizer,
    html_open: Vec<HtmlOpen>,
    html_cells: usize,
    highlight_budget: usize,
}

impl<'s, 'w> Renderer<'s, 'w> {
    fn new(session: &'s Session, base_dir: Option<&'s Path>, writer: &'w mut dyn Write) -> Self {
        let opts = &session.opts;
        Renderer {
            session,
            opts,
            th: &opts.theme,
            sym: if opts.ascii { &ASCII } else { &UNICODE },
            base_dir,
            writer,
            error: None,
            out: String::with_capacity(8 * 1024),
            containers: Vec::new(),
            inline: Vec::new(),
            merge_floor: 0,
            styles: Vec::new(),
            pending_blank: false,
            fresh: false,
            lines_out: 0,
            lists: Vec::new(),
            item_para: Vec::new(),
            links: Vec::new(),
            image: None,
            code: None,
            table: None,
            blocks: Vec::new(),
            html: html::Tokenizer::default(),
            html_open: Vec::new(),
            html_cells: 0,
            highlight_budget: HIGHLIGHT_BUDGET,
        }
    }

    fn flush(&mut self) -> io::Result<()> {
        if !self.out.is_empty() {
            self.writer.write_all(self.out.as_bytes())?;
            self.out.clear();
        }
        Ok(())
    }

    fn finish(mut self) -> io::Result<()> {
        self.flush_inline();
        self.flush()?;
        self.writer.flush()
    }

    // ----- styles -------------------------------------------------------

    fn cur(&self) -> Style {
        self.styles.last().copied().unwrap_or_default()
    }

    fn push_style(&mut self, s: Style) {
        let top = self.cur().over(s);
        self.styles.push(top);
    }

    fn pop_style(&mut self) {
        self.styles.pop();
    }

    fn color(&self) -> bool {
        self.opts.painter.level != crate::style::ColorLevel::None
    }

    fn attrs(&self) -> bool {
        self.opts.painter.attrs
    }

    // ----- inline content ----------------------------------------------

    fn push_text_styled(&mut self, style: Style, s: &str) {
        let ascii = self.opts.ascii;
        if self.inline.len() > self.merge_floor {
            if let Some(Inline::Text(st, t)) = self.inline.last_mut() {
                if *st == style {
                    push_clean(t, s, ascii, false);
                    return;
                }
            }
        }
        let mut text = String::with_capacity(s.len());
        push_clean(&mut text, s, ascii, false);
        self.inline.push(Inline::Text(style, text));
    }

    fn mark_start(&mut self) -> usize {
        self.merge_floor = self.inline.len();
        self.inline.len()
    }

    fn push_text(&mut self, s: &str) {
        self.push_text_styled(self.cur(), s);
    }

    /// Plain text of the inline items from `start` on.
    fn inline_text(&self, start: usize) -> String {
        let mut s = String::new();
        for item in &self.inline[start..] {
            if let Inline::Text(_, t) = item {
                s.push_str(t);
            }
        }
        s
    }

    // ----- line output ---------------------------------------------------

    fn prefix_width(&self) -> usize {
        self.containers.iter().map(|c| c.rest.width).sum()
    }

    fn avail(&self) -> usize {
        self.opts.width.saturating_sub(self.prefix_width()).max(MIN_WIDTH.min(self.opts.width))
    }

    fn prefix(&mut self) -> Line {
        let mut p = Line::new();
        for c in &mut self.containers {
            match c.first.take() {
                Some(f) => p.append(&f),
                None => p.append(&c.rest),
            }
        }
        p
    }

    fn resolve_blank(&mut self) {
        if self.pending_blank && self.lines_out > 0 && !self.fresh {
            let mut p = Line::new();
            for c in &self.containers {
                p.append(&c.rest);
            }
            p.trim_end();
            p.emit(&mut self.out, &self.opts.painter);
            self.out.push('\n');
        }
        self.pending_blank = false;
        self.fresh = false;
    }

    fn emit_line(&mut self, line: &Line) {
        self.resolve_blank();
        let mut full = self.prefix();
        full.append(line);
        full.trim_end();
        full.emit(&mut self.out, &self.opts.painter);
        self.out.push('\n');
        self.lines_out += 1;
    }

    fn push_container(&mut self, first: Option<Line>, rest: Line) {
        self.resolve_blank();
        self.containers.push(Container { first, rest });
        self.fresh = true;
    }

    fn pop_container(&mut self) {
        self.containers.pop();
        self.fresh = false;
    }

    /// Wrap and print the accumulated inline content, if any.
    fn flush_inline(&mut self) {
        if self.inline.is_empty() {
            return;
        }
        let mut items = std::mem::take(&mut self.inline);
        self.merge_floor = 0;
        while matches!(items.last(), Some(Inline::Break)) {
            items.pop();
        }
        for w in wrap(&items, self.avail()) {
            match w {
                Wrapped::Line(l) => self.emit_line(&l),
                Wrapped::Block(id) => {
                    if let Some(b) = self.blocks.get_mut(id).and_then(Option::take) {
                        self.emit_image(b);
                    }
                }
            }
        }
        self.blocks.clear();
        // Unclosed inline HTML must not leak style into the next block.
        if let Some(first) = self.html_open.first() {
            self.styles.truncate(first.depth);
            self.html_open.clear();
        }
    }

    fn block_start(&mut self) {
        // Tight list items contain bare inline text that ends where the
        // next block (e.g. a nested list) begins.
        self.flush_inline();
    }

    fn block_end(&mut self) {
        self.flush_inline();
        self.pending_blank = true;
    }

    fn emit_image(&mut self, b: ImageBlock) {
        self.resolve_blank();
        let painter = self.opts.painter;
        let mut first = self.prefix();
        first.trim_end();
        let pw = self.prefix_width();
        let mut out = std::mem::take(&mut self.out);
        match b.protocol {
            Protocol::Kitty => {
                // Reserve the rows first (scrolling if needed), go back up,
                // draw without moving the cursor, then return below.
                for i in 0..b.rows {
                    if i == 0 {
                        first.emit(&mut out, &painter);
                    } else {
                        let mut p = Line::new();
                        for c in &self.containers {
                            p.append(&c.rest);
                        }
                        p.trim_end();
                        p.emit(&mut out, &painter);
                    }
                    out.push('\n');
                }
                let _ = write!(out, "\x1b[{}A\r", b.rows);
                if pw > 0 {
                    let _ = write!(out, "\x1b[{pw}C");
                }
                image::encode(&b.img, Protocol::Kitty, b.cols, b.rows, &mut out);
                let _ = write!(out, "\x1b[{}B\r", b.rows);
            }
            Protocol::Iterm2 => {
                first.emit(&mut out, &painter);
                for _ in first.width..pw {
                    out.push(' ');
                }
                image::encode(&b.img, Protocol::Iterm2, b.cols, b.rows, &mut out);
                out.push('\n');
            }
        }
        self.out = out;
        self.lines_out += b.rows;
    }

    // ----- events --------------------------------------------------------

    fn event(&mut self, ev: Event<'_>) {
        match ev {
            Event::Start(tag) => self.start(tag),
            Event::End(tag) => self.end(tag),
            Event::Text(t) => {
                if let Some(code) = &mut self.code {
                    code.text.push_str(&t);
                } else if self.links.is_empty() && (t.contains("://") || t.contains("www.")) {
                    self.text_with_urls(&t);
                } else {
                    self.push_text(&t);
                }
            }
            Event::Code(t) => {
                let style = self.cur().over(self.th.code_inline);
                if self.color() {
                    self.push_text_styled(style, &t);
                } else {
                    // Without color, backticks keep code recognizable.
                    self.push_text_styled(style, &format!("`{t}`"));
                }
            }
            Event::InlineMath(t) => {
                let s = math::to_unicode(&t);
                self.push_text_styled(self.cur().over(self.th.math), &s);
            }
            Event::DisplayMath(t) => {
                let s = math::to_unicode(&t);
                if !self.inline.is_empty() {
                    self.inline.push(Inline::Break);
                }
                let style = self.cur().over(self.th.math);
                for (i, line) in s.split('\n').enumerate() {
                    if i > 0 {
                        self.inline.push(Inline::Break);
                    }
                    self.push_text_styled(style, line.trim());
                }
                self.inline.push(Inline::Break);
            }
            Event::Html(t) | Event::InlineHtml(t) => self.html(&t),
            Event::FootnoteReference(label) => {
                let style = self.cur().over(self.th.footnote);
                self.push_text_styled(style, &format!("[{label}]"));
            }
            Event::SoftBreak => self.push_text(" "),
            Event::HardBreak => self.inline.push(Inline::Break),
            Event::Rule => {
                self.block_start();
                let line = Line::from(self.th.rule, &self.sym.rule.repeat(self.avail()));
                self.emit_line(&line);
                self.pending_blank = true;
            }
            Event::TaskListMarker(done) => {
                let (sym, style) =
                    if done { (self.sym.task[1], self.th.task_done) } else { (self.sym.task[0], self.th.marker) };
                if let Some(c) = self.containers.last_mut() {
                    let indent = c.rest.width.max(wrap::width(sym) + 1);
                    let mut first = Line::from(style, sym);
                    first.push_spaces(indent - first.width);
                    c.first = Some(first);
                    c.rest = Line::from(Style::PLAIN, &" ".repeat(indent));
                }
            }
        }
    }

    fn start(&mut self, tag: Tag<'_>) {
        match tag {
            Tag::Paragraph => {
                self.block_start();
                if let Some(p) = self.item_para.last_mut() {
                    *p = true;
                }
            }
            Tag::Heading { level, .. } => {
                self.block_start();
                let n = heading_index(level);
                if self.opts.painter.enabled() {
                    // Styles carry the hierarchy; no Markdown markers.
                    self.push_container(None, Line::new());
                } else {
                    // Plain text output keeps `#` markers so levels stay visible.
                    let mut first = Line::from(self.th.heading_marker, &"#".repeat(n + 1));
                    first.push_spaces(1);
                    let rest = Line::from(Style::PLAIN, &" ".repeat(first.width));
                    self.push_container(Some(first), rest);
                }
                self.push_style(self.th.heading[n]);
            }
            Tag::BlockQuote(kind) => {
                self.block_start();
                let (bar_style, title) = match kind {
                    None => (self.th.quote, None),
                    Some(k) => {
                        let (i, name) = match k {
                            BlockQuoteKind::Note => (0, "Note"),
                            BlockQuoteKind::Tip => (1, "Tip"),
                            BlockQuoteKind::Important => (2, "Important"),
                            BlockQuoteKind::Warning => (3, "Warning"),
                            BlockQuoteKind::Caution => (4, "Caution"),
                        };
                        (self.th.alert[i], Some((self.th.alert[i].with(BOLD), name)))
                    }
                };
                let bar = Line::from(bar_style, self.sym.quote);
                self.push_container(None, bar);
                if let Some((style, name)) = title {
                    self.emit_line(&Line::from(style, name));
                }
            }
            Tag::CodeBlock(kind) => {
                self.block_start();
                let lang = match kind {
                    CodeBlockKind::Fenced(info) => code_lang(&info).to_string(),
                    CodeBlockKind::Indented => String::new(),
                };
                let hidden = matches!(lang.as_str(), "" | "text" | "txt" | "plain" | "plaintext");
                let label = (!hidden).then(|| match lang.as_str() {
                    "mermaid" | "plantuml" | "dot" | "graphviz" => format!("{lang} diagram (source)"),
                    _ => lang.clone(),
                });
                self.code = Some(CodeState { lang, label, text: String::new() });
            }
            Tag::MetadataBlock(kind) => {
                self.block_start();
                let lang = match kind {
                    MetadataBlockKind::YamlStyle => "yaml",
                    MetadataBlockKind::PlusesStyle => "toml",
                };
                self.code = Some(CodeState {
                    lang: lang.to_string(),
                    label: Some("front matter".to_string()),
                    text: String::new(),
                });
            }
            Tag::HtmlBlock => self.block_start(),
            Tag::List(start) => {
                self.block_start();
                self.lists.push(ListState { next: start });
            }
            Tag::Item => {
                self.block_start();
                let depth = self.lists.len().saturating_sub(1);
                let marker = match self.lists.last_mut().and_then(|l| l.next.as_mut()) {
                    Some(n) => {
                        let m = format!("{n}.");
                        *n += 1;
                        m
                    }
                    None => self.sym.bullets[depth % 3].to_string(),
                };
                let mut first = Line::from(self.th.marker, &marker);
                let indent = (first.width + 1).max(2);
                first.push_spaces(indent - first.width);
                self.push_container(Some(first), Line::from(Style::PLAIN, &" ".repeat(indent)));
                self.item_para.push(false);
            }
            Tag::FootnoteDefinition(label) => {
                self.block_start();
                let mut first = Line::new();
                first.push(self.th.footnote, &crate::sanitize::clean(&format!("[{label}]"), self.opts.ascii));
                first.push_spaces(1);
                let rest = Line::from(Style::PLAIN, &" ".repeat(first.width));
                self.push_container(Some(first), rest);
            }
            Tag::DefinitionList => self.block_start(),
            Tag::DefinitionListTitle => {
                self.block_start();
                self.pending_blank = true;
                self.push_style(Style::attrs(BOLD));
            }
            Tag::DefinitionListDefinition => {
                self.block_start();
                self.push_container(None, Line::from(Style::PLAIN, "    "));
            }
            Tag::Table(aligns) => {
                self.block_start();
                self.table = Some(TableState { aligns, rows: Vec::new(), header_rows: 0, row: Vec::new() });
            }
            Tag::TableHead | Tag::TableRow | Tag::TableCell => {}
            Tag::Emphasis => self.push_style(Style::attrs(ITALIC)),
            Tag::Strong => self.push_style(Style::attrs(BOLD)),
            Tag::Strikethrough => {
                self.push_style(Style::attrs(STRIKE));
                if !self.attrs() {
                    self.push_text("~~");
                }
            }
            Tag::Superscript | Tag::Subscript => self.push_style(Style::PLAIN),
            Tag::Link { link_type, dest_url, .. } => {
                let autolink = matches!(link_type, LinkType::Autolink | LinkType::Email);
                let start = self.mark_start();
                self.links.push(LinkState { url: dest_url.to_string(), start, autolink });
                self.push_style(self.th.link);
            }
            Tag::Image { dest_url, .. } => {
                let start = self.mark_start();
                self.image = Some(ImageState { url: dest_url.to_string(), start, in_link: !self.links.is_empty() });
                self.push_style(Style::PLAIN);
            }
        }
    }

    fn end(&mut self, tag: TagEnd) {
        match tag {
            TagEnd::Paragraph => self.block_end(),
            TagEnd::Heading(level) => {
                if self.opts.painter.enabled() {
                    self.styled_heading(heading_index(level));
                }
                self.flush_inline();
                self.pop_style();
                self.pop_container();
                self.pending_blank = true;
            }
            TagEnd::BlockQuote(_) => {
                self.flush_inline();
                self.pop_container();
                self.pending_blank = true;
            }
            TagEnd::CodeBlock | TagEnd::MetadataBlock(_) => {
                if let Some(code) = self.code.take() {
                    self.code_block(code);
                }
                self.pending_blank = true;
            }
            TagEnd::HtmlBlock => {
                let before = self.lines_out;
                self.flush_inline();
                if self.lines_out > before {
                    self.pending_blank = true;
                }
            }
            TagEnd::List(_) => {
                self.flush_inline();
                self.lists.pop();
                self.pending_blank = true;
            }
            TagEnd::Item => {
                self.flush_inline();
                self.pop_container();
                // Loose lists (items with paragraphs) get blank lines between items.
                self.pending_blank = self.item_para.pop().unwrap_or(false);
            }
            TagEnd::FootnoteDefinition => {
                self.flush_inline();
                self.pop_container();
                self.pending_blank = true;
            }
            TagEnd::DefinitionList => self.block_end(),
            TagEnd::DefinitionListTitle => {
                self.flush_inline();
                self.pop_style();
            }
            TagEnd::DefinitionListDefinition => {
                self.flush_inline();
                self.pop_container();
            }
            TagEnd::Table => {
                if let Some(t) = self.table.take() {
                    self.render_table(t);
                }
                self.pending_blank = true;
            }
            TagEnd::TableHead => {
                if let Some(t) = &mut self.table {
                    let row = std::mem::take(&mut t.row);
                    t.rows.push(row);
                    t.header_rows += 1;
                }
            }
            TagEnd::TableRow => {
                if let Some(t) = &mut self.table {
                    let row = std::mem::take(&mut t.row);
                    t.rows.push(row);
                }
            }
            TagEnd::TableCell => {
                let cell = std::mem::take(&mut self.inline);
                self.merge_floor = 0;
                if let Some(t) = &mut self.table {
                    t.row.push(cell);
                }
            }
            TagEnd::Emphasis | TagEnd::Strong | TagEnd::Superscript | TagEnd::Subscript => self.pop_style(),
            TagEnd::Strikethrough => {
                if !self.attrs() {
                    self.push_text("~~");
                }
                self.pop_style();
            }
            TagEnd::Link => {
                self.pop_style();
                if let Some(link) = self.links.pop() {
                    self.link_end(link);
                }
            }
            TagEnd::Image => {
                self.pop_style();
                if let Some(img) = self.image.take() {
                    let alt = self.inline_text(img.start);
                    self.inline.truncate(img.start);
                    self.image(&img.url, &alt, img.in_link);
                }
            }
        }
    }

    /// GFM "extended autolinks": style bare `http(s)://` and `www.` URLs
    /// as links (pulldown-cmark leaves them as text).
    fn text_with_urls(&mut self, t: &str) {
        let mut rest = t;
        while !rest.is_empty() {
            let found = ["https://", "http://", "www."]
                .iter()
                .filter_map(|p| rest.find(p))
                .filter(|&i| i == 0 || !rest.as_bytes()[i - 1].is_ascii_alphanumeric())
                .min();
            let Some(start) = found else { break };
            let tail = &rest[start..];
            let mut end = tail.find(|c: char| c.is_whitespace() || c == '<' || c == '>').unwrap_or(tail.len());
            // Trailing punctuation belongs to the sentence, not the URL.
            while end > 0 && tail[..end].ends_with(['.', ',', ';', ':', '!', '?', ')', '\'', '"', '*', '_']) {
                end -= 1;
            }
            let url = &tail[..end];
            if url.len() <= 8 || !url[4..].contains('.') {
                self.push_text(&rest[..start + end.max(1)]);
                rest = &rest[start + end.max(1)..];
                continue;
            }
            self.push_text(&rest[..start]);
            let style = self.cur().over(self.th.link);
            self.push_text_styled(style, url);
            rest = &tail[end..];
        }
        self.push_text(rest);
    }

    /// Print a styled heading. With the Kitty text sizing protocol, levels
    /// 1-4 get decreasing sizes; level 1 is also followed by a rule as wide
    /// as its text, so it stands out even without colors.
    fn styled_heading(&mut self, level: usize) {
        let scale = HEADING_SCALE.get(level).copied().flatten().filter(|_| self.session.text_sizing());
        if level != 0 && scale.is_none() {
            return; // plain flush by the caller
        }
        let items = std::mem::take(&mut self.inline);
        self.merge_floor = 0;
        // Wrap so that the scaled text fits: width × 2n/d columns.
        let (n, d) = scale.unwrap_or((1, 2));
        let width = (self.avail() * d / (2 * n)).max(1);
        let mut widest = 0;
        for w in wrap(&items, width) {
            if let Wrapped::Line(l) = w {
                widest = widest.max((l.width * 2 * n).div_ceil(d));
                match scale {
                    Some(s) => self.emit_scaled(&l, s),
                    None => self.emit_line(&l),
                }
            }
        }
        if level == 0 && widest > 0 {
            let style = Style { fg: self.th.heading[0].fg, attrs: 0 };
            let sym = if self.opts.ascii { "=" } else { "━" };
            self.emit_line(&Line::from(style, &sym.repeat(widest)));
        }
    }

    /// Emit a line with `OSC 66` at `s=2` scaled by `n/d`: it occupies two
    /// rows, and the container prefix is repeated on both.
    fn emit_scaled(&mut self, line: &Line, (n, d): (usize, usize)) {
        self.resolve_blank();
        let painter = self.opts.painter;
        let mut prefix = self.prefix();
        prefix.trim_end();
        let pad = self.prefix_width().saturating_sub(prefix.width);
        let mut out = std::mem::take(&mut self.out);
        prefix.emit(&mut out, &painter);
        out.extend(std::iter::repeat_n(' ', pad));
        let mut cur = Style::PLAIN;
        for (style, text) in line.segments() {
            painter.transition(&mut out, cur, style);
            cur = style;
            push_scaled(&mut out, text, n, d);
        }
        painter.reset(&mut out, cur);
        out.push('\n');
        let mut second = Line::new();
        for c in &self.containers {
            second.append(&c.rest);
        }
        second.trim_end();
        second.emit(&mut out, &painter);
        out.push('\n');
        self.out = out;
        self.lines_out += 2;
    }

    fn link_end(&mut self, link: LinkState) {
        if link.autolink || link.url.is_empty() || link.url.starts_with('#') {
            return;
        }
        let text = self.inline_text(link.start);
        let url = link.url.as_str();
        let bare = url.strip_prefix("mailto:").unwrap_or(url);
        let no_scheme = bare.split_once("://").map(|(_, r)| r).unwrap_or(bare);
        let t = text.trim();
        if t == url || t == bare || t == no_scheme || t.trim_end_matches('/') == no_scheme.trim_end_matches('/') {
            return;
        }
        let style = self.cur().over(self.th.url);
        self.push_text_styled(style, &format!(" ({url})"));
    }

    // ----- images ----------------------------------------------------------

    fn image(&mut self, url: &str, alt: &str, in_link: bool) {
        let mut note = None;
        let resolved = image::resolve(url, self.base_dir);
        if let (Ok(path), None) = (&resolved, &self.table) {
            // Only now, with a local image in hand, is the terminal queried.
            if let Some((protocol, cell)) = self.session.protocol() {
                match image::load(path, protocol) {
                    Ok(img) => {
                        let cell = cell.unwrap_or((8, 16));
                        let max_rows = (self.opts.term_rows * 2 / 3).clamp(4, 40);
                        let (cols, rows) = image::cell_geometry(img.px, cell, self.avail(), max_rows);
                        debug(&format!(
                            "image {}: {}x{} px, cell {cell:?}, drawn as {cols}x{rows} cells with {protocol:?}",
                            path.display(),
                            img.px.0,
                            img.px.1
                        ));
                        self.blocks.push(Some(ImageBlock { img, protocol, cols, rows }));
                        self.inline.push(Inline::Block(self.blocks.len() - 1));
                        return;
                    }
                    Err(e) => {
                        debug(&format!("image {}: {}", path.display(), e.describe()));
                        note = Some(e.describe());
                    }
                }
            } else {
                debug(&format!("image {url}: no graphics protocol available, using text"));
            }
        } else if let Err(e) = &resolved {
            debug(&format!("image {url}: {}, using text", e.describe()));
        }
        let alt = alt.trim();
        let label = if alt.is_empty() { "[image]".to_string() } else { format!("[image: {alt}]") };
        let style = self.cur().over(self.th.image);
        self.push_text_styled(style, &label);
        let show_url = !url.is_empty() && !(in_link && resolved.is_err());
        if show_url {
            let style = self.cur().over(self.th.url);
            match note {
                Some(n) => self.push_text_styled(style, &format!(" ({url}: {n})")),
                None => self.push_text_styled(style, &format!(" ({url})")),
            }
        }
    }

    // ----- code blocks -----------------------------------------------------

    fn code_block(&mut self, code: CodeState) {
        let indent = if self.avail() >= 40 { 4 } else { 2 };
        self.push_container(None, Line::from(Style::PLAIN, &" ".repeat(indent)));
        if let Some(label) = &code.label {
            let text = if self.attrs() || self.color() { label.clone() } else { format!("[{label}]") };
            self.push_text_styled(self.th.code_label, &text);
            self.flush_inline();
        }
        let text = expand_tabs(&code.text);
        let width = self.avail();
        let mut lines: Vec<Line> = Vec::new();
        if code.lang == "math" {
            for l in math::to_unicode(&text).split('\n') {
                lines.push(Line::from(self.th.math, &crate::sanitize::clean(l.trim(), self.opts.ascii)));
            }
        } else {
            let highlighted = self.color() && !code.lang.is_empty() && text.len() <= self.highlight_budget && {
                self.highlight_budget -= text.len();
                let hl = self.session.highlighter();
                let mut cur = Line::new();
                let ascii = self.opts.ascii;
                let th = self.th;
                let mut buf = String::new();
                let ok = hl.highlight(&code.lang, &text, |piece| match piece {
                    Some((syn, s)) => {
                        buf.clear();
                        push_clean(&mut buf, s, ascii, false);
                        cur.push(th.code_block.over(th.syntax[syn as usize]), &buf);
                    }
                    None => lines.push(std::mem::take(&mut cur)),
                });
                if !ok {
                    lines.clear();
                }
                ok
            };
            if !highlighted {
                for l in text.lines() {
                    lines.push(Line::from(self.th.code_block, &crate::sanitize::clean(l, self.opts.ascii)));
                }
            }
        }
        // Drop trailing empty lines (the fence's final newline).
        while lines.last().is_some_and(Line::is_empty) {
            lines.pop();
        }
        for line in &lines {
            let mut chunks = hard_wrap(line, width).into_iter();
            if let Some(first) = chunks.next() {
                self.emit_line(&first);
            }
            let rest: Vec<Line> = chunks.collect();
            if !rest.is_empty() {
                let mut joined = Line::new();
                for c in &rest {
                    joined.append(c);
                }
                for chunk in hard_wrap(&joined, width.saturating_sub(2).max(1)) {
                    let mut l = Line::from(self.th.muted, self.sym.cont);
                    l.append(&chunk);
                    self.emit_line(&l);
                }
            }
        }
        self.pop_container();
    }

    // ----- tables ----------------------------------------------------------

    fn render_table(&mut self, mut t: TableState) {
        let ncols = t.rows.iter().map(Vec::len).max().unwrap_or(0).max(t.aligns.len());
        if ncols == 0 {
            return;
        }
        for row in &mut t.rows {
            row.resize_with(ncols, Vec::new);
        }
        let header_style = self.th.table_header;
        for row in t.rows.iter_mut().take(t.header_rows) {
            for cell in row.iter_mut() {
                for item in cell.iter_mut() {
                    if let Inline::Text(s, _) = item {
                        *s = s.over(header_style);
                    }
                }
            }
        }
        let avail = self.avail();
        let sep = 3;
        let mut natural = vec![0usize; ncols];
        let mut longest = vec![0usize; ncols];
        for row in &t.rows {
            for (i, cell) in row.iter().enumerate() {
                for w in wrap(cell, usize::MAX / 2) {
                    if let Wrapped::Line(l) = w {
                        natural[i] = natural[i].max(l.width);
                        for word in l.text.split(' ') {
                            longest[i] = longest[i].max(wrap::width(word));
                        }
                    }
                }
            }
        }
        let seps = sep * (ncols - 1);
        let widths = if natural.iter().sum::<usize>() + seps <= avail {
            Some(natural.clone())
        } else {
            fit_columns(&natural, &longest, avail.saturating_sub(seps))
        };
        match widths {
            Some(w) => self.table_grid(&t, &w),
            None => self.table_records(&t),
        }
    }

    fn table_grid(&mut self, t: &TableState, widths: &[usize]) {
        let border = self.th.table_border;
        let vsep = format!(" {} ", self.sym.vbar);
        for (ri, row) in t.rows.iter().enumerate() {
            let cells: Vec<Vec<Line>> = row
                .iter()
                .zip(widths)
                .map(|(cell, &w)| {
                    wrap(cell, w.max(1))
                        .into_iter()
                        .filter_map(|x| match x {
                            Wrapped::Line(l) => Some(l),
                            Wrapped::Block(_) => None,
                        })
                        .collect()
                })
                .collect();
            let height = cells.iter().map(Vec::len).max().unwrap_or(0).max(1);
            for k in 0..height {
                let mut line = Line::new();
                for (ci, cell) in cells.iter().enumerate() {
                    if ci > 0 {
                        line.push(border, &vsep);
                    }
                    let content = cell.get(k);
                    let cw = content.map_or(0, |l| l.width);
                    let pad = widths[ci].saturating_sub(cw);
                    let (left, right) = match t.aligns.get(ci).copied().unwrap_or(Alignment::None) {
                        Alignment::Right => (pad, 0),
                        Alignment::Center => (pad / 2, pad - pad / 2),
                        _ => (0, pad),
                    };
                    line.push_w(Style::PLAIN, &" ".repeat(left), left);
                    if let Some(c) = content {
                        line.append(c);
                    }
                    line.push_w(Style::PLAIN, &" ".repeat(right), right);
                }
                self.emit_line(&line);
            }
            if ri + 1 == t.header_rows {
                let mut line = Line::new();
                for (ci, &w) in widths.iter().enumerate() {
                    if ci > 0 {
                        line.push(border, &format!("{}{}{}", self.sym.hbar, self.sym.cross, self.sym.hbar));
                    }
                    line.push(border, &self.sym.hbar.repeat(w));
                }
                self.emit_line(&line);
            }
        }
    }

    /// Fallback for tables that cannot fit even with wrapped cells: one
    /// record per row, `header: value` per line.
    fn table_records(&mut self, t: &TableState) {
        let headers: Vec<String> = match t.rows.first().filter(|_| t.header_rows > 0) {
            Some(h) => h.iter().map(|c| plain_text(c)).collect(),
            None => Vec::new(),
        };
        let header_style = self.th.table_header;
        for (ri, row) in t.rows.iter().enumerate().skip(t.header_rows) {
            if ri > t.header_rows {
                self.resolve_blank();
                self.pending_blank = true;
            }
            for (ci, cell) in row.iter().enumerate() {
                let name = headers.get(ci).filter(|h| !h.is_empty()).cloned().unwrap_or_else(|| format!("{}", ci + 1));
                let mut items = vec![Inline::Text(header_style, format!("{name}:"))];
                items.push(Inline::Text(Style::PLAIN, " ".to_string()));
                items.extend(cell.iter().cloned());
                for w in wrap(&items, self.avail()) {
                    if let Wrapped::Line(l) = w {
                        self.emit_line(&l);
                    }
                }
            }
        }
    }

    // ----- HTML ------------------------------------------------------------

    fn html(&mut self, raw: &str) {
        let mut tokens = Vec::new();
        let mut tok = std::mem::take(&mut self.html);
        tok.tokenize(raw, &mut tokens);
        self.html = tok;
        for t in tokens {
            match t {
                Token::Text(s) => {
                    let s = html::decode_entities(s);
                    self.push_text_styled(self.cur().over(self.th.html), &s);
                }
                Token::Tag(tag) => self.html_tag(&tag),
            }
        }
    }

    fn html_line_break(&mut self) {
        // Whitespace between tags must not produce empty lines.
        while self.inline.len() > self.merge_floor
            && matches!(self.inline.last(), Some(Inline::Text(_, t)) if t.trim().is_empty())
        {
            self.inline.pop();
        }
        if !self.inline.is_empty() && !matches!(self.inline.last(), Some(Inline::Break)) {
            self.inline.push(Inline::Break);
        }
    }

    fn html_tag(&mut self, tag: &html::Tag<'_>) {
        let name = tag.name.as_str();
        if tag.closing {
            if html::is_block(name) {
                self.html_line_break();
            }
            if let Some(pos) = self.html_open.iter().rposition(|o| o.tag == name) {
                let open = self.html_open.remove(pos);
                self.html_open.truncate(pos);
                self.styles.truncate(open.depth);
                if let Some((href, start)) = open.href {
                    self.link_end(LinkState { url: href, start, autolink: false });
                }
            }
            return;
        }
        let style = match name {
            "br" => {
                self.inline.push(Inline::Break);
                return;
            }
            "hr" => {
                self.html_line_break();
                let rule = self.sym.rule.repeat(self.avail().min(40));
                self.push_text_styled(self.th.rule, &rule);
                self.inline.push(Inline::Break);
                return;
            }
            "img" => {
                let src = tag.attr("src").unwrap_or_default();
                let alt = tag.attr("alt").unwrap_or_default();
                let in_link = self.html_open.iter().any(|o| o.href.is_some()) || !self.links.is_empty();
                self.image(&src, &alt, in_link);
                return;
            }
            "li" => {
                self.html_line_break();
                let depth = self.html_open.iter().filter(|o| o.tag == "ul" || o.tag == "ol").count();
                let bullet = self.sym.bullets[depth.saturating_sub(1) % 3];
                self.push_text_styled(self.th.marker, &format!("{bullet}\u{a0}"));
                None
            }
            "tr" => {
                self.html_line_break();
                self.html_cells = 0;
                None
            }
            "td" | "th" => {
                if self.html_cells > 0 {
                    self.push_text_styled(self.th.table_border, &format!("\u{a0}{}\u{a0}", self.sym.vbar));
                }
                self.html_cells += 1;
                (name == "th").then_some(self.th.table_header)
            }
            "summary" => {
                self.html_line_break();
                self.push_text_styled(self.th.marker, self.sym.summary);
                Some(Style::attrs(BOLD))
            }
            "h1" | "h2" | "h3" | "h4" | "h5" | "h6" => {
                self.html_line_break();
                let n = (name.as_bytes()[1] - b'1') as usize;
                Some(self.th.heading[n.min(5)])
            }
            "b" | "strong" => Some(Style::attrs(BOLD)),
            "i" | "em" | "cite" | "var" | "dfn" => Some(Style::attrs(ITALIC)),
            "u" | "ins" => Some(Style::attrs(UNDERLINE)),
            "s" | "del" | "strike" => Some(Style::attrs(STRIKE)),
            "code" | "kbd" | "samp" | "tt" => Some(self.th.code_inline),
            "mark" => Some(self.th.mark),
            "a" => Some(self.th.link),
            _ => {
                if html::is_block(name) {
                    self.html_line_break();
                }
                None
            }
        };
        if tag.self_closing {
            return;
        }
        let href = (name == "a").then(|| tag.attr("href")).flatten().map(|h| (h, self.mark_start()));
        self.html_open.push(HtmlOpen { tag: name.to_string(), depth: self.styles.len(), href });
        self.push_style(style.unwrap_or_default());
    }
}

/// Font scale per heading level with the text sizing protocol, as a
/// fraction `n/d` of a 2-row block: 2×, 1.75×, 1.5×, 1.25×, then normal.
const HEADING_SCALE: [Option<(usize, usize)>; 6] = [Some((1, 1)), Some((7, 8)), Some((3, 4)), Some((5, 8)), None, None];

/// Append `text` drawn with `OSC 66`. At full scale the terminal lays out
/// each character in its own 2×2 block. With a fractional scale, all text in
/// one escape code shares a block `2w` cells wide, so the text is sent in
/// chunks of 8 columns, which at 1.75×, 1.5× and 1.25× fill exactly 14, 12
/// and 10 cells (w = 7, 6, 5) and leave no gaps between chunks.
fn push_scaled(out: &mut String, text: &str, n: usize, d: usize) {
    if n == d {
        // The protocol limits each escape code to 4096 bytes of text.
        let mut rest = text;
        while !rest.is_empty() {
            let mut cut = rest.len().min(4000);
            while !rest.is_char_boundary(cut) {
                cut -= 1;
            }
            let _ = write!(out, "\x1b]66;s=2;{}\x07", &rest[..cut]);
            rest = &rest[cut..];
        }
        return;
    }
    let flush = |out: &mut String, chunk: &str, cols: usize| {
        if cols > 0 {
            let w = (cols * n).div_ceil(d).clamp(1, 7);
            let _ = write!(out, "\x1b]66;s=2:n={n}:d={d}:v=2:w={w};{chunk}\x07");
        }
    };
    let mut chunk = String::new();
    let mut cols = 0;
    for g in unicode_segmentation::UnicodeSegmentation::graphemes(text, true) {
        let gw = wrap::width(g);
        if cols + gw > 8 && cols > 0 {
            flush(out, &chunk, cols);
            chunk.clear();
            cols = 0;
        }
        chunk.push_str(g);
        cols += gw;
    }
    flush(out, &chunk, cols);
}

fn heading_index(level: HeadingLevel) -> usize {
    match level {
        HeadingLevel::H1 => 0,
        HeadingLevel::H2 => 1,
        HeadingLevel::H3 => 2,
        HeadingLevel::H4 => 3,
        HeadingLevel::H5 => 4,
        HeadingLevel::H6 => 5,
    }
}

/// First word of a fence info string: "rust,ignore" → "rust", "{.python}" → "python".
fn code_lang(info: &str) -> &str {
    let word = info.split(|c: char| c.is_whitespace() || c == ',').next().unwrap_or("");
    word.trim_matches(|c: char| c == '{' || c == '}' || c == '.')
}

fn expand_tabs(s: &str) -> String {
    if !s.contains('\t') {
        return s.to_string();
    }
    let mut out = String::with_capacity(s.len() + 16);
    let mut col = 0;
    for c in s.chars() {
        match c {
            '\t' => {
                let n = 4 - col % 4;
                out.extend(std::iter::repeat_n(' ', n));
                col += n;
            }
            '\n' => {
                out.push('\n');
                col = 0;
            }
            _ => {
                out.push(c);
                col += 1;
            }
        }
    }
    out
}

fn plain_text(items: &[Inline]) -> String {
    let mut s = String::new();
    for i in items {
        match i {
            Inline::Text(_, t) => s.push_str(t),
            Inline::Break => s.push(' '),
            Inline::Block(_) => {}
        }
    }
    s.trim().to_string()
}

/// Shrink columns to fit `budget` columns of content. Columns narrower
/// than their fair share keep their natural width; the others share the
/// rest in proportion to their natural width, keeping at least their
/// longest word (bounded) when possible. Returns `None` when even a minimal
/// layout would not fit.
fn fit_columns(natural: &[usize], longest: &[usize], budget: usize) -> Option<Vec<usize>> {
    let mut widths = vec![0usize; natural.len()];
    let mut open: Vec<usize> = (0..natural.len()).collect();
    let mut left = budget;
    while !open.is_empty() {
        let fair = left / open.len();
        let (small, big): (Vec<usize>, Vec<usize>) = open.iter().partition(|&&i| natural[i] <= fair);
        if small.is_empty() {
            break;
        }
        for i in small {
            widths[i] = natural[i];
            left -= natural[i];
        }
        open = big;
    }
    if open.is_empty() {
        return Some(widths);
    }
    let mut floors: Vec<usize> = open.iter().map(|&i| natural[i].min(longest[i].clamp(4, 16))).collect();
    if floors.iter().sum::<usize>() > left {
        floors = open.iter().map(|&i| natural[i].min(4)).collect();
        if floors.iter().sum::<usize>() > left {
            return None;
        }
    }
    let mut extra = left - floors.iter().sum::<usize>();
    let deficit: usize = open.iter().zip(&floors).map(|(&i, f)| natural[i] - f).sum();
    let share = extra;
    for (k, &i) in open.iter().enumerate() {
        let add = ((natural[i] - floors[k]) * share).checked_div(deficit).unwrap_or(0);
        widths[i] = floors[k] + add;
        extra -= add;
    }
    // Hand out the rounding remainder.
    for &i in &open {
        if extra == 0 {
            break;
        }
        if widths[i] < natural[i] {
            widths[i] += 1;
            extra -= 1;
        }
    }
    Some(widths)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fits_columns() {
        assert_eq!(fit_columns(&[10, 50], &[5, 8], 40), Some(vec![10, 30]));
        let w = fit_columns(&[30, 30, 30], &[6, 6, 6], 45).unwrap();
        assert_eq!(w.iter().sum::<usize>(), 45);
        assert_eq!(fit_columns(&[30; 10], &[10; 10], 20), None);
    }

    #[test]
    fn code_lang_parsing() {
        assert_eq!(code_lang("rust,ignore"), "rust");
        assert_eq!(code_lang("{.python} title"), "python");
        assert_eq!(code_lang(""), "");
    }

    #[test]
    fn tabs_expand_to_stops() {
        assert_eq!(expand_tabs("a\tb\n\tc"), "a   b\n    c");
    }
}
