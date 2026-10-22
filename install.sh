#!/bin/sh
# Install the latest inkmd release on Linux or macOS.
#
#   curl -fsSL https://raw.githubusercontent.com/alejandrofonsecacuza/Inkmd/main/install.sh | sh
#
# Environment variables:
#   INKMD_VERSION       version to install, e.g. v1.0.0 (default: latest)
#   INKMD_INSTALL_DIR   where to put the binary (default: ~/.local/bin)
set -eu

REPO="alejandrofonsecacuza/Inkmd"
BIN_DIR="${INKMD_INSTALL_DIR:-$HOME/.local/bin}"
MAN_DIR="$(dirname "$BIN_DIR")/share/man/man1"

say() { printf 'inkmd: %s\n' "$*"; }
die() { printf 'inkmd: error: %s\n' "$*" >&2; exit 1; }

if command -v curl > /dev/null 2>&1; then
    fetch() { curl -fsSL "$1" -o "$2"; }
    latest_url() { curl -fsSLI -o /dev/null -w '%{url_effective}' "$1"; }
elif command -v wget > /dev/null 2>&1; then
    fetch() { wget -qO "$2" "$1"; }
    latest_url() { wget -S --spider "$1" 2>&1 | sed -n 's/^ *[Ll]ocation: *\([^ ]*\).*/\1/p' | tail -n 1; }
else
    die "curl or wget is required"
fi

os=$(uname -s)
arch=$(uname -m)
case "$arch" in
    x86_64 | amd64) arch=x86_64 ;;
    aarch64 | arm64) arch=aarch64 ;;
    *) die "unsupported architecture: $arch" ;;
esac
case "$os" in
    Linux) target="$arch-unknown-linux-musl" ;;
    Darwin)
        # A shell running under Rosetta reports x86_64 on Apple Silicon.
        if [ "$arch" = x86_64 ] && [ "$(sysctl -n hw.optional.arm64 2> /dev/null || echo 0)" = 1 ]; then
            arch=aarch64
        fi
        target="$arch-apple-darwin"
        ;;
    *) die "unsupported operating system: $os (Linux and macOS are supported)" ;;
esac

version="${INKMD_VERSION:-}"
if [ -z "$version" ]; then
    url=$(latest_url "https://github.com/$REPO/releases/latest") || die "cannot reach GitHub"
    version="${url##*/}"
    case "$version" in
        v*) ;;
        *) die "could not determine the latest version" ;;
    esac
fi

name="inkmd-$version-$target"
base="https://github.com/$REPO/releases/download/$version"
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT INT TERM

say "downloading $name"
fetch "$base/$name.tar.gz" "$tmp/$name.tar.gz" || die "download failed: $base/$name.tar.gz"
fetch "$base/$name.tar.gz.sha256" "$tmp/$name.tar.gz.sha256" || die "checksum download failed"

expected=$(cut -d ' ' -f 1 < "$tmp/$name.tar.gz.sha256")
if command -v sha256sum > /dev/null 2>&1; then
    actual=$(sha256sum "$tmp/$name.tar.gz" | cut -d ' ' -f 1)
else
    actual=$(shasum -a 256 "$tmp/$name.tar.gz" | cut -d ' ' -f 1)
fi
[ "$expected" = "$actual" ] || die "checksum mismatch for $name.tar.gz"

tar xzf "$tmp/$name.tar.gz" -C "$tmp"
mkdir -p "$BIN_DIR"
cp "$tmp/$name/inkmd" "$BIN_DIR/inkmd"
chmod 755 "$BIN_DIR/inkmd"
say "installed $BIN_DIR/inkmd ($version)"

if [ -f "$tmp/$name/inkmd.1" ]; then
    mkdir -p "$MAN_DIR"
    cp "$tmp/$name/inkmd.1" "$MAN_DIR/inkmd.1"
    say "installed manual page: man inkmd"
fi

case ":$PATH:" in
    *":$BIN_DIR:"*) ;;
    *)
        say "$BIN_DIR is not in your PATH. Add this line to your ~/.zshrc or ~/.bashrc:"
        printf '\n    export PATH="%s:$PATH"\n\n' "$BIN_DIR"
        ;;
esac
