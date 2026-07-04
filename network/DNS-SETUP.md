# TKS Network — DNS Setup Guide

## Overview

The TKS network uses 6 DNS seed domains across 3 servers. This means:
- **No hardcoded IPs** anywhere in the codebase
- **Server IP changes** → just update DNS, nothing else
- **2 of 3 servers down** → network still works (remaining seeds active)
- **All users worldwide** get automatic peer discovery

---

## Your 6 Seed Domains

| Domain | Server | Peer ID |
|--------|--------|---------|
| `seed.tokenkickstarter.com` | Server 1 (US) | `12D3KooWEgkL3KD5NT7zHiXgRh61YV5c9vsgzfBzZHr2bWG5ga6C` |
| `seed.tokenkickstarter.ink` | Server 1 (US) | `12D3KooWEgkL3KD5NT7zHiXgRh61YV5c9vsgzfBzZHr2bWG5ga6C` |
| `seed.tkstoken.com` | Server 2 (EU) | `12D3KooWADUbEfUqDAnhFBYEVKcERnS2fW5Fy2JiJuRBisqrQ8nR` |
| `seed.tokenkickstarter.xyz` | Server 2 (EU) | `12D3KooWADUbEfUqDAnhFBYEVKcERnS2fW5Fy2JiJuRBisqrQ8nR` |
| `seed.tksscan.com` | Server 3 (Asia) | `12D3KooWDJwaet3SPVZDv7cC8Aq22BXwNC7kEzBNARSebsR2obn8` |
| `seed.tokenkickstarter.pw` | Server 3 (Asia) | `12D3KooWDJwaet3SPVZDv7cC8Aq22BXwNC7kEzBNARSebsR2obn8` |

---

## Step 1: Get 3 Server IPs

Recommended providers (cheap, reliable, globally distributed):

| Provider | Region | ~Cost/mo | Link |
|----------|--------|----------|------|
| Hetzner | EU (Falkenstein) | $4 | hetzner.com |
| Vultr | US (New York) | $6 | vultr.com |
| DigitalOcean | Asia (Singapore) | $6 | digitalocean.com |
| Contabo | Any | $5 | contabo.com |

Minimum specs per server: **2 vCPU, 4GB RAM, 80GB SSD**

---

## Step 2: Set DNS A Records

### Using Cloudflare (Recommended)

Log into Cloudflare → Select domain → DNS → Add records:

#### For `tokenkickstarter.com`
```
Type    Name                    Content          TTL
A       seed                    <SERVER_1_IP>    Auto
```

#### For `tokenkickstarter.ink`
```
Type    Name    Content          TTL
A       seed    <SERVER_1_IP>    Auto
```

#### For `tkstoken.com`
```
Type    Name    Content          TTL
A       seed    <SERVER_2_IP>    Auto
```

#### For `tokenkickstarter.xyz`
```
Type    Name    Content          TTL
A       seed    <SERVER_2_IP>    Auto
```

#### For `tksscan.com`
```
Type    Name    Content          TTL
A       seed    <SERVER_3_IP>    Auto
```

#### For `tokenkickstarter.pw`
```
Type    Name    Content          TTL
A       seed    <SERVER_3_IP>    Auto
```

> ⚠️ **Important**: Set **Proxy status to DNS only** (grey cloud, not orange).
> The node needs a direct TCP connection — Cloudflare proxy won't work for port 30333.

---

## Step 3: Verify DNS Resolution

After setting DNS records (propagation takes 1–5 minutes with Cloudflare):

```bash
# Should return your Server 1 IP
dig seed.tokenkickstarter.com A +short
dig seed.tokenkickstarter.ink A +short

# Should return your Server 2 IP
dig seed.tkstoken.com A +short
dig seed.tokenkickstarter.xyz A +short

# Should return your Server 3 IP
dig seed.tksscan.com A +short
dig seed.tokenkickstarter.pw A +short
```

---

## Step 4: Deploy Bootnodes

SSH into each server and run the setup:

### Server 1 (US)
```bash
# Upload files
scp -r network/ user@SERVER_1_IP:/opt/tks/
scp target/release/tks-chain-node user@SERVER_1_IP:/opt/tks/

# SSH and start
ssh user@SERVER_1_IP
cd /opt/tks
chmod +x start-bootnode-1.sh
./start-bootnode-1.sh
```

### Server 2 (EU)
```bash
scp -r network/ user@SERVER_2_IP:/opt/tks/
scp target/release/tks-chain-node user@SERVER_2_IP:/opt/tks/
ssh user@SERVER_2_IP "cd /opt/tks && chmod +x start-bootnode-2.sh && ./start-bootnode-2.sh"
```

### Server 3 (Asia)
```bash
scp -r network/ user@SERVER_3_IP:/opt/tks/
scp target/release/tks-chain-node user@SERVER_3_IP:/opt/tks/
ssh user@SERVER_3_IP "cd /opt/tks && chmod +x start-bootnode-3.sh && ./start-bootnode-3.sh"
```

### Or with Docker (easier)
```bash
# On each server, pull the docker-compose and run its service
docker compose up -d tks-seed-1  # on Server 1
docker compose up -d tks-seed-2  # on Server 2
docker compose up -d tks-seed-3  # on Server 3
```

---

## Step 5: Open Firewall Ports

Run on each server:
```bash
# UFW (Ubuntu)
sudo ufw allow 30333/tcp comment "TKS P2P"
sudo ufw allow 9944/tcp  comment "TKS RPC"
sudo ufw reload

# iptables
sudo iptables -A INPUT -p tcp --dport 30333 -j ACCEPT
sudo iptables -A INPUT -p tcp --dport 9944  -j ACCEPT
```

---

## Step 6: Verify Connectivity

After DNS propagates and bootnodes are running:

```bash
# Test P2P reachability
nc -zv seed.tokenkickstarter.com 30333
nc -zv seed.tkstoken.com 30333
nc -zv seed.tksscan.com 30333

# Test RPC health
curl http://seed.tokenkickstarter.com:9944/health
# Should return: {"health":{"isSyncing":false,"peers":2,"shouldHavePeers":true}}
```

---

## Changing Server IP (No Binary Update Needed)

If a server IP changes:

```bash
# 1. Start new server with same bootnode key
scp network/bootnodes/bootnode-1.key user@NEW_SERVER_1_IP:/opt/tks/

# 2. Update DNS A record in Cloudflare
#    seed.tokenkickstarter.com → NEW_SERVER_1_IP
#    seed.tokenkickstarter.ink → NEW_SERVER_1_IP

# 3. Done. All nodes reconnect automatically within minutes.
#    No chainspec change. No binary update.
```

---

## Redundancy Matrix

| Server 1 | Server 2 | Server 3 | New nodes can join? |
|----------|----------|----------|---------------------|
| ✅ UP | ✅ UP | ✅ UP | ✅ Yes (any seed) |
| ❌ DOWN | ✅ UP | ✅ UP | ✅ Yes (seeds 2,3) |
| ❌ DOWN | ❌ DOWN | ✅ UP | ✅ Yes (seed 3) |
| ❌ DOWN | ❌ DOWN | ❌ DOWN | ⚠️ Existing nodes still sync via saved peers |

---

## Summary

Once DNS is set and bootnodes are running, **users just run**:

```bash
./start-node.sh
```

No IP. No configuration. It just works. 🚀
