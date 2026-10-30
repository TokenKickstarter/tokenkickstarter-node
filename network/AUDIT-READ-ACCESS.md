# Independent audit — read-only infrastructure access

SSH-only checks cannot be reproduced by third parties unless operators grant **read-only** access.

## Recommended setup (VPS `144.91.122.247`, SSH port **7422**)

1. Create a Unix user `audit` (no sudo).
2. Add auditor SSH **public** key to `/home/audit/.ssh/authorized_keys`.
3. Grant read-only Docker and logs:

```bash
usermod -aG docker audit
# Optional: allow only inspect/logs
# echo 'audit ALL=(root) NOPASSWD: /usr/bin/docker inspect *, /usr/bin/docker logs *, /usr/bin/docker ps *' > /etc/sudoers.d/audit-docker
```

4. Publish a **`network/scripts/vps-docker-audit.sh`** wrapper the auditor can run:

```bash
ssh -p 7422 audit@144.91.122.247 'bash -s' < network/scripts/vps-docker-audit.sh
```

5. **Do not** share validator session keys, sudo member seeds, or `production-keys/` via chat or git.

## What auditors should verify

- Container images / `tks-chain-node --version` → **v0.1.14+**
- `docker inspect` Cmd: `--pool-future-divisor 10`, `--pool-type fork-aware`, `--rpc-hide-node-identity` on public fullnode
- Cipher: `--cipher-auto-post-anchor`, anchor `.posted` markers under `/data/cipher/anchors/.posted`
- Validator CPU/RAM: `docker stats --no-stream`

Operator action: create the `audit` user and send the auditor their **public** key fingerprint confirmation out of band.
