//! Deterministic Markdown corpus for benchmarks (shared by the criterion
//! bench and the `gendoc` example used by scripts/bench.sh).

const PROSE: &str = "Lorem ipsum dolor sit amet, **consectetur** adipiscing elit, sed do *eiusmod* tempor \
incididunt ut labore et dolore magna aliqua. Ut enim ad minim veniam, quis `nostrud` exercitation \
ullamco laboris nisi ut [aliquip](https://example.com/aliquip) ex ea commodo consequat.";

const CODE: &str = "```rust\nfn fibonacci(n: u64) -> u64 {\n    // Naive recursion.\n    match n {\n        0 | 1 => n,\n        _ => fibonacci(n - 1) + fibonacci(n - 2),\n    }\n}\n```\n";

const TABLE: &str = "| Name | Type | Default | Description |\n|------|:----:|--------:|-------------|\n\
| `width` | number | 80 | Rendering width in columns, wraps long paragraphs |\n\
| `color` | enum | auto | When to emit colors and text attributes |\n\
| `images` | enum | auto | Inline image protocol selection |\n";

/// A document mixing all common elements, roughly `kb` kilobytes long.
pub fn mixed(kb: usize) -> String {
    let mut s = String::with_capacity(kb * 1024 + 4096);
    let mut i = 0;
    while s.len() < kb * 1024 {
        s.push_str(&format!("## Section {i}\n\n{PROSE}\n\n- item one\n- item two with `code`\n  - nested\n\n> A quote about section {i}.\n\n"));
        if i % 3 == 0 {
            s.push_str(CODE);
            s.push('\n');
        }
        if i % 4 == 0 {
            s.push_str(TABLE);
            s.push('\n');
        }
        i += 1;
    }
    s
}

/// Only prose paragraphs (wrapping cost).
pub fn prose(kb: usize) -> String {
    let mut s = String::new();
    while s.len() < kb * 1024 {
        s.push_str(PROSE);
        s.push_str("\n\n");
    }
    s
}

/// Many highlighted code blocks.
pub fn code(kb: usize) -> String {
    let mut s = String::new();
    while s.len() < kb * 1024 {
        s.push_str(CODE);
        s.push('\n');
    }
    s
}

/// Many tables.
pub fn tables(kb: usize) -> String {
    let mut s = String::new();
    while s.len() < kb * 1024 {
        s.push_str(TABLE);
        s.push('\n');
    }
    s
}
