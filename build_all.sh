#!/usr/bin/env bash
# ═══════════════════════════════════════════════════════════════════════
# TKS Chain Node — Cross-Platform Build Script
# Compiles the TKS Substrate node for Linux, macOS, and Windows
# ═══════════════════════════════════════════════════════════════════════
#
# Usage:
#   ./build_all.sh              # Build for all platforms
#   ./build_all.sh linux        # Build for Linux only
#   ./build_all.sh macos        # Build for macOS only
#   ./build_all.sh windows      # Build for Windows only
#   ./build_all.sh native       # Build for current platform only
#
# Prerequisites:
#   - Rust toolchain (rustup)
#   - For cross-compilation: cross (cargo install cross)
#   - For Windows: mingw-w64 toolchain
#   - WASM target: wasm32-unknown-unknown (for Substrate runtime)

set -euo pipefail

# ─── Configuration ──────────────────────────────────────────────────

BINARY_NAME="tks-chain-node"
VERSION=$(grep '^version' node/Cargo.toml | head -1 | sed 's/.*"\(.*\)".*/\1/')
if [ -z "$VERSION" ]; then
    VERSION="0.1.0"
fi

OUTPUT_DIR="./releases/v${VERSION}"
BUILD_MODE="release"
CARGO_FLAGS="--release"

RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
CYAN='\033[0;36m'
NC='\033[0m' # No Color

log_info()  { echo -e "${CYAN}[INFO]${NC}  $1"; }
log_ok()    { echo -e "${GREEN}[OK]${NC}    $1"; }
log_warn()  { echo -e "${YELLOW}[WARN]${NC}  $1"; }
log_error() { echo -e "${RED}[ERROR]${NC} $1"; }

# ─── Setup ──────────────────────────────────────────────────────────

echo ""
echo "╔══════════════════════════════════════════════════════════╗"
echo "║       TKS Chain Node — Cross-Platform Builder           ║"
echo "║       Version: ${VERSION}                                     ║"
echo "╚══════════════════════════════════════════════════════════╝"
echo ""

mkdir -p "${OUTPUT_DIR}"

# Ensure WASM target is installed (required for Substrate runtime)
log_info "Checking WASM target..."
rustup target add wasm32-unknown-unknown 2>/dev/null || true
log_ok "WASM target ready"

# ─── Build Functions ────────────────────────────────────────────────

build_native() {
    log_info "Building for native platform..."
    cargo build ${CARGO_FLAGS} -p tks-chain-node

    local native_binary="./target/release/${BINARY_NAME}"
    if [ -f "$native_binary" ]; then
        local os=$(uname -s | tr '[:upper:]' '[:lower:]')
        local arch=$(uname -m)
        local output_name="${BINARY_NAME}-${VERSION}-${os}-${arch}"
        cp "$native_binary" "${OUTPUT_DIR}/${output_name}"
        log_ok "Native build: ${OUTPUT_DIR}/${output_name}"
    else
        log_error "Native build failed — binary not found"
        return 1
    fi
}

build_linux() {
    log_info "Building for Linux (x86_64)..."

    local target="x86_64-unknown-linux-gnu"
    local output_name="${BINARY_NAME}-${VERSION}-linux-x86_64"

    # Check if we're already on Linux
    if [ "$(uname -s)" = "Linux" ]; then
        cargo build ${CARGO_FLAGS} -p tks-chain-node
        cp "./target/release/${BINARY_NAME}" "${OUTPUT_DIR}/${output_name}"
    else
        # Cross-compile using 'cross' tool
        if command -v cross &> /dev/null; then
            cross build ${CARGO_FLAGS} -p tks-chain-node --target "${target}"
            cp "./target/${target}/release/${BINARY_NAME}" "${OUTPUT_DIR}/${output_name}"
        else
            log_warn "'cross' not installed. Install with: cargo install cross"
            log_info "Trying Docker-based cross compilation..."

            # Fallback: use Docker directly
            if command -v docker &> /dev/null; then
                docker run --rm \
                    -v "$(pwd)":/build \
                    -w /build \
                    paritytech/ci-linux:production \
                    cargo build ${CARGO_FLAGS} -p tks-chain-node

                if [ -f "./target/release/${BINARY_NAME}" ]; then
                    cp "./target/release/${BINARY_NAME}" "${OUTPUT_DIR}/${output_name}"
                fi
            else
                log_error "Neither 'cross' nor 'docker' available for Linux cross-compilation"
                log_info "Install cross: cargo install cross"
                return 1
            fi
        fi
    fi

    if [ -f "${OUTPUT_DIR}/${output_name}" ]; then
        log_ok "Linux build: ${OUTPUT_DIR}/${output_name}"
        # Create compressed archive
        tar -czf "${OUTPUT_DIR}/${output_name}.tar.gz" -C "${OUTPUT_DIR}" "${output_name}"
        log_ok "Archive: ${OUTPUT_DIR}/${output_name}.tar.gz"
    else
        log_error "Linux build failed"
        return 1
    fi
}

build_macos() {
    log_info "Building for macOS..."

    if [ "$(uname -s)" = "Darwin" ]; then
        local arch=$(uname -m)
        local output_name="${BINARY_NAME}-${VERSION}-macos-${arch}"

        cargo build ${CARGO_FLAGS} -p tks-chain-node
        cp "./target/release/${BINARY_NAME}" "${OUTPUT_DIR}/${output_name}"

        if [ -f "${OUTPUT_DIR}/${output_name}" ]; then
            log_ok "macOS build: ${OUTPUT_DIR}/${output_name}"
            # Create compressed archive
            tar -czf "${OUTPUT_DIR}/${output_name}.tar.gz" -C "${OUTPUT_DIR}" "${output_name}"
            log_ok "Archive: ${OUTPUT_DIR}/${output_name}.tar.gz"
        fi

        # If on Apple Silicon, also try building for x86_64 (Intel Macs)
        if [ "$arch" = "arm64" ]; then
            log_info "Also building for macOS x86_64 (Intel)..."
            rustup target add x86_64-apple-darwin 2>/dev/null || true
            if cargo build ${CARGO_FLAGS} -p tks-chain-node --target x86_64-apple-darwin 2>/dev/null; then
                local intel_output="${BINARY_NAME}-${VERSION}-macos-x86_64"
                cp "./target/x86_64-apple-darwin/release/${BINARY_NAME}" "${OUTPUT_DIR}/${intel_output}"
                tar -czf "${OUTPUT_DIR}/${intel_output}.tar.gz" -C "${OUTPUT_DIR}" "${intel_output}"
                log_ok "macOS Intel build: ${OUTPUT_DIR}/${intel_output}"
            else
                log_warn "macOS Intel (x86_64) cross-build failed — this is OK, ARM build is available"
            fi
        fi
    else
        log_warn "macOS builds can only be done on macOS (Apple does not allow cross-compilation)"
        log_info "Use a macOS CI runner (GitHub Actions macos-latest) for macOS builds"
        return 1
    fi
}

build_windows() {
    log_info "Building for Windows (x86_64)..."

    local target="x86_64-pc-windows-gnu"
    local output_name="${BINARY_NAME}-${VERSION}-windows-x86_64.exe"

    rustup target add ${target} 2>/dev/null || true

    # Check for mingw-w64 linker
    if command -v x86_64-w64-mingw32-gcc &> /dev/null; then
        # Set up cross-compilation environment
        export CARGO_TARGET_X86_64_PC_WINDOWS_GNU_LINKER=x86_64-w64-mingw32-gcc
        export CC_x86_64_pc_windows_gnu=x86_64-w64-mingw32-gcc
        export AR_x86_64_pc_windows_gnu=x86_64-w64-mingw32-ar

        if cargo build ${CARGO_FLAGS} -p tks-chain-node --target "${target}" 2>/dev/null; then
            cp "./target/${target}/release/${BINARY_NAME}.exe" "${OUTPUT_DIR}/${output_name}"
            log_ok "Windows build: ${OUTPUT_DIR}/${output_name}"
            # Create zip archive for Windows
            if command -v zip &> /dev/null; then
                (cd "${OUTPUT_DIR}" && zip "${BINARY_NAME}-${VERSION}-windows-x86_64.zip" "${output_name}")
                log_ok "Archive: ${OUTPUT_DIR}/${BINARY_NAME}-${VERSION}-windows-x86_64.zip"
            fi
        else
            log_error "Windows build failed"
            return 1
        fi
    elif command -v cross &> /dev/null; then
        # Use cross for Windows builds
        if cross build ${CARGO_FLAGS} -p tks-chain-node --target "${target}" 2>/dev/null; then
            cp "./target/${target}/release/${BINARY_NAME}.exe" "${OUTPUT_DIR}/${output_name}"
            log_ok "Windows build: ${OUTPUT_DIR}/${output_name}"
        else
            log_error "Windows cross-build failed"
            return 1
        fi
    else
        log_warn "Windows cross-compilation requires mingw-w64 or 'cross'"
        log_info "Install: brew install mingw-w64  (macOS)"
        log_info "Install: apt install mingw-w64   (Linux)"
        log_info "Or:      cargo install cross"
        return 1
    fi
}

# ─── GitHub Actions CI Workflow ─────────────────────────────────────

generate_ci_workflow() {
    local ci_dir=".github/workflows"
    mkdir -p "${ci_dir}"

    cat > "${ci_dir}/build-release.yml" << 'CIEOF'
name: Build TKS Chain Node (Multi-Platform)

on:
  push:
    tags: ['v*']
  workflow_dispatch:

env:
  CARGO_TERM_COLOR: always

jobs:
  build:
    strategy:
      matrix:
        include:
          - os: ubuntu-22.04
            target: x86_64-unknown-linux-gnu
            artifact_name: tks-chain-node-linux-x86_64
            archive_ext: tar.gz
          - os: macos-14
            target: aarch64-apple-darwin
            artifact_name: tks-chain-node-macos-arm64
            archive_ext: tar.gz
          - os: macos-13
            target: x86_64-apple-darwin
            artifact_name: tks-chain-node-macos-x86_64
            archive_ext: tar.gz
          - os: windows-2022
            target: x86_64-pc-windows-msvc
            artifact_name: tks-chain-node-windows-x86_64.exe
            archive_ext: zip

    runs-on: ${{ matrix.os }}

    steps:
      - uses: actions/checkout@v4

      - name: Install Rust
        uses: dtolnay/rust-toolchain@stable
        with:
          targets: wasm32-unknown-unknown,${{ matrix.target }}

      - name: Install Protobuf (Linux)
        if: runner.os == 'Linux'
        run: sudo apt-get update && sudo apt-get install -y protobuf-compiler

      - name: Install Protobuf (macOS)
        if: runner.os == 'macOS'
        run: brew install protobuf

      - name: Install Protobuf (Windows)
        if: runner.os == 'Windows'
        run: choco install protoc

      - name: Build
        run: cargo build --release -p tks-chain-node --target ${{ matrix.target }}

      - name: Package (Unix)
        if: runner.os != 'Windows'
        run: |
          cp target/${{ matrix.target }}/release/tks-chain-node ${{ matrix.artifact_name }}
          tar -czf ${{ matrix.artifact_name }}.${{ matrix.archive_ext }} ${{ matrix.artifact_name }}

      - name: Package (Windows)
        if: runner.os == 'Windows'
        run: |
          copy target\${{ matrix.target }}\release\tks-chain-node.exe ${{ matrix.artifact_name }}
          Compress-Archive -Path ${{ matrix.artifact_name }} -DestinationPath ${{ matrix.artifact_name }}.zip

      - name: Upload Artifact
        uses: actions/upload-artifact@v4
        with:
          name: ${{ matrix.artifact_name }}
          path: ${{ matrix.artifact_name }}.${{ matrix.archive_ext }}

  release:
    needs: build
    runs-on: ubuntu-latest
    if: startsWith(github.ref, 'refs/tags/v')
    permissions:
      contents: write

    steps:
      - name: Download all artifacts
        uses: actions/download-artifact@v4

      - name: Create GitHub Release
        uses: softprops/action-gh-release@v2
        with:
          files: |
            tks-chain-node-linux-x86_64/*.tar.gz
            tks-chain-node-macos-arm64/*.tar.gz
            tks-chain-node-macos-x86_64/*.tar.gz
            tks-chain-node-windows-x86_64.exe/*.zip
          draft: false
          prerelease: false
          generate_release_notes: true
CIEOF

    log_ok "GitHub Actions CI workflow: ${ci_dir}/build-release.yml"
}

# ─── Docker Multi-Arch Build ───────────────────────────────────────

build_docker() {
    log_info "Building Docker image (linux/amd64)..."

    if command -v docker &> /dev/null; then
        docker build \
            -t "tks-chain-node:${VERSION}" \
            -t "tks-chain-node:latest" \
            -f Dockerfile \
            ..

        log_ok "Docker image: tks-chain-node:${VERSION}"

        # Export image as tar for distribution
        docker save "tks-chain-node:${VERSION}" | gzip > "${OUTPUT_DIR}/tks-chain-node-${VERSION}-docker.tar.gz"
        log_ok "Docker archive: ${OUTPUT_DIR}/tks-chain-node-${VERSION}-docker.tar.gz"
    else
        log_warn "Docker not available — skipping Docker build"
    fi
}

# ─── Main ───────────────────────────────────────────────────────────

TARGET="${1:-all}"

case "$TARGET" in
    native)
        build_native
        ;;
    linux)
        build_linux
        ;;
    macos)
        build_macos
        ;;
    windows)
        build_windows
        ;;
    docker)
        build_docker
        ;;
    ci)
        generate_ci_workflow
        ;;
    all)
        log_info "Building for all platforms..."
        echo ""

        build_native || log_warn "Native build failed"
        echo ""
        build_linux || log_warn "Linux build skipped"
        echo ""
        build_macos || log_warn "macOS build skipped"
        echo ""
        build_windows || log_warn "Windows build skipped"
        echo ""
        build_docker || log_warn "Docker build skipped"
        echo ""
        generate_ci_workflow
        ;;
    *)
        echo "Usage: $0 {native|linux|macos|windows|docker|ci|all}"
        exit 1
        ;;
esac

echo ""
echo "╔══════════════════════════════════════════════════════════╗"
echo "║                  Build Complete!                         ║"
echo "╚══════════════════════════════════════════════════════════╝"
echo ""
log_info "Outputs in: ${OUTPUT_DIR}/"
ls -lh "${OUTPUT_DIR}/" 2>/dev/null || true
echo ""
