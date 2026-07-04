# Security

## Responsible Disclosure

The TokenKickstarter team takes security seriously. If you discover a vulnerability in the TKS node, runtime, or associated infrastructure, please report it responsibly.

**Do not open a public GitHub Issue for security vulnerabilities.**

### How to Report

Send an email to: **security@tokenkickstarter.com**

Include:
- A description of the vulnerability
- Steps to reproduce
- The potential impact
- Any suggested mitigations (optional)

We will acknowledge your report within 48 hours and provide a status update within 7 days.

---

## Scope

### In Scope

- TKS node binary (`node/`)
- WASM runtime (`runtime/`)
- Custom pallets (`pallets/`)
- Network configuration and scripts (`network/`)
- CI/CD pipeline (`.github/workflows/`)

### Out of Scope

- Third-party dependencies (report upstream to the relevant project)
- Issues requiring physical access to infrastructure
- Social engineering attacks
- Issues already publicly known

---

## Severity Classification

| Severity | Description | Response Time |
|----------|-------------|---------------|
| Critical | Fund loss, consensus failure, remote code execution | 24 hours |
| High | Significant chain disruption, auth bypass | 3 days |
| Medium | Denial of service, data integrity issues | 7 days |
| Low | Minor information disclosure, configuration issues | 30 days |

---

## Security Considerations for Node Operators

### Firewall

Expose only the required ports:

| Port | Expose To | Purpose |
|------|-----------|---------|
| `30333` | Public internet | P2P networking (required) |
| `9944` | Trusted IPs only | RPC (restrict to your application servers) |
| `9615` | Internal network | Prometheus metrics |

### Keystore Protection

- The keystore (`~/.tks/validator/chains/tks_network/keystore/`) contains session key secrets
- Set permissions to `700` (owner read/write/execute only)
- Back up keystore contents securely before any system migration
- Never copy the keystore to an untrusted machine

```bash
chmod 700 ~/.tks/validator/chains/tks_network/keystore/
```

### RPC Access

- Never expose `author_insertKey` or `author_rotateKeys` to public internet
- Use a reverse proxy (nginx/caddy) with authentication for any public-facing RPC
- Prefer WebSocket with TLS (`wss://`) for any public endpoints

### Validator Key Separation

- Use a **stash account** for bonded funds (cold storage — never online)
- Use a **controller account** for day-to-day staking operations
- The validator keystore only needs session keys — never store the stash private key on the validator server

### Software Updates

- Keep the node binary updated to the latest release
- Subscribe to the [GitHub Releases page](https://github.com/TokenKickstarter/tokenkickstarter-node/releases) for notifications
- Apply runtime upgrades promptly after governance passage

---

## Known Limitations (Current Phase)

- **Sudo governance** — The current governance model uses `pallet-sudo`. This is a centralized admin key and will be replaced by on-chain governance (Democracy or OpenGov) in a future release.
- **Testnet phase** — The network is in testnet. Tokens have no monetary value. Infrastructure may be reset.

---

## Audit Status

| Component | Status |
|-----------|--------|
| Custom pallets | Not yet audited |
| Runtime configuration | Not yet audited |
| Network scripts | Not yet audited |

Audit engagements are planned for mainnet preparation. Community security reviews are welcome in the interim.
