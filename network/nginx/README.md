# Public RPC hardening (rpc host only)

All steps below apply **only** to `rpc.tokenkickstarter.com`. Do not edit global nginx, other vhosts, or other Cloudflare zones/hosts.

## Cloudflare (DNS record only)

- Open **DNS** → find the **`rpc`** A/AAAA record (not `@`, `www`, or other subdomains).
- Toggle **Proxied → DNS only** (grey cloud) for **`rpc` only** if edge rules cause 308 loops and you cannot change those rules yet.
- Leave **SSL/TLS** zone settings and **every other hostname** unchanged.

Orange cloud is fine once redirect/normalization rules on `rpc` are fixed; grey cloud is the isolated bypass.

## aaPanel reverse proxy (this site only)

### 1) Proxy Catalog (line-by-line, each line ends with `;`)

Paste **only** into **Proxy Catalog** for `rpc.tokenkickstarter.com`. aaPanel’s proxy project **already** injects `proxy_connect_timeout`, `proxy_send/read_timeout`, `proxy_http_version`, and `Upgrade`/`Connection` in the main vhost — duplicating them in Catalog causes `nginx: [emerg] "proxy_connect_timeout" directive is duplicate`.

```nginx
client_max_body_size 4m;
client_body_buffer_size 256k;
proxy_buffering off;
proxy_request_buffering on;
```

To use `Connection $connection_upgrade` instead of `Connection "upgrade"`, edit **Configuration file** once (replace the duplicate `Connection` line inside `location`, do not add a second set in Catalog). Add the **HTTP Custom** `map` only if `$connection_upgrade` is unknown.

### 2) HTTP Custom (same proxy site — not Proxy Catalog)

aaPanel → this site → config **HTTP** / **Custom** (injected in `http { }` for this vhost file only):

```nginx
map $http_upgrade $connection_upgrade {
    default upgrade;
    ''      close;
}
```

(Skip if your panel already loads `0.websocket.conf` and nginx -t shows no duplicate `map` errors.)

### 3) Identity deny — not in Proxy Catalog

Lua must live in the **Configuration file** inside `location ^~ / { ... }` (before `proxy_pass`), or use one include line in Proxy Catalog:

```nginx
include /www/server/panel/vhost/nginx/custom/rpc.tokenkickstarter.com-lua.conf;
```

Create that file with the `access_by_lua_block { ... }` from ops notes (POST body filter for four `system_*` methods). Proxy Catalog cannot express multi-line Lua.

### 4) Do not add in Proxy Catalog

- `return 301` / `return 308` / `rewrite` (conflicts with Cloudflare orange cloud)
- `map` (wrong context)
- `limit_except` blocking POST (breaks JSON-RPC)

**Configuration file** basics: **`proxy_pass http://127.0.0.1:9944;`** — node on localhost Docker; no extra redirects on this vhost.

## Optional map file (usually skip on aaPanel)

`rpc-deny-node-identity.conf` is for `http { }` **includes**; including it globally affects the whole nginx `http` context. Prefer Lua on the **rpc** `location` instead of a global include.

## Node flag (preferred)

From **v0.1.16**, `--rpc-hide-node-identity` on the fullnode removes identity methods at the node (HTTP + WS). Nginx Lua / PHP gateway is optional extra hardening, not required for audits.

## Verify (rpc URL only)

```bash
curl -sS -X POST 'https://rpc.tokenkickstarter.com/' \
  -H 'Content-Type: application/json' \
  -d '{"jsonrpc":"2.0","id":1,"method":"eth_blockNumber","params":[]}'
# expect 200 + result

curl -sS -X POST 'https://rpc.tokenkickstarter.com/' \
  -H 'Content-Type: application/json' \
  -d '{"jsonrpc":"2.0","id":1,"method":"system_localPeerId","params":[]}'
# expect 403 + "Method not allowed on public RPC" (with Lua)
```
