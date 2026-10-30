# Public RPC hardening

Include `rpc-deny-node-identity.conf` in the `http { }` context, then in the RPC `location /`:

```nginx
if ($tks_deny_identity_rpc) {
    return 403;
}
proxy_pass http://127.0.0.1:9944;
```

The node flag `--rpc-hide-node-identity` logs a reminder; Substrate registers `system_localPeerId` after the node's RPC module.
