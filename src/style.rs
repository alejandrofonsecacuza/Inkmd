//! Styles, colors, themes and SGR emission.
//!
//! The renderer only ever deals with abstract [`Style`] values. Converting
//! them to escape sequences happens here, according to the color depth and
//! whether attributes are allowed at all (plain output for pipes).

use std::fmt::Write as _;

/// Color depth the output is allowed to use.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum ColorLevel {
    None,
    Ansi16,
    Ansi256,
    TrueColor,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Color {
    /// One of the 16 terminal palette colors (0-7 normal, 8-15 bright).
    /// These follow the user's terminal theme, which is why the default
    /// theme uses them exclusively.
    Ansi(u8),
    Rgb(u8, u8, u8),
}

pub const BOLD: u8 = 1;
pub const DIM: u8 = 2;
pub const ITALIC: u8 = 4;
pub const UNDERLINE: u8 = 8;
pub const STRIKE: u8 = 16;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Style {
    pub fg: Option<Color>,
    pub attrs: u8,
}

impl Style {
    pub const PLAIN: Style = Style { fg: None, attrs: 0 };

    pub const fn fg(c: Color) -> Style {
        Style { fg: Some(c), attrs: 0 }
    }
    pub const fn attrs(a: u8) -> Style {
        Style { fg: None, attrs: a }
    }
    pub const fn with(self, a: u8) -> Style {
        Style { fg: self.fg, attrs: self.attrs | a }
    }
    /// Layer `top` over `self`: attributes accumulate, color is replaced.
    pub fn over(self, top: Style) -> Style {
        Style { fg: top.fg.or(self.fg), attrs: self.attrs | top.attrs }
    }
    pub fn has(self, a: u8) -> bool {
        self.attrs & a != 0
    }
}

/// Semantic slots used by syntax highlighting.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Syn {
    Plain = 0,
    Comment,
    Keyword,
    String,
    Number,
    Function,
    Type,
    Inserted,
    Deleted,
}

impl Syn {
    pub fn from_u8(v: u8) -> Syn {
        match v {
            1 => Syn::Comment,
            2 => Syn::Keyword,
            3 => Syn::String,
            4 => Syn::Number,
            5 => Syn::Function,
            6 => Syn::Type,
            7 => Syn::Inserted,
            8 => Syn::Deleted,
            _ => Syn::Plain,
        }
    }
}

#[derive(Clone, Debug)]
pub struct Theme {
    pub heading: [Style; 6],
    pub heading_marker: Style,
    pub code_inline: Style,
    pub code_block: Style,
    pub code_label: Style,
    pub link: Style,
    pub url: Style,
    pub marker: Style,
    pub quote: Style,
    pub rule: Style,
    pub table_border: Style,
    pub table_header: Style,
    pub image: Style,
    pub math: Style,
    pub footnote: Style,
    pub task_done: Style,
    pub html: Style,
    pub muted: Style,
    pub mark: Style,
    /// note, tip, important, warning, caution
    pub alert: [Style; 5],
    pub syntax: [Style; 9],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ThemeName {
    /// Uses only the terminal's 16-color palette, so it follows the user's
    /// own color scheme (dark or light).
    Ansi,
    Dark,
    Light,
}

impl Theme {
    pub fn new(name: ThemeName) -> Theme {
        match name {
            ThemeName::Ansi => Theme::ansi(),
            ThemeName::Dark => Theme::rgb(&DARK),
            ThemeName::Light => Theme::rgb(&LIGHT),
        }
    }

    fn ansi() -> Theme {
        use Color::Ansi;
        const BLUE: Color = Ansi(4);
        const CYAN: Color = Ansi(6);
        const MAGENTA: Color = Ansi(5);
        const GREEN: Color = Ansi(2);
        const YELLOW: Color = Ansi(3);
        const RED: Color = Ansi(1);
        let dim = Style::attrs(DIM);
        Theme {
            heading: [
                Style::fg(MAGENTA).with(BOLD),
                Style::fg(BLUE).with(BOLD),
                Style::fg(CYAN).with(BOLD),
                Style::attrs(BOLD),
                Style::attrs(BOLD | ITALIC),
                Style::attrs(DIM | ITALIC),
            ],
            heading_marker: dim,
            code_inline: Style::fg(YELLOW),
            code_block: Style::PLAIN,
            code_label: Style::attrs(DIM | ITALIC),
            link: Style::fg(BLUE).with(UNDERLINE),
            url: dim,
            marker: Style::fg(CYAN),
            quote: Style::fg(CYAN).with(DIM),
            rule: dim,
            table_border: dim,
            table_header: Style::attrs(BOLD),
            image: Style::fg(MAGENTA).with(ITALIC),
            math: Style::fg(GREEN).with(ITALIC),
            footnote: Style::fg(CYAN),
            task_done: Style::fg(GREEN),
            html: Style::PLAIN,
            muted: dim,
            mark: Style::fg(YELLOW).with(BOLD),
            alert: [Style::fg(BLUE), Style::fg(GREEN), Style::fg(MAGENTA), Style::fg(YELLOW), Style::fg(RED)],
            syntax: [
                Style::PLAIN,
                Style::attrs(DIM | ITALIC),
                Style::fg(MAGENTA),
                Style::fg(GREEN),
                Style::fg(YELLOW),
                Style::fg(BLUE),
                Style::fg(CYAN),
                Style::fg(GREEN),
                Style::fg(RED),
            ],
        }
    }

    fn rgb(p: &Palette) -> Theme {
        let c = |rgb: (u8, u8, u8)| Style::fg(Color::Rgb(rgb.0, rgb.1, rgb.2));
        let muted = c(p.muted);
        Theme {
            heading: [
                c(p.h1).with(BOLD),
                c(p.h2).with(BOLD),
                c(p.h3).with(BOLD),
                c(p.fg).with(BOLD),
                c(p.fg).with(BOLD | ITALIC),
                muted.with(ITALIC),
            ],
            heading_marker: muted,
            code_inline: c(p.code),
            code_block: c(p.fg),
            code_label: muted.with(ITALIC),
            link: c(p.link).with(UNDERLINE),
            url: muted,
            marker: c(p.marker),
            quote: c(p.quote),
            rule: muted,
            table_border: muted,
            table_header: c(p.fg).with(BOLD),
            image: c(p.h1).with(ITALIC),
            math: c(p.string).with(ITALIC),
            footnote: c(p.marker),
            task_done: c(p.string),
            html: Style::PLAIN,
            muted,
            mark: c(p.code).with(BOLD),
            alert: [c(p.link), c(p.string), c(p.h1), c(p.code), c(p.red)],
            syntax: [
                c(p.fg),
                muted.with(ITALIC),
                c(p.keyword),
                c(p.string),
                c(p.number),
                c(p.function),
                c(p.typ),
                c(p.string),
                c(p.red),
            ],
        }
    }
}

struct Palette {
    fg: (u8, u8, u8),
    muted: (u8, u8, u8),
    h1: (u8, u8, u8),
    h2: (u8, u8, u8),
    h3: (u8, u8, u8),
    code: (u8, u8, u8),
    link: (u8, u8, u8),
    marker: (u8, u8, u8),
    quote: (u8, u8, u8),
    keyword: (u8, u8, u8),
    string: (u8, u8, u8),
    number: (u8, u8, u8),
    function: (u8, u8, u8),
    typ: (u8, u8, u8),
    red: (u8, u8, u8),
}

const DARK: Palette = Palette {
    fg: (0xd8, 0xdc, 0xe4),
    muted: (0x7a, 0x82, 0x90),
    h1: (0xc6, 0x9c, 0xf0),
    h2: (0x7a, 0xb4, 0xf0),
    h3: (0x6c, 0xc8, 0xc8),
    code: (0xe8, 0xc0, 0x7a),
    link: (0x7a, 0xb4, 0xf0),
    marker: (0x6c, 0xc8, 0xc8),
    quote: (0x8a, 0x9a, 0xb0),
    keyword: (0xc6, 0x9c, 0xf0),
    string: (0x9c, 0xcc, 0x84),
    number: (0xe8, 0xa0, 0x6c),
    function: (0x7a, 0xb4, 0xf0),
    typ: (0xe8, 0xc0, 0x7a),
    red: (0xe8, 0x7a, 0x84),
};

const LIGHT: Palette = Palette {
    fg: (0x28, 0x2c, 0x34),
    muted: (0x70, 0x76, 0x80),
    h1: (0x80, 0x30, 0xa8),
    h2: (0x20, 0x5c, 0xb8),
    h3: (0x10, 0x78, 0x78),
    code: (0x98, 0x58, 0x00),
    link: (0x20, 0x5c, 0xb8),
    marker: (0x10, 0x78, 0x78),
    quote: (0x50, 0x5c, 0x70),
    keyword: (0x80, 0x30, 0xa8),
    string: (0x38, 0x80, 0x20),
    number: (0xb0, 0x50, 0x10),
    function: (0x20, 0x5c, 0xb8),
    typ: (0x98, 0x58, 0x00),
    red: (0xc0, 0x28, 0x38),
};

/// Converts abstract styles to escape sequences for one output target.
#[derive(Clone, Copy, Debug)]
pub struct Painter {
    pub level: ColorLevel,
    /// Whether SGR attributes (bold, italic...) may be emitted at all.
    pub attrs: bool,
}

impl Painter {
    pub fn plain() -> Painter {
        Painter { level: ColorLevel::None, attrs: false }
    }

    pub fn enabled(&self) -> bool {
        self.attrs || self.level != ColorLevel::None
    }

    /// Append the SGR sequence that switches from `from` to `to`.
    pub fn transition(&self, out: &mut String, from: Style, to: Style) {
        if !self.enabled() || from == to {
            return;
        }
        out.push_str("\x1b[0");
        if self.attrs {
            for (bit, code) in [(BOLD, "1"), (DIM, "2"), (ITALIC, "3"), (UNDERLINE, "4"), (STRIKE, "9")] {
                if to.attrs & bit != 0 {
                    out.push(';');
                    out.push_str(code);
                }
            }
        }
        if let Some(c) = to.fg {
            self.push_color(out, c);
        }
        out.push('m');
    }

    fn push_color(&self, out: &mut String, c: Color) {
        match (self.level, c) {
            (ColorLevel::None, _) => {}
            (_, Color::Ansi(n)) => push_ansi16(out, n),
            (ColorLevel::Ansi16, Color::Rgb(r, g, b)) => push_ansi16(out, rgb_to_16(r, g, b)),
            (ColorLevel::Ansi256, Color::Rgb(r, g, b)) => {
                let _ = write!(out, ";38;5;{}", rgb_to_256(r, g, b));
            }
            (ColorLevel::TrueColor, Color::Rgb(r, g, b)) => {
                let _ = write!(out, ";38;2;{r};{g};{b}");
            }
        }
    }

    pub fn reset(&self, out: &mut String, current: Style) {
        if self.enabled() && current != Style::PLAIN {
            out.push_str("\x1b[0m");
        }
    }
}

fn push_ansi16(out: &mut String, n: u8) {
    let n = n & 15;
    let code = if n < 8 { 30 + n } else { 90 + n - 8 };
    let _ = write!(out, ";{code}");
}

const CUBE: [u8; 6] = [0, 95, 135, 175, 215, 255];

fn rgb_to_256(r: u8, g: u8, b: u8) -> u8 {
    let idx = |v: u8| -> usize {
        let v = v as i32;
        (0..6).min_by_key(|&i| (CUBE[i] as i32 - v).abs()).unwrap()
    };
    let (ri, gi, bi) = (idx(r), idx(g), idx(b));
    let cube_err = dist((r, g, b), (CUBE[ri], CUBE[gi], CUBE[bi]));
    // Gray ramp 232..=255: 8, 18, ..., 238
    let avg = (r as u32 + g as u32 + b as u32) / 3;
    let gi_ramp = ((avg.saturating_sub(3)) / 10).min(23) as u8;
    let gv = 8 + gi_ramp * 10;
    if dist((r, g, b), (gv, gv, gv)) < cube_err {
        232 + gi_ramp
    } else {
        16 + (36 * ri + 6 * gi + bi) as u8
    }
}

fn dist(a: (u8, u8, u8), b: (u8, u8, u8)) -> u32 {
    let d = |x: u8, y: u8| (x as i32 - y as i32).pow(2) as u32;
    d(a.0, b.0) + d(a.1, b.1) + d(a.2, b.2)
}

const ANSI16_RGB: [(u8, u8, u8); 16] = [
    (0, 0, 0),
    (205, 49, 49),
    (13, 188, 121),
    (229, 229, 16),
    (36, 114, 200),
    (188, 63, 188),
    (17, 168, 205),
    (229, 229, 229),
    (102, 102, 102),
    (241, 76, 76),
    (35, 209, 139),
    (245, 245, 67),
    (59, 142, 234),
    (214, 112, 214),
    (41, 184, 219),
    (255, 255, 255),
];

fn rgb_to_16(r: u8, g: u8, b: u8) -> u8 {
    // Nearest entry of a typical 16-color palette; the real one is unknown.
    (0..16u8).min_by_key(|&i| dist((r, g, b), ANSI16_RGB[i as usize])).unwrap()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sgr_levels() {
        let s = Style::fg(Color::Rgb(255, 0, 0)).with(BOLD);
        let mut out = String::new();
        Painter { level: ColorLevel::TrueColor, attrs: true }.transition(&mut out, Style::PLAIN, s);
        assert_eq!(out, "\x1b[0;1;38;2;255;0;0m");
        out.clear();
        Painter { level: ColorLevel::Ansi256, attrs: true }.transition(&mut out, Style::PLAIN, s);
        assert_eq!(out, "\x1b[0;1;38;5;196m");
        out.clear();
        Painter { level: ColorLevel::Ansi16, attrs: true }.transition(&mut out, Style::PLAIN, s);
        assert_eq!(out, "\x1b[0;1;31m");
        out.clear();
        Painter { level: ColorLevel::None, attrs: true }.transition(&mut out, Style::PLAIN, s);
        assert_eq!(out, "\x1b[0;1m");
        out.clear();
        Painter::plain().transition(&mut out, Style::PLAIN, s);
        assert_eq!(out, "");
    }

    #[test]
    fn gray_maps_to_ramp() {
        assert_eq!(rgb_to_256(128, 128, 128), 244);
        assert_eq!(rgb_to_256(0, 0, 0), 16);
    }
}
