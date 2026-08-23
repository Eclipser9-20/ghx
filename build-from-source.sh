#!/usr/bin/env bash
# Builds ghx from the source in this directory and installs it using the
# same layout and permission scheme as install.sh (system-wide with a
# shared-maintenance group if run as root, per-user under ~/.local
# otherwise). Use this instead of install.sh when you'd rather compile
# locally than download a release binary — e.g. on macOS before a release
# exists for your architecture, or to build with local changes.
#
# Usage: ./build-from-source.sh [--no-install]
#   --no-install   just build; leave the binary at target/release/ghx

set -euo pipefail
cd "$(dirname "${BASH_SOURCE[0]}")"

if ! command -v cargo >/dev/null 2>&1; then
    echo "cargo not found. Install Rust first: https://rustup.rs" >&2
    exit 1
fi

echo "==> cargo build --release"
cargo build --release

BIN="target/release/ghx"
if [ ! -f "$BIN" ]; then
    echo "Build succeeded but $BIN was not found." >&2
    exit 1
fi

if [ "${1:-}" = "--no-install" ]; then
    echo "==> Built: $BIN"
    exit 0
fi

GHX_LOCAL_BINARY="$(pwd)/$BIN" exec ./install.sh
