//! # Cipher Storage Swarm Relay
//!
//! Per TKS Blueprint §3.5 — a relay server running alongside the TKS node.
//! Exposes two interfaces:
//! 1. **HTTP REST API** on port 4002 — for mobile clients (simple, cross-platform)
//! 2. **libp2p TCP** on port 4001 — HyperSwarm relay mesh (gossip + discovery)
//!
//! Endpoints:
//! - POST /store    — upload encrypted chunk for a recipient
//! - POST /fetch    — download chunks addressed to you
//! - POST /ack      — confirm receipt (deletes chunks)
//! - POST /register — register EVM address → presence
//! - POST /signal/push — federated instant push (peer relays → local signal sidecar)

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;

use bytes::Bytes;
use cipher_hybrid::HybridController;
use http_body_util::{BodyExt, Full};
use hyper::service::service_fn;
use hyper::header::HeaderMap;
use hyper::{body::Incoming, Method, Request, Response, StatusCode};
use hyper_util::rt::TokioIo;
use log::{debug, info, warn};
use tokio::sync::{Notify, RwLock};

pub mod build_info;
pub mod discovery;
pub mod hyperswarm;
mod signal;

pub use build_info::{ChainBuildInfo, SharedChainBuildInfo};

// ─────────────────────────────────────────────────────────
// Protocol Types (shared with client-core)
// ─────────────────────────────────────────────────────────

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct EncryptedChunk {
    pub recipient_id: String,
    pub message_id: String,
    pub ciphertext: Vec<u8>,
    pub stored_at: u64,
    pub expires_at: u64,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub enum RelayRequest {
    Store(EncryptedChunk),
    Fetch {
        recipient_id: String,
    },
    Ack {
        recipient_id: String,
        message_ids: Vec<String>,
    },
    Register {
        evm_address: String,
    },
    RpcProxy {
        endpoint: String,
        body: String,
    },
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub enum RelayResponse {
    Stored { message_id: String },
    Chunks(Vec<EncryptedChunk>),
    Acked { deleted: usize },
    Registered,
    RpcResult { body: String },
    Error(String),
}

// ─────────────────────────────────────────────────────────
// Storage Backend (in-memory with TTL)
// ─────────────────────────────────────────────────────────

pub(crate) struct ChunkStore {
    chunks: HashMap<String, Vec<EncryptedChunk>>,
    registered_peers: HashMap<String, u64>, // evm_address → last_seen timestamp
    notifiers: HashMap<String, Arc<Notify>>,
    max_bytes: usize,
    current_bytes: usize,
}

impl ChunkStore {
    fn new(max_bytes: usize) -> Self {
        Self {
            chunks: HashMap::new(),
            registered_peers: HashMap::new(),
            notifiers: HashMap::new(),
            max_bytes,
            current_bytes: 0,
        }
    }

    fn store(&mut self, chunk: EncryptedChunk) -> Result<String, String> {
        let chunk_size = chunk.ciphertext.len();
        if self.current_bytes + chunk_size > self.max_bytes {
            return Err("Storage full".into());
        }
        let msg_id = chunk.message_id.clone();
        self.current_bytes += chunk_size;
        self.chunks
            .entry(chunk.recipient_id.clone())
            .or_default()
            .push(chunk);
        Ok(msg_id)
    }

    fn fetch(&self, recipient_id: &str) -> Vec<EncryptedChunk> {
        self.chunks.get(recipient_id).cloned().unwrap_or_default()
    }

    fn ack(&mut self, recipient_id: &str, message_ids: &[String]) -> usize {
        if let Some(chunks) = self.chunks.get_mut(recipient_id) {
            let before = chunks.len();
            chunks.retain(|c| {
                if message_ids.contains(&c.message_id) {
                    self.current_bytes = self.current_bytes.saturating_sub(c.ciphertext.len());
                    false
                } else {
                    true
                }
            });
            before - chunks.len()
        } else {
            0
        }
    }

    fn register(&mut self, evm_address: &str) {
        let now = chrono::Utc::now().timestamp() as u64;
        self.registered_peers
            .insert(evm_address.to_lowercase(), now);
    }

    fn cleanup_expired(&mut self) {
        let now = chrono::Utc::now().timestamp() as u64;
        for chunks in self.chunks.values_mut() {
            chunks.retain(|c| {
                if c.expires_at < now {
                    self.current_bytes = self.current_bytes.saturating_sub(c.ciphertext.len());
                    false
                } else {
                    true
                }
            });
        }
        self.chunks.retain(|_, v| !v.is_empty());
    }

    fn stats(&self) -> (usize, usize, usize) {
        let total_chunks: usize = self.chunks.values().map(|v| v.len()).sum();
        (
            total_chunks,
            self.current_bytes,
            self.registered_peers.len(),
        )
    }

    pub(crate) fn get_chunk(&self, recipient_id: &str, message_id: &str) -> Option<EncryptedChunk> {
        self.chunks
            .get(recipient_id)
            .and_then(|list| list.iter().find(|c| c.message_id == message_id).cloned())
    }

    /// Idempotent store for HyperSwarm replication (same message_id allowed once).
    pub(crate) fn store_replicate(&mut self, chunk: EncryptedChunk) -> Result<(), String> {
        if self
            .get_chunk(&chunk.recipient_id, &chunk.message_id)
            .is_some()
        {
            return Ok(());
        }
        self.store(chunk).map(|_| ())
    }
}

pub(crate) type SharedStore = Arc<RwLock<ChunkStore>>;

/// HTTP relay server options (embedded in TKS node or standalone).
#[derive(Clone, Debug)]
pub struct RelayServerConfig {
    pub port: u16,
    /// When set, successful `/store` and `Store` relay ops forward batch leaves here.
    pub shard_ingest_url: Option<String>,
    /// Shared adaptive throughput controller (1k idle → 500k peak aggregate).
    pub hybrid: Option<Arc<HybridController>>,
    /// HyperSwarm libp2p gossip (peer relay discovery).
    pub enable_hyperswarm: bool,
    pub hyperswarm_port: u16,
    /// Public HTTPS base for this relay (advertised in GET /discovery).
    pub public_relay_url: Option<String>,
    /// Public WSS base for push (e.g. wss://my-node.example/ws). Defaults from relay URL + `/ws`.
    pub public_signal_ws: Option<String>,
    /// Local HTTP endpoint for instant WS push after store (e.g. http://127.0.0.1:4003/internal/push).
    pub signal_push_url: Option<String>,
    /// Substrate RPC for `tks_listSwarmValidators` (e.g. http://127.0.0.1:9944).
    pub chain_rpc_url: Option<String>,
    pub hyperswarm_bootnodes: Vec<String>,
    pub hyperswarm_region: Option<String>,
    /// Embedded node version + chain identity (GET /version).
    pub build_info: Option<SharedChainBuildInfo>,
    /// Shared secret for `POST /signal/push` from peer relays (`X-TKS-Federated-Push-Key`).
    pub federated_push_key: Option<String>,
    /// Embedded WebSocket signal server (replaces Node `vps-signal-server`).
    pub enable_embedded_signal: bool,
    pub signal_port: u16,
}

// ─────────────────────────────────────────────────────────
// HTTP Handler
// ─────────────────────────────────────────────────────────

struct RelayHttpContext {
    store: SharedStore,
    shard_ingest_url: Option<String>,
    hybrid: Option<Arc<HybridController>>,
    public_relay_url: Option<String>,
    public_signal_ws: Option<String>,
    signal_push_url: Option<String>,
    chain_rpc_url: Option<String>,
    hyperswarm: Option<Arc<hyperswarm::HyperSwarmHandle>>,
    build_info: Option<SharedChainBuildInfo>,
    federated_push_key: Option<String>,
    signal_hub: Option<Arc<signal::SignalHub>>,
}

#[derive(Clone)]
struct MeshPushContext {
    self_http: Option<String>,
    chain_rpc_url: Option<String>,
    federated_push_key: Option<String>,
    hyperswarm: Option<Arc<hyperswarm::HyperSwarmHandle>>,
}

async fn handle_request(
    req: Request<Incoming>,
    ctx: RelayHttpContext,
    remote_addr: SocketAddr,
) -> Result<Response<Full<Bytes>>, hyper::Error> {
    let RelayHttpContext {
        store,
        shard_ingest_url,
        hybrid,
        public_relay_url,
        public_signal_ws,
        signal_push_url,
        chain_rpc_url,
        hyperswarm,
        build_info,
        federated_push_key,
        signal_hub,
    } = ctx;

    let mesh_push = MeshPushContext {
        self_http: public_relay_url.clone(),
        chain_rpc_url: chain_rpc_url.clone(),
        federated_push_key: federated_push_key.clone(),
        hyperswarm: hyperswarm.clone(),
    };
    // CORS headers for all responses
    let cors_headers = |mut resp: Response<Full<Bytes>>| -> Response<Full<Bytes>> {
        resp.headers_mut()
            .insert("Access-Control-Allow-Origin", "*".parse().unwrap());
        resp.headers_mut().insert(
            "Access-Control-Allow-Methods",
            "POST, OPTIONS".parse().unwrap(),
        );
        resp.headers_mut().insert(
            "Access-Control-Allow-Headers",
            "Content-Type, X-TKS-Federated-Push-Key, X-TKS-Relay-Peer".parse().unwrap(),
        );
        resp
    };

    // Handle CORS preflight
    if req.method() == Method::OPTIONS {
        return Ok(cors_headers(Response::new(Full::new(Bytes::new()))));
    }

    let path = req.uri().path().to_string();
    let get_ok = matches!(
        path.as_str(),
        "/stats" | "/hybrid/stats" | "/discovery" | "/version"
    );
    if req.method() != Method::POST && !(req.method() == Method::GET && get_ok) {
        let resp = Response::builder()
            .status(StatusCode::METHOD_NOT_ALLOWED)
            .body(Full::new(Bytes::from("Method not allowed")))
            .unwrap();
        return Ok(cors_headers(resp));
    }

    let auth_headers = req.headers().clone();
    let body_bytes = if req.method() == Method::POST {
        req.collect().await?.to_bytes()
    } else {
        Bytes::new()
    };

    // Route by path
    let response = match path.as_str() {
        "/signal/push" => {
            if !authorize_federated_push(
                remote_addr,
                &auth_headers,
                federated_push_key.as_ref(),
            ) {
                return Ok(cors_headers(
                    Response::builder()
                        .status(StatusCode::FORBIDDEN)
                        .body(Full::new(Bytes::from(
                            r#"{"error":"federated push forbidden"}"#,
                        )))
                        .unwrap(),
                ));
            }
            match parse_signal_push_body(&body_bytes) {
                Ok((recipient, message_id, ciphertext)) => {
                    deliver_signal_push(
                        signal_hub.as_deref(),
                        signal_push_url.as_deref(),
                        &recipient,
                        &message_id,
                        &ciphertext,
                    )
                    .await;
                    serde_json::to_vec(&serde_json::json!({"ok": true}))
                        .unwrap_or_default()
                }
                Err(e) => serde_json::to_vec(&serde_json::json!({"error": e}))
                    .unwrap_or_default(),
            }
        }
        "/relay" => {
            // Generic bincode endpoint (used by client-core direct.rs)
            match bincode::deserialize::<RelayRequest>(&body_bytes) {
                Ok(relay_req) => {
                    handle_relay_request(
                        relay_req,
                        &store,
                        shard_ingest_url.clone(),
                        hybrid.clone(),
                        hyperswarm.clone(),
                        signal_push_url.clone(),
                        mesh_push.clone(),
                        signal_hub.clone(),
                    )
                    .await
                }
                Err(e) => {
                    let resp = RelayResponse::Error(format!("Decode error: {}", e));
                    bincode::serialize(&resp).unwrap_or_default()
                }
            }
        }
        "/store" => {
            // JSON endpoint for storing a chunk
            match serde_json::from_slice::<EncryptedChunk>(&body_bytes) {
                Ok(chunk) => {
                    let req = RelayRequest::Store(chunk.clone());
                    let resp_bytes =
                        handle_relay_request(
                            req,
                            &store,
                            shard_ingest_url.clone(),
                            hybrid.clone(),
                            hyperswarm.clone(),
                            signal_push_url.clone(),
                        mesh_push.clone(),
                        signal_hub.clone(),
                    )
                    .await;
                    if bincode::deserialize::<RelayResponse>(&resp_bytes)
                        .map(|r| matches!(r, RelayResponse::Stored { .. }))
                        .unwrap_or(false)
                    {
                        mesh_replicate(
                            chunk,
                            public_relay_url.clone(),
                            chain_rpc_url.clone(),
                            hyperswarm.as_ref().map(|h| h.directory.clone()),
                        );
                    }
                    match bincode::deserialize::<RelayResponse>(&resp_bytes) {
                        Ok(resp) => serde_json::to_vec(&resp).unwrap_or_default(),
                        Err(_) => resp_bytes,
                    }
                }
                Err(e) => serde_json::to_vec(&serde_json::json!({"error": e.to_string()})).unwrap_or_default(),
            }
        }
        "/fetch" => {
            // REST/mobile clients poll on their own — do not hold HTTP connections for 25s.
            match serde_json::from_slice::<serde_json::Value>(&body_bytes) {
                Ok(val) => {
                    let recipient_id = val["recipient_id"].as_str().unwrap_or("").to_string();
                    if recipient_id.is_empty() {
                        serde_json::to_vec(&serde_json::json!({"error": "missing recipient_id"}))
                            .unwrap_or_default()
                    } else {
                        // Read local mailbox first — do not block HTTP on libp2p chunk pull.
                        let chunks = store.read().await.fetch(&recipient_id);
                        if !chunks.is_empty() {
                            info!(
                                "📬 Fetch (HTTP): {} chunk(s) for {}",
                                chunks.len(),
                                recipient_id
                            );
                            serde_json::to_vec(&RelayResponse::Chunks(chunks)).unwrap_or_default()
                        } else if let Some(ref hs) = hyperswarm {
                            let _ = tokio::time::timeout(
                                std::time::Duration::from_secs(2),
                                hs.pull_for_recipient(&recipient_id),
                            )
                            .await;
                            let chunks = store.read().await.fetch(&recipient_id);
                            if !chunks.is_empty() {
                                info!(
                                    "📬 Fetch (HTTP+pulled): {} chunk(s) for {}",
                                    chunks.len(),
                                    recipient_id
                                );
                            }
                            serde_json::to_vec(&RelayResponse::Chunks(chunks)).unwrap_or_default()
                        } else {
                            serde_json::to_vec(&RelayResponse::Chunks(chunks)).unwrap_or_default()
                        }
                    }
                }
                Err(e) => serde_json::to_vec(&serde_json::json!({"error": e.to_string()})).unwrap_or_default(),
            }
        }
        "/ack" => {
            match serde_json::from_slice::<serde_json::Value>(&body_bytes) {
                Ok(val) => {
                    let recipient_id = val["recipient_id"].as_str().unwrap_or("").to_string();
                    let message_ids: Vec<String> = val["message_ids"]
                        .as_array()
                        .map(|arr| arr.iter().filter_map(|v| v.as_str().map(|s| s.to_string())).collect())
                        .unwrap_or_default();
                    let req = RelayRequest::Ack { recipient_id, message_ids };
                    let resp_bytes =
                        handle_relay_request(
                            req,
                            &store,
                            shard_ingest_url.clone(),
                            hybrid.clone(),
                            hyperswarm.clone(),
                            signal_push_url.clone(),
                        mesh_push.clone(),
                        signal_hub.clone(),
                    )
                    .await;
                    match bincode::deserialize::<RelayResponse>(&resp_bytes) {
                        Ok(resp) => serde_json::to_vec(&resp).unwrap_or_default(),
                        Err(_) => resp_bytes,
                    }
                }
                Err(e) => serde_json::to_vec(&serde_json::json!({"error": e.to_string()})).unwrap_or_default(),
            }
        }
        "/register" => {
            match serde_json::from_slice::<serde_json::Value>(&body_bytes) {
                Ok(val) => {
                    let evm_address = val["evm_address"].as_str().unwrap_or("").to_string();
                    let req = RelayRequest::Register { evm_address };
                    let resp_bytes =
                        handle_relay_request(
                            req,
                            &store,
                            shard_ingest_url.clone(),
                            hybrid.clone(),
                            hyperswarm.clone(),
                            signal_push_url.clone(),
                        mesh_push.clone(),
                        signal_hub.clone(),
                    )
                    .await;
                    match bincode::deserialize::<RelayResponse>(&resp_bytes) {
                        Ok(resp) => serde_json::to_vec(&resp).unwrap_or_default(),
                        Err(_) => resp_bytes,
                    }
                }
                Err(e) => serde_json::to_vec(&serde_json::json!({"error": e.to_string()})).unwrap_or_default(),
            }
        }
        "/stats" => {
            let s = store.read().await;
            let (chunks, bytes, peers) = s.stats();
            serde_json::to_vec(&serde_json::json!({
                "total_chunks": chunks,
                "total_bytes": bytes,
                "registered_peers": peers,
                "status": "ok"
            })).unwrap_or_default()
        }
        "/hybrid/stats" => {
            if let Some(h) = hybrid {
                serde_json::to_vec(&h.snapshot()).unwrap_or_default()
            } else {
                serde_json::to_vec(&serde_json::json!({"error":"hybrid disabled"})).unwrap_or_default()
            }
        }
        "/discovery" => {
            let peer_id = hyperswarm
                .as_ref()
                .map(|s| s.directory.local_peer_id().to_string());
            let swarm_dir = hyperswarm.as_ref().map(|h| h.directory.clone());
            let disc = discovery::build_discovery(
                public_relay_url.clone(),
                public_signal_ws.clone(),
                peer_id,
                chain_rpc_url.clone(),
                swarm_dir,
                hybrid.clone(),
            )
            .await;
            serde_json::to_vec(&disc).unwrap_or_default()
        }
        "/version" => {
            if let Some(info) = build_info {
                serde_json::to_vec(info.as_ref()).unwrap_or_default()
            } else {
                serde_json::to_vec(&serde_json::json!({"error":"build_info unavailable"}))
                    .unwrap_or_default()
            }
        }
        _ => {
            serde_json::to_vec(&serde_json::json!({
                "error": "Unknown endpoint",
                "endpoints": ["/relay", "/store", "/fetch", "/ack", "/register", "/signal/push", "/stats", "/hybrid/stats", "/discovery", "/version"]
            })).unwrap_or_default()
        }
    };

    let resp = Response::builder()
        .status(StatusCode::OK)
        .header("Content-Type", "application/octet-stream")
        .body(Full::new(Bytes::from(response)))
        .unwrap();

    Ok(cors_headers(resp))
}

/// Best-effort copy to other known relays (chain + gossip) so one host down ≠ lost messages.
fn mesh_replicate(
    chunk: EncryptedChunk,
    self_http: Option<String>,
    chain_rpc_url: Option<String>,
    swarm: Option<Arc<hyperswarm::SwarmDirectory>>,
) {
    tokio::spawn(async move {
        let peer_id = swarm.as_ref().map(|s| s.local_peer_id().to_string());
        let disc = discovery::build_discovery(
            self_http.clone(),
            None,
            peer_id,
            chain_rpc_url,
            swarm,
            None,
        )
        .await;
        let client = reqwest::Client::new();
        for relay in disc.relays.iter().take(8) {
            if relay.http_base.is_empty() {
                continue;
            }
            if Some(relay.http_base.as_str()) == self_http.as_deref() {
                continue;
            }
            let url = format!("{}/store", relay.http_base.trim_end_matches('/'));
            let _ = client.post(url).json(&chunk).send().await;
        }
    });
}

/// After local store, ask peer relays to run their co-located signal push (Phase 2).
fn mesh_signal_push(
    recipient_id: String,
    message_id: String,
    ciphertext: Vec<u8>,
    ctx: MeshPushContext,
) {
    tokio::spawn(async move {
        let peer_id = ctx
            .hyperswarm
            .as_ref()
            .map(|h| h.directory.local_peer_id().to_string());
        let disc = discovery::build_discovery(
            ctx.self_http.clone(),
            None,
            peer_id.clone(),
            ctx.chain_rpc_url.clone(),
            ctx.hyperswarm.as_ref().map(|h| h.directory.clone()),
            None,
        )
        .await;
        use base64::Engine;
        let data = base64::engine::general_purpose::STANDARD.encode(&ciphertext);
        let body = serde_json::json!({
            "to": recipient_id,
            "type": "message",
            "message_id": message_id,
            "data": data,
        });
        let client = reqwest::Client::new();
        for relay in disc.relays.iter().take(8) {
            if Some(relay.http_base.as_str()) == ctx.self_http.as_deref() {
                continue;
            }
            let Some(push_url) = relay
                .federated_push
                .clone()
                .or_else(|| discovery::federated_signal_push_url(&relay.http_base))
            else {
                continue;
            };
            let mut req = client
                .post(push_url)
                .json(&body)
                .timeout(std::time::Duration::from_secs(2));
            if let Some(ref key) = ctx.federated_push_key {
                req = req.header("X-TKS-Federated-Push-Key", key.as_str());
            }
            if let Some(ref pid) = peer_id {
                req = req.header("X-TKS-Relay-Peer", pid.as_str());
            }
            let _ = req.send().await;
        }
    });
}

fn authorize_federated_push(
    remote: SocketAddr,
    headers: &HeaderMap,
    federated_key: Option<&String>,
) -> bool {
    if remote.ip().is_loopback() {
        return true;
    }
    if let Some(expected) = federated_key.filter(|k| !k.is_empty()) {
        return header_value_eq(headers, "X-TKS-Federated-Push-Key", expected);
    }
    false
}

fn header_value_eq(headers: &HeaderMap, name: &str, expected: &str) -> bool {
    headers
        .get(name)
        .and_then(|v| v.to_str().ok())
        .map(|got| got == expected)
        .unwrap_or(false)
}

fn parse_signal_push_body(body: &[u8]) -> Result<(String, String, Vec<u8>), String> {
    let val: serde_json::Value =
        serde_json::from_slice(body).map_err(|e| format!("invalid JSON: {e}"))?;
    let recipient = val
        .get("to")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    if recipient.is_empty() {
        return Err("missing to".into());
    }
    let message_id = val
        .get("message_id")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    if message_id.is_empty() {
        return Err("missing message_id".into());
    }
    if let Some(b64) = val.get("data").and_then(|v| v.as_str()) {
        use base64::Engine;
        let ct = base64::engine::general_purpose::STANDARD
            .decode(b64)
            .map_err(|e| format!("invalid base64 data: {e}"))?;
        return Ok((recipient, message_id, ct));
    }
    if let Some(arr) = val.get("ciphertext").and_then(|v| v.as_array()) {
        let ct: Option<Vec<u8>> = arr
            .iter()
            .map(|v| v.as_u64().map(|n| n as u8))
            .collect();
        return ct
            .map(|c| (recipient, message_id, c))
            .ok_or_else(|| "invalid ciphertext array".into());
    }
    Err("missing data (base64) or ciphertext".into())
}

async fn deliver_signal_push(
    hub: Option<&signal::SignalHub>,
    push_url: Option<&str>,
    recipient_id: &str,
    message_id: &str,
    ciphertext: &[u8],
) {
    use base64::Engine;
    let data = base64::engine::general_purpose::STANDARD.encode(ciphertext);
    if let Some(h) = hub {
        let payload = serde_json::json!({
            "type": "message",
            "message_id": message_id,
            "data": data,
        });
        let outcome = h.push_json(recipient_id, payload).await;
        if outcome.delivered {
            log::debug!("⚡ WS push delivered for {}", recipient_id);
        }
        return;
    }
    if let Some(url) = push_url {
        forward_signal_push(url, recipient_id, message_id, ciphertext).await;
    }
}

/// Notify co-located signal server so online recipients get sub-100ms WS push.
async fn forward_signal_push(
    url: &str,
    recipient_id: &str,
    message_id: &str,
    ciphertext: &[u8],
) {
    use base64::Engine;
    let data = base64::engine::general_purpose::STANDARD.encode(ciphertext);
    let body = serde_json::json!({
        "to": recipient_id,
        "type": "message",
        "message_id": message_id,
        "data": data,
    });
    match reqwest::Client::new()
        .post(url)
        .json(&body)
        .timeout(std::time::Duration::from_secs(2))
        .send()
        .await
    {
        Ok(resp) if resp.status().is_success() => {
            log::debug!("⚡ WS push queued for {}", recipient_id);
        }
        Ok(resp) => {
            log::warn!(
                "Signal push HTTP {} for msg {}",
                resp.status(),
                message_id
            );
        }
        Err(e) => log::debug!("Signal push skipped ({e})"),
    }
}

async fn forward_shard_ingest(url: &str, chunk: &EncryptedChunk, msg_id: &str) {
    use sha2::{Digest, Sha256};
    let hash: [u8; 32] = Sha256::digest(&chunk.ciphertext).into();
    let body = serde_json::json!({
        "msg_id": msg_id,
        "sender_id": "cipher-relay",
        "recipient_id": chunk.recipient_id,
        "ciphertext_hash": hash,
        "stored_at": chunk.stored_at,
    });
    match reqwest::Client::new().post(url).json(&body).send().await {
        Ok(resp) if resp.status().is_success() => {}
        Ok(resp) => {
            log::warn!(
                "Cipher shard ingest returned HTTP {} for msg {}",
                resp.status(),
                msg_id
            );
        }
        Err(e) => log::warn!("Cipher shard ingest forward failed for {}: {e}", msg_id),
    }
}

async fn handle_relay_request(
    req: RelayRequest,
    store: &SharedStore,
    shard_ingest_url: Option<String>,
    hybrid: Option<Arc<HybridController>>,
    hyperswarm: Option<Arc<hyperswarm::HyperSwarmHandle>>,
    signal_push_url: Option<String>,
    mesh_push: MeshPushContext,
    signal_hub: Option<Arc<signal::SignalHub>>,
) -> Vec<u8> {
    let response = match req {
        RelayRequest::Store(chunk) => {
            let recipient = chunk.recipient_id.clone();
            let mut s = store.write().await;
            match s.store(chunk.clone()) {
                Ok(msg_id) => {
                    if let Some(ref h) = hybrid {
                        h.record_ingress();
                    }
                    info!("📦 Stored message {} for {}", msg_id, recipient);
                    if let Some(notifier) = s.notifiers.get(&recipient) {
                        notifier.notify_waiters();
                    }
                    if let Some(url) = shard_ingest_url {
                        let mid = msg_id.clone();
                        let c = chunk.clone();
                        tokio::spawn(async move {
                            forward_shard_ingest(&url, &c, &mid).await;
                        });
                    }
                    {
                        let hub = signal_hub.clone();
                        let push_url = signal_push_url.clone();
                        let mid = msg_id.clone();
                        let rid = recipient.clone();
                        let ct = chunk.ciphertext.clone();
                        tokio::spawn(async move {
                            deliver_signal_push(
                                hub.as_deref(),
                                push_url.as_deref(),
                                &rid,
                                &mid,
                                &ct,
                            )
                            .await;
                        });
                    }
                    {
                        let mid = msg_id.clone();
                        let rid = recipient.clone();
                        let ct = chunk.ciphertext.clone();
                        mesh_signal_push(rid, mid, ct, mesh_push);
                    }
                    drop(s);
                    if let Some(ref hs) = hyperswarm {
                        let ann = hyperswarm::chunk_avail_from(
                            &chunk,
                            &hs.directory.local_peer_id().to_string(),
                        );
                        hs.announce_chunk(ann);
                    }
                    RelayResponse::Stored { message_id: msg_id }
                }
                Err(e) => RelayResponse::Error(e),
            }
        }
        RelayRequest::Fetch { recipient_id } => {
            if let Some(ref hs) = hyperswarm {
                let pulled = hs.pull_for_recipient(&recipient_id).await;
                if pulled > 0 {
                    info!("🕸️ HyperSwarm pulled {pulled} chunk(s) for {recipient_id}");
                }
            }
            let notifier = {
                let mut s = store.write().await;
                let chunks = s.fetch(&recipient_id);
                if !chunks.is_empty() {
                    info!(
                        "📬 Fetch (Instant): {} chunks for {}",
                        chunks.len(),
                        recipient_id
                    );
                    return bincode::serialize(&RelayResponse::Chunks(chunks)).unwrap_or_default();
                }
                s.notifiers
                    .entry(recipient_id.clone())
                    .or_insert_with(|| Arc::new(tokio::sync::Notify::new()))
                    .clone()
            };

            let _ =
                tokio::time::timeout(std::time::Duration::from_secs(25), notifier.notified()).await;

            if let Some(ref hs) = hyperswarm {
                let _ = hs.pull_for_recipient(&recipient_id).await;
            }

            let s = store.read().await;
            let chunks = s.fetch(&recipient_id);
            if chunks.is_empty() {
                debug!("💤 Fetch (Timeout): 0 chunks for {}", recipient_id);
            } else {
                info!(
                    "📬 Fetch (Woken): {} chunks for {}",
                    chunks.len(),
                    recipient_id
                );
            }
            RelayResponse::Chunks(chunks)
        }
        RelayRequest::Ack {
            recipient_id,
            message_ids,
        } => {
            let mut s = store.write().await;
            let deleted = s.ack(&recipient_id, &message_ids);
            info!("✓ Acked {} chunks for {}", deleted, recipient_id);
            RelayResponse::Acked { deleted }
        }
        RelayRequest::Register { evm_address } => {
            let mut s = store.write().await;
            s.register(&evm_address);
            info!("📋 Registered peer: {}", evm_address);
            RelayResponse::Registered
        }
        RelayRequest::RpcProxy { endpoint, body } => {
            info!("🔄 Proxying RPC to {} ({} bytes)", endpoint, body.len());
            // Create a temporary client. In production, this can be cached.
            let client = reqwest::Client::new();
            match client
                .post(&endpoint)
                .header("Content-Type", "application/json")
                .body(body)
                .send()
                .await
            {
                Ok(resp) => match resp.text().await {
                    Ok(text) => RelayResponse::RpcResult { body: text },
                    Err(e) => RelayResponse::Error(format!("Failed to read RPC response: {}", e)),
                },
                Err(e) => RelayResponse::Error(format!("Failed to proxy RPC: {}", e)),
            }
        }
    };

    bincode::serialize(&response).unwrap_or_default()
}

// ─────────────────────────────────────────────────────────
// Main
// ─────────────────────────────────────────────────────────

pub async fn run_relay_server(port: u16) -> Result<(), Box<dyn std::error::Error>> {
    run_relay_server_with(RelayServerConfig {
        port,
        shard_ingest_url: None,
        hybrid: None,
        enable_hyperswarm: true,
        hyperswarm_port: 4001,
        public_relay_url: None,
        public_signal_ws: None,
        signal_push_url: std::env::var("CIPHER_SIGNAL_PUSH_URL")
            .ok()
            .filter(|s| !s.is_empty())
            .or_else(|| Some("http://127.0.0.1:4003/internal/push".into())),
        chain_rpc_url: None,
        hyperswarm_bootnodes: vec![],
        hyperswarm_region: None,
        build_info: None,
        federated_push_key: std::env::var("CIPHER_FEDERATED_PUSH_KEY")
            .ok()
            .filter(|s| !s.is_empty()),
        enable_embedded_signal: true,
        signal_port: 4003,
    })
    .await
}

pub async fn run_relay_server_with(
    config: RelayServerConfig,
) -> Result<(), Box<dyn std::error::Error>> {
    let port = config.port;
    let shard_ingest_url = config.shard_ingest_url.clone();
    let hybrid = config.hybrid.clone();
    let public_relay_url = config.public_relay_url.clone();
    let public_signal_ws = config
        .public_signal_ws
        .clone()
        .or_else(|| {
            public_relay_url
                .as_ref()
                .and_then(|u| discovery::http_base_to_signal_ws(u))
        });
    let enable_embedded_signal = config.enable_embedded_signal;
    let signal_port = config.signal_port;
    let signal_hub = if enable_embedded_signal {
        let hub = signal::SignalHub::new();
        let hub_run = hub.clone();
        tokio::spawn(async move {
            if let Err(e) = signal::run_signal_server(signal_port, hub_run).await {
                warn!("Embedded signal server exited: {e}");
            }
        });
        Some(hub)
    } else {
        None
    };

    let signal_push_url = config
        .signal_push_url
        .clone()
        .or_else(|| {
            std::env::var("CIPHER_SIGNAL_PUSH_URL")
                .ok()
                .filter(|s| !s.is_empty())
        })
        .or_else(|| {
            if enable_embedded_signal {
                Some(format!("http://127.0.0.1:{signal_port}/internal/push"))
            } else {
                None
            }
        });
    let chain_rpc_url = config.chain_rpc_url.clone();
    let build_info = config.build_info.clone();
    let federated_push_key = config
        .federated_push_key
        .clone()
        .or_else(|| {
            std::env::var("CIPHER_FEDERATED_PUSH_KEY")
                .ok()
                .filter(|s| !s.is_empty())
        });

    info!("🚀 Cipher Storage Swarm Relay v0.1.5 (HyperSwarm chunk pull)");
    info!("📋 Blueprint §3.5: Encrypted chunk storage with TTL auto-delete");

    let store: SharedStore = Arc::new(RwLock::new(ChunkStore::new(100 * 1024 * 1024 * 1024)));

    let hyperswarm = if config.enable_hyperswarm {
        let bootnodes: Result<Vec<_>, _> = config
            .hyperswarm_bootnodes
            .iter()
            .map(|s| s.parse())
            .collect();
        match hyperswarm::run_hyperswarm(
            store.clone(),
            hyperswarm::HyperSwarmConfig {
                listen_port: config.hyperswarm_port,
                public_http_base: public_relay_url.clone(),
                public_signal_ws: public_signal_ws.clone(),
                region: config.hyperswarm_region.clone(),
                bootnodes: bootnodes.unwrap_or_default(),
            },
        )
        .await
        {
            Ok(h) => Some(Arc::new(h)),
            Err(e) => {
                warn!("HyperSwarm libp2p failed to start: {e}");
                None
            }
        }
    } else {
        None
    };

    // Cleanup timer — every 5 minutes
    let store_cleanup = store.clone();
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(std::time::Duration::from_secs(300)).await;
            let mut s = store_cleanup.write().await;
            s.cleanup_expired();
            let (chunks, bytes, peers) = s.stats();
            info!(
                "🧹 Cleanup: {} chunks, {} bytes, {} peers",
                chunks, bytes, peers
            );
        }
    });

    // HTTP REST API on port 4002
    let addr: SocketAddr = format!("0.0.0.0:{}", port).parse()?;
    let listener = tokio::net::TcpListener::bind(addr).await?;
    info!("🌐 HTTP REST API listening on http://0.0.0.0:{}", port);
    info!("   Endpoints: /relay /store /fetch /ack /register /signal/push /stats /discovery");
    if let Some(ref url) = shard_ingest_url {
        info!("   Shard ingest forward: {url}");
    }
    if hybrid.is_some() {
        info!("   Hybrid throughput: enabled (GET /hybrid/stats)");
    }
    if hyperswarm.is_some() {
        info!(
            "   HyperSwarm libp2p on port {} (GET /discovery, chunk pull)",
            config.hyperswarm_port
        );
    }
    if signal_hub.is_some() {
        info!("   Embedded signal WS: port {signal_port} (/ws, /internal/push)");
    } else if let Some(ref push) = signal_push_url {
        info!("   WS instant push (external): {push}");
    }
    if let Some(ref ws) = public_signal_ws {
        info!("   Advertised signal WS: {ws}");
    }
    if federated_push_key.is_some() {
        info!("   Federated signal push: enabled (POST /signal/push + mesh fanout)");
    }

    loop {
        let (stream, remote_addr) = listener.accept().await?;
        let ctx = RelayHttpContext {
            store: store.clone(),
            shard_ingest_url: shard_ingest_url.clone(),
            hybrid: hybrid.clone(),
            public_relay_url: public_relay_url.clone(),
            public_signal_ws: public_signal_ws.clone(),
            signal_push_url: signal_push_url.clone(),
            chain_rpc_url: chain_rpc_url.clone(),
            hyperswarm: hyperswarm.clone(),
            build_info: build_info.clone(),
            federated_push_key: federated_push_key.clone(),
            signal_hub: signal_hub.clone(),
        };
        debug!("Connection from {}", remote_addr);

        tokio::spawn(async move {
            let io = TokioIo::new(stream);
            let service = service_fn(move |req| {
                let ctx = RelayHttpContext {
                    store: ctx.store.clone(),
                    shard_ingest_url: ctx.shard_ingest_url.clone(),
                    hybrid: ctx.hybrid.clone(),
                    public_relay_url: ctx.public_relay_url.clone(),
                    public_signal_ws: ctx.public_signal_ws.clone(),
                    signal_push_url: ctx.signal_push_url.clone(),
                    chain_rpc_url: ctx.chain_rpc_url.clone(),
                    hyperswarm: ctx.hyperswarm.clone(),
                    build_info: ctx.build_info.clone(),
                    federated_push_key: ctx.federated_push_key.clone(),
                    signal_hub: ctx.signal_hub.clone(),
                };
                let peer = remote_addr;
                async move { handle_request(req, ctx, peer).await }
            });

            if let Err(e) = hyper::server::conn::http1::Builder::new()
                .serve_connection(io, service)
                .await
            {
                warn!("HTTP error: {}", e);
            }
        });
    }
}
