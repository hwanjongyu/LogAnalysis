#!/usr/bin/env bash
set -euo pipefail

# Project root directory
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"
APPDIR="${PROJECT_ROOT}/target/appimage/LogAnalysis.AppDir"
OUTPUT_DIR="${PROJECT_ROOT}/dist"
OUTPUT_APPIMAGE="${OUTPUT_DIR}/LogAnalysis-x86_64.AppImage"

echo "=================================================="
echo " Building LogAnalysis AppImage for Ubuntu / Linux"
echo "=================================================="

# 1. Cargo Release Build
echo "==> [1/4] Compiling optimized release binary..."
cargo build --release -p loganalysis-gui

BIN_SRC="${PROJECT_ROOT}/target/release/loganalysis"
if [ ! -f "${BIN_SRC}" ]; then
    echo "Error: Binary not found at ${BIN_SRC}" >&2
    exit 1
fi

# 2. Setup AppDir
echo "==> [2/4] Constructing AppDir structure..."
rm -rf "${APPDIR}"
mkdir -p "${APPDIR}/usr/bin"
mkdir -p "${APPDIR}/usr/share/applications"
mkdir -p "${APPDIR}/usr/share/icons/hicolor/256x256/apps"
mkdir -p "${OUTPUT_DIR}"

# Copy binary & strip
cp "${BIN_SRC}" "${APPDIR}/usr/bin/loganalysis"
chmod 755 "${APPDIR}/usr/bin/loganalysis"
if command -v strip >/dev/null 2>&1; then
    strip "${APPDIR}/usr/bin/loganalysis" 2>/dev/null || true
fi

# Copy Desktop Entry
cp "${PROJECT_ROOT}/assets/loganalysis.desktop" "${APPDIR}/loganalysis.desktop"
cp "${PROJECT_ROOT}/assets/loganalysis.desktop" "${APPDIR}/usr/share/applications/loganalysis.desktop"

# Copy Icon
if [ -f "${PROJECT_ROOT}/assets/loganalysis.png" ]; then
    cp "${PROJECT_ROOT}/assets/loganalysis.png" "${APPDIR}/loganalysis.png"
    cp "${PROJECT_ROOT}/assets/loganalysis.png" "${APPDIR}/usr/share/icons/hicolor/256x256/apps/loganalysis.png"
    ln -sf "loganalysis.png" "${APPDIR}/.DirIcon"
fi

# Create AppRun entrypoint
cat << 'EOF' > "${APPDIR}/AppRun"
#!/bin/sh
SELF=$(readlink -f "$0")
HERE=${SELF%/*}
export PATH="${HERE}/usr/bin:${PATH}"
export LD_LIBRARY_PATH="${HERE}/usr/lib:${LD_LIBRARY_PATH}"
exec "${HERE}/usr/bin/loganalysis" "$@"
EOF
chmod 755 "${APPDIR}/AppRun"

# 3. Setup appimagetool
echo "==> [3/4] Preparing appimagetool..."
TOOL_DIR="${PROJECT_ROOT}/target/tools"
mkdir -p "${TOOL_DIR}"
APPIMAGETOOL="${TOOL_DIR}/appimagetool-x86_64.AppImage"

if [ ! -f "${APPIMAGETOOL}" ]; then
    echo "Downloading official appimagetool..."
    curl -fsSL -o "${APPIMAGETOOL}" https://github.com/AppImage/AppImageKit/releases/download/continuous/appimagetool-x86_64.AppImage
    chmod 755 "${APPIMAGETOOL}"
fi

# 4. Generate AppImage
echo "==> [4/4] Generating ${OUTPUT_APPIMAGE}..."
export ARCH=x86_64
"${APPIMAGETOOL}" --appimage-extract-and-run "${APPDIR}" "${OUTPUT_APPIMAGE}"

chmod 755 "${OUTPUT_APPIMAGE}"

echo "=================================================="
echo " Successfully created AppImage:"
echo "   ${OUTPUT_APPIMAGE}"
ls -lh "${OUTPUT_APPIMAGE}"
echo "=================================================="
