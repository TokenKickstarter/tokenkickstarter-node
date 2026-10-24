# Cipher / HyperSwarm — privacy for investors & auditors

## Can relay peers decrypt photos, video, or messages?

**No — not if the Cipher client did its job.**

| What relays / peers store | What they do **not** have |
|---------------------------|---------------------------|
| Opaque **`ciphertext` bytes** (encrypted blob) | Message keys, passwords, seed phrases |
| **`ciphertext_sha256`** on shard (L1 anchor path) | Plaintext body, media, contacts |
| Gossip **metadata** (message id, recipient id, size, hash) | Ability to decrypt without the recipient’s keys |

HyperSwarm nodes are **dumb caches** of already-encrypted data — like BitTorrent seeding **encrypted files**. Peers replicate **ciphertext**, not keys.

## End-to-end encryption (E2E)

- Encryption and decryption happen in the **Cipher app** (keys on device).
- TKS node, relay operator, and libp2p peers only see **public metadata + ciphertext**.
- L1 `@username` maps identity; it does **not** hold chat keys.

## Chunking & “randomized pieces”

| Layer | Today |
|-------|--------|
| **Relay HTTP / HyperSwarm pull** | One `EncryptedChunk` per client upload (opaque bytes). |
| **Large media (photos/video)** | App should **split → encrypt each piece → upload many chunks** before send. |
| **Randomized / padded pieces** | **Client protocol** (recommended); not rewritten by the node. |
| **Shard L1 path** | Stores **hash** of ciphertext for anchors, not full media. |

So: **peers cannot decrypt**; **chunking strategy is defined by Cipher clients** — use many small encrypted chunks for big files so pulls look like torrent pieces.

## HyperSwarm v0.1.5+ (torrent-style pull)

- Gossip **`ChunkAvailAnnouncement`** (magnet-style: ids + hash, **no** ciphertext in gossip).
- libp2p **`/tks-hyperswarm/chunk/1.0.0`** request-response: **pull ciphertext** from any peer that has it.
- On **`/fetch`**, relay tries **HyperSwarm pull** before returning empty.

This is **replication sync**, not a full **CRDT** (no automatic merge of edit histories). CRDT for message edits remains a client/protocol roadmap item.

## Threat model (short)

- **Honest but curious relay:** sees traffic patterns + ciphertext; cannot read content without keys.
- **Compromised relay:** can **withhold** or **drop** data; cannot forge valid decryption for recipients (E2E).
- **Global passive observer:** sees IP + sizes; use TLS on `https://relay.*` and libp2p noise for transport.

## What to tell investors

> “Media and messages are **encrypted on the device**. The network only moves **ciphertext** between peers; **500k TPS** is delivery throughput, not access to user content. Keys never live on the TKS validator.”
