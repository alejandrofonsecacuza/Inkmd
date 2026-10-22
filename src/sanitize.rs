//! Neutralization of control characters coming from untrusted documents.
//!
//! Every piece of text that originates in the document (or in file names)
//! goes through [`push_clean`] before being placed in the output. The only
//! escape sequences that reach the terminal are the ones the renderer builds
//! itself, so a document can never move the cursor, change the title, set
//! the palette, write to the clipboard (OSC 52) or similar.

/// Append `s` to `out`, replacing control characters with visible
/// placeholders. Tabs become a single space (callers that care about tab
/// stops expand them before calling this). Newlines are kept only when
/// `keep_newlines` is set.
pub fn push_clean(out: &mut String, s: &str, ascii: bool, keep_newlines: bool) {
    if !needs_cleaning(s) {
        out.push_str(s);
        return;
    }
    for c in s.chars() {
        match c {
            '\n' if keep_newlines => out.push('\n'),
            '\t' | '\n' | '\r' => out.push(' '),
            '\0'..='\x1f' | '\x7f' => {
                if ascii {
                    out.push('^');
                    out.push(if c == '\x7f' { '?' } else { (c as u8 + 0x40) as char });
                } else if c == '\x7f' {
                    out.push('\u{2421}');
                } else {
                    // Unicode "control pictures" block: ␛ for ESC, ␇ for BEL...
                    out.push(char::from_u32(0x2400 + c as u32).unwrap_or('\u{fffd}'));
                }
            }
            // C1 controls (CSI, OSC, DCS... as single code points).
            '\u{80}'..='\u{9f}' => out.push(if ascii { '?' } else { '\u{fffd}' }),
            _ => out.push(c),
        }
    }
}

pub fn clean(s: &str, ascii: bool) -> String {
    let mut out = String::with_capacity(s.len());
    push_clean(&mut out, s, ascii, false);
    out
}

#[inline]
fn needs_cleaning(s: &str) -> bool {
    // C1 controls are encoded as 0xC2 0x80..=0x9F in UTF-8.
    let b = s.as_bytes();
    b.iter()
        .enumerate()
        .any(|(i, &x)| x < 0x20 || x == 0x7f || (x == 0xc2 && matches!(b.get(i + 1), Some(0x80..=0x9f))))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn escapes_are_neutralized() {
        let evil = "a\x1b[31mred\x1b]0;title\x07\u{9b}2Jz";
        let out = clean(evil, false);
        assert!(!out.contains('\x1b'));
        assert!(!out.contains('\x07'));
        assert!(!out.contains('\u{9b}'));
        assert_eq!(out, "a\u{241b}[31mred\u{241b}]0;title\u{2407}\u{fffd}2Jz");
        assert_eq!(clean(evil, true), "a^[[31mred^[]0;title^G?2Jz");
    }

    #[test]
    fn clean_text_is_untouched() {
        assert_eq!(clean("héllo 世界 🎉", false), "héllo 世界 🎉");
    }
}
