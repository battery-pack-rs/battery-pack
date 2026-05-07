#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
TARGET=""
BP_SOURCE=""
CLEAN=false

usage() {
    echo "Usage: ./setup.sh --target <path> [--bp-source <path>] [--clean]"
    echo ""
    echo "Options:"
    echo "  --target      Path to the test project (must be a git repo with Cargo.toml)"
    echo "  --bp-source   Path to the battery-pack repo (default: inferred from script location)"
    echo "  --clean       Reset the target project to a clean git state before setup"
    exit 1
}

while [[ $# -gt 0 ]]; do
    case "$1" in
        --target) TARGET="$2"; shift 2 ;;
        --bp-source) BP_SOURCE="$2"; shift 2 ;;
        --clean) CLEAN=true; shift ;;
        --help|-h) usage ;;
        *) usage ;;
    esac
done

[[ -z "$TARGET" ]] && usage
BP_SOURCE="${BP_SOURCE:-$(cd "$SCRIPT_DIR/../.." && pwd)}"

# Clone mini-redis if target doesn't exist
if [[ ! -d "$TARGET" ]]; then
    echo "Cloning tokio-rs/mini-redis into $TARGET..."
    git clone --depth 1 https://github.com/tokio-rs/mini-redis.git "$TARGET"
fi

cd "$TARGET"

if [[ "$CLEAN" == true ]]; then
    echo "Cleaning $TARGET..."
    git checkout -- .
    git clean -fd
fi

echo "Installing error battery pack..."
cargo bp add error --path "$BP_SOURCE/battery-packs/error-battery-pack" --non-interactive

echo "Syncing symposium skills..."
cargo agents sync --update fetch

echo ""
echo "Done."
cargo agents plugin list
