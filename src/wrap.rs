//! Styled lines and width-aware word wrapping.
//!
//! Widths are measured per grapheme cluster with `unicode-width`, so wide
//! (CJK, emoji) and zero-width (combining marks, ZWJ) characters are handled,
//! and a cluster is never split across lines.

use crate::style::{Painter, Style};
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

/// A run of text with uniform style inside a [`Line`].
#[derive(Clone, Debug, Default)]
pub struct Line {
    pub text: String,
    /// `(byte offset, style)` pairs; each style applies until the next offset.
    pub runs: Vec<(usize, Style)>,
    pub width: usize,
}

impl Line {
    pub fn new() -> Line {
        Line::default()
    }

    pub fn from(style: Style, s: &str) -> Line {
        let mut l = Line::new();
        l.push(style, s);
        l
    }

    pub fn is_empty(&self) -> bool {
        self.text.is_empty()
    }

    /// Append already-sanitized text, measuring its width.
    pub fn push(&mut self, style: Style, s: &str) {
        let w = width(s);
        self.push_w(style, s, w);
    }

    pub fn push_w(&mut self, style: Style, s: &str, w: usize) {
        if s.is_empty() {
            return;
        }
        if self.runs.last().map(|r| r.1) != Some(style) {
            self.runs.push((self.text.len(), style));
        }
        self.text.push_str(s);
        self.width += w;
    }

    pub fn push_spaces(&mut self, n: usize) {
        if n > 0 {
            let style = self.runs.last().map(|r| r.1).unwrap_or_default();
            // Padding must not carry underline/strike across empty space.
            let pad = Style { fg: style.fg, attrs: 0 };
            let pad = if style == Style::PLAIN { style } else { pad };
            self.push_w(pad, &" ".repeat(n), n);
        }
    }

    pub fn append(&mut self, other: &Line) {
        for (i, (start, style)) in other.runs.iter().enumerate() {
            let end = other.runs.get(i + 1).map(|r| r.0).unwrap_or(other.text.len());
            let s = &other.text[*start..end];
            if self.runs.last().map(|r| r.1) != Some(*style) {
                self.runs.push((self.text.len(), *style));
            }
            self.text.push_str(s);
        }
        self.width += other.width;
    }

    /// Iterate `(style, text)` segments.
    pub fn segments(&self) -> impl Iterator<Item = (Style, &str)> {
        self.runs.iter().enumerate().map(move |(i, (start, style))| {
            let end = self.runs.get(i + 1).map(|r| r.0).unwrap_or(self.text.len());
            (*style, &self.text[*start..end])
        })
    }

    /// Write the line (without newline) followed by a style reset.
    pub fn emit(&self, out: &mut String, painter: &Painter) {
        let mut cur = Style::PLAIN;
        for (style, s) in self.segments() {
            painter.transition(out, cur, style);
            cur = style;
            out.push_str(s);
        }
        painter.reset(out, cur);
    }

    /// Remove trailing spaces (keeps blank-line prefixes tidy).
    pub fn trim_end(&mut self) {
        let trimmed = self.text.trim_end_matches(' ').len();
        if trimmed < self.text.len() {
            self.width -= self.text.len() - trimmed;
            self.text.truncate(trimmed);
            while self.runs.last().is_some_and(|r| r.0 >= trimmed) {
                self.runs.pop();
            }
        }
    }
}

#[inline]
pub fn width(s: &str) -> usize {
    if s.is_ascii() {
        s.len()
    } else {
        s.graphemes(true).map(UnicodeWidthStr::width).sum()
    }
}

/// Inline content of a paragraph-like block.
#[derive(Clone, Debug)]
pub enum Inline {
    Text(Style, String),
    /// Forced line break.
    Break,
    /// A block-level object (image) placed in the flow; the index refers to
    /// a list kept by the caller.
    Block(usize),
}

#[derive(Debug)]
pub enum Wrapped {
    Line(Line),
    Block(usize),
}

/// Greedy word wrap of inline content into lines of at most `width` columns.
pub fn wrap(items: &[Inline], width: usize) -> Vec<Wrapped> {
    let mut w =
        Wrapper { width: width.max(1), out: Vec::new(), line: Line::new(), word: Line::new(), word_space: None };
    let mut pending_space: Option<Style> = None;
    for item in items {
        match item {
            Inline::Text(style, s) => {
                let style = *style;
                if s.is_ascii() {
                    let b = s.as_bytes();
                    let mut start = 0;
                    for (i, &c) in b.iter().enumerate() {
                        if c == b' ' || c == b'\n' {
                            if start < i {
                                w.word_push(&s[start..i], i - start, style, &mut pending_space);
                            }
                            w.flush_word();
                            pending_space = Some(style);
                            start = i + 1;
                        }
                    }
                    if start < b.len() {
                        w.word_push(&s[start..], b.len() - start, style, &mut pending_space);
                    }
                } else {
                    let mut break_after = false;
                    for g in s.graphemes(true) {
                        if g == " " || g == "\n" || g == "\u{3000}" {
                            w.flush_word();
                            pending_space = Some(style);
                            break_after = false;
                            continue;
                        }
                        let gw = UnicodeWidthStr::width(g);
                        if break_after && gw > 0 {
                            // Wide characters (CJK) allow a line break between them.
                            w.flush_word();
                        }
                        w.word_push(g, gw, style, &mut pending_space);
                        break_after = gw >= 2;
                    }
                }
            }
            Inline::Break => {
                w.flush_word();
                w.finish_line(true);
                pending_space = None;
            }
            Inline::Block(id) => {
                w.flush_word();
                if !w.line.is_empty() {
                    w.finish_line(false);
                }
                w.out.push(Wrapped::Block(*id));
                pending_space = None;
            }
        }
    }
    w.flush_word();
    if !w.line.is_empty() {
        w.finish_line(false);
    }
    w.out
}

struct Wrapper {
    width: usize,
    out: Vec<Wrapped>,
    line: Line,
    word: Line,
    /// Style of the space preceding the current word, if any.
    word_space: Option<Style>,
}

impl Wrapper {
    fn word_push(&mut self, s: &str, w: usize, style: Style, pending_space: &mut Option<Style>) {
        if self.word.is_empty() {
            self.word_space = pending_space.take();
        }
        self.word.push_w(style, s, w);
    }

    fn finish_line(&mut self, force: bool) {
        let mut line = std::mem::take(&mut self.line);
        line.trim_end();
        if force || !line.is_empty() {
            self.out.push(Wrapped::Line(line));
        }
    }

    fn flush_word(&mut self) {
        if self.word.is_empty() {
            return;
        }
        let word = std::mem::take(&mut self.word);
        let space = self.word_space.take();
        let sep = usize::from(space.is_some() && !self.line.is_empty());
        if !self.line.is_empty() && self.line.width + sep + word.width > self.width {
            self.finish_line(false);
        } else if sep == 1 {
            self.line.push_w(space.unwrap(), " ", 1);
        }
        if self.line.width + word.width <= self.width {
            self.line.append(&word);
        } else {
            // Longer than a full line: split at grapheme boundaries.
            for (style, s) in word.segments() {
                for g in s.graphemes(true) {
                    let gw = UnicodeWidthStr::width(g);
                    if self.line.width + gw > self.width && !self.line.is_empty() {
                        self.finish_line(false);
                    }
                    self.line.push_w(style, g, gw);
                }
            }
        }
        self.word = word;
        self.word.text.clear();
        self.word.runs.clear();
        self.word.width = 0;
    }
}

/// Split a single (unwrappable) line into chunks of at most `width` columns.
/// Used for code, where whitespace is significant.
pub fn hard_wrap(line: &Line, width: usize) -> Vec<Line> {
    if line.width <= width {
        return vec![line.clone()];
    }
    let width = width.max(1);
    let mut out = Vec::new();
    let mut cur = Line::new();
    for (style, s) in line.segments() {
        for g in s.graphemes(true) {
            let gw = UnicodeWidthStr::width(g);
            if cur.width + gw > width && !cur.is_empty() {
                out.push(std::mem::take(&mut cur));
            }
            cur.push_w(style, g, gw);
        }
    }
    if !cur.is_empty() {
        out.push(cur);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn texts(items: &[Inline], w: usize) -> Vec<String> {
        wrap(items, w)
            .into_iter()
            .map(|x| match x {
                Wrapped::Line(l) => l.text,
                Wrapped::Block(i) => format!("<block {i}>"),
            })
            .collect()
    }

    fn t(s: &str) -> Inline {
        Inline::Text(Style::PLAIN, s.to_string())
    }

    #[test]
    fn greedy_wrap() {
        assert_eq!(texts(&[t("the quick brown fox jumps")], 10), ["the quick", "brown fox", "jumps"]);
    }

    #[test]
    fn words_across_styles_stay_together() {
        let items = [t("aaaa "), Inline::Text(Style::attrs(1), "bb".into()), t("cc dd")];
        assert_eq!(texts(&items, 6), ["aaaa", "bbcc", "dd"]);
    }

    #[test]
    fn long_words_are_split() {
        assert_eq!(texts(&[t("ab abcdefghij")], 4), ["ab", "abcd", "efgh", "ij"]);
    }

    #[test]
    fn cjk_breaks_between_characters() {
        let out = texts(&[t("日本語のテキスト")], 6);
        assert_eq!(out, ["日本語", "のテキ", "スト"]);
        for l in &out {
            assert!(width(l) <= 6);
        }
    }

    #[test]
    fn combining_marks_and_emoji_are_not_split() {
        let s = "e\u{301}e\u{301}e\u{301}";
        assert_eq!(width(s), 3);
        assert_eq!(texts(&[t(s)], 2), ["e\u{301}e\u{301}", "e\u{301}"]);
        let family = "👨‍👩‍👧";
        assert_eq!(width(family), 2);
        assert_eq!(texts(&[t("👨‍👩‍👧👨‍👩‍👧")], 3), [family, family]);
    }

    #[test]
    fn hard_break_and_blocks() {
        let items = [t("a"), Inline::Break, t("b "), Inline::Block(0), t("c")];
        assert_eq!(texts(&items, 10), ["a", "b", "<block 0>", "c"]);
    }
}
