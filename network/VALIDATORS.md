# Validator set (production)

| Phase | Validators | Operator |
|-------|------------|----------|
| v0.1.13 audit | **2** (Aura + GRANDPA) | TokenKickstarter (VPS Docker) |
| Target | **4+** independent | Third-party bare metal or cloud |

Adding validators requires new session keys, `pallet-session` / `pallet-staking` onboarding, and HyperSwarm `register_validator` for Cipher duty. There is **no** trustless permissionless validator slot yet — document this in audit materials until a public invitation / bond process ships.

See [AUDIT-READ-ACCESS.md](./AUDIT-READ-ACCESS.md) for log access without sharing validator keys.
