//! inkmd: render Markdown in the terminal.
//!
//! The crate is split in small layers:
//! * [`render`] walks the pulldown-cmark event stream and lays out blocks;
//! * [`wrap`] measures and wraps styled text;
//! * [`style`] turns abstract styles into SGR sequences for a color depth;
//! * [`term`] detects terminal capabilities;
//! * [`highlight`], [`image`], [`html`], [`math`] provide optional features;
//! * [`sanitize`] neutralizes control characters from the document.

pub mod highlight;
pub mod html;
pub mod image;
pub mod math;
pub mod render;
pub mod sanitize;
pub mod style;
pub mod term;
pub mod wrap;

pub use render::{parser_options, render, render_to_string, BigHeadings, ImageMode, Options, Session};
pub use style::{ColorLevel, Painter, Theme, ThemeName};
