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

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;

use cipher_hybrid::HybridController;
use tokio::sync::{RwLock, Notify};
use log::{info, warn, debug};
use hyper::{body::Incoming, Request, Response, Method, StatusCode};
use hyper::service::service_fn;
use hyper_util::rt::TokioIo;
use http_body_util::{Full, BodyExt};
use bytes::Bytes;

pub mod build_info;
pub mod discovery;
pub mod hyperswarm;

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
    Fetch { recipient_id: String },
    Ack { recipient_id: String, message_ids: Vec<String> },
    Register { evm_address: String },
    RpcProxy { endpoint: String, body: String },
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
        self.registered_peers.insert(evm_address.to_lowercase(), now);
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
        (total_chunks, self.current_bytes, self.registered_peers.len())
    }

    pub(crate) fn get_chunk(&self, recipient_id: &str, message_id: &str) -> Option<EncryptedChunk> {
        self.chunks.get(recipient_id).and_then(|list| {
            list.iter()
                .find(|c| c.message_id == message_id)
                .cloned()
        })
    }

    /// Idempotent store for HyperSwarm replication (same message_id allowed once).
    pub(crate) fn store_replicate(&mut self, chunk: EncryptedChunk) -> Result<(), String> {
        if self.get_chunk(&chunk.recipient_id, &chunk.message_id).is_some() {
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
    /// Public HTTPS base for this relay (e.g. https://relay.tokenkickstarter.com).
    pub public_relay_url: Option<String>,
    /// Substrate RPC for `tks_listSwarmValidators` (e.g. http://127.0.0.1:9944).
    pub chain_rpc_url: Option<String>,
    pub hyperswarm_bootnodes: Vec<String>,
    pub hyperswarm_region: Option<String>,
    /// Embedded node version + chain identity (GET /version).
    pub build_info: Option<SharedChainBuildInfo>,
}

// ─────────────────────────────────────────────────────────
// HTTP Handler
// ─────────────────────────────────────────────────────────

struct RelayHttpContext {
    store: SharedStore,
    shard_ingest_url: Option<String>,
    hybrid: Option<Arc<HybridController>>,
    public_relay_url: Option<String>,
    chain_rpc_url: Option<String>,
    hyperswarm: Option<Arc<hyperswarm::HyperSwarmHandle>>,
    build_info: Option<SharedChainBuildInfo>,
}

async fn handle_request(
    req: Request<Incoming>,
    ctx: RelayHttpContext,
) -> Result<Response<Full<Bytes>>, hyper::Error> {
    let RelayHttpContext {
        store,
        shard_ingest_url,
        hybrid,
        public_relay_url,
        chain_rpc_url,
        hyperswarm,
        build_info,
    } = ctx;
    // CORS headers for all responses
    let cors_headers = |mut resp: Response<Full<Bytes>>| -> Response<Full<Bytes>> {
        resp.headers_mut().insert("Access-Control-Allow-Origin", "*".parse().unwrap());
        resp.headers_mut().insert("Access-Control-Allow-Methods", "POST, OPTIONS".parse().unwrap());
        resp.headers_mut().insert("Access-Control-Allow-Headers", "Content-Type".parse().unwrap());
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

    let body_bytes = if req.method() == Method::POST {
        req.collect().await?.to_bytes()
    } else {
        Bytes::new()
    };

    // Route by path
    let response = match path.as_str() {
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
            match serde_json::from_slice::<serde_json::Value>(&body_bytes) {
                Ok(val) => {
                    let recipient_id = val["recipient_id"].as_str().unwrap_or("").to_string();
                    let req = RelayRequest::Fetch { recipient_id };
                    let resp_bytes =
                        handle_relay_request(
                            req,
                            &store,
                            shard_ingest_url.clone(),
                            hybrid.clone(),
                            hyperswarm.clone(),
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
                "endpoints": ["/relay", "/store", "/fetch", "/ack", "/register", "/stats", "/hybrid/stats", "/discovery", "/version"]
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
                    info!("📬 Fetch (Instant): {} chunks for {}", chunks.len(), recipient_id);
                    return bincode::serialize(&RelayResponse::Chunks(chunks)).unwrap_or_default();
                }
                s.notifiers.entry(recipient_id.clone())
                    .or_insert_with(|| Arc::new(tokio::sync::Notify::new()))
                    .clone()
            };

            let _ = tokio::time::timeout(std::time::Duration::from_secs(25), notifier.notified()).await;

            if let Some(ref hs) = hyperswarm {
                let _ = hs.pull_for_recipient(&recipient_id).await;
            }

            let s = store.read().await;
            let chunks = s.fetch(&recipient_id);
            if chunks.is_empty() {
                debug!("💤 Fetch (Timeout): 0 chunks for {}", recipient_id);
            } else {
                info!("📬 Fetch (Woken): {} chunks for {}", chunks.len(), recipient_id);
            }
            RelayResponse::Chunks(chunks)
        }
        RelayRequest::Ack { recipient_id, message_ids } => {
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
            match client.post(&endpoint)
                .header("Content-Type", "application/json")
                .body(body)
                .send()
                .await
            {
                Ok(resp) => {
                    match resp.text().await {
                        Ok(text) => RelayResponse::RpcResult { body: text },
                        Err(e) => RelayResponse::Error(format!("Failed to read RPC response: {}", e)),
                    }
                }
                Err(e) => {
                    RelayResponse::Error(format!("Failed to proxy RPC: {}", e))
                }
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
        chain_rpc_url: None,
        hyperswarm_bootnodes: vec![],
        hyperswarm_region: None,
        build_info: None,
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
    let chain_rpc_url = config.chain_rpc_url.clone();
    let build_info = config.build_info.clone();

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
            info!("🧹 Cleanup: {} chunks, {} bytes, {} peers", chunks, bytes, peers);
        }
    });

    // HTTP REST API on port 4002
    let addr: SocketAddr = format!("0.0.0.0:{}", port).parse()?;
    let listener = tokio::net::TcpListener::bind(addr).await?;
    info!("🌐 HTTP REST API listening on http://0.0.0.0:{}", port);
    info!("   Endpoints: /relay /store /fetch /ack /register /stats /discovery");
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

    loop {
        let (stream, remote_addr) = listener.accept().await?;
        let ctx = RelayHttpContext {
            store: store.clone(),
            shard_ingest_url: shard_ingest_url.clone(),
            hybrid: hybrid.clone(),
            public_relay_url: public_relay_url.clone(),
            chain_rpc_url: chain_rpc_url.clone(),
            hyperswarm: hyperswarm.clone(),
            build_info: build_info.clone(),
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
                    chain_rpc_url: ctx.chain_rpc_url.clone(),
                    hyperswarm: ctx.hyperswarm.clone(),
                    build_info: ctx.build_info.clone(),
                };
                async move { handle_request(req, ctx).await }
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
