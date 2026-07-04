# Build and Release

## CI/CD Pipeline

TKS uses GitHub Actions for automated multi-platform builds and releases. Two workflows are active:

| Workflow | File | Trigger |
|----------|------|---------|
| Build TKS Chain Node | `.github/workflows/build-release.yml` | Push to `v*` tag, manual |
| TKS Node Release | `.github/workflows/release.yml` | Push to `v*` tag, manual |

---

## Build Matrix

Binaries are built for 5 platform/architecture targets using **native** runners (no cross-compilation):

| Platform | Architecture | Runner | Rust Target |
|----------|-------------|--------|-------------|
| macOS | Apple Silicon (arm64) | `macos-14` | `aarch64-apple-darwin` |
| macOS | Intel (x86_64) | `macos-13` | `x86_64-apple-darwin` |
| Linux | x86_64 | `ubuntu-22.04` | `x86_64-unknown-linux-gnu` |
| Linux | ARM64 | `ubuntu-22.04-arm` | `aarch64-unknown-linux-gnu` |
| Windows | x86_64 | `windows-2022` | `x86_64-pc-windows-msvc` |

Native ARM64 runners are used for Linux arm64 to avoid cross-compilation complexity.

---

## Rust Toolchain

The toolchain is pinned via `rust-toolchain.toml` in the repository root:

```toml
[toolchain]
channel = "stable"
targets = ["wasm32-unknown-unknown"]
components = ["rust-src"]
```

The CI additionally installs the platform's native target:

```yaml
- uses: dtolnay/rust-toolchain@stable
  with:
    targets: wasm32-unknown-unknown,${{ matrix.target }}
    components: rust-src
```

---

## Platform Dependencies

### Linux

```bash
sudo apt-get install -y \
  build-essential pkg-config libssl-dev \
  clang libclang-dev protobuf-compiler
```

### macOS

```bash
HOMEBREW_NO_REQUIRE_TAP_TRUST=1 brew install protobuf llvm
export LIBCLANG_PATH="$(brew --prefix llvm)/lib"
```

### Windows

- LLVM installed via Chocolatey (`choco install llvm`)
- `protoc` installed via `arduino/setup-protoc@v3` GitHub Action
- `LIBCLANG_PATH=C:\Program Files\LLVM\bin`

---

## WASM Runtime Build

The runtime is compiled to both native and WASM. The WASM build is handled automatically by `substrate-wasm-builder` as part of `cargo build`.

Key environment variable for CI:

```yaml
WASM_BUILD_RUSTFLAGS: "-C link-arg=--allow-undefined"
```

This allows `rust-lld` (Rust ≥ 1.88) to treat Substrate host functions (`ext_storage_*`, `ext_hashing_*`, etc.) as WASM imports resolved at runtime by the host environment.

---

## Release Process

### Creating a Release

1. Ensure all changes are merged to `main`
2. Create and push a version tag:

```bash
git tag v0.2.0
git push origin v0.2.0
```

3. GitHub Actions automatically:
   - Builds all 5 platform binaries
   - Packages each binary (`.tar.gz` for Linux/macOS, `.zip` for Windows)
   - Generates `SHA256SUMS.txt`
   - Creates a GitHub Release with all artifacts attached
   - Includes operator scripts (`start-node.sh`, `install.sh`, etc.)

### Versioning

Releases follow **Semantic Versioning** (`v{MAJOR}.{MINOR}.{PATCH}`).

| Component | Description |
|-----------|-------------|
| MAJOR | Breaking changes to chain spec or runtime API |
| MINOR | New features, backward-compatible |
| PATCH | Bug fixes, CI changes |

---

## Cache Strategy

Cargo build caches are keyed per-target with the `Cargo.lock` hash:

```yaml
key: ${{ matrix.target }}-cargo-v3-${{ hashFiles('**/Cargo.lock') }}
```

This ensures:
- Cache hits when `Cargo.lock` hasn't changed
- Automatic cache invalidation on dependency updates
- No stale artifacts between versions

---

## Building Locally

```bash
# Standard release build
cargo build --release

# Build only the runtime (WASM)
cargo build --release -p tks-runtime

# Build only the node binary
cargo build --release -p tks-chain-node

# Run tests
cargo test --workspace

# Check without building
cargo check --workspace

# Format code
cargo fmt --all

# Lint
cargo clippy --workspace -- -D warnings
```

---

## Verifying Release Checksums

```bash
# Download binary and checksum file
curl -LO https://github.com/TokenKickstarter/tokenkickstarter-node/releases/latest/download/tks-chain-node-linux-x86_64-v0.1.0.tar.gz
curl -LO https://github.com/TokenKickstarter/tokenkickstarter-node/releases/latest/download/SHA256SUMS.txt

# Verify
sha256sum --check --ignore-missing SHA256SUMS.txt
```
