//! A deliberately small HTML tokenizer for the subset of HTML found in
//! READMEs (`<br>`, `<img>`, `<kbd>`, `<details>`, alignment wrappers...).
//!
//! HTML is never executed or fully interpreted: tags are mapped to a few
//! text styles or structural hints, unknown tags are dropped while their
//! text is kept, and `<script>`/`<style>` content and comments are hidden.

#[derive(Debug, PartialEq, Eq)]
pub enum Token<'a> {
    Text(&'a str),
    Tag(Tag<'a>),
}

#[derive(Debug, PartialEq, Eq)]
pub struct Tag<'a> {
    /// Lowercased tag name.
    pub name: String,
    pub closing: bool,
    pub self_closing: bool,
    attrs: &'a str,
}

impl<'a> Tag<'a> {
    /// Value of attribute `key` (entity-decoded), if present.
    pub fn attr(&self, key: &str) -> Option<String> {
        let mut s = self.attrs;
        loop {
            s = s.trim_start_matches(|c: char| c.is_whitespace() || c == '/');
            if s.is_empty() {
                return None;
            }
            let name_end = s.find(|c: char| c.is_whitespace() || c == '=' || c == '/' || c == '>').unwrap_or(s.len());
            let name = &s[..name_end];
            s = s[name_end..].trim_start();
            let mut value = "";
            if let Some(rest) = s.strip_prefix('=') {
                let rest = rest.trim_start();
                if let Some(q @ ('"' | '\'')) = rest.chars().next() {
                    let body = &rest[1..];
                    let end = body.find(q).unwrap_or(body.len());
                    value = &body[..end];
                    s = body.get(end + 1..).unwrap_or("");
                } else {
                    let end = rest.find(|c: char| c.is_whitespace() || c == '>').unwrap_or(rest.len());
                    value = &rest[..end];
                    s = &rest[end..];
                }
            }
            if name.eq_ignore_ascii_case(key) {
                return Some(decode_entities(value));
            }
            if name.is_empty() {
                // Defensive: avoid looping on garbage.
                s = s.get(1..).unwrap_or("");
            }
        }
    }
}

/// Tokenizer state that persists across pulldown-cmark HTML events, since a
/// comment or `<script>` element can span several of them.
#[derive(Default)]
pub struct Tokenizer {
    in_comment: bool,
    /// Inside `<script>`, `<style>`, ...: skip until this closing tag.
    raw_until: Option<&'static str>,
}

impl Tokenizer {
    pub fn tokenize<'a>(&mut self, mut s: &'a str, out: &mut Vec<Token<'a>>) {
        while !s.is_empty() {
            if self.in_comment {
                match s.find("-->") {
                    Some(i) => {
                        s = &s[i + 3..];
                        self.in_comment = false;
                    }
                    None => return,
                }
                continue;
            }
            if let Some(end_tag) = self.raw_until {
                match find_ci(s, end_tag) {
                    Some(i) => {
                        s = &s[i..];
                        self.raw_until = None;
                    }
                    None => return,
                }
            }
            let Some(lt) = s.find('<') else {
                out.push(Token::Text(s));
                return;
            };
            if lt > 0 {
                out.push(Token::Text(&s[..lt]));
                s = &s[lt..];
            }
            if let Some(rest) = s.strip_prefix("<!--") {
                self.in_comment = true;
                s = rest;
                continue;
            }
            if s.starts_with("<!") || s.starts_with("<?") {
                // Doctype, CDATA, processing instruction: drop.
                s = s.find('>').map(|i| &s[i + 1..]).unwrap_or("");
                continue;
            }
            match parse_tag(s) {
                Some((tag, rest)) => {
                    if !tag.closing && !tag.self_closing {
                        self.raw_until = match tag.name.as_str() {
                            "script" => Some("</script"),
                            "style" => Some("</style"),
                            "textarea" => Some("</textarea"),
                            "template" => Some("</template"),
                            _ => None,
                        };
                    }
                    out.push(Token::Tag(tag));
                    s = rest;
                }
                None => {
                    // A lone '<' that is not a tag: it is text.
                    out.push(Token::Text("<"));
                    s = &s[1..];
                }
            }
        }
    }
}

fn find_ci(hay: &str, needle: &str) -> Option<usize> {
    let h = hay.as_bytes();
    let n = needle.as_bytes();
    (0..h.len().saturating_sub(n.len() - 1)).find(|&i| h[i..i + n.len()].eq_ignore_ascii_case(n))
}

fn parse_tag(s: &str) -> Option<(Tag<'_>, &str)> {
    let mut rest = s.strip_prefix('<')?;
    let closing = rest.starts_with('/');
    if closing {
        rest = &rest[1..];
    }
    let name_len = rest.find(|c: char| !(c.is_ascii_alphanumeric() || c == '-' || c == ':')).unwrap_or(rest.len());
    if name_len == 0 || !rest.as_bytes()[0].is_ascii_alphabetic() {
        return None;
    }
    let name = rest[..name_len].to_ascii_lowercase();
    rest = &rest[name_len..];
    // Find the closing '>' while respecting quoted attribute values.
    let mut quote = None;
    let mut end = None;
    for (i, c) in rest.char_indices() {
        match (quote, c) {
            (Some(q), c) if c == q => quote = None,
            (Some(_), _) => {}
            (None, '"' | '\'') => quote = Some(c),
            (None, '>') => {
                end = Some(i);
                break;
            }
            _ => {}
        }
    }
    let end = end?;
    let attrs = &rest[..end];
    let self_closing = attrs.trim_end().ends_with('/') || is_void(&name);
    Some((Tag { name, closing, self_closing, attrs }, &rest[end + 1..]))
}

fn is_void(name: &str) -> bool {
    matches!(name, "br" | "img" | "hr" | "input" | "meta" | "link" | "source" | "wbr" | "area" | "col" | "embed")
}

/// Block-level tags: they start a new line in the output.
pub fn is_block(name: &str) -> bool {
    matches!(
        name,
        "p" | "div"
            | "center"
            | "section"
            | "article"
            | "header"
            | "footer"
            | "nav"
            | "main"
            | "aside"
            | "h1"
            | "h2"
            | "h3"
            | "h4"
            | "h5"
            | "h6"
            | "ul"
            | "ol"
            | "li"
            | "table"
            | "thead"
            | "tbody"
            | "tr"
            | "details"
            | "summary"
            | "blockquote"
            | "pre"
            | "figure"
            | "figcaption"
            | "dl"
            | "dt"
            | "dd"
            | "picture"
            | "video"
            | "audio"
    )
}

/// Decode the HTML entities that commonly appear in Markdown documents.
/// Unknown named entities are left as written.
pub fn decode_entities(s: &str) -> String {
    if !s.contains('&') {
        return s.to_string();
    }
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(i) = rest.find('&') {
        out.push_str(&rest[..i]);
        rest = &rest[i..];
        let end = rest.find(';').filter(|&e| e <= 10);
        let decoded = end.and_then(|e| decode_one(&rest[1..e]));
        match (end, decoded) {
            (Some(e), Some(c)) => {
                out.push(c);
                rest = &rest[e + 1..];
            }
            _ => {
                out.push('&');
                rest = &rest[1..];
            }
        }
    }
    out.push_str(rest);
    out
}

fn decode_one(name: &str) -> Option<char> {
    if let Some(num) = name.strip_prefix('#') {
        let n = match num.strip_prefix(['x', 'X']) {
            Some(hex) => u32::from_str_radix(hex, 16).ok()?,
            None => num.parse().ok()?,
        };
        // Control characters are rejected here; the sanitizer would catch them anyway.
        return char::from_u32(n).filter(|c| !c.is_control() || *c == '\n');
    }
    Some(match name {
        "amp" => '&',
        "lt" => '<',
        "gt" => '>',
        "quot" => '"',
        "apos" => '\'',
        "nbsp" => '\u{a0}',
        "ensp" | "emsp" | "thinsp" => ' ',
        "copy" => '©',
        "reg" => '®',
        "trade" => '™',
        "mdash" => '—',
        "ndash" => '–',
        "hellip" => '…',
        "middot" => '·',
        "bull" => '•',
        "times" => '×',
        "divide" => '÷',
        "deg" => '°',
        "plusmn" => '±',
        "rarr" => '→',
        "larr" => '←',
        "uarr" => '↑',
        "darr" => '↓',
        "harr" => '↔',
        "laquo" => '«',
        "raquo" => '»',
        "lsquo" => '‘',
        "rsquo" => '’',
        "ldquo" => '“',
        "rdquo" => '”',
        "check" => '✓',
        "hearts" => '♥',
        "star" => '☆',
        "euro" => '€',
        "pound" => '£',
        "yen" => '¥',
        "sect" => '§',
        "para" => '¶',
        "zwj" => '\u{200d}',
        "zwnj" => '\u{200c}',
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn toks(s: &str) -> Vec<String> {
        let mut t = Tokenizer::default();
        let mut v = Vec::new();
        t.tokenize(s, &mut v);
        v.iter()
            .map(|t| match t {
                Token::Text(s) => format!("T:{s}"),
                Token::Tag(t) => format!("{}{}", if t.closing { "/" } else { "" }, t.name),
            })
            .collect()
    }

    #[test]
    fn tokenizes() {
        assert_eq!(toks("<p align=\"center\">Hi <b>x</b></p>"), ["p", "T:Hi ", "b", "T:x", "/b", "/p"]);
        assert_eq!(toks("a <!-- hidden --> b"), ["T:a ", "T: b"]);
        assert_eq!(toks("<script>alert('<b>')</script>after"), ["script", "/script", "T:after"]);
        assert_eq!(toks("1 < 2"), ["T:1 ", "T:<", "T: 2"]);
    }

    #[test]
    fn attributes() {
        let mut v = Vec::new();
        Tokenizer::default().tokenize("<img src='a b.png' alt=\"A &amp; B\" width=100/>", &mut v);
        let Token::Tag(t) = &v[0] else { panic!() };
        assert_eq!(t.attr("src").as_deref(), Some("a b.png"));
        assert_eq!(t.attr("alt").as_deref(), Some("A & B"));
        assert_eq!(t.attr("width").as_deref(), Some("100/"));
        assert!(t.self_closing);
        assert_eq!(t.attr("href"), None);
    }

    #[test]
    fn entities() {
        assert_eq!(decode_entities("a &amp; b &#x41;&#66; &bogus; &"), "a & b AB &bogus; &");
        assert_eq!(decode_entities("&#27;"), "&#27;");
    }
}
