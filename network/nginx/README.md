# Public RPC hardening (rpc host only)

Steps apply **only** to `rpc.tokenkickstarter.com`. Do not edit global nginx or unrelated vhosts.

## Cloudflare (DNS record only)

- Open **DNS** → the **`rpc`** A/AAAA record (not `@`, `www`, or other subdomains).
- Toggle **Proxied → DNS only** (grey cloud) for **`rpc` only** if edge rules cause redirect loops.
- Leave **SSL/TLS** zone settings and other hostnames unchanged.

## Reverse proxy (this site only)

### Proxy tuning (per-site custom / catalog)

Paste into your panel’s **per-vhost** proxy custom block (not global `http { }`). Many panels already inject `proxy_connect_timeout`, `proxy_http_version`, and WebSocket `Upgrade` headers — **do not duplicate** those or `nginx -t` may fail.

```nginx
client_max_body_size 4m;
client_body_buffer_size 256k;
proxy_buffering off;
proxy_request_buffering on;
```

For WebSocket RPC, ensure the vhost maps `$http_upgrade` → `$connection_upgrade` once (panel websocket snippet or a single `map` in site HTTP custom).

### Block identity RPC methods (optional)

Prefer **`--rpc-hide-node-identity`** on the node (v0.1.16+). Extra edge filtering (Lua or app gateway) can deny `system_localPeerId`, `system_localListenAddresses`, `system_peers`, and `system_nodeRoles` on POST — implement in the **site configuration file** inside `location ^~ /`, not in a global include.

**Backend:** `proxy_pass http://127.0.0.1:9944;` (node on localhost; no redirects on this vhost).

### Do not add on the public RPC vhost

- `return 301` / `308` / `rewrite` (conflicts with CDN orange-cloud)
- `limit_except` blocking POST (breaks JSON-RPC)

## Relay host (`relay.tokenkickstarter.com`)

See [`relay.tokenkickstarter.com.conf`](./relay.tokenkickstarter.com.conf) — short `/fetch`, `/store`, `/ack` timeouts; long timeouts on generic proxy break mobile clients.

## Verify (rpc URL only)

```bash
curl -sS -X POST 'https://rpc.tokenkickstarter.com/' \
  -H 'Content-Type: application/json' \
  -d '{"jsonrpc":"2.0","id":1,"method":"eth_blockNumber","params":[]}'
# expect 200 + result

curl -sS -X POST 'https://rpc.tokenkickstarter.com/' \
  -H 'Content-Type: application/json' \
  -d '{"jsonrpc":"2.0","id":1,"method":"system_localPeerId","params":[]}'
# expect 403 or method-not-found when hardening is enabled
```
