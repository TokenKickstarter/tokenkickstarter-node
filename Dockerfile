# ===============================================
# BUILD STAGE
# ===============================================
FROM paritytech/ci-linux:production AS builder

WORKDIR /build

# Repository root is the workspace (node, runtime, cipher-* crates).
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

# Add a non-root user
RUN useradd -m -u 1000 -U -s /bin/sh tks && \
    mkdir -p /data && \
    chown -R tks:tks /data

WORKDIR /app
COPY --from=builder /build/target/release/tks-chain-node /app/tks-chain-node
RUN chown -R tks:tks /app/tks-chain-node

# Expose standard Substrate + Frontier EVM ports
# 30333 = P2P
# 9944 = WebSockets
# 9933 = RPC
EXPOSE 30333 9944 9933

# Expose new embedded Cipher Relay HTTP API
EXPOSE 4002

USER tks

# Run the node securely
ENTRYPOINT ["/app/tks-chain-node", "-d", "/data"]
