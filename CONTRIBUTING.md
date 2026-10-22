# Contributing

Thanks for your interest in improving inkmd! Bug reports, rendering issues
and focused pull requests are all welcome.

## Before you start

inkmd aims to be a small, fast, single-purpose Unix tool. Changes are judged
against these principles, in order:

1. **Performance**: no new startup cost; expensive work only when the
   document needs it.
2. **Minimal design**: readable output without decoration.
3. **Markdown coverage**: render as much as is useful in a terminal.
4. **Graceful degradation**: every feature must have a readable fallback.

For larger changes or new dependencies, please open an issue first to discuss
the approach.

## Development

```bash
cargo build
cargo test                         # unit, rendering, golden and CLI tests
cargo test --no-default-features   # the build without optional features
cargo fmt
cargo clippy --all-targets -- -D warnings
```

Rendering changes usually change the golden snapshots in `tests/golden/`.
Regenerate them and **review the diff** before committing:

```bash
UPDATE_GOLDEN=1 cargo test --test golden
git diff tests/golden
```

In the snapshots, `ESC` is written as `\e`. To add a new case, put a Markdown
file in `tests/fixtures/`. It is picked up automatically.

To see how a change looks in a real terminal:

```bash
cargo run --release -- tests/fixtures/showcase.md
INKMD_DEBUG=1 cargo run --release -- tests/fixtures/showcase.md   # explains capability decisions
```

For performance-sensitive changes, compare before and after:

```bash
cargo bench
scripts/bench.sh
```

## Guidelines

- Keep the existing structure: parsing, layout (`render.rs`, `wrap.rs`),
  styling (`style.rs`) and terminal capabilities (`term.rs`) stay separate.
- **All text from the document must go through `sanitize`.** Never write
  document content to the output without it.
- New terminal features must be detected (preferably by query, not by
  terminal name), must be possible to disable, and must fall back to text.
- Add tests for new behavior: a focused test in `tests/render.rs` or
  `tests/cli.rs`, plus a fixture when it affects layout.

## Pull requests

- One topic per pull request, with a short description of the change and why
  it is needed. Include before/after output or screenshots for visual changes.
- Make sure `cargo fmt`, `cargo clippy` and `cargo test` pass. CI runs them
  too.

## License

Unless you explicitly state otherwise, any contribution intentionally
submitted for inclusion in this work by you, as defined in the Apache-2.0
license, shall be dual licensed under the MIT and Apache-2.0 licenses, without
any additional terms or conditions.
