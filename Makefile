# TKS Substrate Node — Build Shortcuts
#
# IMPORTANT: Always build using `make build` or `make run-dev`
# NOT plain `cargo build`. See .cargo/config.toml for why.
#
# This ensures the correct rustc (rustup, not Homebrew) is used for WASM compilation.

TOOLCHAIN    := 1.94.0-aarch64-apple-darwin
RUSTUP_BIN   := $(HOME)/.rustup/toolchains/$(TOOLCHAIN)/bin
# ROOT FIX: Prepend the rustup toolchain bin dir to PATH.
#
# WHY: This system has both Homebrew Rust (/opt/homebrew/bin/rustc) and rustup
# Rust (~/.rustup/toolchains/.../bin/rustc). The substrate-wasm-builder
# INTENTIONALLY strips the RUSTC env var from its subprocess (see prerequisites.rs
# line 175: cmd.env_remove("RUSTC")), then invokes `rustup run <toolchain> cargo`
# which searches PATH for rustc. Since rustup does NOT prepend the toolchain bin
# dir to PATH on this Homebrew-rustup installation, cargo finds Homebrew rustc first.
#
# By prepending the rustup bin dir here, ALL subprocesses (including the wasm-builder
# dummy crate compilation) find the correct rustc before Homebrew's.
export PATH := $(RUSTUP_BIN):$(PATH)
CARGO        := cargo
NODE_BINARY  := target/release/tks-chain-node

.PHONY: build run-dev check clean purge

## Build the full release node (with WASM runtime embedded)
build:
	$(CARGO) build --release -p tks-chain-node

## Fast compile-check only (no WASM, much faster)
check:
	SKIP_WASM_BUILD=1 $(CARGO) check -p tks-runtime

## Run a local development node (single validator, no peers)
run-dev: build
	$(NODE_BINARY) --dev --rpc-external --rpc-cors all \
	  --rpc-port 9944 --ws-port 9945

## Run development node on existing binary (no rebuild)
run-dev-fast:
	$(NODE_BINARY) --dev --rpc-external --rpc-cors all \
	  --rpc-port 9944 --ws-port 9945

## Purge dev chain state (reset blocks)
purge:
	$(NODE_BINARY) purge-chain --dev -y

## Full clean (rebuild everything from scratch)
clean:
	$(CARGO) clean

## Show installed WASM targets
wasm-check:
	rustup target list --installed --toolchain $(TOOLCHAIN) | grep wasm
	@echo "Rustc sysroot:"
	@rustup run $(TOOLCHAIN) rustc --print sysroot

help:
	@echo "TKS Node Build Commands:"
	@echo "  make build        — Full release build (WASM embedded)"
	@echo "  make check        — Fast compile check (no WASM)"
	@echo "  make run-dev      — Build + run dev node"
	@echo "  make run-dev-fast — Run existing binary"
	@echo "  make purge        — Purge dev chain"
	@echo "  make clean        — Clean all build artifacts"
	@echo "  make wasm-check   — Verify WASM targets are installed"
