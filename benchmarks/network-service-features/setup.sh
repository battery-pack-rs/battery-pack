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
    echo "  --target      Path to the generated service project (created by 'cargo bp new')"
    echo "  --bp-source   Path to the battery-pack repo (default: inferred from script location)"
    echo "  --clean       Delete and regenerate the target project before setup"
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
PACK="$BP_SOURCE/battery-packs/network-service-battery-pack"

if [[ "$CLEAN" == true ]]; then
    echo "Removing $TARGET..."
    rm -rf "$TARGET"
fi

# The rate-limit feature must be on so there is a global limiter to upgrade.
if [[ ! -f "$TARGET/Cargo.toml" ]]; then
    PARENT="$(dirname "$TARGET")"
    NAME="$(basename "$TARGET")"
    mkdir -p "$PARENT"
    echo "Generating network-service into $TARGET..."
    (cd "$PARENT" && cargo bp new network-service-battery-pack \
        --name "$NAME" --template service --path "$PACK" --non-interactive \
        -d allocator=jemalloc -d rate_limit=true -d benchmarks=true)
    (cd "$TARGET" && git init -q && git add -A && git commit -qm "scaffold" >/dev/null)
fi

cd "$TARGET"

echo "Syncing symposium skills..."
cargo agents sync --update fetch

echo ""
echo "Done."
cargo agents plugin list
