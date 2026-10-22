use inkmd::render::{BigHeadings, ImageMode, Options, Session};
use inkmd::sanitize::clean;
use inkmd::style::{Painter, Style, Theme, ThemeName, BOLD, DIM};
use inkmd::term::{self, ColorMode, Protocol};
use inkmd::wrap::Line;
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

const NAME: &str = env!("CARGO_PKG_NAME");
const VERSION: &str = env!("CARGO_PKG_VERSION");
/// Inputs larger than this are refused instead of exhausting memory.
const MAX_INPUT: u64 = 256 * 1024 * 1024;
/// Default maximum width when it comes from the terminal size: long lines
/// are hard to read. An explicit --width is not capped.
const DEFAULT_MAX_WIDTH: usize = 100;

fn usage() -> String {
    format!(
        "\
{NAME} {VERSION} - render Markdown in the terminal

Usage: {NAME} [OPTIONS] [FILE]...

Reads standard input when no FILE is given or FILE is '-'.

Options:
  -w, --width <N>      Wrap at N columns [default: terminal width, at most {DEFAULT_MAX_WIDTH}]
      --color <WHEN>   Use colors and styles: auto, always, never [default: auto]
      --no-color       Same as --color never
      --images <MODE>  Inline images: auto, never, kitty, iterm2 [default: auto]
      --big-headings <WHEN>
                       Scale headings by level (Kitty text sizing):
                       auto, always, never [default: auto]
      --theme <NAME>   ansi (terminal palette), dark, light [default: ansi]
      --ascii          Use only ASCII symbols for markers and borders
  -h, --help           Print help
  -V, --version        Print version

Environment: NO_COLOR disables colors; CLICOLOR_FORCE forces them;
INKMD_THEME sets the default theme; INKMD_DEBUG=1 explains image decisions.
"
    )
}

struct Args {
    files: Vec<PathBuf>,
    width: Option<usize>,
    color: ColorMode,
    images: Option<ImageMode>,
    big_headings: Option<BigHeadings>,
    theme: ThemeName,
    ascii: bool,
}

fn parse_theme(v: &str) -> Option<ThemeName> {
    match v {
        "ansi" => Some(ThemeName::Ansi),
        "dark" => Some(ThemeName::Dark),
        "light" => Some(ThemeName::Light),
        _ => None,
    }
}

fn parse_args() -> Result<Option<Args>, String> {
    use lexopt::prelude::*;
    let mut args = Args {
        files: Vec::new(),
        width: None,
        color: ColorMode::Auto,
        images: None,
        big_headings: None,
        theme: std::env::var("INKMD_THEME").ok().as_deref().and_then(parse_theme).unwrap_or(ThemeName::Ansi),
        ascii: false,
    };
    let mut p = lexopt::Parser::from_env();
    let value = |p: &mut lexopt::Parser| -> Result<String, String> {
        p.value().map_err(|e| e.to_string())?.into_string().map_err(|_| "invalid UTF-8 in option value".to_string())
    };
    while let Some(arg) = p.next().map_err(|e| e.to_string())? {
        match arg {
            Short('w') | Long("width") => {
                let v = value(&mut p)?;
                let n: usize = v.parse().map_err(|_| format!("invalid width '{}'", clean(&v, false)))?;
                if !(10..=10_000).contains(&n) {
                    return Err("width must be between 10 and 10000".into());
                }
                args.width = Some(n);
            }
            Long("color") | Long("colour") => {
                args.color = match value(&mut p)?.as_str() {
                    "auto" => ColorMode::Auto,
                    "always" => ColorMode::Always,
                    "never" => ColorMode::Never,
                    v => return Err(format!("invalid --color '{}' (expected auto, always or never)", clean(v, false))),
                }
            }
            Long("no-color") | Long("no-colour") => args.color = ColorMode::Never,
            Long("images") => {
                args.images = Some(match value(&mut p)?.as_str() {
                    "auto" => ImageMode::Auto,
                    "never" => ImageMode::Never,
                    "kitty" => ImageMode::Force(Protocol::Kitty),
                    "iterm2" => ImageMode::Force(Protocol::Iterm2),
                    v => {
                        return Err(format!(
                            "invalid --images '{}' (expected auto, never, kitty or iterm2)",
                            clean(v, false)
                        ))
                    }
                })
            }
            Long("theme") => {
                let v = value(&mut p)?;
                args.theme = parse_theme(&v)
                    .ok_or_else(|| format!("invalid --theme '{}' (expected ansi, dark or light)", clean(&v, false)))?;
            }
            Long("big-headings") => {
                args.big_headings = Some(match value(&mut p)?.as_str() {
                    "auto" => BigHeadings::Auto,
                    "always" => BigHeadings::Always,
                    "never" => BigHeadings::Never,
                    v => {
                        return Err(format!(
                            "invalid --big-headings '{}' (expected auto, always or never)",
                            clean(v, false)
                        ))
                    }
                })
            }
            Long("ascii") => args.ascii = true,
            Short('h') | Long("help") => {
                print!("{}", usage());
                return Ok(None);
            }
            Short('V') | Long("version") => {
                println!("{NAME} {VERSION}");
                return Ok(None);
            }
            Value(v) => args.files.push(PathBuf::from(v)),
            _ => return Err(arg.unexpected().to_string()),
        }
    }
    Ok(Some(args))
}

fn read_input(path: &Path) -> io::Result<String> {
    let mut bytes = Vec::new();
    if path.as_os_str() == "-" {
        io::stdin().lock().take(MAX_INPUT + 1).read_to_end(&mut bytes)?;
    } else {
        let f = std::fs::File::open(path)?;
        let meta = f.metadata()?;
        if meta.is_dir() {
            return Err(io::Error::new(io::ErrorKind::InvalidInput, "is a directory"));
        }
        if meta.len() > MAX_INPUT {
            return Err(io::Error::new(io::ErrorKind::InvalidData, "file too large"));
        }
        // `take` also bounds special files whose metadata reports size 0.
        f.take(MAX_INPUT + 1).read_to_end(&mut bytes)?;
    }
    if bytes.len() as u64 > MAX_INPUT {
        return Err(io::Error::new(io::ErrorKind::InvalidData, "input too large"));
    }
    let mut text = match String::from_utf8(bytes) {
        Ok(s) => s,
        Err(e) => String::from_utf8_lossy(e.as_bytes()).into_owned(),
    };
    if text.starts_with('\u{feff}') {
        text.drain(..3);
    }
    Ok(text)
}

fn error_message(e: &io::Error) -> String {
    match e.kind() {
        io::ErrorKind::NotFound => "no such file or directory".into(),
        io::ErrorKind::PermissionDenied => "permission denied".into(),
        _ => {
            let s = e.to_string();
            // Strip the " (os error N)" suffix.
            s.split(" (os error").next().unwrap_or(&s).to_lowercase()
        }
    }
}

fn main() -> ExitCode {
    // Behave like other Unix filters when the reader goes away (`| head`).
    // SAFETY: resetting a signal disposition to its default is always sound.
    unsafe {
        libc::signal(libc::SIGPIPE, libc::SIG_DFL);
    }
    let args = match parse_args() {
        Ok(Some(a)) => a,
        Ok(None) => return ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("{NAME}: {e}\nTry '{NAME} --help' for more information.");
            return ExitCode::from(2);
        }
    };
    let mut files = args.files;
    if files.is_empty() {
        if term::is_tty(0) {
            eprintln!("{NAME}: no input file and standard input is a terminal\nUsage: {NAME} [OPTIONS] [FILE]...  (see --help)");
            return ExitCode::from(2);
        }
        files.push(PathBuf::from("-"));
    }

    let stdout_tty = term::is_tty(1);
    let (level, attrs) = term::color_support(args.color, stdout_tty);
    let winsize = if stdout_tty { term::window_size(1) } else { None };
    let width = args.width.unwrap_or_else(|| {
        let cols = winsize
            .map(|w| w.cols as usize)
            .or_else(|| std::env::var("COLUMNS").ok().and_then(|c| c.parse().ok()))
            .unwrap_or(80);
        cols.clamp(10, DEFAULT_MAX_WIDTH)
    });
    let images = args.images.unwrap_or(if stdout_tty { ImageMode::Auto } else { ImageMode::Never });
    let opts = Options {
        width,
        painter: Painter { level, attrs },
        theme: Theme::new(args.theme),
        ascii: args.ascii || !term::utf8_locale(),
        images,
        term_rows: winsize.map(|w| w.rows as usize).filter(|&r| r > 0).unwrap_or(24),
        // Only probed on a styled terminal: pipes and pagers never get OSC 66.
        big_headings: args.big_headings.unwrap_or(if stdout_tty && (attrs || level != inkmd::ColorLevel::None) {
            BigHeadings::Auto
        } else {
            BigHeadings::Never
        }),
    };
    let session = Session::new(opts);

    let stdout = io::stdout();
    let mut out = stdout.lock();
    let mut failed = false;
    let multiple = files.len() > 1;
    for (i, path) in files.iter().enumerate() {
        let display = if path.as_os_str() == "-" {
            "<stdin>".to_string()
        } else {
            clean(&path.to_string_lossy(), session.opts.ascii)
        };
        let text = match read_input(path) {
            Ok(t) => t,
            Err(e) => {
                let _ = out.flush();
                eprintln!("{NAME}: {display}: {}", error_message(&e));
                failed = true;
                continue;
            }
        };
        if multiple {
            if i > 0 {
                let _ = writeln!(out);
            }
            let _ = write_header(&mut out, &session.opts, &display);
        }
        let base = if path.as_os_str() == "-" {
            None
        } else {
            path.parent().map(|p| if p.as_os_str().is_empty() { Path::new(".") } else { p })
        };
        if let Err(e) = session.render(&text, base, &mut out) {
            eprintln!("{NAME}: {display}: {}", error_message(&e));
            return ExitCode::FAILURE;
        }
    }
    if failed {
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    }
}

fn write_header(out: &mut impl Write, opts: &Options, name: &str) -> io::Result<()> {
    let rule = if opts.ascii { "==" } else { "━━" };
    let mut line = Line::from(Style::attrs(DIM), rule);
    line.push(Style::PLAIN, " ");
    line.push(Style::attrs(BOLD), name);
    let mut s = String::new();
    line.emit(&mut s, &opts.painter);
    writeln!(out, "{s}\n")
}
