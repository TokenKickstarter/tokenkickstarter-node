# Validator set (production)

| Phase | Validators | Operator |
|-------|------------|----------|
| v0.1.13 audit | **2** (Aura + GRANDPA) | TokenKickstarter (VPS Docker) |
| Target | **4+** independent | Third-party bare metal or cloud |

Adding validators requires new session keys, `pallet-session` / `pallet-staking` onboarding, and HyperSwarm `register_validator` for Cipher duty. There is **no** trustless permissionless validator slot yet.

**R4 status:** Still **2 validators**, both on TokenKickstarter VPS. Target **4+** operators — we are recruiting **2 external validators** (bare metal or cloud, separate legal entity). Contact: `validators@tokenkickstarter.com` with Aura/GRANDPA/ECDSA keys and a public relay/shard endpoint plan. Until onboarding completes, audits should treat the set as **centralized (2-of-2 operator)**.

See [AUDIT-READ-ACCESS.md](./AUDIT-READ-ACCESS.md) for log access without sharing validator keys.
