//! Terminal capability detection.
//!
//! Cheap checks (isatty, ioctl, environment) are done once at startup. The
//! only expensive check, querying the terminal for graphics support, is done
//! lazily and only when a document actually contains a displayable image.

use crate::style::ColorLevel;
use std::env;
use std::time::{Duration, Instant};

pub fn is_tty(fd: i32) -> bool {
    // SAFETY: isatty only inspects the descriptor.
    unsafe { libc::isatty(fd) == 1 }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct WinSize {
    pub cols: u16,
    pub rows: u16,
    pub xpixel: u16,
    pub ypixel: u16,
}

pub fn window_size(fd: i32) -> Option<WinSize> {
    // SAFETY: TIOCGWINSZ writes a `winsize` struct into the provided pointer.
    let mut ws: libc::winsize = unsafe { std::mem::zeroed() };
    let r = unsafe { libc::ioctl(fd, libc::TIOCGWINSZ, &mut ws) };
    (r == 0 && ws.ws_col > 0).then_some(WinSize {
        cols: ws.ws_col,
        rows: ws.ws_row,
        xpixel: ws.ws_xpixel,
        ypixel: ws.ws_ypixel,
    })
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ColorMode {
    Auto,
    Always,
    Never,
}

fn env_set(name: &str) -> bool {
    env::var_os(name).is_some_and(|v| !v.is_empty())
}

/// Returns `(color depth, attributes allowed)`.
///
/// * `Never`: no color; attributes only when writing to a terminal.
/// * `Auto`: nothing at all when not writing to a terminal or `TERM=dumb`;
///   `NO_COLOR` disables color but keeps bold/italic.
/// * `Always` (or `CLICOLOR_FORCE`): styled output even through pipes.
pub fn color_support(mode: ColorMode, tty: bool) -> (ColorLevel, bool) {
    let dumb = env::var("TERM").is_ok_and(|t| t == "dumb");
    let mode = match mode {
        ColorMode::Auto if env::var("CLICOLOR_FORCE").is_ok_and(|v| v != "0") => ColorMode::Always,
        m => m,
    };
    match mode {
        ColorMode::Never => (ColorLevel::None, tty && !dumb),
        ColorMode::Auto if !tty || dumb => (ColorLevel::None, false),
        ColorMode::Auto if env_set("NO_COLOR") => (ColorLevel::None, true),
        _ => (color_depth(), true),
    }
}

fn color_depth() -> ColorLevel {
    let colorterm = env::var("COLORTERM").unwrap_or_default().to_ascii_lowercase();
    if colorterm == "truecolor" || colorterm == "24bit" {
        return ColorLevel::TrueColor;
    }
    let term = env::var("TERM").unwrap_or_default();
    if term.ends_with("-direct") {
        ColorLevel::TrueColor
    } else if term.contains("256") || term == "xterm-kitty" || term == "alacritty" || term.contains("ghostty") {
        ColorLevel::Ansi256
    } else {
        ColorLevel::Ansi16
    }
}

/// Whether the locale says the terminal can display UTF-8.
pub fn utf8_locale() -> bool {
    for var in ["LC_ALL", "LC_CTYPE", "LANG"] {
        if let Ok(v) = env::var(var) {
            if !v.is_empty() {
                let v = v.to_ascii_lowercase();
                return v.contains("utf-8") || v.contains("utf8");
            }
        }
    }
    // No locale configured at all: modern systems are UTF-8 nearly everywhere,
    // except the bare "C"/"POSIX" locale which we treat as ASCII above.
    true
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Protocol {
    Kitty,
    Iterm2,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct Graphics {
    pub protocol: Option<Protocol>,
    /// Size of one character cell in pixels, when known.
    pub cell: Option<(u16, u16)>,
    /// Kitty text sizing protocol (OSC 66): scaled text.
    pub text_sizing: bool,
}

/// Ask the terminal what it supports.
///
/// Sends, in one write: a Kitty graphics query, the text sizing probe
/// (cursor position reports around `OSC 66` with `w=2` and `s=2`, erased
/// right away), XTVERSION, a cell-size
/// report request and finally Primary Device Attributes (DA1). Every
/// terminal answers DA1, so as soon as that reply arrives all other replies
/// (if any) have arrived too, and no fixed sleep is needed. A short timeout
/// protects against terminals (or multiplexers) that answer nothing.
pub fn query_graphics() -> Graphics {
    let mut g = Graphics::default();
    if let Some(ws) = window_size(1) {
        if ws.xpixel > 0 && ws.ypixel > 0 {
            g.cell = Some((ws.xpixel / ws.cols.max(1), ws.ypixel / ws.rows.max(1)));
        }
    }
    let Some(reply) = query_tty(
        b"\x1b_Gi=31,s=1,v=1,a=q,t=d,f=24;AAAA\x1b\\\
          \r\x1b[6n\x1b]66;w=2; \x07\x1b[6n\x1b]66;s=2; \x07\x1b[6n\r\x1b[2K\
          \x1b[>0q\x1b[16t\x1b[c",
        Duration::from_millis(300),
    ) else {
        return g;
    };
    let reply = String::from_utf8_lossy(&reply);
    if reply.contains("_Gi=31;OK") {
        g.protocol = Some(Protocol::Kitty);
    } else if let Some(v) = xtversion(&reply) {
        if v.starts_with("iTerm2") || v.starts_with("WezTerm") {
            g.protocol = Some(Protocol::Iterm2);
        }
    }
    if g.cell.is_none() {
        g.cell = cell_size_reply(&reply);
    }
    g.text_sizing = text_sizing_reply(&reply);
    g
}

/// Columns of all cursor position reports (`CSI row ; col R`), in order.
fn cpr_columns(reply: &str) -> Vec<u32> {
    let mut cols = Vec::new();
    let mut rest = reply;
    while let Some(i) = rest.find("\x1b[") {
        rest = &rest[i + 2..];
        let end = rest.find(|c: char| !(c.is_ascii_digit() || c == ';')).unwrap_or(rest.len());
        if rest[end..].starts_with('R') {
            if let Some(col) = rest[..end].split(';').nth(1).and_then(|c| c.parse().ok()) {
                cols.push(col);
            }
        }
    }
    cols
}

/// Both `w=2` and `s=2` must advance the cursor by two cells each.
fn text_sizing_reply(reply: &str) -> bool {
    let c = cpr_columns(reply);
    c.len() >= 3 && c[1] == c[0] + 2 && c[2] == c[1] + 2
}

fn xtversion(reply: &str) -> Option<&str> {
    let start = reply.find("\x1bP>|")? + 4;
    let end = reply[start..].find('\x1b').map(|e| start + e).unwrap_or(reply.len());
    Some(&reply[start..end])
}

/// Parse `CSI 6 ; height ; width t`.
fn cell_size_reply(reply: &str) -> Option<(u16, u16)> {
    let start = reply.find("\x1b[6;")? + 4;
    let end = start + reply[start..].find('t')?;
    let mut it = reply[start..end].split(';').map(|n| n.parse::<u16>().ok());
    let h = it.next()??;
    let w = it.next()??;
    (w > 0 && h > 0).then_some((w, h))
}

fn has_da1(buf: &[u8]) -> bool {
    // ESC [ ? <digits and ;> c
    let mut i = 0;
    while i + 2 < buf.len() {
        if buf[i] == 0x1b && buf[i + 1] == b'[' && buf[i + 2] == b'?' {
            let mut j = i + 3;
            while j < buf.len() && (buf[j].is_ascii_digit() || buf[j] == b';') {
                j += 1;
            }
            if j < buf.len() && buf[j] == b'c' {
                return true;
            }
        }
        i += 1;
    }
    false
}

/// Wait until `fd` has data or `timeout` expires. Uses select(2) rather than
/// poll(2): on macOS, poll does not work on terminal devices.
fn readable(fd: i32, timeout: Duration) -> bool {
    if fd >= libc::FD_SETSIZE as i32 {
        return false;
    }
    // SAFETY: `set` is a zeroed fd_set initialized with FD_ZERO/FD_SET for a
    // descriptor below FD_SETSIZE; select only reads and writes these locals.
    unsafe {
        let mut set: libc::fd_set = std::mem::zeroed();
        libc::FD_ZERO(&mut set);
        libc::FD_SET(fd, &mut set);
        let mut tv = libc::timeval { tv_sec: timeout.as_secs() as _, tv_usec: timeout.subsec_micros() as _ };
        libc::select(fd + 1, &mut set, std::ptr::null_mut(), std::ptr::null_mut(), &mut tv) > 0
    }
}

/// Write `query` to the controlling terminal in non-canonical, no-echo mode
/// and collect the reply until a DA1 response is seen or `timeout` expires.
fn query_tty(query: &[u8], timeout: Duration) -> Option<Vec<u8>> {
    // SAFETY: plain POSIX calls on a descriptor we own; the original termios
    // state is restored before returning on every path after it was changed.
    unsafe {
        let fd = libc::open(c"/dev/tty".as_ptr(), libc::O_RDWR | libc::O_NOCTTY | libc::O_CLOEXEC);
        if fd < 0 {
            return None;
        }
        let mut orig: libc::termios = std::mem::zeroed();
        if libc::tcgetattr(fd, &mut orig) != 0 {
            libc::close(fd);
            return None;
        }
        let mut raw = orig;
        raw.c_lflag &= !(libc::ICANON | libc::ECHO);
        raw.c_cc[libc::VMIN] = 0;
        raw.c_cc[libc::VTIME] = 0;
        if libc::tcsetattr(fd, libc::TCSANOW, &raw) != 0 {
            libc::close(fd);
            return None;
        }
        let mut reply = Vec::new();
        let written = libc::write(fd, query.as_ptr().cast(), query.len());
        if written == query.len() as isize {
            let deadline = Instant::now() + timeout;
            let mut buf = [0u8; 512];
            loop {
                let left = deadline.saturating_duration_since(Instant::now());
                if left.is_zero() {
                    break;
                }
                if !readable(fd, left) {
                    break;
                }
                let n = libc::read(fd, buf.as_mut_ptr().cast(), buf.len());
                if n <= 0 {
                    break;
                }
                reply.extend_from_slice(&buf[..n as usize]);
                if has_da1(&reply) || reply.len() > 8192 {
                    break;
                }
            }
        }
        libc::tcsetattr(fd, libc::TCSANOW, &orig);
        libc::close(fd);
        Some(reply)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_replies() {
        let r = "\x1b_Gi=31;OK\x1b\\\x1bP>|kitty(0.39.1)\x1b\\\x1b[6;20;10t\x1b[?62;c";
        assert!(has_da1(r.as_bytes()));
        assert_eq!(xtversion(r), Some("kitty(0.39.1)"));
        assert_eq!(cell_size_reply(r), Some((10, 20)));
        assert!(!has_da1(b"\x1b[?62;"));
        assert!(text_sizing_reply("\x1b[5;1R\x1b[5;3R\x1b[5;5R\x1b[?62c"));
        assert!(!text_sizing_reply("\x1b[5;1R\x1b[5;3R\x1b[5;3R"));
        assert!(!text_sizing_reply("\x1b[5;1R\x1b[5;1R\x1b[5;1R"));
        assert!(!text_sizing_reply("\x1b[6;20;10t\x1b[?62c"));
    }
}
