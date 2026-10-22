//! Criterion benchmarks. Run with `cargo bench`.
//!
//! `parse/*` measures pulldown-cmark alone (iterating events), `render/*`
//! the full pipeline into memory (parse + layout + SGR), so the difference
//! is inkmd's own cost. Writing to the terminal is measured separately by
//! scripts/bench.sh, end to end.

use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use inkmd::{render_to_string, ColorLevel, Options, Painter};
use std::hint::black_box;

#[path = "gen.rs"]
mod gen;

fn docs() -> Vec<(&'static str, String)> {
    vec![
        ("small-2k", gen::mixed(2)),
        ("medium-100k", gen::mixed(100)),
        ("large-5m", gen::mixed(5 * 1024)),
        ("prose-100k", gen::prose(100)),
        ("code-100k", gen::code(100)),
        ("tables-100k", gen::tables(100)),
    ]
}

fn bench(c: &mut Criterion) {
    let plain = Options { width: 80, ..Options::default() };
    let styled =
        Options { width: 80, painter: Painter { level: ColorLevel::Ansi16, attrs: true }, ..Options::default() };
    let docs = docs();
    for (group, opts) in [("parse", None), ("render-plain", Some(&plain)), ("render-styled", Some(&styled))] {
        let mut g = c.benchmark_group(group);
        for (name, doc) in &docs {
            if name.starts_with("large") {
                g.sample_size(10);
            } else {
                g.sample_size(50);
            }
            g.throughput(Throughput::Bytes(doc.len() as u64));
            g.bench_with_input(BenchmarkId::from_parameter(name), doc, |b, doc| match opts {
                None => b.iter(|| {
                    let n = pulldown_cmark::Parser::new_ext(doc, inkmd::parser_options()).count();
                    black_box(n)
                }),
                Some(o) => b.iter(|| black_box(render_to_string(doc, o))),
            });
        }
        g.finish();
    }
}

criterion_group!(benches, bench);
criterion_main!(benches);
