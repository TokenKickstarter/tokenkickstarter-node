# ===============================================
# BUILD STAGE — Rust stable (Cargo.lock v4+) + Substrate deps
# ===============================================
FROM rust:1-bookworm AS builder

RUN apt-get update && apt-get install -y --no-install-recommends \
    build-essential \
    clang \
    cmake \
    git \
    libssl-dev \
    llvm \
    libudev-dev \
    pkg-config \
    protobuf-compiler \
    && rm -rf /var/lib/apt/lists/*

RUN rustup component add rust-src \
    && rustup target add wasm32-unknown-unknown

WORKDIR /build

COPY . .

RUN cargo build --release -p tks-chain-node

# ===============================================
# RUN STAGE
# ===============================================
FROM ubuntu:22.04

LABEL maintainer="Token Kickstarter"
LABEL description="TKS Blockchain Node + Cipher Relay"

RUN apt-get update && \
    apt-get install -y ca-certificates curl jq && \
    rm -rf /var/lib/apt/lists/*

RUN useradd -m -u 1000 -U -s /bin/sh tks && \
    mkdir -p /data && \
    chown -R tks:tks /data

WORKDIR /app
COPY --from=builder /build/target/release/tks-chain-node /app/tks-chain-node
COPY --from=builder /build/target/release/wbuild/tks-runtime/tks_runtime.compact.compressed.wasm /app/tks_runtime.compact.compressed.wasm
RUN chown -R tks:tks /app/tks-chain-node /app/tks_runtime.compact.compressed.wasm

EXPOSE 30333 9944 9933 4002

USER tks

ENTRYPOINT ["/app/tks-chain-node", "-d", "/data"]
