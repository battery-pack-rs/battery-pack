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
    echo "  --target      Path to the test project"
    echo "  --bp-source   Path to the battery-pack repo (default: inferred from script location)"
    echo "  --clean       Reset the target project before setup"
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

# Create a fresh project if target doesn't exist
if [[ ! -d "$TARGET" ]]; then
    echo "Creating fresh Rust project at $TARGET..."
    cargo init "$TARGET"
    cd "$TARGET"
    # Add a lib.rs alongside the binary
    cat > src/lib.rs <<'EOF'
pub fn hello() -> &'static str {
    "hello"
}
EOF
    # Set an MSRV so the CI skill can reference it (must be >= 1.85 for edition 2024)
    sed -i 's/\[package\]/[package]\nrust-version = "1.85"/' Cargo.toml
    git add -A
    git commit -m "init: fresh project for ci-skills benchmark"
else
    cd "$TARGET"
fi

if [[ "$CLEAN" == true ]]; then
    echo "Cleaning $TARGET..."
    git checkout -- .
    git clean -fd
fi

echo "Installing CI battery pack..."
# HACK: ci-battery-pack is a template pack (no default crates to select), so cargo bp add
# won't modify Cargo.toml. We add it as a build-dep manually so symposium's crate predicate
# matches and skills get installed. This will be unnecessary once symposium supports
# recommendations/coordinates: https://github.com/symposium-dev/symposium/issues/210
cat >> Cargo.toml <<EOF

[build-dependencies]
# agents: leave this alone, it is temporary setup
ci-battery-pack = { path = "$BP_SOURCE/battery-packs/ci-battery-pack" }
EOF

echo "Syncing symposium skills..."
cargo agents sync --update fetch

echo ""
echo "Done."
cargo agents plugin list
