#!/usr/bin/env bash
# Install git pre-commit hook for the music_browser project.
# Run from the repo root: bash music_browser/scripts/install-hooks.sh

set -euo pipefail

REPO_ROOT="$(git rev-parse --show-toplevel)"
HOOK_FILE="$REPO_ROOT/.git/hooks/pre-commit"

cat > "$HOOK_FILE" << 'EOF'
#!/usr/bin/env bash
set -euo pipefail

cd "$(git rev-parse --show-toplevel)/music_browser"

echo "==> cargo fmt --check"
cargo fmt -- --check || { echo "❌ fmt failed. Run: cargo fmt"; exit 1; }

echo "==> cargo clippy"
cargo clippy -- -D warnings || { echo "❌ clippy failed"; exit 1; }

echo "==> cargo test"
cargo test || { echo "❌ tests failed"; exit 1; }

echo "✅ All pre-commit checks passed"
EOF

chmod +x "$HOOK_FILE"
echo "Installed pre-commit hook at $HOOK_FILE"
