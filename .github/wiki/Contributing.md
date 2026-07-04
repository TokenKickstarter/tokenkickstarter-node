# Contributing

Thank you for your interest in contributing to TKS. This guide covers the development workflow, code standards, and how to submit changes.

---

## Getting the Code

```bash
git clone https://github.com/TokenKickstarter/tokenkickstarter-node
cd tokenkickstarter-node
```

The workspace is a standard Cargo workspace. All members are in the root `Cargo.toml`.

---

## Development Setup

Follow [Getting Started](Getting-Started) to install Rust and system dependencies, then verify the build:

```bash
cargo check --workspace
```

For faster iteration during development:

```bash
# Check without building
cargo check -p tks-chain-node

# Check the runtime only
cargo check -p tks-runtime

# Check a specific pallet
cargo check -p pallet-adaptive-gas
```

---

## Code Standards

### Formatting

All Rust code must be formatted with `rustfmt`:

```bash
cargo fmt --all
```

### Linting

No Clippy warnings allowed:

```bash
cargo clippy --workspace -- -D warnings
```

Fix all warnings before submitting a PR. Clippy warnings are treated as errors in CI.

### Tests

Run the full test suite:

```bash
cargo test --workspace
```

Run tests for a specific crate:

```bash
cargo test -p pallet-adaptive-gas
cargo test -p pallet-name-registry
```

---

## Project Structure for Contributors

When adding features, follow this pattern:

| Change Type | Where |
|-------------|-------|
| New storage, logic, extrinsic | New or existing pallet in `pallets/` |
| New pallet wire-up | `runtime/src/lib.rs` — `construct_runtime!` |
| New RPC method | `node/src/rpc.rs` |
| New CLI flag | `node/src/cli.rs` |
| Chain spec change | `node/src/chain_spec.rs` |
| New EVM precompile | `runtime/src/precompiles.rs` |

---

## Submitting a Pull Request

1. **Fork** the repository
2. **Create a branch** with a descriptive name:
   ```bash
   git checkout -b feat/my-feature
   ```
3. **Make your changes**, following the code standards above
4. **Write tests** for any new logic
5. **Run the full check**:
   ```bash
   cargo fmt --all && cargo clippy --workspace -- -D warnings && cargo test --workspace
   ```
6. **Commit** with a clear message (see below)
7. **Push** to your fork and open a PR against `main`

### Commit Message Format

```
<type>: <short description>

<longer explanation if needed>
```

Types:

| Type | Use Case |
|------|----------|
| `feat` | New feature or pallet |
| `fix` | Bug fix |
| `ci` | CI/CD changes |
| `docs` | Documentation only |
| `refactor` | Code restructure without behavior change |
| `test` | New or updated tests |
| `chore` | Dependency updates, toolchain bumps |

**Examples:**

```
feat: add pallet-name-registry with free name registration

fix: resolve WASM linker undefined symbol on Rust >= 1.88

ci: switch Linux arm64 to native ubuntu-22.04-arm runner
```

---

## Adding a Custom Pallet

1. Create the directory structure:
   ```
   pallets/pallet-your-pallet/
   ├── Cargo.toml
   └── src/
       └── lib.rs
   ```

2. `Cargo.toml` template:
   ```toml
   [package]
   name = "pallet-your-pallet"
   version = "0.1.0"
   edition = "2021"

   [dependencies]
   frame-support = { workspace = true }
   frame-system = { workspace = true }
   sp-runtime = { workspace = true }
   parity-scale-codec = { workspace = true }
   scale-info = { workspace = true }

   [features]
   default = ["std"]
   std = [
     "frame-support/std",
     "frame-system/std",
     "sp-runtime/std",
     "parity-scale-codec/std",
     "scale-info/std",
   ]
   ```

3. Add to root `Cargo.toml` workspace members
4. Add to `runtime/Cargo.toml` dependencies
5. Implement `Config` and add to `construct_runtime!` in `runtime/src/lib.rs`

---

## Dependency Policy

- Do not add new dependencies without a clear reason
- Prefer dependencies already in the Substrate SDK workspace
- All new dependencies must be pinned to a specific version or compatible range
- Avoid `git` dependencies (use crates.io versions where possible)

---

## Reporting Issues

Open a [GitHub Issue](https://github.com/TokenKickstarter/tokenkickstarter-node/issues) with:

- A clear title describing the problem
- Steps to reproduce
- Expected vs actual behavior
- Rust version (`rustc --version`) and OS
- Node version (`./tks-chain-node --version`)
- Relevant logs or error output

---

## Security

For security vulnerabilities, do **not** open a public GitHub Issue. See [Security](Security) for responsible disclosure.
