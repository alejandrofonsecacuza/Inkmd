//! Print a benchmark document: `cargo run --release --example gendoc -- mixed 100`
#[path = "../benches/gen.rs"]
mod gen;

fn main() {
    let mut args = std::env::args().skip(1);
    let kind = args.next().unwrap_or_else(|| "mixed".into());
    let kb: usize = args.next().and_then(|s| s.parse().ok()).unwrap_or(100);
    let doc = match kind.as_str() {
        "prose" => gen::prose(kb),
        "code" => gen::code(kb),
        "tables" => gen::tables(kb),
        _ => gen::mixed(kb),
    };
    print!("{doc}");
}
