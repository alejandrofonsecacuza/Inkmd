#!/usr/bin/env bash
# End-to-end benchmark of the release binary: wall time and peak memory.
#
#   scripts/bench.sh            # uses hyperfine when installed
#
# Documents are generated deterministically into target/bench-docs.
set -euo pipefail
cd "$(dirname "$0")/.."

cargo build --release --quiet
cargo build --release --quiet --example gendoc
BIN=target/release/inkmd
GEN=target/release/examples/gendoc
DIR=target/bench-docs
mkdir -p "$DIR"

declare -A DOCS=(
  [small]="mixed 2"
  [medium]="mixed 100"
  [large]="mixed 5120"
  [code]="code 100"
  [tables]="tables 100"
  [prose]="prose 100"
)
for name in small medium large code tables prose; do
  # shellcheck disable=SC2086
  "$GEN" ${DOCS[$name]} > "$DIR/$name.md"
done

median_ms() { # median wall time of N runs, in ms
  local n=$1; shift
  local times=()
  for ((i = 0; i < n; i++)); do
    local s e
    s=$(date +%s%N)
    "$@" > /dev/null
    e=$(date +%s%N)
    times+=($(( (e - s) / 1000 )))
  done
  printf '%s\n' "${times[@]}" | sort -n | awk '{a[NR]=$1} END {printf "%.2f", a[int((NR+1)/2)]/1000}'
}

peak_kb() {
  if [[ -x /usr/bin/time ]]; then
    /usr/bin/time -f '%M' "$@" 2>&1 > /dev/null | tail -1
  else
    echo "n/a"
  fi
}

printf '%-8s %10s %14s %14s %12s\n' doc size plain_ms styled_ms peak_rss_kb
for name in small medium large code tables prose; do
  f="$DIR/$name.md"
  size=$(du -h "$f" | cut -f1)
  runs=30; [[ $name == large ]] && runs=5
  p=$(median_ms $runs "$BIN" --color never "$f")
  c=$(median_ms $runs "$BIN" --color always "$f")
  m=$(peak_kb "$BIN" --color always "$f")
  printf '%-8s %10s %14s %14s %12s\n' "$name" "$size" "$p" "$c" "$m"
done

if command -v hyperfine > /dev/null; then
  hyperfine -N --warmup 5 \
    "$BIN --color always $DIR/small.md" \
    "$BIN --color always $DIR/medium.md" \
    "$BIN --color always $DIR/large.md"
fi
