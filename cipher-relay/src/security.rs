//! Transport-independent security primitives for `cipher-relay` & `signal`:
//! - Trusted-proxy real client IP extraction (ignores spoofed headers from remote peers)
//! - Bounded per-IP fixed-window rate limiting
//! - secp256k1 recoverable ECDSA authentication binding mailboxes (`0x...`,
//!   `sealed_mailbox`, `peer-dir-v1:...`, and unlinkable `contact-v1:...`)
//! - Sender-authenticated `/store` verification with on-chain trust cache
//!   (`eth_getBalance` / `NameRegistry::ReverseLookup`) + Hashcash PoW fallback

use std::collections::HashMap;
use std::hash::Hasher as _;
use std::net::{IpAddr, SocketAddr};
use std::sync::Mutex;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use hyper::header::HeaderMap;
use hyper::StatusCode;
use k256::ecdsa::{RecoveryId, Signature, VerifyingKey};
use sha2::{Digest as _, Sha256};
use sha3::Keccak256;

pub const AUTH_MAX_SKEW_MS: u64 = 300_000;
const RATE_WINDOW_SECS: u64 = 60;
const DEFAULT_RATE_LIMIT_PER_MIN: u32 = 1_200;
const MAX_TRACKED_IPS: usize = 50_000;
const DEFAULT_POW_BITS: u32 = 16;
const MAX_POW_BITS: u32 = 28;
const TRUST_CACHE_TTL_SECS: u64 = 120;
const MAX_TRUST_CACHE_ENTRIES: usize = 50_000;
pub const PROBE_RECIPIENT_ID: &str =
    "0000000000000000000000000000000000000000000000000000000000000000";

pub fn require_client_sig_enabled() -> bool {
    match std::env::var("CIPHER_REQUIRE_CLIENT_SIG") {
        Ok(raw) => !matches!(
            raw.trim().to_ascii_lowercase().as_str(),
            "0" | "false" | "no" | "off"
        ),
        Err(_) => true,
    }
}

pub fn configured_pow_bits() -> u32 {
    match std::env::var("CIPHER_POW_BITS") {
        Ok(raw) => raw
            .trim()
            .parse::<u32>()
            .map(|v| v.min(MAX_POW_BITS))
            .unwrap_or(DEFAULT_POW_BITS),
        Err(_) => DEFAULT_POW_BITS,
    }
}

pub fn now_unix_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// Extract caller IP for rate limiting.
/// Proxy headers are trusted ONLY when the direct TCP peer is loopback (e.g. local Nginx).
pub fn extract_client_ip(remote: SocketAddr, headers: &HeaderMap) -> IpAddr {
    if !remote.ip().is_loopback() {
        return remote.ip();
    }
    if let Some(ip) = headers
        .get("x-real-ip")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.trim().parse::<IpAddr>().ok())
    {
        return ip;
    }
    if let Some(xff) = headers.get("x-forwarded-for").and_then(|v| v.to_str().ok()) {
        if let Some(last_valid) = xff
            .split(',')
            .rev()
            .filter_map(|part| part.trim().parse::<IpAddr>().ok())
            .next()
        {
            return last_valid;
        }
    }
    remote.ip()
}

pub struct IpRateLimiter {
    limit_per_min: u32,
    windows: Mutex<HashMap<IpAddr, (Instant, u32)>>,
}

impl IpRateLimiter {
    pub fn from_env() -> Self {
        let limit = std::env::var("CIPHER_RATE_LIMIT_PER_MIN")
            .ok()
            .and_then(|s| s.trim().parse::<u32>().ok())
            .filter(|&n| n > 0)
            .unwrap_or(DEFAULT_RATE_LIMIT_PER_MIN);
        Self::new(limit)
    }

    pub fn new(limit_per_min: u32) -> Self {
        Self {
            limit_per_min: limit_per_min.max(1),
            windows: Mutex::new(HashMap::new()),
        }
    }

    pub fn allow(&self, ip: IpAddr, cost: u32) -> bool {
        let now = Instant::now();
        let window = Duration::from_secs(RATE_WINDOW_SECS);
        let Ok(mut map) = self.windows.lock() else {
            return true;
        };
        if map.len() >= MAX_TRACKED_IPS && !map.contains_key(&ip) {
            map.retain(|_, (started, _)| now.duration_since(*started) < window);
            if map.len() >= MAX_TRACKED_IPS {
                return false;
            }
        }
        let entry = map.entry(ip).or_insert((now, 0));
        if now.duration_since(entry.0) >= window {
            *entry = (now, 0);
        }
        let next = entry.1.saturating_add(cost);
        if next > self.limit_per_min {
            return false;
        }
        entry.1 = next;
        true
    }
}

pub fn sealed_mailbox_for_evm(evm_addr: &str) -> String {
    let mut h = Sha256::new();
    h.update(b"cipher-sealed-v1:");
    h.update(evm_addr.trim().to_ascii_lowercase().as_bytes());
    hex::encode(h.finalize())
}

pub fn peer_dir_mailbox_for_evm(evm_addr: &str) -> String {
    let mut h = Sha256::new();
    h.update(b"cipher-peer-dir-v1:");
    h.update(evm_addr.trim().to_ascii_lowercase().as_bytes());
    format!("peer-dir-v1:{}", hex::encode(h.finalize()))
}

pub fn contact_mailbox_for_evm(sub_evm_addr: &str) -> String {
    let mut h = Sha256::new();
    h.update(b"cipher-contact-mailbox-v1:");
    h.update(sub_evm_addr.trim().to_ascii_lowercase().as_bytes());
    format!("contact-v1:{}", hex::encode(h.finalize()))
}

pub fn is_probe_recipient(id: &str) -> bool {
    id.trim() == PROBE_RECIPIENT_ID
}

pub fn is_public_directory_mailbox(id: &str) -> bool {
    id.trim().starts_with("peer-dir-v1:")
}

pub fn is_contact_mailbox(id: &str) -> bool {
    id.trim().starts_with("contact-v1:")
}

pub fn canonical_auth_message(action: &str, mailbox: &str, ts_ms: u64) -> String {
    format!(
        "TKS-CIPHER-AUTH-V1\naction:{}\nmailbox:{}\nts:{}",
        action.trim(),
        mailbox.trim().to_ascii_lowercase(),
        ts_ms
    )
}

pub fn canonical_store_message(recipient: &str, payload_sha256_hex: &str, ts_ms: u64) -> String {
    format!(
        "TKS-CIPHER-STORE-V1\nrecipient:{}\npayload_sha256:{}\nts:{}",
        recipient.trim().to_ascii_lowercase(),
        payload_sha256_hex.trim().to_ascii_lowercase(),
        ts_ms
    )
}

pub fn single_payload_sha256(ciphertext: &[u8]) -> String {
    hex::encode(Sha256::digest(ciphertext))
}

pub fn batch_payload_sha256<'a, I>(items: I) -> String
where
    I: IntoIterator<Item = (&'a str, &'a [u8])>,
{
    let mut outer = Sha256::new();
    for (recipient_id, ciphertext) in items {
        let ct_hex = single_payload_sha256(ciphertext);
        outer.update(recipient_id.trim().to_ascii_lowercase().as_bytes());
        outer.update(b":");
        outer.update(ct_hex.as_bytes());
        outer.update(b"\n");
    }
    hex::encode(outer.finalize())
}

fn recover_evm_from_message(msg: &str, sig_hex: &str) -> Result<String, &'static str> {
    let clean_hex = sig_hex.trim().strip_prefix("0x").unwrap_or(sig_hex.trim());
    let sig_bytes = hex::decode(clean_hex).map_err(|_| "invalid signature hex")?;
    if sig_bytes.len() != 65 {
        return Err("invalid signature length");
    }
    let rec_byte = match sig_bytes[64] {
        0 | 1 => sig_bytes[64],
        27 | 28 => sig_bytes[64] - 27,
        _ => return Err("invalid recovery id"),
    };
    let rec_id = RecoveryId::from_byte(rec_byte).ok_or("invalid recovery id")?;
    let signature =
        Signature::from_slice(&sig_bytes[..64]).map_err(|_| "malformed ECDSA signature")?;
    let digest = Keccak256::digest(msg.as_bytes());
    let vk = VerifyingKey::recover_from_prehash(&digest, &signature, rec_id)
        .map_err(|_| "signature recovery failed")?;
    let encoded = vk.to_encoded_point(false);
    let uncompressed = encoded.as_bytes();
    if uncompressed.len() != 65 {
        return Err("invalid recovered key");
    }
    let addr_hash = Keccak256::digest(&uncompressed[1..65]);
    Ok(format!("0x{}", hex::encode(&addr_hash[12..32])))
}

pub fn recover_signer_evm(
    action: &str,
    mailbox: &str,
    ts_ms: u64,
    sig_hex: &str,
) -> Result<String, &'static str> {
    let msg = canonical_auth_message(action, mailbox, ts_ms);
    recover_evm_from_message(&msg, sig_hex)
}

pub fn verify_mailbox_signature(
    action: &str,
    mailbox: &str,
    ts_ms: u64,
    sig_hex: &str,
    now_ms: u64,
) -> Result<(), &'static str> {
    if now_ms.abs_diff(ts_ms) > AUTH_MAX_SKEW_MS {
        return Err("signature timestamp expired");
    }
    let target = mailbox.trim().to_ascii_lowercase();
    if target.is_empty() || target.len() > 128 {
        return Err("invalid mailbox id");
    }
    let signer_evm = recover_signer_evm(action, &target, ts_ms, sig_hex)?;
    let authorized = if target.starts_with("0x") && target.len() == 42 {
        signer_evm == target
    } else if target.starts_with("peer-dir-v1:") {
        peer_dir_mailbox_for_evm(&signer_evm) == target
    } else if target.starts_with("contact-v1:") {
        contact_mailbox_for_evm(&signer_evm) == target
    } else {
        sealed_mailbox_for_evm(&signer_evm) == target
    };
    if authorized {
        Ok(())
    } else {
        Err("signer does not own mailbox")
    }
}

pub fn authorize_http_mailbox(
    action: &str,
    mailbox: &str,
    headers: &HeaderMap,
    enforce: bool,
) -> Result<(), (StatusCode, &'static str)> {
    if !enforce {
        return Ok(());
    }
    let ts_raw = headers
        .get("x-tks-ts")
        .and_then(|v| v.to_str().ok())
        .ok_or((StatusCode::UNAUTHORIZED, "missing X-TKS-Ts header"))?;
    let ts_ms = ts_raw
        .trim()
        .parse::<u64>()
        .map_err(|_| (StatusCode::UNAUTHORIZED, "invalid X-TKS-Ts header"))?;
    let sig_hex = headers
        .get("x-tks-sig")
        .and_then(|v| v.to_str().ok())
        .ok_or((StatusCode::UNAUTHORIZED, "missing X-TKS-Sig header"))?;
    verify_mailbox_signature(action, mailbox, ts_ms, sig_hex, now_unix_ms())
        .map_err(|e| (StatusCode::UNAUTHORIZED, e))
}

pub fn authorize_ws_mailbox(
    mailbox: &str,
    query: Option<&str>,
    enforce: bool,
) -> Result<(), (StatusCode, &'static str)> {
    if !enforce {
        return Ok(());
    }
    let q = query.ok_or((
        StatusCode::UNAUTHORIZED,
        "missing WebSocket auth parameters",
    ))?;
    let mut ts_val: Option<&str> = None;
    let mut sig_val: Option<&str> = None;
    for pair in q.split('&') {
        let Some((k, v)) = pair.split_once('=') else {
            continue;
        };
        match k {
            "ts" => ts_val = Some(v),
            "sig" => sig_val = Some(v),
            _ => {}
        }
    }
    let ts_ms = ts_val.and_then(|s| s.trim().parse::<u64>().ok()).ok_or((
        StatusCode::UNAUTHORIZED,
        "missing or invalid ws ts parameter",
    ))?;
    let sig_hex = sig_val.ok_or((StatusCode::UNAUTHORIZED, "missing ws sig parameter"))?;
    verify_mailbox_signature("ws", mailbox, ts_ms, sig_hex, now_unix_ms())
        .map_err(|e| (StatusCode::UNAUTHORIZED, e))
}

pub fn leading_zero_bits(hash: &[u8; 32]) -> u32 {
    let mut bits = 0u32;
    for &b in hash {
        if b == 0 {
            bits += 8;
        } else {
            bits += b.leading_zeros();
            break;
        }
    }
    bits
}

pub fn pow_preimage(
    sender_evm: &str,
    recipient: &str,
    payload_sha256_hex: &str,
    ts_ms: u64,
    nonce: u64,
) -> String {
    format!(
        "TKS-CIPHER-POW-V1:{}:{}:{}:{}:{}",
        sender_evm.trim().to_ascii_lowercase(),
        recipient.trim().to_ascii_lowercase(),
        payload_sha256_hex.trim().to_ascii_lowercase(),
        ts_ms,
        nonce
    )
}

pub fn verify_pow_stamp(
    sender_evm: &str,
    recipient: &str,
    payload_sha256_hex: &str,
    ts_ms: u64,
    nonce: u64,
    required_bits: u32,
) -> bool {
    if required_bits == 0 {
        return true;
    }
    let msg = pow_preimage(sender_evm, recipient, payload_sha256_hex, ts_ms, nonce);
    let digest: [u8; 32] = Sha256::digest(msg.as_bytes()).into();
    leading_zero_bits(&digest) >= required_bits
}

#[cfg(test)]
pub(crate) fn solve_pow_stamp(
    sender_evm: &str,
    recipient: &str,
    payload_sha256_hex: &str,
    ts_ms: u64,
    required_bits: u32,
) -> u64 {
    if required_bits == 0 {
        return 0;
    }
    let mut nonce = 0u64;
    loop {
        if verify_pow_stamp(
            sender_evm,
            recipient,
            payload_sha256_hex,
            ts_ms,
            nonce,
            required_bits,
        ) {
            return nonce;
        }
        nonce = nonce.wrapping_add(1);
    }
}

/// Result of authenticating a `/store` or `/store/batch` request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StoreCallerAuth {
    /// Recovered EVM address + whether sender proved on-chain trust OR Hashcash PoW.
    Signed {
        sender_evm: String,
        ts_ms: u64,
        on_chain_trusted: bool,
        pow_verified: bool,
    },
    /// Legacy unsigned mode when `CIPHER_REQUIRE_CLIENT_SIG=0`.
    LegacyUnauth,
}

impl StoreCallerAuth {
    pub fn sender_key(&self) -> &str {
        match self {
            Self::Signed { sender_evm, .. } => sender_evm.as_str(),
            Self::LegacyUnauth => "legacy",
        }
    }

    pub fn is_verified_sender(&self) -> bool {
        match self {
            Self::Signed {
                on_chain_trusted,
                pow_verified,
                ..
            } => *on_chain_trusted || *pow_verified,
            Self::LegacyUnauth => false,
        }
    }
}

pub struct OnChainTrustCache {
    entries: Mutex<HashMap<String, (bool, Instant)>>,
}

impl Default for OnChainTrustCache {
    fn default() -> Self {
        Self::new()
    }
}

impl OnChainTrustCache {
    pub fn new() -> Self {
        Self {
            entries: Mutex::new(HashMap::new()),
        }
    }

    pub async fn is_trusted_sender(
        &self,
        http_client: &reqwest::Client,
        chain_rpc_url: Option<&str>,
        sender_evm: &str,
    ) -> bool {
        let Some(rpc_url) = chain_rpc_url else {
            return false;
        };
        let key = sender_evm.trim().to_ascii_lowercase();
        if !key.starts_with("0x") || key.len() != 42 {
            return false;
        }

        let now = Instant::now();
        let ttl = Duration::from_secs(TRUST_CACHE_TTL_SECS);
        if let Ok(map) = self.entries.lock() {
            if let Some(&(trusted, ts)) = map.get(&key) {
                if now.duration_since(ts) < ttl {
                    return trusted;
                }
            }
        }

        let Some(trusted) = query_chain_sender_trusted(http_client, rpc_url, &key).await else {
            // Do not cache transient RPC outages; caller will still be accepted with PoW.
            return false;
        };

        if let Ok(mut map) = self.entries.lock() {
            if map.len() >= MAX_TRUST_CACHE_ENTRIES && !map.contains_key(&key) {
                map.retain(|_, (_, ts)| now.duration_since(*ts) < ttl);
                if map.len() >= MAX_TRUST_CACHE_ENTRIES {
                    map.clear();
                }
            }
            map.insert(key, (trusted, now));
        }
        trusted
    }
}

fn twox_128(data: &[u8]) -> [u8; 16] {
    let mut h0 = twox_hash::XxHash64::with_seed(0);
    h0.write(data);
    let r0 = h0.finish();
    let mut h1 = twox_hash::XxHash64::with_seed(1);
    h1.write(data);
    let r1 = h1.finish();
    let mut result = [0u8; 16];
    result[..8].copy_from_slice(&r0.to_le_bytes());
    result[8..].copy_from_slice(&r1.to_le_bytes());
    result
}

pub(crate) fn name_registry_reverse_key(address_hex: &str) -> Option<String> {
    use blake2::digest::{Update as _, VariableOutput as _};
    let clean = address_hex.trim().strip_prefix("0x").unwrap_or(address_hex);
    let bytes = hex::decode(clean).ok()?;
    if bytes.len() != 20 {
        return None;
    }
    let pallet = twox_128(b"NameRegistry");
    let storage = twox_128(b"ReverseLookup");
    let mut hasher = blake2::Blake2bVar::new(16).ok()?;
    hasher.update(&bytes);
    let mut b2 = vec![0u8; 16];
    hasher.finalize_variable(&mut b2).ok()?;
    b2.extend_from_slice(&bytes);
    Some(format!(
        "0x{}{}{}",
        hex::encode(pallet),
        hex::encode(storage),
        hex::encode(b2)
    ))
}

async fn query_chain_sender_trusted(
    http_client: &reqwest::Client,
    rpc_url: &str,
    sender_evm: &str,
) -> Option<bool> {
    // 1) Check non-zero TKS EVM balance (`eth_getBalance`).
    let bal_body = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "eth_getBalance",
        "params": [sender_evm, "latest"]
    });
    let bal_resp = http_client
        .post(rpc_url)
        .json(&bal_body)
        .timeout(Duration::from_secs(2))
        .send()
        .await
        .ok()?;
    let bal_json: serde_json::Value = bal_resp.json().await.ok()?;
    if let Some(hex_bal) = bal_json.get("result").and_then(|r| r.as_str()) {
        let stripped = hex_bal.trim().strip_prefix("0x").unwrap_or(hex_bal.trim());
        if stripped.bytes().any(|b| b != b'0') {
            return Some(true);
        }
    }

    // 2) Fallback: check registered username in `NameRegistry::ReverseLookup`.
    let storage_key = name_registry_reverse_key(sender_evm)?;
    let reg_body = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "state_getStorage",
        "params": [storage_key]
    });
    let reg_resp = http_client
        .post(rpc_url)
        .json(&reg_body)
        .timeout(Duration::from_secs(2))
        .send()
        .await
        .ok()?;
    let reg_json: serde_json::Value = reg_resp.json().await.ok()?;
    if let Some(res) = reg_json.get("result").and_then(|r| r.as_str()) {
        if !res.is_empty() && res != "0x" {
            return Some(true);
        }
    }
    Some(false)
}

#[derive(Debug, Clone)]
pub struct StoreAuthError {
    pub status: StatusCode,
    pub message: &'static str,
    pub required_pow_bits: Option<u32>,
}

/// Authenticate a `/store` or `/store/batch` request:
/// - Layer 1: Any `peer-dir-v1:*` write MUST be signed by its wallet owner.
/// - Layer 3: Every store signs `TKS-CIPHER-STORE-V1` (when `enforce = true`) and must
///   either pass on-chain trust (`eth_getBalance > 0` or `NameRegistry`) OR present
///   a valid `X-TKS-Pow` Hashcash stamp at `required_pow_bits`.
#[allow(clippy::too_many_arguments)]
pub async fn authorize_store_request(
    recipient_scope: &str,
    peer_dir_targets: &[&str],
    payload_sha256_hex: &str,
    headers: &HeaderMap,
    enforce: bool,
    required_pow_bits: u32,
    http_client: &reqwest::Client,
    chain_rpc_url: Option<&str>,
    trust_cache: &OnChainTrustCache,
) -> Result<StoreCallerAuth, StoreAuthError> {
    let ts_hdr = headers.get("x-tks-ts").and_then(|v| v.to_str().ok());
    let sig_hdr = headers.get("x-tks-sig").and_then(|v| v.to_str().ok());

    let (ts_raw, sig_hex) = match (ts_hdr, sig_hdr) {
        (Some(t), Some(s)) => (t, s),
        _ if !enforce => {
            return Ok(StoreCallerAuth::LegacyUnauth);
        }
        (None, _) => {
            return Err(StoreAuthError {
                status: StatusCode::UNAUTHORIZED,
                message: "missing X-TKS-Ts header",
                required_pow_bits: None,
            });
        }
        (_, None) => {
            return Err(StoreAuthError {
                status: StatusCode::UNAUTHORIZED,
                message: "missing X-TKS-Sig header",
                required_pow_bits: None,
            });
        }
    };

    let ts_ms = ts_raw.trim().parse::<u64>().map_err(|_| StoreAuthError {
        status: StatusCode::UNAUTHORIZED,
        message: "invalid X-TKS-Ts header",
        required_pow_bits: None,
    })?;
    if now_unix_ms().abs_diff(ts_ms) > AUTH_MAX_SKEW_MS {
        return Err(StoreAuthError {
            status: StatusCode::UNAUTHORIZED,
            message: "signature timestamp expired",
            required_pow_bits: None,
        });
    }

    let canonical = canonical_store_message(recipient_scope, payload_sha256_hex, ts_ms);
    let sender_evm = recover_evm_from_message(&canonical, sig_hex).map_err(|e| StoreAuthError {
        status: StatusCode::UNAUTHORIZED,
        message: e,
        required_pow_bits: None,
    })?;

    // Layer 1: Only the wallet owner may publish to their `peer-dir-v1:*` mailbox.
    for target in peer_dir_targets {
        if peer_dir_mailbox_for_evm(&sender_evm) != target.trim().to_ascii_lowercase() {
            return Err(StoreAuthError {
                status: StatusCode::UNAUTHORIZED,
                message: "signer does not own peer-dir mailbox",
                required_pow_bits: None,
            });
        }
    }

    if !enforce {
        return Ok(StoreCallerAuth::Signed {
            sender_evm,
            ts_ms,
            on_chain_trusted: false,
            pow_verified: false,
        });
    }

    let pow_verified = headers
        .get("x-tks-pow")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.trim().parse::<u64>().ok())
        .map(|nonce| {
            verify_pow_stamp(
                &sender_evm,
                recipient_scope,
                payload_sha256_hex,
                ts_ms,
                nonce,
                required_pow_bits,
            )
        })
        .unwrap_or(false);

    if pow_verified || required_pow_bits == 0 {
        return Ok(StoreCallerAuth::Signed {
            sender_evm,
            ts_ms,
            on_chain_trusted: false,
            pow_verified: required_pow_bits == 0 || pow_verified,
        });
    }

    let on_chain_trusted = trust_cache
        .is_trusted_sender(http_client, chain_rpc_url, &sender_evm)
        .await;
    if on_chain_trusted {
        return Ok(StoreCallerAuth::Signed {
            sender_evm,
            ts_ms,
            on_chain_trusted: true,
            pow_verified: false,
        });
    }

    Err(StoreAuthError {
        status: StatusCode::TOO_MANY_REQUESTS,
        message: "PoW stamp or funded/registered TKS account required",
        required_pow_bits: Some(required_pow_bits),
    })
}

#[cfg(test)]
pub(crate) fn sign_for_test(
    sk: &k256::ecdsa::SigningKey,
    action: &str,
    mailbox: &str,
    ts_ms: u64,
) -> String {
    let msg = canonical_auth_message(action, mailbox, ts_ms);
    let digest = Keccak256::digest(msg.as_bytes());
    let (sig, rec) = sk.sign_prehash_recoverable(&digest).unwrap();
    let mut raw = [0u8; 65];
    raw[..64].copy_from_slice(&sig.to_bytes());
    raw[64] = rec.to_byte();
    hex::encode(raw)
}

#[cfg(test)]
pub(crate) fn sign_store_for_test(
    sk: &k256::ecdsa::SigningKey,
    recipient: &str,
    payload_sha256_hex: &str,
    ts_ms: u64,
) -> String {
    let msg = canonical_store_message(recipient, payload_sha256_hex, ts_ms);
    let digest = Keccak256::digest(msg.as_bytes());
    let (sig, rec) = sk.sign_prehash_recoverable(&digest).unwrap();
    let mut raw = [0u8; 65];
    raw[..64].copy_from_slice(&sig.to_bytes());
    raw[64] = rec.to_byte();
    hex::encode(raw)
}

#[cfg(test)]
mod tests {
    use super::*;
    use k256::ecdsa::SigningKey;

    fn test_key_and_evm(seed: u8) -> (SigningKey, String) {
        let sk = SigningKey::from_slice(&[seed; 32]).unwrap();
        let vk = sk.verifying_key();
        let pt = vk.to_encoded_point(false);
        let h = Keccak256::digest(&pt.as_bytes()[1..65]);
        let evm = format!("0x{}", hex::encode(&h[12..32]));
        (sk, evm)
    }

    #[test]
    fn verifies_sealed_peer_dir_contact_and_raw_evm_mailboxes() {
        let (alice_sk, alice_evm) = test_key_and_evm(11);
        let (bob_sk, bob_evm) = test_key_and_evm(22);
        let (sub_sk, sub_evm) = test_key_and_evm(33);
        let now = now_unix_ms();

        let alice_sealed = sealed_mailbox_for_evm(&alice_evm);
        let sig_fetch = sign_for_test(&alice_sk, "fetch", &alice_sealed, now);
        assert!(verify_mailbox_signature("fetch", &alice_sealed, now, &sig_fetch, now).is_ok());

        // Cross-action & cross-mailbox signatures must fail.
        assert!(verify_mailbox_signature("ack", &alice_sealed, now, &sig_fetch, now).is_err());
        let bob_sig_on_alice = sign_for_test(&bob_sk, "ack", &alice_sealed, now);
        assert!(
            verify_mailbox_signature("ack", &alice_sealed, now, &bob_sig_on_alice, now).is_err()
        );

        // Peer directory ownership
        let alice_dir = peer_dir_mailbox_for_evm(&alice_evm);
        let sig_dir = sign_for_test(&alice_sk, "ack", &alice_dir, now);
        assert!(verify_mailbox_signature("ack", &alice_dir, now, &sig_dir, now).is_ok());
        assert!(verify_mailbox_signature("ack", &alice_dir, now - 400_000, &sig_dir, now).is_err());

        // Unlinkable contact mailbox ownership
        let contact_box = contact_mailbox_for_evm(&sub_evm);
        let sig_contact = sign_for_test(&sub_sk, "fetch", &contact_box, now);
        assert!(verify_mailbox_signature("fetch", &contact_box, now, &sig_contact, now).is_ok());
        let wrong_contact = sign_for_test(&bob_sk, "fetch", &contact_box, now);
        assert!(verify_mailbox_signature("fetch", &contact_box, now, &wrong_contact, now).is_err());

        // Raw EVM address
        let sig_reg = sign_for_test(&bob_sk, "register", &bob_evm, now);
        assert!(verify_mailbox_signature("register", &bob_evm, now, &sig_reg, now).is_ok());
    }

    #[tokio::test]
    async fn authorize_store_enforces_peer_dir_owner_and_pow() {
        let (alice_sk, alice_evm) = test_key_and_evm(41);
        let (bob_sk, bob_evm) = test_key_and_evm(42);
        let bob_dir = peer_dir_mailbox_for_evm(&bob_evm);
        let payload = b"bundle-bytes";
        let payload_hash = single_payload_sha256(payload);
        let ts = now_unix_ms();
        let client = reqwest::Client::new();
        let cache = OnChainTrustCache::new();

        // Unsigned store rejected when enforced.
        let err = authorize_store_request(
            &bob_dir,
            &[bob_dir.as_str()],
            &payload_hash,
            &HeaderMap::new(),
            true,
            8,
            &client,
            None,
            &cache,
        )
        .await
        .unwrap_err();
        assert_eq!(err.status, StatusCode::UNAUTHORIZED);

        // Alice CANNOT write to Bob's peer-dir mailbox even with a valid signature & PoW.
        let alice_sig = sign_store_for_test(&alice_sk, &bob_dir, &payload_hash, ts);
        let alice_nonce = solve_pow_stamp(&alice_evm, &bob_dir, &payload_hash, ts, 8);
        let mut h = HeaderMap::new();
        h.insert("X-TKS-Ts", ts.to_string().parse().unwrap());
        h.insert("X-TKS-Sig", alice_sig.parse().unwrap());
        h.insert("X-TKS-Pow", alice_nonce.to_string().parse().unwrap());
        let err = authorize_store_request(
            &bob_dir,
            &[bob_dir.as_str()],
            &payload_hash,
            &h,
            true,
            8,
            &client,
            None,
            &cache,
        )
        .await
        .unwrap_err();
        assert_eq!(err.status, StatusCode::UNAUTHORIZED);

        // Bob CAN write to Bob's peer-dir mailbox with PoW.
        let bob_sig = sign_store_for_test(&bob_sk, &bob_dir, &payload_hash, ts);
        let bob_nonce = solve_pow_stamp(&bob_evm, &bob_dir, &payload_hash, ts, 8);
        let mut h_bob = HeaderMap::new();
        h_bob.insert("X-TKS-Ts", ts.to_string().parse().unwrap());
        h_bob.insert("X-TKS-Sig", bob_sig.parse().unwrap());
        h_bob.insert("X-TKS-Pow", bob_nonce.to_string().parse().unwrap());
        let caller = authorize_store_request(
            &bob_dir,
            &[bob_dir.as_str()],
            &payload_hash,
            &h_bob,
            true,
            8,
            &client,
            None,
            &cache,
        )
        .await
        .unwrap();
        assert!(caller.is_verified_sender());

        // Missing PoW triggers 429 with X-TKS-Pow-Required-Bits hint.
        h_bob.remove("X-TKS-Pow");
        let err = authorize_store_request(
            &bob_dir,
            &[bob_dir.as_str()],
            &payload_hash,
            &h_bob,
            true,
            8,
            &client,
            None,
            &cache,
        )
        .await
        .unwrap_err();
        assert_eq!(err.status, StatusCode::TOO_MANY_REQUESTS);
        assert_eq!(err.required_pow_bits, Some(8));
    }

    #[test]
    fn client_ip_extraction_ignores_spoofed_headers_from_remote() {
        let mut h = HeaderMap::new();
        h.insert("x-forwarded-for", "1.2.3.4".parse().unwrap());
        h.insert("x-real-ip", "5.6.7.8".parse().unwrap());

        let remote: SocketAddr = "198.51.100.99:4444".parse().unwrap();
        assert_eq!(
            extract_client_ip(remote, &h),
            "198.51.100.99".parse::<IpAddr>().unwrap()
        );

        let loopback: SocketAddr = "127.0.0.1:4444".parse().unwrap();
        assert_eq!(
            extract_client_ip(loopback, &h),
            "5.6.7.8".parse::<IpAddr>().unwrap()
        );
    }

    #[test]
    fn rate_limiter_enforces_per_ip_window() {
        let rl = IpRateLimiter::new(2);
        let ip1: IpAddr = "203.0.113.1".parse().unwrap();
        let ip2: IpAddr = "203.0.113.2".parse().unwrap();
        assert!(rl.allow(ip1, 1));
        assert!(rl.allow(ip1, 1));
        assert!(!rl.allow(ip1, 1));
        assert!(rl.allow(ip2, 1));
    }

    #[test]
    fn name_registry_key_derives_expected_prefix_length() {
        let key = name_registry_reverse_key("0x1111111111111111111111111111111111111111").unwrap();
        // 0x + 32 (twox pallet) + 32 (twox storage) + 32 (blake2_128) + 40 (20-byte addr) = 138 chars
        assert_eq!(key.len(), 138);
    }
}
