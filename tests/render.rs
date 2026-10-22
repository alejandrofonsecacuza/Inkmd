//! Behavioural tests per Markdown element, on plain and styled output.

use inkmd::{render_to_string, ColorLevel, Options, Painter};

fn plain(md: &str) -> String {
    render_to_string(md, &Options::default())
}

fn plain_w(md: &str, width: usize) -> String {
    render_to_string(md, &Options { width, ..Options::default() })
}

fn styled(md: &str) -> String {
    render_to_string(md, &Options { painter: Painter { level: ColorLevel::Ansi16, attrs: true }, ..Options::default() })
}

fn no_color_attrs(md: &str) -> String {
    render_to_string(md, &Options { painter: Painter { level: ColorLevel::None, attrs: true }, ..Options::default() })
}

#[test]
fn headings_all_levels() {
    let out = plain("# a\n## b\n### c\n#### d\n##### e\n###### f\n");
    assert_eq!(out, "# a\n\n## b\n\n### c\n\n#### d\n\n##### e\n\n###### f\n");
    // With styles, markers are replaced by styles; H1 gets an underline rule.
    let s = styled("# Title\n\n## Sub\n");
    assert_eq!(s, "\x1b[0;1;35mTitle\x1b[0m\n\x1b[0;35m━━━━━\x1b[0m\n\n\x1b[0;1;34mSub\x1b[0m\n");
    assert!(!no_color_attrs("### x").contains('#'));
}

#[test]
fn heading_wraps_with_hanging_indent() {
    let out = plain_w("## one two three four five six", 16);
    assert_eq!(out, "## one two three\n   four five six\n");
}

#[test]
fn paragraphs_and_breaks() {
    assert_eq!(plain("a\nb\n\nc"), "a b\n\nc\n");
    assert_eq!(plain("a  \nb\\\nc"), "a\nb\nc\n");
}

#[test]
fn emphasis_styles() {
    let s = styled("*i* **b** ***bi*** ~~s~~ `c`");
    assert!(s.contains("\x1b[0;3mi"));
    assert!(s.contains("\x1b[0;1mb"));
    assert!(s.contains("\x1b[0;1;3mbi"));
    assert!(s.contains("\x1b[0;9ms"));
    assert!(s.contains("\x1b[0;33mc"));
    // Without any styling, information that would be lost is kept textually.
    assert_eq!(plain("~~s~~ `c`"), "~~s~~ `c`\n");
    // Without colors but with attributes, code still gets backticks.
    assert!(no_color_attrs("`c`").contains("`c`"));
    assert!(no_color_attrs("~~s~~").contains("\x1b[0;9ms"));
}

#[test]
fn styles_never_leak_past_line_end() {
    let s = render_to_string(
        "**bold text that wraps across several lines of output**",
        &Options { width: 12, painter: Painter { level: ColorLevel::Ansi16, attrs: true }, ..Options::default() },
    );
    for line in s.lines() {
        assert!(line.ends_with("\x1b[0m"), "{line:?}");
        assert!(line.starts_with("\x1b[0;1m"), "{line:?}");
    }
}

#[test]
fn unordered_nested_lists() {
    assert_eq!(plain("- a\n  - b\n    - c\n      - d"), "• a\n  ◦ b\n    ▪ c\n      • d\n");
}

#[test]
fn ordered_lists_keep_start_and_alignment() {
    assert_eq!(plain("3. a\n4. b"), "3. a\n4. b\n");
    let out = plain_w("1. one two three four", 10);
    assert_eq!(out, "1. one two\n   three\n   four\n");
}

#[test]
fn loose_lists_have_blank_lines() {
    assert_eq!(plain("- a\n\n- b"), "• a\n\n• b\n");
}

#[test]
fn task_lists() {
    assert_eq!(plain("- [ ] todo\n- [x] done"), "☐ todo\n☑ done\n");
    let ascii = render_to_string("- [ ] todo\n- [x] done", &Options { ascii: true, ..Options::default() });
    assert_eq!(ascii, "[ ] todo\n[x] done\n");
}

#[test]
fn blockquotes_nested_and_lazy() {
    assert_eq!(plain("> a\n> > b"), "│ a\n│\n│ │ b\n");
    assert_eq!(plain("> a\ncontinued"), "│ a continued\n");
}

#[test]
fn quote_contains_blocks() {
    let out = plain("> # H\n> - item\n> ```\n> code\n> ```");
    assert_eq!(out, "│ # H\n│\n│ • item\n│\n│     code\n");
}

#[test]
fn gfm_alerts() {
    assert_eq!(plain("> [!NOTE]\n> text"), "│ Note\n│ text\n");
    assert_eq!(plain("> [!WARNING]\n> text"), "│ Warning\n│ text\n");
}

#[test]
fn horizontal_rule_spans_width() {
    assert_eq!(plain_w("---", 20), format!("{}\n", "─".repeat(20)));
    assert_eq!(plain_w("> ***", 20), format!("│ {}\n", "─".repeat(18)));
}

#[test]
fn code_blocks() {
    assert_eq!(plain("```\nx = 1\n```"), "    x = 1\n");
    assert_eq!(plain("```rust\nfn x() {}\n```"), "    [rust]\n    fn x() {}\n");
    assert_eq!(plain("    indented"), "    indented\n");
    // Blank lines inside code are preserved.
    assert_eq!(plain("```\na\n\nb\n```"), "    a\n\n    b\n");
    // Leading whitespace is significant in code.
    assert_eq!(plain("```\n  x\n```"), "      x\n");
}

#[test]
fn unknown_language_is_not_highlighted() {
    let s = styled("```nosuchlang\nfoo bar\n```");
    assert!(s.contains("    foo bar\n"), "{s:?}");
}

#[cfg(feature = "highlight")]
#[test]
fn known_language_is_highlighted() {
    let s = styled("```rust\nfn main() { let x = \"s\"; }\n```");
    assert!(s.contains("\x1b[0;35mfn"), "{s:?}");
    assert!(s.contains("\x1b[0;32m\"s\""), "{s:?}");
}

#[test]
fn long_code_lines_wrap_with_marker() {
    let out = plain_w(&format!("```\n{}\n```", "x".repeat(40)), 24);
    assert_eq!(out, format!("  {}\n  ↪ {}\n", "x".repeat(22), "x".repeat(18)));
}

#[test]
fn tabs_in_code_are_expanded() {
    assert_eq!(plain("```\n\tx\n```"), "        x\n");
}

#[test]
fn tables_alignment() {
    let out = plain("| a | b | c |\n|:--|:-:|--:|\n| 1 | 2 | 3 |\n| xxx | yyy | zzz |");
    assert_eq!(out, "a   │  b  │   c\n────┼─────┼────\n1   │  2  │   3\nxxx │ yyy │ zzz\n");
}

#[test]
fn tables_wrap_cells_when_too_wide() {
    let md = "| k | v |\n|---|---|\n| key | a long value that needs to wrap |";
    let out = plain_w(md, 20);
    for l in out.lines() {
        assert!(inkmd::wrap::width(l) <= 20, "{l}");
    }
    assert!(out.contains("key │ a long value"));
}

#[test]
fn tables_fall_back_to_records_when_very_narrow() {
    let md = "| one | two | three | four |\n|---|---|---|---|\n| aaaaa | bbbbb | ccccc | ddddd |";
    let out = plain_w(md, 12);
    assert_eq!(out, "one: aaaaa\ntwo: bbbbb\nthree: ccccc\nfour: ddddd\n");
}

#[test]
fn table_with_cjk_is_aligned() {
    let out = plain("| a | b |\n|---|---|\n| 日本 | x |\n| y | z |");
    let lines: Vec<_> = out.lines().collect();
    let pos: Vec<usize> = lines.iter().map(|l| inkmd::wrap::width(&l[..l.find(['│', '┼']).unwrap()])).collect();
    assert!(pos.windows(2).all(|w| w[0] == w[1]), "{out}");
}

#[test]
fn links() {
    assert_eq!(plain("[text](https://a.b)"), "text (https://a.b)\n");
    assert_eq!(plain("<https://a.b>"), "https://a.b\n");
    assert_eq!(plain("[https://a.b](https://a.b)"), "https://a.b\n");
    assert_eq!(plain("[a.b](https://a.b/)"), "a.b\n");
    assert_eq!(plain("[go](#anchor)"), "go\n");
    assert_eq!(plain("[r][x]\n\n[x]: /ref"), "r (/ref)\n");
    assert_eq!(plain("<me@x.org>"), "me@x.org\n");
    assert!(styled("[t](u)").contains("\x1b[0;4;34mt"));
}

#[test]
fn bare_urls_are_styled_as_links() {
    let s = styled("visit https://example.com/path.");
    assert!(s.contains("\x1b[0;4;34mhttps://example.com/path\x1b[0m."), "{s:?}");
}

#[test]
fn images_fallback() {
    assert_eq!(plain("![alt](a.png)"), "[image: alt] (a.png)\n");
    assert_eq!(plain("![](a.png)"), "[image] (a.png)\n");
    assert_eq!(plain("![x *y*](a.png \"t\")"), "[image: x y] (a.png)\n");
    // Badge: image inside a link shows the link target only.
    assert_eq!(plain("[![ci](https://b/x.svg)](https://ci)"), "[image: ci] (https://ci)\n");
}

#[test]
fn footnotes() {
    let out = plain("a[^1]\n\n[^1]: note");
    assert_eq!(out, "a[1]\n\n[1] note\n");
}

#[test]
fn definition_lists() {
    assert_eq!(plain("Term\n: def one\n: def two"), "Term\n    def one\n    def two\n");
}

#[test]
fn math_is_converted() {
    assert_eq!(plain("$x^2$"), "x²\n");
    assert_eq!(plain("$$\\frac{a}{b}$$"), "a/b\n");
}

#[test]
fn front_matter() {
    assert_eq!(plain("---\ntitle: x\n---\n\n# H"), "    [front matter]\n    title: x\n\n# H\n");
}

#[test]
fn html_policy() {
    assert_eq!(plain("a<br>b"), "a\nb\n");
    assert_eq!(plain("<!-- hidden -->\n\nvisible"), "visible\n");
    assert_eq!(plain("<script>evil()</script>\n\nok"), "ok\n");
    assert_eq!(plain("<div>\n<b>x</b> &amp; y\n</div>"), "x & y\n");
    assert_eq!(plain("<img src=\"a.png\" alt=\"A\">"), "[image: A] (a.png)\n");
    assert_eq!(plain("<details><summary>More</summary>body</details>"), "▸ More\nbody\n");
    assert!(styled("<kbd>K</kbd>").contains("\x1b[0;33mK"));
}

#[test]
fn unclosed_html_style_does_not_leak() {
    let s = styled("<b>bold\n\nnext paragraph");
    let next = s.lines().find(|l| l.contains("next")).unwrap();
    assert_eq!(next, "next paragraph");
}

#[test]
fn entities_and_escapes() {
    assert_eq!(plain("&copy; &amp; \\*x\\*"), "© & *x*\n");
}

#[test]
fn unicode_width_wrapping() {
    let out = plain_w("日本語のテキストを折り返す", 10);
    for l in out.lines() {
        assert!(inkmd::wrap::width(l) <= 10);
    }
    assert_eq!(out.replace('\n', ""), "日本語のテキストを折り返す");
}

#[test]
fn control_characters_are_neutralized_everywhere() {
    let e = '\x1b';
    let md = format!(
        "# t{e}[2J\n\np{e}]0;x\x07\n\n`c{e}[1m`\n\n```\nb{e}[H\n```\n\n| h{e}[3m |\n|--|\n| c{e}[4m |\n\n[l{e}c](u)\n\n![i{e}x](p)\n\n<b>h{e}t</b>\n\n[^f]\n\n[^f]: n{e}x\n"
    );
    for out in [plain(&md), styled(&md)] {
        let stripped = out.replace("\x1b[0m", "").replace("\x1b[0;", "");
        assert!(!stripped.contains('\x1b'), "{out:?}");
        assert!(!out.contains('\x07'));
    }
}

#[test]
fn empty_and_whitespace_documents() {
    assert_eq!(plain(""), "");
    assert_eq!(plain("\n\n   \n"), "");
}

#[test]
fn narrow_widths_never_panic() {
    let md = include_str!("fixtures/showcase.md");
    for w in 1..40 {
        let _ = plain_w(md, w);
    }
}

#[test]
fn deeply_nested_structures_are_bounded() {
    let md = "> ".repeat(200) + "deep";
    let out = plain_w(&md, 40);
    assert!(out.contains("deep"));
    let md: String = (0..100).map(|i| format!("{}- item\n", "  ".repeat(i))).collect();
    let out = plain_w(&md, 40);
    assert_eq!(out.matches("item").count(), 100);
}

#[test]
fn big_headings_use_text_sizing_protocol() {
    let opts = Options {
        painter: Painter { level: ColorLevel::Ansi16, attrs: true },
        big_headings: inkmd::BigHeadings::Always,
        width: 20,
        ..Options::default()
    };
    let out = render_to_string("# Big title here\n\n## Small\n\n> # Quoted", &opts);
    // Wrapped at half the width, two rows per scaled line.
    assert!(out.starts_with("\x1b[0;1;35m\x1b]66;s=2;Big title\x07\x1b[0m\n\n"), "{out:?}");
    assert!(out.contains("\x1b]66;s=2;here\x07"));
    // Lower levels shrink: H2 at 1.75× in 8-column chunks, H5 and H6 normal.
    assert!(out.contains("\x1b[0;1;34m\x1b]66;s=2:n=7:d=8:v=2:w=5;Small\x07"), "{out:?}");
    let lower = render_to_string("### Level three\n\n##### Five", &opts);
    assert!(lower.contains("\x1b]66;s=2:n=3:d=4:v=2:w=6;Level th\x07\x1b]66;s=2:n=3:d=4:v=2:w=3;ree\x07"), "{lower:?}");
    assert!(lower.contains("\x1b[0;1;3mFive\x1b[0m"), "{lower:?}");
    // Prefixes are repeated on both rows.
    assert!(out.contains("\x1b[0;2;36m│\x1b[0m \x1b[0;1;35m\x1b]66;s=2;Quoted"), "{out:?}");
    assert!(out.contains("Quoted\x07\x1b[0m\n\x1b[0;2;36m│\x1b[0m\n"), "{out:?}");
    // Never without styles.
    let plain = render_to_string("# T", &Options { big_headings: inkmd::BigHeadings::Always, ..Options::default() });
    assert_eq!(plain, "# T\n");
}
