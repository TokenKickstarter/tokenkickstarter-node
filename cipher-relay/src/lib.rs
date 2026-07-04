//! # Cipher Storage Swarm Relay
//!
//! Per TKS Blueprint §3.5 — a relay server running alongside the TKS node.
//! Exposes two interfaces:
//! 1. **HTTP REST API** on port 4002 — for mobile clients (simple, cross-platform)
//! 2. **libp2p TCP** on port 4001 — for full TKS nodes (future swarm)
//!
//! Endpoints:
//! - POST /store    — upload encrypted chunk for a recipient
//! - POST /fetch    — download chunks addressed to you
//! - POST /ack      — confirm receipt (deletes chunks)
//! - POST /register — register EVM address → presence

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::sync::{RwLock, Notify};
use log::{info, warn, debug};
use hyper::{body::Incoming, Request, Response, Method, StatusCode};
use hyper::service::service_fn;
use hyper_util::rt::TokioIo;
use http_body_util::{Full, BodyExt};
use bytes::Bytes;

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

struct ChunkStore {
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
}

type SharedStore = Arc<RwLock<ChunkStore>>;

// ─────────────────────────────────────────────────────────
// HTTP Handler
// ─────────────────────────────────────────────────────────

async fn handle_request(
    req: Request<Incoming>,
    store: SharedStore,
) -> Result<Response<Full<Bytes>>, hyper::Error> {
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

    if req.method() != &Method::POST {
        let resp = Response::builder()
            .status(StatusCode::METHOD_NOT_ALLOWED)
            .body(Full::new(Bytes::from("Method not allowed")))
            .unwrap();
        return Ok(cors_headers(resp));
    }

    // Extract path before consuming the request body
    let path = req.uri().path().to_string();

    // Read body
    let body_bytes = req.collect().await?.to_bytes();

    // Route by path
    let response = match path.as_str() {
        "/relay" => {
            // Generic bincode endpoint (used by client-core direct.rs)
            match bincode::deserialize::<RelayRequest>(&body_bytes) {
                Ok(relay_req) => handle_relay_request(relay_req, &store).await,
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
                    let req = RelayRequest::Store(chunk);
                    let resp_bytes = handle_relay_request(req, &store).await;
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
                    let resp_bytes = handle_relay_request(req, &store).await;
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
                    let resp_bytes = handle_relay_request(req, &store).await;
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
                    let resp_bytes = handle_relay_request(req, &store).await;
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
        _ => {
            serde_json::to_vec(&serde_json::json!({
                "error": "Unknown endpoint",
                "endpoints": ["/relay", "/store", "/fetch", "/ack", "/register", "/stats"]
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

async fn handle_relay_request(req: RelayRequest, store: &SharedStore) -> Vec<u8> {
    let response = match req {
        RelayRequest::Store(chunk) => {
            let recipient = chunk.recipient_id.clone();
            let mut s = store.write().await;
            match s.store(chunk) {
                Ok(msg_id) => {
                    info!("📦 Stored message {} for {}", msg_id, recipient);
                    if let Some(notifier) = s.notifiers.get(&recipient) {
                        notifier.notify_waiters();
                    }
                    RelayResponse::Stored { message_id: msg_id }
                }
                Err(e) => RelayResponse::Error(e),
            }
        }
        RelayRequest::Fetch { recipient_id } => {
            let notifier = {
                let mut s = store.write().await;
                let chunks = s.fetch(&recipient_id);
                if !chunks.is_empty() {
                    info!("📬 Fetch (Instant): {} chunks for {}", chunks.len(), recipient_id);
                    return bincode::serialize(&RelayResponse::Chunks(chunks)).unwrap_or_default();
                }
                // Register a wakeup channel
                s.notifiers.entry(recipient_id.clone())
                    .or_insert_with(|| Arc::new(tokio::sync::Notify::new()))
                    .clone()
            };

            // Hold the connection open for up to 25 seconds for <100ms instant delivery
            let _ = tokio::time::timeout(std::time::Duration::from_secs(25), notifier.notified()).await;

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
    // Note: Logging should be initialized by the outer caller (e.g. Substrate node or our main.rs wrapper)

    info!("🚀 Cipher Storage Swarm Relay v0.1.0");
    info!("📋 Blueprint §3.5: Encrypted chunk storage with TTL auto-delete");

    // Storage backend (100 GB max)
    let store: SharedStore = Arc::new(RwLock::new(ChunkStore::new(100 * 1024 * 1024 * 1024)));

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
    info!("   Endpoints: /relay /store /fetch /ack /register /stats");

    loop {
        let (stream, remote_addr) = listener.accept().await?;
        let store = store.clone();
        debug!("Connection from {}", remote_addr);

        tokio::spawn(async move {
            let io = TokioIo::new(stream);
            let service = service_fn(move |req| {
                let store = store.clone();
                handle_request(req, store)
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
