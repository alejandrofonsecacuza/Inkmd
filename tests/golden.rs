//! Golden (snapshot) tests: every `tests/fixtures/*.md` is rendered in
//! several modes and compared with `tests/golden/<name>.<mode>.txt`.
//!
//! Run with `UPDATE_GOLDEN=1 cargo test --test golden` to regenerate the
//! snapshots after an intentional change, then review the diff.
//! In snapshots, ESC is written as the two characters `\e` for readability.

use inkmd::{render_to_string, ColorLevel, ImageMode, Options, Painter, Theme, ThemeName};
use std::fs;
use std::path::Path;

fn modes() -> Vec<(&'static str, Options)> {
    let base = Options { images: ImageMode::Never, ..Options::default() };
    vec![
        ("plain", Options { width: 60, ..base.clone() }),
        ("ansi", Options { width: 60, painter: Painter { level: ColorLevel::Ansi16, attrs: true }, ..base.clone() }),
        ("narrow", Options { width: 30, ascii: true, ..base.clone() }),
        (
            "truecolor",
            Options {
                width: 80,
                painter: Painter { level: ColorLevel::TrueColor, attrs: true },
                theme: Theme::new(ThemeName::Dark),
                ..base
            },
        ),
    ]
}

fn readable(s: &str) -> String {
    s.replace('\x1b', "\\e")
}

#[test]
fn golden() {
    let update = std::env::var_os("UPDATE_GOLDEN").is_some();
    let fixtures = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
    let golden = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/golden");
    let mut names: Vec<_> = fs::read_dir(&fixtures)
        .unwrap()
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "md"))
        .collect();
    names.sort();
    assert!(!names.is_empty());
    let mut failures = Vec::new();
    for path in &names {
        let input = fs::read_to_string(path).unwrap();
        let stem = path.file_stem().unwrap().to_string_lossy();
        for (mode, opts) in modes() {
            // Truecolor snapshots only for a couple of representative files.
            if mode == "truecolor" && stem != "showcase" && stem != "code" {
                continue;
            }
            // Styled snapshots include syntax highlighting.
            if !cfg!(feature = "highlight") && (mode == "ansi" || mode == "truecolor") {
                continue;
            }
            let out = readable(&render_to_string(&input, &opts));
            let file = golden.join(format!("{stem}.{mode}.txt"));
            if update {
                fs::create_dir_all(&golden).unwrap();
                fs::write(&file, &out).unwrap();
                continue;
            }
            match fs::read_to_string(&file) {
                Ok(expected) if expected == out => {}
                Ok(expected) => failures.push(format!("{}: differs\n{}", file.display(), first_diff(&expected, &out))),
                Err(_) => failures.push(format!("{}: missing (run with UPDATE_GOLDEN=1)", file.display())),
            }
        }
    }
    assert!(failures.is_empty(), "golden mismatches:\n{}", failures.join("\n"));
}

fn first_diff(expected: &str, actual: &str) -> String {
    for (i, (e, a)) in expected.lines().zip(actual.lines()).enumerate() {
        if e != a {
            return format!("  line {}:\n  expected: {e}\n  actual:   {a}", i + 1);
        }
    }
    format!("  line counts differ: expected {}, actual {}", expected.lines().count(), actual.lines().count())
}

/// Output must never be wider than requested, in any mode (except content
/// that cannot be broken at all, which does not exist in the fixtures).
#[test]
fn fixtures_respect_width() {
    let fixtures = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
    for entry in fs::read_dir(&fixtures).unwrap() {
        let path = entry.unwrap().path();
        if path.extension().is_none_or(|x| x != "md") {
            continue;
        }
        let input = fs::read_to_string(&path).unwrap();
        for width in [20, 30, 45, 60, 100] {
            let opts = Options { width, ..Options::default() };
            let out = render_to_string(&input, &opts);
            for line in out.lines() {
                let w = inkmd::wrap::width(line);
                assert!(w <= width, "{}: width {width}: line of {w} columns: {line:?}", path.display());
            }
        }
    }
}
