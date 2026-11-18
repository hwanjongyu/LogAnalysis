#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"
cd "${PROJECT_ROOT}"

echo "======================================================================"
echo " LogAnalysis Workspace Status Dashboard"
echo "======================================================================"
echo "Generated: $(date '+%Y-%m-%d %H:%M:%S %Z')"
echo ""

echo "[1. Git Working Tree]"
git status -s || true
echo ""

echo "[2. Recent Commits]"
git log -5 --oneline || true
echo ""

echo "[3. Workspace Crate Inventory]"
echo "  - crates/loganalysis-core (headless parsing, buffers, filtering, search, export, adb)"
echo "  - crates/loganalysis-gui  (GPUI desktop frontend: workspace, controllers, components, modals)"
echo ""

echo "[4. Verification Commands]"
echo "  cargo check --workspace --all-targets"
echo "  cargo clippy --workspace --all-targets -- -D warnings"
echo "  cargo test --workspace"
echo "  RUSTDOCFLAGS=\"-D warnings\" cargo doc --workspace --no-deps"
echo "======================================================================"
