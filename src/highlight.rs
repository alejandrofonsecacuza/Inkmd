//! Syntax highlighting for fenced code blocks.
//!
//! Grammars come from syntect + two-face (bat's grammar set). They are only
//! deserialized the first time a document contains a code block with a
//! language tag, so documents without code pay nothing.
//!
//! Instead of using a syntect color theme, a tiny synthetic theme maps
//! scopes to semantic slots ([`Syn`]); the renderer's own theme then picks
//! the actual colors, which keeps code consistent with the rest of the
//! output and makes it work with 16-color terminals.

use crate::style::Syn;

/// Code larger than this is shown without highlighting: regex grammars get
/// slow on huge inputs and a viewer must stay responsive.
const MAX_BYTES: usize = 256 * 1024;
const MAX_LINE: usize = 4096;

#[cfg(feature = "highlight")]
mod imp {
    use super::*;
    use std::str::FromStr;
    use syntect::easy::HighlightLines;
    use syntect::highlighting::{Color, ScopeSelectors, StyleModifier, Theme, ThemeItem, ThemeSettings};
    use syntect::parsing::{SyntaxReference, SyntaxSet};

    pub struct Highlighter {
        syntaxes: SyntaxSet,
        theme: Theme,
    }

    const RULES: &[(&str, Syn)] = &[
        ("comment, punctuation.definition.comment", Syn::Comment),
        ("string, constant.character.escape, punctuation.definition.string", Syn::String),
        ("constant.numeric, constant.language, constant.character, constant.other, support.constant", Syn::Number),
        ("keyword, storage, keyword.operator.word, entity.name.tag, markup.heading", Syn::Keyword),
        ("keyword.operator, punctuation.separator", Syn::Plain),
        ("entity.name.function, support.function, meta.function-call variable.function, variable.function", Syn::Function),
        (
            "entity.name.type, entity.name.class, entity.name.struct, entity.name.enum, entity.name.trait, \
             support.type, support.class, storage.type.class, entity.other.attribute-name, entity.other.inherited-class",
            Syn::Type,
        ),
        ("markup.inserted, meta.diff.header.to-file", Syn::Inserted),
        ("markup.deleted, meta.diff.header.from-file", Syn::Deleted),
    ];

    fn marker(s: Syn) -> Color {
        Color { r: s as u8, g: 0, b: 0, a: 255 }
    }

    impl Default for Highlighter {
        fn default() -> Self {
            Self::new()
        }
    }

    impl Highlighter {
        pub fn new() -> Highlighter {
            let scopes = RULES.iter().filter_map(|(sel, syn)| {
                Some(ThemeItem {
                    scope: ScopeSelectors::from_str(sel).ok()?,
                    style: StyleModifier { foreground: Some(marker(*syn)), background: None, font_style: None },
                })
            });
            let theme = Theme {
                settings: ThemeSettings { foreground: Some(marker(Syn::Plain)), ..Default::default() },
                scopes: scopes.collect(),
                ..Default::default()
            };
            Highlighter { syntaxes: two_face::syntax::extra_newlines(), theme }
        }

        fn find(&self, lang: &str) -> Option<&SyntaxReference> {
            let lang = lang.to_ascii_lowercase();
            let lang = match lang.as_str() {
                "shell" | "console" | "zsh" | "sh" | "shellscript" => "bash",
                "jsonc" | "json5" => "json",
                "py3" | "python3" => "python",
                "golang" => "go",
                "c++" => "cpp",
                "objc" => "objective-c",
                "text" | "txt" | "plain" | "plaintext" | "none" => return None,
                other => other,
            };
            self.syntaxes.find_syntax_by_token(lang)
        }

        /// Highlight `code`, calling `emit` with `Some((slot, text))` for every
        /// piece and `None` at the end of every line. Returns false (without
        /// having emitted anything) if the language is unknown or the input
        /// too big.
        pub fn highlight(&self, lang: &str, code: &str, mut emit: impl FnMut(Option<(Syn, &str)>)) -> bool {
            let Some(syntax) = self.find(lang) else { return false };
            if code.len() > MAX_BYTES || code.lines().any(|l| l.len() > MAX_LINE) {
                return false;
            }
            let mut h = HighlightLines::new(syntax, &self.theme);
            // Highlight everything first so a mid-block failure falls back cleanly.
            let mut pieces: Vec<(Syn, &str, bool)> = Vec::new();
            for line in code.split_inclusive('\n') {
                let Ok(ranges) = h.highlight_line(line, &self.syntaxes) else { return false };
                for (style, text) in ranges {
                    let text = text.strip_suffix('\n').unwrap_or(text);
                    if !text.is_empty() {
                        pieces.push((Syn::from_u8(style.foreground.r), text, false));
                    }
                }
                pieces.push((Syn::Plain, "", true));
            }
            for (syn, text, nl) in pieces {
                emit((!nl).then_some((syn, text)));
            }
            true
        }
    }
}

#[cfg(not(feature = "highlight"))]
mod imp {
    use super::*;
    #[derive(Default)]
    pub struct Highlighter;
    impl Highlighter {
        pub fn new() -> Highlighter {
            let _ = (MAX_BYTES, MAX_LINE);
            Highlighter
        }
        pub fn highlight(&self, _: &str, _: &str, _: impl FnMut(Option<(Syn, &str)>)) -> bool {
            false
        }
    }
}

pub use imp::Highlighter;

#[cfg(all(test, feature = "highlight"))]
mod tests {
    #[test]
    fn common_languages_are_available() {
        let h = super::Highlighter::new();
        for lang in ["rust", "python", "typescript", "toml", "dockerfile", "yaml", "sh", "c++", "go", "json", "diff"] {
            assert!(h.highlight(lang, "x\n", |_| {}), "{lang}");
        }
    }
}
