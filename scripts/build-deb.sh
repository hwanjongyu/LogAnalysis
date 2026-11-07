#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"
DIST_DIR="${PROJECT_ROOT}/dist"
STAGE_DIR="${PROJECT_ROOT}/target/deb/loganalysis_amd64"

echo "=================================================="
echo " Building LogAnalysis Debian (.deb) Package"
echo "=================================================="

# 1. Compile release binary if missing
BIN_SRC="${PROJECT_ROOT}/target/release/loganalysis"
if [ ! -f "${BIN_SRC}" ]; then
    echo "==> [1/3] Compiling release binary..."
    cargo build --release -p loganalysis-gui
fi

# Extract version from Cargo.toml
VERSION=$(grep -m 1 '^version = ' "${PROJECT_ROOT}/crates/loganalysis-gui/Cargo.toml" | cut -d '"' -f2)
if [ -z "${VERSION}" ]; then
    VERSION="0.2.2"
fi

echo "==> [2/3] Staging Debian package layout (v${VERSION})..."
rm -rf "${STAGE_DIR}"
mkdir -p "${STAGE_DIR}/DEBIAN"
mkdir -p "${STAGE_DIR}/usr/bin"
mkdir -p "${STAGE_DIR}/usr/share/applications"
mkdir -p "${STAGE_DIR}/usr/share/icons/hicolor/256x256/apps"
mkdir -p "${DIST_DIR}"

# Install binary
cp "${BIN_SRC}" "${STAGE_DIR}/usr/bin/loganalysis"
chmod 755 "${STAGE_DIR}/usr/bin/loganalysis"
if command -v strip >/dev/null 2>&1; then
    strip "${STAGE_DIR}/usr/bin/loganalysis" 2>/dev/null || true
fi

# Install Desktop Entry
if [ -f "${PROJECT_ROOT}/assets/loganalysis.desktop" ]; then
    cp "${PROJECT_ROOT}/assets/loganalysis.desktop" "${STAGE_DIR}/usr/share/applications/loganalysis.desktop"
    chmod 644 "${STAGE_DIR}/usr/share/applications/loganalysis.desktop"
fi

# Install Icon
if [ -f "${PROJECT_ROOT}/assets/loganalysis.png" ]; then
    cp "${PROJECT_ROOT}/assets/loganalysis.png" "${STAGE_DIR}/usr/share/icons/hicolor/256x256/apps/loganalysis.png"
    chmod 644 "${STAGE_DIR}/usr/share/icons/hicolor/256x256/apps/loganalysis.png"
fi

# Calculate installed size in KB
INSTALLED_SIZE=$(du -sk "${STAGE_DIR}/usr" | cut -f1)

# Write DEBIAN/control
cat << EOF > "${STAGE_DIR}/DEBIAN/control"
Package: loganalysis
Version: ${VERSION}
Architecture: amd64
Maintainer: Drew Yu
Installed-Size: ${INSTALLED_SIZE}
Section: utils
Priority: optional
Homepage: https://github.com/hwanjongyu/loganalysis
Description: GPU-accelerated desktop log analysis and Android logcat explorer
 LogAnalysis is an ultra-fast desktop log analysis and real-time Android
 adb logcat exploration tool built with Rust and GPUI.
EOF

# 3. Build .deb package
OUTPUT_DEB="${DIST_DIR}/loganalysis_${VERSION}_amd64.deb"
DEFAULT_DEB="${DIST_DIR}/loganalysis_amd64.deb"
echo "==> [3/3] Assembling debian package with dpkg-deb..."
dpkg-deb --build --root-owner-group "${STAGE_DIR}" "${OUTPUT_DEB}"
cp "${OUTPUT_DEB}" "${DEFAULT_DEB}"

echo "=================================================="
echo " Successfully created Debian package:"
echo "   ${OUTPUT_DEB}"
echo "   ${DEFAULT_DEB}"
ls -lh "${OUTPUT_DEB}"
echo "=================================================="
