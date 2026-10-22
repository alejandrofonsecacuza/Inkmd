//! Integration tests for the command-line interface.

use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Output, Stdio};

fn bin() -> Command {
    let mut c = Command::new(env!("CARGO_BIN_EXE_inkmd"));
    // A predictable environment, independent of the developer's shell.
    for v in ["NO_COLOR", "CLICOLOR_FORCE", "COLORTERM", "INKMD_THEME", "COLUMNS"] {
        c.env_remove(v);
    }
    c.env("TERM", "xterm-256color").env("LANG", "C.UTF-8").env_remove("LC_ALL").env_remove("LC_CTYPE");
    c
}

fn run(args: &[&str], stdin: Option<&[u8]>) -> Output {
    let mut c = bin();
    c.args(args).stdout(Stdio::piped()).stderr(Stdio::piped());
    c.stdin(if stdin.is_some() { Stdio::piped() } else { Stdio::null() });
    let mut child = c.spawn().unwrap();
    if let Some(input) = stdin {
        // The process may exit before reading its input (e.g. on a usage error).
        match child.stdin.take().unwrap().write_all(input) {
            Err(e) if e.kind() == std::io::ErrorKind::BrokenPipe => {}
            r => r.unwrap(),
        }
    }
    child.wait_with_output().unwrap()
}

fn stdout(o: &Output) -> String {
    String::from_utf8(o.stdout.clone()).unwrap()
}

fn fixture(name: &str) -> String {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures").join(name).to_string_lossy().into_owned()
}

fn scratch(name: &str, content: &[u8]) -> PathBuf {
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("cli");
    std::fs::create_dir_all(&dir).unwrap();
    let p = dir.join(name);
    std::fs::write(&p, content).unwrap();
    p
}

#[test]
fn renders_a_file() {
    let o = run(&[&fixture("blocks.md")], None);
    assert!(o.status.success());
    let out = stdout(&o);
    assert!(out.starts_with("# Heading 1\n"));
    assert!(out.contains("│ Quote level one"));
}

#[test]
fn reads_stdin_without_arguments() {
    let o = run(&[], Some(b"# Title\n\nSome *text*.\n"));
    assert!(o.status.success());
    assert_eq!(stdout(&o), "# Title\n\nSome text.\n");
}

#[test]
fn dash_means_stdin() {
    let o = run(&["-"], Some(b"hello\n"));
    assert_eq!(stdout(&o), "hello\n");
}

#[test]
fn empty_input_produces_no_output() {
    let o = run(&[], Some(b""));
    assert!(o.status.success());
    assert!(o.stdout.is_empty());
    let p = scratch("empty.md", b"");
    let o = run(&[p.to_str().unwrap()], None);
    assert!(o.status.success());
    assert!(o.stdout.is_empty());
}

#[test]
fn multiple_files_get_headers() {
    let a = scratch("a.md", b"first\n");
    let b = scratch("b.md", b"second\n");
    let o = run(&[a.to_str().unwrap(), b.to_str().unwrap()], None);
    let out = stdout(&o);
    assert!(o.status.success());
    let ia = out.find("a.md").unwrap();
    let ib = out.find("b.md").unwrap();
    assert!(ia < out.find("first").unwrap() && out.find("first").unwrap() < ib);
    assert!(out.contains("second"));
}

#[test]
fn redirected_output_has_no_escape_sequences() {
    for f in ["showcase.md", "code.md", "html.md", "security.md"] {
        let o = run(&[&fixture(f)], None);
        assert!(o.status.success());
        assert!(!o.stdout.contains(&0x1b), "{f} produced ESC when piped");
    }
}

#[test]
fn big_headings_only_on_request_when_piped() {
    let f = fixture("blocks.md");
    let o = run(&["--color", "always", &f], None);
    assert!(!stdout(&o).contains("\x1b]66;"));
    let o = run(&["--color", "always", "--big-headings", "always", &f], None);
    assert!(stdout(&o).contains("\x1b]66;s=2;Heading 1\x07"));
    let o = run(&["--big-headings", "sometimes", &f], None);
    assert_eq!(o.status.code(), Some(2));
}

#[test]
fn color_always_forces_styles() {
    let o = run(&["--color", "always", &fixture("showcase.md")], None);
    assert!(stdout(&o).contains("\x1b["));
    let mut c = bin();
    let o = c.arg(fixture("showcase.md")).env("CLICOLOR_FORCE", "1").output().unwrap();
    assert!(String::from_utf8_lossy(&o.stdout).contains("\x1b["));
}

#[test]
fn no_color_flags() {
    for args in [&["--no-color"][..], &["--color", "never"][..]] {
        let mut a = args.to_vec();
        let f = fixture("showcase.md");
        a.push(&f);
        let o = run(&a, None);
        assert!(!o.stdout.contains(&0x1b));
    }
}

#[test]
fn no_color_env_keeps_attributes_but_drops_colors() {
    let mut c = bin();
    let o = c.args(["--color", "always"]).arg(fixture("inline.md")).env("NO_COLOR", "1").output().unwrap();
    let out = String::from_utf8_lossy(&o.stdout);
    // --color always wins over NO_COLOR...
    assert!(out.contains("\x1b[0;1m"));
    // ...but NO_COLOR with auto and a pipe means no styling at all.
    let o = bin().arg(fixture("inline.md")).env("NO_COLOR", "1").output().unwrap();
    assert!(!o.stdout.contains(&0x1b));
}

#[test]
fn truecolor_theme_uses_24bit_only_when_supported() {
    let f = fixture("code.md");
    let o = bin().args(["--color", "always", "--theme", "dark", &f]).env("COLORTERM", "truecolor").output().unwrap();
    assert!(String::from_utf8_lossy(&o.stdout).contains(";38;2;"));
    let o = bin().args(["--color", "always", "--theme", "dark", &f]).env("TERM", "xterm").output().unwrap();
    let out = String::from_utf8_lossy(&o.stdout).into_owned();
    assert!(!out.contains(";38;2;") && !out.contains(";38;5;"));
    let o = bin().args(["--color", "always", "--theme", "dark", &f]).output().unwrap();
    assert!(String::from_utf8_lossy(&o.stdout).contains(";38;5;"));
}

#[test]
fn width_option() {
    let long = "word ".repeat(100);
    let o = run(&["--width", "30"], Some(long.as_bytes()));
    for line in stdout(&o).lines() {
        assert!(line.len() <= 30);
    }
    let o = run(&["-w", "5"], Some(b"x"));
    assert_eq!(o.status.code(), Some(2));
    let o = run(&["-w", "abc"], Some(b"x"));
    assert_eq!(o.status.code(), Some(2));
}

#[test]
fn columns_env_is_used_when_not_a_terminal() {
    let long = "word ".repeat(100);
    let mut c = bin();
    c.env("COLUMNS", "40").stdin(Stdio::piped()).stdout(Stdio::piped());
    let mut child = c.spawn().unwrap();
    child.stdin.take().unwrap().write_all(long.as_bytes()).unwrap();
    let out = String::from_utf8(child.wait_with_output().unwrap().stdout).unwrap();
    assert!(out.lines().all(|l| l.len() <= 40));
    assert!(out.lines().any(|l| l.len() > 35));
}

#[test]
fn ascii_mode() {
    let o = run(&["--ascii", &fixture("lists.md")], None);
    let out = stdout(&o);
    assert!(out.is_ascii() || !out.contains('•'));
    assert!(out.contains("* alpha"));
    assert!(out.contains("[x] write the parser"));
    // A non-UTF-8 locale switches to ASCII symbols automatically.
    let o = bin().arg(fixture("lists.md")).env("LANG", "C").output().unwrap();
    assert!(String::from_utf8_lossy(&o.stdout).contains("* alpha"));
}

#[test]
fn help_and_version() {
    let o = run(&["--help"], None);
    assert!(o.status.success());
    assert!(stdout(&o).contains("Usage: inkmd"));
    let o = run(&["-V"], None);
    assert_eq!(stdout(&o), format!("inkmd {}\n", env!("CARGO_PKG_VERSION")));
}

#[test]
fn missing_file_is_reported_and_others_still_render() {
    let ok = scratch("ok.md", b"fine\n");
    let o = run(&["/nonexistent/file.md", ok.to_str().unwrap()], None);
    assert_eq!(o.status.code(), Some(1));
    let err = String::from_utf8_lossy(&o.stderr);
    assert!(err.contains("/nonexistent/file.md: no such file or directory"), "{err}");
    assert!(stdout(&o).contains("fine"));
}

#[test]
fn directory_and_permission_errors() {
    let o = run(&[env!("CARGO_MANIFEST_DIR")], None);
    assert_eq!(o.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&o.stderr).contains("is a directory"));

    let p = scratch("secret.md", b"secret");
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o000)).unwrap();
    let readable = std::fs::read(&p).is_ok(); // running as root
    let o = run(&[p.to_str().unwrap()], None);
    std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o644)).unwrap();
    if !readable {
        assert_eq!(o.status.code(), Some(1));
        assert!(String::from_utf8_lossy(&o.stderr).contains("permission denied"));
    }
}

#[test]
fn bad_arguments() {
    for args in [&["--bogus"][..], &["--color", "sometimes"][..], &["--images", "sixel"][..], &["--theme", "x"][..]] {
        let o = run(args, Some(b""));
        assert_eq!(o.status.code(), Some(2), "{args:?}");
        assert!(String::from_utf8_lossy(&o.stderr).contains("--help"));
    }
}

#[test]
fn file_names_with_control_characters_are_sanitized_in_errors() {
    let o = run(&["evil\x1b[2Jname.md"], None);
    assert!(!o.stderr.contains(&0x1b));
    assert!(String::from_utf8_lossy(&o.stderr).contains("evil␛[2Jname.md"));
}

#[test]
fn invalid_utf8_and_bom_are_tolerated() {
    let o = run(&[], Some(b"\xef\xbb\xbf# Title \xff\xfe ok\n"));
    assert!(o.status.success());
    assert_eq!(stdout(&o), "# Title \u{fffd}\u{fffd} ok\n");
}

#[test]
fn crlf_line_endings() {
    let o = run(&[], Some(b"# Title\r\n\r\n- a\r\n- b\r\n"));
    assert_eq!(stdout(&o), "# Title\n\n• a\n• b\n");
}

#[test]
fn control_sequences_cannot_reach_the_terminal() {
    let o = run(&["--color", "always", &fixture("security.md")], None);
    let out = stdout(&o);
    // Every ESC in the output must start one of our own SGR sequences.
    let mut rest = out.as_str();
    while let Some(i) = rest.find('\x1b') {
        let seq = &rest[i..];
        let end = seq.find('m').expect("unterminated sequence");
        assert!(
            seq[1..end].starts_with("[0") && seq[2..end].chars().all(|c| c.is_ascii_digit() || c == ';'),
            "{seq:?}"
        );
        rest = &seq[end..];
    }
    assert!(!out.contains('\x07') && !out.contains('\u{9b}') && !out.contains('\x08'));
}

#[test]
fn images_never_by_default_when_piped_and_forced_kitty_works() {
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("img");
    std::fs::create_dir_all(&dir).unwrap();
    // 1x1 PNG.
    let png: &[u8] = &[
        0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0, 0, 0, 0x0d, 0x49, 0x48, 0x44, 0x52, 0, 0, 0, 1, 0, 0, 0, 1,
        8, 6, 0, 0, 0, 0x1f, 0x15, 0xc4, 0x89, 0, 0, 0, 0x0d, 0x49, 0x44, 0x41, 0x54, 0x78, 0x9c, 0x63, 0, 1, 0, 0, 5,
        0, 1, 0x0d, 0x0a, 0x2d, 0xb4, 0, 0, 0, 0, 0x49, 0x45, 0x4e, 0x44, 0xae, 0x42, 0x60, 0x82,
    ];
    std::fs::write(dir.join("dot.png"), png).unwrap();
    let md = dir.join("doc.md");
    std::fs::write(&md, "before\n\n![dot](dot.png)\n\nafter\n").unwrap();
    let md = md.to_str().unwrap();

    let o = run(&[md], None);
    assert_eq!(stdout(&o), "before\n\n[image: dot] (dot.png)\n\nafter\n");

    let o = run(&["--images", "kitty", md], None);
    let out = stdout(&o);
    assert!(out.contains("\x1b_Ga=T,f=100,"), "{out:?}");
    assert!(out.contains("before") && out.contains("after"));
    assert!(!out.contains("[image: dot]"));

    let o = run(&["--images", "iterm2", md], None);
    assert!(stdout(&o).contains("\x1b]1337;File=inline=1;"));

    // A missing image never breaks rendering.
    std::fs::write(dir.join("missing.md"), "![gone](nope.png) text\n").unwrap();
    let o = run(&["--images", "kitty", dir.join("missing.md").to_str().unwrap()], None);
    assert!(o.status.success());
    assert_eq!(stdout(&o), "[image: gone] (nope.png: not found) text\n");

    // Non-image content with an image extension is rejected safely.
    std::fs::write(dir.join("fake.png"), b"not really a png").unwrap();
    std::fs::write(dir.join("fake.md"), "![fake](fake.png)\n").unwrap();
    let o = run(&["--images", "kitty", dir.join("fake.md").to_str().unwrap()], None);
    assert_eq!(stdout(&o), "[image: fake] (fake.png: unsupported format)\n");
}

#[test]
fn large_document() {
    let mut doc = String::new();
    for i in 0..20_000 {
        doc.push_str(&format!(
            "## Section {i}\n\nSome *text* with `code` and a [link](https://x.y/{i}).\n\n- item\n- item\n\n"
        ));
    }
    let o = run(&[], Some(doc.as_bytes()));
    assert!(o.status.success());
    assert_eq!(stdout(&o).matches("## Section").count(), 20_000);
}

#[test]
fn broken_pipe_is_silent() {
    let mut doc = String::new();
    for i in 0..50_000 {
        doc.push_str(&format!("line {i}\n\n"));
    }
    let p = scratch("big.md", doc.as_bytes());
    let mut child = bin().arg(&p).stdout(Stdio::piped()).stderr(Stdio::piped()).spawn().unwrap();
    // Close the reading end immediately.
    drop(child.stdout.take());
    let o = child.wait_with_output().unwrap();
    assert!(o.stderr.is_empty(), "{}", String::from_utf8_lossy(&o.stderr));
}
