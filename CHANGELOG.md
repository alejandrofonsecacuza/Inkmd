# Changelog

All notable changes to this project are documented in this file. The format
is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and the
project follows [Semantic Versioning](https://semver.org/).

## [Unreleased]

## [0.1.0] - 2026-10-22

First public release.

### Added

- Markdown rendering in the terminal: CommonMark plus GFM tables, task lists,
  strikethrough, footnotes, alerts, autolinks and bare URLs, front matter,
  definition lists, wikilinks, approximate TeX math and a safe HTML subset.
- Syntax highlighting for ~220 languages, colored with the terminal palette.
- Inline images through the Kitty graphics and iTerm2 protocols, detected by
  querying the terminal, with text fallbacks.
- Headings scaled by level with Kitty's text sizing protocol.
- Unicode-aware wrapping, adaptive tables, ASCII mode, 16/256/24-bit color,
  `NO_COLOR` and `CLICOLOR_FORCE` support, and `ansi`, `dark` and `light`
  themes.
- Neutralization of control characters from documents.
- Unit, rendering, golden snapshot and CLI tests; criterion benchmarks and an
  end-to-end benchmark script.
