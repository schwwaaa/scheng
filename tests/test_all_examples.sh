#!/usr/bin/env bash

set -euo pipefail

REPO_ROOT="$(pwd)"
TARGET_DIR="$REPO_ROOT/target"
TIMEOUT_SECONDS=5

echo "======================================"
echo " ShadeCore Full Example Test Runner"
echo "======================================"
echo ""

# --------------------------------------
# 1. Sanity checks
# --------------------------------------

if [[ ! -f "Cargo.toml" ]]; then
  echo "❌ Must run from repo root (Cargo.toml not found)"
  exit 1
fi

if [[ "$OSTYPE" == "darwin"* ]]; then
  if [[ ! -d "vendor/Syphon.framework" ]]; then
    echo "❌ vendor/Syphon.framework not found"
    exit 1
  fi
  echo "✅ Syphon.framework found"
fi

echo ""

# --------------------------------------
# 2. Clean build
# --------------------------------------

echo "🧹 Cleaning..."
cargo clean

echo ""
echo "🔨 Building workspace..."
cargo build --workspace

echo ""
echo "🧪 Running tests..."
cargo test --workspace --all-targets

echo ""
echo "📦 Building all examples..."
cargo build --workspace --examples

echo ""

# --------------------------------------
# 3. Discover and run examples
# --------------------------------------

echo "🚀 Running examples (each for ${TIMEOUT_SECONDS}s)..."
echo ""

EXAMPLES=$(cargo metadata --format-version 1 \
  | jq -r '.packages[].targets[] | select(.kind[]=="example") | .name')

if [[ -z "$EXAMPLES" ]]; then
  echo "⚠️  No examples found."
  exit 0
fi

for EX in $EXAMPLES; do
  echo "--------------------------------------"
  echo "▶ Running example: $EX"
  echo "--------------------------------------"

  if command -v gtimeout >/dev/null 2>&1; then
    gtimeout ${TIMEOUT_SECONDS}s cargo run --example "$EX" || true
  else
    (cargo run --example "$EX" &) 
    PID=$!
    sleep $TIMEOUT_SECONDS
    kill $PID >/dev/null 2>&1 || true
  fi

  echo ""
done

echo "======================================"
echo "✅ All examples executed"
echo "======================================"
