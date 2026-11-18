//! Transport-independent security primitives for `cipher-relay` & `signal`:
//! - Trusted-proxy real client IP extraction (ignores spoofed headers from remote peers)
//! - Bounded per-IP fixed-window rate limiting
//! - secp256k1 recoverable ECDSA authentication binding mailboxes (`0x...`,
//!   `sealed_mailbox`, and `peer-dir-v1:...`) to wallet ownership

use std::collections::HashMap;
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

pub fn is_probe_recipient(id: &str) -> bool {
    id.trim() == PROBE_RECIPIENT_ID
}

pub fn is_public_directory_mailbox(id: &str) -> bool {
    id.trim().starts_with("peer-dir-v1:")
}

pub fn canonical_auth_message(action: &str, mailbox: &str, ts_ms: u64) -> String {
    format!(
        "TKS-CIPHER-AUTH-V1\naction:{}\nmailbox:{}\nts:{}",
        action.trim(),
        mailbox.trim().to_ascii_lowercase(),
        ts_ms
    )
}

pub fn recover_signer_evm(
    action: &str,
    mailbox: &str,
    ts_ms: u64,
    sig_hex: &str,
) -> Result<String, &'static str> {
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
    let msg = canonical_auth_message(action, mailbox, ts_ms);
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
        let mut it = pair.splitn(2, '=');
        let Some(k) = it.next() else { continue };
        let Some(v) = it.next() else { continue };
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
    fn verifies_sealed_peer_dir_and_raw_evm_mailboxes() {
        let (alice_sk, alice_evm) = test_key_and_evm(11);
        let (bob_sk, bob_evm) = test_key_and_evm(22);
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

        // Raw EVM address
        let sig_reg = sign_for_test(&bob_sk, "register", &bob_evm, now);
        assert!(verify_mailbox_signature("register", &bob_evm, now, &sig_reg, now).is_ok());
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
}
