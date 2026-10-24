# Cipher client media chunking spec (v1)

**Audience:** Mobile/desktop/web Cipher clients, `client-core`, relay operators  
**Aligns with:** `client-core/src/message.rs` (`File`, `FileChunk`), `file_chunker.rs`, TKS relay `POST /store`, HyperSwarm libp2p **≤1 MiB** request limit (v0.1.5+)

---

## 1. Goals

1. **E2E:** Plaintext never leaves the device unencrypted.
2. **Torrent-style:** Photos/video split into many small **encrypted** pieces; HyperSwarm peers replicate **ciphertext** only.
3. **Integrity:** Merkle root over plaintext chunk hashes (matches L1 shard `ciphertext_hash` story).
4. **Traffic analysis:** Optional fixed-size padding buckets per piece.

---

## 2. Size limits (normative)

| Constant | Value | Reason |
|----------|-------|--------|
| `PLAIN_CHUNK_SIZE` | **512 KiB** (524_288 bytes) | Plaintext per piece before AEAD |
| `MAX_RELAY_CIPHERTEXT` | **900_000** bytes | Stay under libp2p JSON RR **~1 MiB** request cap |
| `PAD_BUCKET` | **256 KiB** or **512 KiB** | Optional outer padding (§6) |

After AES-256-GCM, one piece is typically `PLAIN_CHUNK_SIZE + 16` tag → **well under 1 MiB**.

**Deprecated:** 2 MiB chunks in older `file_chunker` — **do not use** with HyperSwarm pull.

---

## 3. Identifiers

| ID | Format | Example |
|----|--------|---------|
| `file_id` | UUID v4 string | `7b2e…` |
| `message_id` (relay) | `{file_id}#c{index:05}` | `7b2e…#c00000` |
| `recipient_id` | EVM `0x…` or `@username` resolved to storage id | |

Regex: `^(?<file_id>[0-9a-f-]{36})#c(?<index>[0-9]{5})$`

Header message (separate relay post) uses `message_id = {file_id}#header`.

---

## 4. Cryptography (per piece)

- **Algorithm:** AES-256-GCM
- **Key:** Session key from Double Ratchet / file transfer key derived from chat session (same as `client-core/encryption`).
- **Nonce:** 12 random bytes **unique per piece** (never reuse key+nonce).
- **AAD (recommended):** UTF-8 `file_id || "#" || chunk_index || "/" || total_chunks` bound into GCM AAD.
- **Plaintext chunk hash:** `SHA-256(plain_piece)` → leaf in Merkle tree (same as `file_chunker::chunk_hash`).
- **Merkle root:** Binary tree SHA-256 over leaf hashes (odd leaf duplicated), same as `compute_merkle_root` in `file_chunker.rs`.

---

## 5. Wire format: relay `POST /store`

Relay API (unchanged): JSON `EncryptedChunk`:

```json
{
  "recipient_id": "0xRecipient…",
  "message_id": "550e8400-e29b-41d4-a716-446655440000#c00003",
  "ciphertext": "<base64 in docs; raw bytes on wire>",
  "stored_at": 1710000000,
  "expires_at": 1712592000
}
```

**`ciphertext` bytes** = bincode of **`RelayWireChunk`** (versioned):

```rust
/// client-core / all platforms — bincode, little-endian
pub struct RelayWireChunk {
    pub version: u8,              // = 1
    pub chunk_index: u32,
    pub total_chunks: u32,
    pub nonce: [u8; 12],
    pub chunk_hash: [u8; 32],     // SHA-256 plaintext piece
    pub merkle_root: [u8; 32],
    pub aes_ciphertext: Vec<u8>,  // AES-GCM output (includes tag)
}
```

Implementations **must** reject `version != 1`.

---

## 6. Optional padding (traffic shaping)

Before bincode, optionally pad `aes_ciphertext` so **len(RelayWireChunk encoded)** aligns to `PAD_BUCKET`:

```
pad_len = (PAD_BUCKET - (len % PAD_BUCKET)) % PAD_BUCKET
append pad_len bytes of value pad_len (PKCS7-style mod 256)
```

Receivers strip using last byte. **Do not** pad plaintext before encrypt — pad **outer** wire blob only.

---

## 7. Send pipeline (sender)

```
┌─────────────┐   1. encrypt_and_chunk()    ┌──────────────────┐
│ File bytes  │ ──────────────────────────► │ N RelayWireChunk │
└─────────────┘                             └────────┬─────────┘
                                                       │
  2. POST header CipherMessage (File metadata)         │
     message_id = {file_id}#header                     │
     payload = ratchet-encrypt(FileMetadata)           │
                                                       │
  3. For each piece (parallelism 8–16):                ▼
     POST /store with message_id #c{i}          HyperSwarm gossip
     + optional discovery bootstrap              + mesh pull
```

**Order:** Header **first**, then pieces in **any order** (parallel uploads OK).

**`FileMetadata`** (inside encrypted header, see `message.rs`):

- `file_name`, `mime_type`, `size_bytes`, `total_chunks`, `merkle_root`, optional `thumbnail`

**Ratchet:** Header uses next message key; file pieces may use **derived** `file_key = HKDF(session_key, "cipher-file-v1", file_id)`.

---

## 8. Receive pipeline (recipient)

1. `POST /fetch` with `recipient_id` → collect all chunks + header.
2. Parse `message_id` → group by `file_id`.
3. For each `file_id`: require `#header` + all `#c00000…#c{total-1}`.
4. Decode `RelayWireChunk`, decrypt pieces, verify `chunk_hash` + **Merkle root**.
5. Concatenate plaintext in index order → file bytes.
6. `POST /ack` with **all** `message_id`s for that transfer (frees relay storage).

On missing pieces: call relay again (HyperSwarm **pull** on server), or wait for push; retry with backoff.

---

## 9. Text vs media

| Content | Chunks? | Relay posts |
|---------|---------|-------------|
| Text (small) | No | 1× `CipherMessage` encrypted blob, single `message_id` = UUID |
| Photo/video/file | Yes | 1× header + **N** piece posts |

---

## 10. HyperSwarm & L1

- Gossip announces **SHA-256 of relay `ciphertext` bytes** (opaque on wire).
- libp2p pull returns same JSON/`EncryptedChunk` as HTTP store.
- Shard executor still sees **hash** of relay ciphertext per message (M2 path).

---

## 11. Conformance tests

| Test | Pass criteria |
|------|----------------|
| Roundtrip 5 MiB file | 10 pieces @ 512 KiB; Merkle verifies |
| Max piece size | Every `len(ciphertext) <= MAX_RELAY_CIPHERTEXT` |
| Tamper 1 piece | Merkle or GCM fails |
| Missing piece | Reassembly errors until complete |
| message_id parse | Stable `file_id` + index |

Reference: `client-core` tests in `file_chunker.rs`.

---

## 12. Implementation checklist

- [ ] `PLAIN_CHUNK_SIZE = 512 KiB` in `file_chunker.rs`
- [ ] `RelayWireChunk` encode/decode + `relay_message_id()`
- [ ] Upload worker pool (8–16 concurrent `/store`)
- [ ] Header `File` message before pieces
- [ ] Fetch/group/ack by `file_id`
- [ ] iOS/Android FFI mirrors Rust API
- [ ] Load test: 50 MiB video through relay + second peer pull

---

## 13. Related docs

- [HYPERSWARM-SECURITY.md](./HYPERSWARM-SECURITY.md)
- [HYPERSWARM-PEERS.md](./HYPERSWARM-PEERS.md)
- [CIPHER-TKS-THROUGHPUT-SPEC.md](./CIPHER-TKS-THROUGHPUT-SPEC.md)
