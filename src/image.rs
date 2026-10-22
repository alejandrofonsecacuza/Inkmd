//! Inline images through the Kitty graphics protocol or the iTerm2 inline
//! image protocol (also implemented by WezTerm and others).
//!
//! Policy:
//! * only local files are displayed; remote URLs are never fetched;
//! * files are identified by content (magic bytes), not by extension;
//! * size limits protect against huge or special files;
//! * on any failure the caller falls back to a textual placeholder.
//!
//! No temporary files are created: image data is sent inline, base64-encoded.

use crate::term::Protocol;
use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};

const MAX_FILE: u64 = 32 * 1024 * 1024;
/// Decoded images larger than this (in pixels) are downscaled before
/// sending them as raw RGBA, to keep the escape sequence reasonably sized.
#[cfg(feature = "image-decode")]
const MAX_PIXELS: u32 = 2048 * 2048;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Format {
    Png,
    Jpeg,
    Gif,
    Webp,
}

fn sniff(data: &[u8]) -> Option<Format> {
    if data.starts_with(b"\x89PNG\r\n\x1a\n") {
        Some(Format::Png)
    } else if data.starts_with(&[0xff, 0xd8, 0xff]) {
        Some(Format::Jpeg)
    } else if data.starts_with(b"GIF87a") || data.starts_with(b"GIF89a") {
        Some(Format::Gif)
    } else if data.len() > 12 && &data[..4] == b"RIFF" && &data[8..12] == b"WEBP" {
        Some(Format::Webp)
    } else {
        None
    }
}

pub struct Loaded {
    format: Format,
    data: Vec<u8>,
    pub px: (u32, u32),
}

#[derive(Debug, PartialEq, Eq)]
pub enum LoadError {
    Remote,
    NotFound,
    NotAFile,
    TooLarge,
    Unsupported,
    Unreadable,
}

impl LoadError {
    pub fn describe(&self) -> &'static str {
        match self {
            LoadError::Remote => "remote",
            LoadError::NotFound => "not found",
            LoadError::NotAFile => "not a file",
            LoadError::TooLarge => "too large",
            LoadError::Unsupported => "unsupported format",
            LoadError::Unreadable => "unreadable",
        }
    }
}

/// Resolve an image reference relative to the document's directory.
pub fn resolve(url: &str, base: Option<&Path>) -> Result<PathBuf, LoadError> {
    let lower = url.to_ascii_lowercase();
    if let Some(p) = url.strip_prefix("file://") {
        return Ok(PathBuf::from(percent_decode(p)));
    }
    if lower.contains("://") || lower.starts_with("data:") || lower.starts_with("//") {
        return Err(LoadError::Remote);
    }
    let path = PathBuf::from(percent_decode(url.split(['?', '#']).next().unwrap_or(url)));
    Ok(match base {
        Some(b) if path.is_relative() => b.join(path),
        _ => path,
    })
}

fn percent_decode(s: &str) -> String {
    if !s.contains('%') {
        return s.to_string();
    }
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' && i + 2 < b.len() {
            if let Some(v) = s.get(i + 1..i + 3).and_then(|h| u8::from_str_radix(h, 16).ok()) {
                out.push(v);
                i += 3;
                continue;
            }
        }
        out.push(b[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

pub fn load(path: &Path, protocol: Protocol) -> Result<Loaded, LoadError> {
    let meta = fs::metadata(path).map_err(|_| LoadError::NotFound)?;
    if !meta.is_file() {
        return Err(LoadError::NotAFile);
    }
    if meta.len() > MAX_FILE {
        return Err(LoadError::TooLarge);
    }
    let data = fs::read(path).map_err(|_| LoadError::Unreadable)?;
    let format = sniff(&data).ok_or(LoadError::Unsupported)?;
    let px = match format {
        Format::Png if data.len() >= 24 => {
            (u32::from_be_bytes(data[16..20].try_into().unwrap()), u32::from_be_bytes(data[20..24].try_into().unwrap()))
        }
        _ => decoded_dimensions(&data).ok_or(LoadError::Unsupported)?,
    };
    if px.0 == 0 || px.1 == 0 {
        return Err(LoadError::Unsupported);
    }
    // Kitty accepts PNG directly; other formats must be decoded to RGBA.
    // iTerm2 accepts the file as is (PNG/JPEG/GIF; WebP depends on the OS).
    if protocol == Protocol::Kitty && format != Format::Png && !cfg!(feature = "image-decode") {
        return Err(LoadError::Unsupported);
    }
    if protocol == Protocol::Iterm2 && format == Format::Webp {
        return Err(LoadError::Unsupported);
    }
    Ok(Loaded { format, data, px })
}

#[cfg(feature = "image-decode")]
fn decoded_dimensions(data: &[u8]) -> Option<(u32, u32)> {
    image::ImageReader::new(std::io::Cursor::new(data)).with_guessed_format().ok()?.into_dimensions().ok()
}

#[cfg(not(feature = "image-decode"))]
fn decoded_dimensions(data: &[u8]) -> Option<(u32, u32)> {
    match sniff(data)? {
        Format::Gif if data.len() >= 10 => {
            Some((u16::from_le_bytes([data[6], data[7]]) as u32, u16::from_le_bytes([data[8], data[9]]) as u32))
        }
        _ => None,
    }
}

/// Number of terminal cells `(cols, rows)` the image will occupy.
pub fn cell_geometry(px: (u32, u32), cell: (u16, u16), max_cols: usize, max_rows: usize) -> (usize, usize) {
    let (cw, ch) = (cell.0.max(1) as f64, cell.1.max(1) as f64);
    let (w, h) = (px.0 as f64, px.1 as f64);
    let mut cols = ((w / cw).ceil() as usize).clamp(1, max_cols.max(1));
    let mut rows = ((cols as f64 * cw * h / w) / ch).ceil().max(1.0) as usize;
    if rows > max_rows {
        rows = max_rows.max(1);
        cols = ((rows as f64 * ch * w / h) / cw).ceil().clamp(1.0, max_cols.max(1) as f64) as usize;
    }
    (cols, rows)
}

/// Append the escape sequence that draws `img` at the cursor position,
/// scaled to `cols` x `rows` cells. The cursor is not moved (Kitty) or is
/// moved below the image (iTerm2), see the renderer for how it handles this.
pub fn encode(img: &Loaded, protocol: Protocol, cols: usize, rows: usize, out: &mut String) -> bool {
    match protocol {
        Protocol::Kitty => encode_kitty(img, cols, rows, out),
        Protocol::Iterm2 => {
            let _ = write!(
                out,
                "\x1b]1337;File=inline=1;size={};width={cols};height={rows};preserveAspectRatio=1:",
                img.data.len()
            );
            base64(&img.data, out);
            out.push('\x07');
            true
        }
    }
}

fn encode_kitty(img: &Loaded, cols: usize, rows: usize, out: &mut String) -> bool {
    let mut payload = String::new();
    let header = if img.format == Format::Png {
        base64(&img.data, &mut payload);
        format!("a=T,f=100,t=d,q=2,C=1,c={cols},r={rows}")
    } else {
        let Some((w, h, rgba)) = decode_rgba(&img.data) else { return false };
        base64(&rgba, &mut payload);
        format!("a=T,f=32,s={w},v={h},t=d,q=2,C=1,c={cols},r={rows}")
    };
    // The protocol requires chunks of at most 4096 bytes of base64 data.
    let bytes = payload.as_bytes();
    let mut chunks = bytes.chunks(4096).peekable();
    let mut first = true;
    while let Some(chunk) = chunks.next() {
        let more = u8::from(chunks.peek().is_some());
        out.push_str("\x1b_G");
        if first {
            out.push_str(&header);
            out.push(',');
            first = false;
        }
        let _ = write!(out, "m={more};");
        // base64 output is ASCII, so this is valid UTF-8.
        out.push_str(std::str::from_utf8(chunk).unwrap_or(""));
        out.push_str("\x1b\\");
    }
    true
}

#[cfg(feature = "image-decode")]
fn decode_rgba(data: &[u8]) -> Option<(u32, u32, Vec<u8>)> {
    let img = image::load_from_memory(data).ok()?;
    let img = if img.width().saturating_mul(img.height()) > MAX_PIXELS { img.thumbnail(2048, 2048) } else { img };
    let rgba = img.into_rgba8();
    Some((rgba.width(), rgba.height(), rgba.into_raw()))
}

#[cfg(not(feature = "image-decode"))]
fn decode_rgba(_: &[u8]) -> Option<(u32, u32, Vec<u8>)> {
    None
}

fn base64(data: &[u8], out: &mut String) {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    out.reserve(data.len().div_ceil(3) * 4);
    for c in data.chunks(3) {
        let n = (c[0] as u32) << 16 | (*c.get(1).unwrap_or(&0) as u32) << 8 | *c.get(2).unwrap_or(&0) as u32;
        out.push(T[(n >> 18) as usize & 63] as char);
        out.push(T[(n >> 12) as usize & 63] as char);
        out.push(if c.len() > 1 { T[(n >> 6) as usize & 63] as char } else { '=' });
        out.push(if c.len() > 2 { T[n as usize & 63] as char } else { '=' });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base64_matches_rfc4648() {
        for (i, o) in [("", ""), ("f", "Zg=="), ("fo", "Zm8="), ("foo", "Zm9v"), ("foobar", "Zm9vYmFy")] {
            let mut s = String::new();
            base64(i.as_bytes(), &mut s);
            assert_eq!(s, o);
        }
    }

    #[test]
    fn resolves_paths() {
        let base = Path::new("/docs");
        assert_eq!(resolve("img/a.png", Some(base)), Ok(PathBuf::from("/docs/img/a.png")));
        assert_eq!(resolve("/abs/a.png", Some(base)), Ok(PathBuf::from("/abs/a.png")));
        assert_eq!(resolve("a%20b.png?raw=1", None), Ok(PathBuf::from("a b.png")));
        assert_eq!(resolve("https://x/a.png", Some(base)), Err(LoadError::Remote));
        assert_eq!(resolve("data:image/png;base64,AAAA", None), Err(LoadError::Remote));
    }

    #[test]
    fn geometry_keeps_aspect_ratio() {
        assert_eq!(cell_geometry((400, 200), (10, 20), 80, 30), (40, 10));
        assert_eq!(cell_geometry((4000, 2000), (10, 20), 80, 30), (80, 20));
        assert_eq!(cell_geometry((200, 2000), (10, 20), 80, 10), (2, 10));
    }
}
