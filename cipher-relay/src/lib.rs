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
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;

use bytes::Bytes;
use cipher_hybrid::HybridController;
use futures_util::future::join_all;
use http_body_util::{BodyExt, Full};
use hyper::header::HeaderMap;
use hyper::service::service_fn;
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

const STORE_SHARDS: usize = 64;

#[derive(Default)]
struct ChunkShard {
    chunks: HashMap<String, Vec<EncryptedChunk>>,
    registered_peers: HashMap<String, u64>, // evm_address → last_seen timestamp
    notifiers: HashMap<String, Arc<Notify>>,
}

impl ChunkShard {
    fn push_chunk(&mut self, chunk: EncryptedChunk) -> (String, Option<Arc<Notify>>) {
        let msg_id = chunk.message_id.clone();
        let notifier = self.notifiers.get(&chunk.recipient_id).cloned();
        self.chunks
            .entry(chunk.recipient_id.clone())
            .or_default()
            .push(chunk);
        (msg_id, notifier)
    }

    fn fetch(&self, recipient_id: &str) -> Vec<EncryptedChunk> {
        self.chunks.get(recipient_id).cloned().unwrap_or_default()
    }

    fn ack(&mut self, recipient_id: &str, message_ids: &[String]) -> (usize, usize) {
        if let Some(chunks) = self.chunks.get_mut(recipient_id) {
            let before = chunks.len();
            let mut freed_bytes = 0usize;
            chunks.retain(|c| {
                if message_ids.contains(&c.message_id) {
                    freed_bytes += c.ciphertext.len();
                    false
                } else {
                    true
                }
            });
            let deleted = before - chunks.len();
            if chunks.is_empty() {
                self.chunks.remove(recipient_id);
            }
            (deleted, freed_bytes)
        } else {
            (0, 0)
        }
    }

    fn cleanup_expired(&mut self, now: u64) -> usize {
        let mut freed_bytes = 0usize;
        for chunks in self.chunks.values_mut() {
            chunks.retain(|c| {
                if c.expires_at < now {
                    freed_bytes += c.ciphertext.len();
                    false
                } else {
                    true
                }
            });
        }
        self.chunks.retain(|_, v| !v.is_empty());
        freed_bytes
    }
}

/// Sharded concurrent chunk store across `STORE_SHARDS` buckets so independent
/// recipients never contend on a single global `RwLock`.
pub struct ShardedChunkStore {
    shards: Box<[RwLock<ChunkShard>]>,
    max_bytes: usize,
    current_bytes: AtomicUsize,
}

impl ShardedChunkStore {
    pub fn new(max_bytes: usize) -> Self {
        let mut shards = Vec::with_capacity(STORE_SHARDS);
        for _ in 0..STORE_SHARDS {
            shards.push(RwLock::new(ChunkShard::default()));
        }
        Self {
            shards: shards.into_boxed_slice(),
            max_bytes,
            current_bytes: AtomicUsize::new(0),
        }
    }

    fn shard_idx(key: &str) -> usize {
        // FNV-1a 64-bit hash for uniform key distribution across shards.
        let mut hash: u64 = 0xcbf29ce484222325;
        for b in key.as_bytes() {
            hash ^= *b as u64;
            hash = hash.wrapping_mul(0x100000001b3);
        }
        (hash as usize) & (STORE_SHARDS - 1)
    }

    fn try_reserve_bytes(&self, len: usize) -> bool {
        let mut current = self.current_bytes.load(Ordering::Relaxed);
        loop {
            let Some(next) = current.checked_add(len) else {
                return false;
            };
            if next > self.max_bytes {
                return false;
            }
            match self.current_bytes.compare_exchange_weak(
                current,
                next,
                Ordering::AcqRel,
                Ordering::Relaxed,
            ) {
                Ok(_) => return true,
                Err(actual) => current = actual,
            }
        }
    }

    fn release_bytes(&self, len: usize) {
        if len == 0 {
            return;
        }
        let mut current = self.current_bytes.load(Ordering::Relaxed);
        loop {
            let next = current.saturating_sub(len);
            match self.current_bytes.compare_exchange_weak(
                current,
                next,
                Ordering::AcqRel,
                Ordering::Relaxed,
            ) {
                Ok(_) => return,
                Err(actual) => current = actual,
            }
        }
    }

    pub async fn store(&self, chunk: EncryptedChunk) -> Result<String, String> {
        let chunk_size = chunk.ciphertext.len();
        if !self.try_reserve_bytes(chunk_size) {
            return Err("Storage full".into());
        }
        let idx = Self::shard_idx(&chunk.recipient_id);
        let (msg_id, notifier) = {
            let mut shard = self.shards[idx].write().await;
            shard.push_chunk(chunk)
        };
        if let Some(n) = notifier {
            n.notify_waiters();
        }
        Ok(msg_id)
    }

    /// Store multiple chunks while acquiring each touched shard's write lock once.
    pub async fn store_batch(
        &self,
        chunks: Vec<EncryptedChunk>,
    ) -> Result<Vec<EncryptedChunk>, String> {
        if chunks.is_empty() {
            return Ok(Vec::new());
        }
        let total_len: usize = chunks.iter().map(|c| c.ciphertext.len()).sum();
        if !self.try_reserve_bytes(total_len) {
            return Err("Storage full".into());
        }

        let mut buckets: HashMap<usize, Vec<EncryptedChunk>> = HashMap::new();
        for chunk in chunks {
            let idx = Self::shard_idx(&chunk.recipient_id);
            buckets.entry(idx).or_default().push(chunk);
        }

        let mut stored = Vec::new();
        let mut notifiers: Vec<Arc<Notify>> = Vec::new();
        for (idx, bucket) in buckets {
            let mut shard = self.shards[idx].write().await;
            for chunk in bucket {
                let (_, notifier) = shard.push_chunk(chunk.clone());
                if let Some(n) = notifier {
                    notifiers.push(n);
                }
                stored.push(chunk);
            }
        }
        for n in notifiers {
            n.notify_waiters();
        }
        Ok(stored)
    }

    pub async fn fetch(&self, recipient_id: &str) -> Vec<EncryptedChunk> {
        let idx = Self::shard_idx(recipient_id);
        let shard = self.shards[idx].read().await;
        shard.fetch(recipient_id)
    }

    pub async fn fetch_or_register_notifier(
        &self,
        recipient_id: &str,
    ) -> Result<Vec<EncryptedChunk>, Arc<Notify>> {
        let idx = Self::shard_idx(recipient_id);
        let shard = self.shards[idx].read().await;
        let chunks = shard.fetch(recipient_id);
        if !chunks.is_empty() {
            return Ok(chunks);
        }
        if let Some(n) = shard.notifiers.get(recipient_id) {
            return Err(n.clone());
        }
        drop(shard);

        let mut shard = self.shards[idx].write().await;
        let chunks = shard.fetch(recipient_id);
        if !chunks.is_empty() {
            return Ok(chunks);
        }
        let notifier = shard
            .notifiers
            .entry(recipient_id.to_string())
            .or_insert_with(|| Arc::new(Notify::new()))
            .clone();
        Err(notifier)
    }

    pub async fn ack(&self, recipient_id: &str, message_ids: &[String]) -> usize {
        let idx = Self::shard_idx(recipient_id);
        let (deleted, freed) = {
            let mut shard = self.shards[idx].write().await;
            shard.ack(recipient_id, message_ids)
        };
        self.release_bytes(freed);
        deleted
    }

    pub async fn register(&self, evm_address: &str) {
        let key = evm_address.to_lowercase();
        let idx = Self::shard_idx(&key);
        let now = chrono::Utc::now().timestamp() as u64;
        let mut shard = self.shards[idx].write().await;
        shard.registered_peers.insert(key, now);
    }

    pub async fn cleanup_expired(&self) {
        let now = chrono::Utc::now().timestamp() as u64;
        let mut freed_total = 0usize;
        for shard in self.shards.iter() {
            let mut s = shard.write().await;
            freed_total += s.cleanup_expired(now);
        }
        self.release_bytes(freed_total);
    }

    pub async fn stats(&self) -> (usize, usize, usize) {
        let mut total_chunks = 0usize;
        let mut registered_peers = 0usize;
        for shard in self.shards.iter() {
            let s = shard.read().await;
            total_chunks += s.chunks.values().map(|v| v.len()).sum::<usize>();
            registered_peers += s.registered_peers.len();
        }
        (
            total_chunks,
            self.current_bytes.load(Ordering::Relaxed),
            registered_peers,
        )
    }

    pub async fn get_chunk(&self, recipient_id: &str, message_id: &str) -> Option<EncryptedChunk> {
        let idx = Self::shard_idx(recipient_id);
        let shard = self.shards[idx].read().await;
        shard
            .chunks
            .get(recipient_id)
            .and_then(|list| list.iter().find(|c| c.message_id == message_id).cloned())
    }

    /// Idempotent store for HyperSwarm replication (same message_id allowed once).
    pub async fn store_replicate(&self, chunk: EncryptedChunk) -> Result<(), String> {
        let idx = Self::shard_idx(&chunk.recipient_id);
        let chunk_size = chunk.ciphertext.len();
        let notifier = {
            let mut shard = self.shards[idx].write().await;
            let already_present = shard
                .chunks
                .get(&chunk.recipient_id)
                .map(|list| list.iter().any(|c| c.message_id == chunk.message_id))
                .unwrap_or(false);
            if already_present {
                return Ok(());
            }
            if !self.try_reserve_bytes(chunk_size) {
                return Err("Storage full".into());
            }
            shard.push_chunk(chunk).1
        };
        if let Some(n) = notifier {
            n.notify_waiters();
        }
        Ok(())
    }
}

pub type SharedStore = Arc<ShardedChunkStore>;

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

#[derive(Clone)]
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
    http_client: reqwest::Client,
    discovery_cache: discovery::DiscoveryCache,
}

#[derive(Clone)]
struct MeshPushContext {
    self_http: Option<String>,
    chain_rpc_url: Option<String>,
    federated_push_key: Option<String>,
    hyperswarm: Option<Arc<hyperswarm::HyperSwarmHandle>>,
    http_client: reqwest::Client,
    discovery_cache: discovery::DiscoveryCache,
}

#[derive(serde::Deserialize)]
#[serde(untagged)]
enum BatchStorePayload {
    Array(Vec<EncryptedChunk>),
    Wrapped { chunks: Vec<EncryptedChunk> },
}

impl MeshPushContext {
    fn from_relay_ctx(ctx: &RelayHttpContext) -> Self {
        Self {
            self_http: ctx.public_relay_url.clone(),
            chain_rpc_url: ctx.chain_rpc_url.clone(),
            federated_push_key: ctx.federated_push_key.clone(),
            hyperswarm: ctx.hyperswarm.clone(),
            http_client: ctx.http_client.clone(),
            discovery_cache: ctx.discovery_cache.clone(),
        }
    }

    fn has_peer_mesh(&self) -> bool {
        self.chain_rpc_url.is_some()
            || self
                .hyperswarm
                .as_ref()
                .map(|h| !h.directory.snapshot().is_empty())
                .unwrap_or(false)
    }
}

async fn handle_request(
    mut req: Request<Incoming>,
    ctx: RelayHttpContext,
    remote_addr: SocketAddr,
) -> Result<Response<Full<Bytes>>, hyper::Error> {
    if let Some(ref hub) = ctx.signal_hub {
        match signal::try_upgrade_ws(req, hub) {
            Ok(resp) => return Ok(resp),
            Err(original_req) => req = original_req,
        }
    }

    // CORS headers for all responses
    let cors_headers = |mut resp: Response<Full<Bytes>>| -> Response<Full<Bytes>> {
        resp.headers_mut()
            .insert("Access-Control-Allow-Origin", "*".parse().unwrap());
        resp.headers_mut().insert(
            "Access-Control-Allow-Methods",
            "GET, POST, OPTIONS".parse().unwrap(),
        );
        resp.headers_mut().insert(
            "Access-Control-Allow-Headers",
            "Content-Type, X-TKS-Federated-Push-Key, X-TKS-Relay-Peer"
                .parse()
                .unwrap(),
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
        "/stats" | "/hybrid/stats" | "/discovery" | "/version" | "/health"
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

    let RelayHttpContext {
        store,
        shard_ingest_url: _,
        hybrid,
        public_relay_url,
        public_signal_ws,
        signal_push_url,
        chain_rpc_url,
        hyperswarm,
        build_info,
        federated_push_key,
        signal_hub,
        http_client,
        discovery_cache,
    } = ctx.clone();
    let mesh_push = MeshPushContext::from_relay_ctx(&ctx);

    // Route by path
    let response = match path.as_str() {
        "/health" => serde_json::to_vec(&serde_json::json!({"status": "ok"})).unwrap_or_default(),
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
                        &http_client,
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
                    let resp = process_relay_request(relay_req, &ctx, true).await;
                    bincode::serialize(&resp).unwrap_or_default()
                }
                Err(e) => {
                    let resp = RelayResponse::Error(format!("Decode error: {}", e));
                    bincode::serialize(&resp).unwrap_or_default()
                }
            }
        }
        "/store" => {
            // JSON endpoint for storing a single chunk (direct JSON response, zero bincode roundtrip).
            match serde_json::from_slice::<EncryptedChunk>(&body_bytes) {
                Ok(chunk) => {
                    let resp = process_relay_request(
                        RelayRequest::Store(chunk.clone()),
                        &ctx,
                        false,
                    )
                    .await;
                    if matches!(resp, RelayResponse::Stored { .. }) {
                        mesh_replicate(
                            chunk,
                            mesh_push,
                        );
                    }
                    serde_json::to_vec(&resp).unwrap_or_default()
                }
                Err(e) => serde_json::to_vec(&serde_json::json!({"error": e.to_string()})).unwrap_or_default(),
            }
        }
        "/store/batch" => {
            // High-throughput batch store endpoint: accepts `[chunk, ...]` or `{"chunks":[...]}`.
            match serde_json::from_slice::<BatchStorePayload>(&body_bytes) {
                Ok(payload) => {
                    let chunks = match payload {
                        BatchStorePayload::Array(c) | BatchStorePayload::Wrapped { chunks: c } => c,
                    };
                    match store.store_batch(chunks).await {
                        Ok(stored_chunks) => {
                            let message_ids: Vec<String> = stored_chunks
                                .iter()
                                .map(|c| c.message_id.clone())
                                .collect();
                            if let Some(ref h) = hybrid {
                                for _ in 0..stored_chunks.len() {
                                    h.record_ingress();
                                }
                            }
                            spawn_batch_side_effects(stored_chunks, &ctx);
                            serde_json::to_vec(&serde_json::json!({
                                "stored": message_ids.len(),
                                "message_ids": message_ids,
                            }))
                            .unwrap_or_default()
                        }
                        Err(e) => serde_json::to_vec(&serde_json::json!({"error": e}))
                            .unwrap_or_default(),
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
                        // Read local shard first — do not block HTTP on libp2p chunk pull.
                        let chunks = store.fetch(&recipient_id).await;
                        if !chunks.is_empty() {
                            debug!(
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
                            let chunks = store.fetch(&recipient_id).await;
                            if !chunks.is_empty() {
                                debug!(
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
                    let resp = process_relay_request(
                        RelayRequest::Ack { recipient_id, message_ids },
                        &ctx,
                        false,
                    )
                    .await;
                    serde_json::to_vec(&resp).unwrap_or_default()
                }
                Err(e) => serde_json::to_vec(&serde_json::json!({"error": e.to_string()})).unwrap_or_default(),
            }
        }
        "/register" => {
            match serde_json::from_slice::<serde_json::Value>(&body_bytes) {
                Ok(val) => {
                    let evm_address = val["evm_address"].as_str().unwrap_or("").to_string();
                    let resp = process_relay_request(
                        RelayRequest::Register { evm_address },
                        &ctx,
                        false,
                    )
                    .await;
                    serde_json::to_vec(&resp).unwrap_or_default()
                }
                Err(e) => serde_json::to_vec(&serde_json::json!({"error": e.to_string()})).unwrap_or_default(),
            }
        }
        "/stats" => {
            let (chunks, bytes, peers) = store.stats().await;
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
            let disc = discovery::build_discovery_cached(
                public_relay_url.clone(),
                public_signal_ws.clone(),
                peer_id,
                chain_rpc_url.clone(),
                swarm_dir,
                hybrid.clone(),
                &http_client,
                &discovery_cache,
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
                "endpoints": ["/relay", "/store", "/store/batch", "/fetch", "/ack", "/register", "/signal/push", "/ws", "/stats", "/hybrid/stats", "/discovery", "/version", "/health"]
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
fn mesh_replicate(chunk: EncryptedChunk, ctx: MeshPushContext) {
    if !ctx.has_peer_mesh() {
        return;
    }
    tokio::spawn(async move {
        let swarm = ctx.hyperswarm.as_ref().map(|h| h.directory.clone());
        let peer_id = swarm.as_ref().map(|s| s.local_peer_id().to_string());
        let disc = discovery::build_discovery_cached(
            ctx.self_http.clone(),
            None,
            peer_id,
            ctx.chain_rpc_url.clone(),
            swarm,
            None,
            &ctx.http_client,
            &ctx.discovery_cache,
        )
        .await;
        let futures = disc.relays.iter().take(8).filter_map(|relay| {
            if relay.http_base.is_empty()
                || Some(relay.http_base.as_str()) == ctx.self_http.as_deref()
            {
                return None;
            }
            let url = format!("{}/store", relay.http_base.trim_end_matches('/'));
            let client = ctx.http_client.clone();
            let payload = chunk.clone();
            Some(async move {
                let _ = client
                    .post(url)
                    .json(&payload)
                    .timeout(Duration::from_secs(2))
                    .send()
                    .await;
            })
        });
        join_all(futures).await;
    });
}

/// After local store, ask peer relays to run their co-located signal push (Phase 2).
fn mesh_signal_push(
    recipient_id: String,
    message_id: String,
    ciphertext: Vec<u8>,
    ctx: MeshPushContext,
) {
    if !ctx.has_peer_mesh() {
        return;
    }
    tokio::spawn(async move {
        let swarm = ctx.hyperswarm.as_ref().map(|h| h.directory.clone());
        let peer_id = swarm.as_ref().map(|s| s.local_peer_id().to_string());
        let disc = discovery::build_discovery_cached(
            ctx.self_http.clone(),
            None,
            peer_id.clone(),
            ctx.chain_rpc_url.clone(),
            swarm,
            None,
            &ctx.http_client,
            &ctx.discovery_cache,
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
        let futures = disc.relays.iter().take(8).filter_map(|relay| {
            if Some(relay.http_base.as_str()) == ctx.self_http.as_deref() {
                return None;
            }
            let push_url = relay
                .federated_push
                .clone()
                .or_else(|| discovery::federated_signal_push_url(&relay.http_base))?;
            let client = ctx.http_client.clone();
            let body = body.clone();
            let fed_key = ctx.federated_push_key.clone();
            let peer_hdr = peer_id.clone();
            Some(async move {
                let mut req = client
                    .post(push_url)
                    .json(&body)
                    .timeout(Duration::from_secs(2));
                if let Some(ref key) = fed_key {
                    req = req.header("X-TKS-Federated-Push-Key", key.as_str());
                }
                if let Some(ref pid) = peer_hdr {
                    req = req.header("X-TKS-Relay-Peer", pid.as_str());
                }
                let _ = req.send().await;
            })
        });
        join_all(futures).await;
    });
}

fn spawn_batch_side_effects(chunks: Vec<EncryptedChunk>, ctx: &RelayHttpContext) {
    let mesh_ctx = MeshPushContext::from_relay_ctx(ctx);
    let need_side_effects = ctx.shard_ingest_url.is_some()
        || ctx.signal_hub.is_some()
        || ctx.signal_push_url.is_some()
        || ctx.hyperswarm.is_some()
        || mesh_ctx.has_peer_mesh();
    if !need_side_effects || chunks.is_empty() {
        return;
    }

    let ctx = ctx.clone();
    tokio::spawn(async move {
        if let Some(ref url) = ctx.shard_ingest_url {
            forward_shard_ingest_batch(&ctx.http_client, url, &chunks).await;
        }
        for chunk in chunks {
            deliver_signal_push(
                ctx.signal_hub.as_deref(),
                ctx.signal_push_url.as_deref(),
                &ctx.http_client,
                &chunk.recipient_id,
                &chunk.message_id,
                &chunk.ciphertext,
            )
            .await;
            if let Some(ref hs) = ctx.hyperswarm {
                let ann =
                    hyperswarm::chunk_avail_from(&chunk, &hs.directory.local_peer_id().to_string());
                hs.announce_chunk(ann);
            }
            if mesh_ctx.has_peer_mesh() {
                mesh_signal_push(
                    chunk.recipient_id.clone(),
                    chunk.message_id.clone(),
                    chunk.ciphertext.clone(),
                    mesh_ctx.clone(),
                );
                mesh_replicate(chunk, mesh_ctx.clone());
            }
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
        let ct: Option<Vec<u8>> = arr.iter().map(|v| v.as_u64().map(|n| n as u8)).collect();
        return ct
            .map(|c| (recipient, message_id, c))
            .ok_or_else(|| "invalid ciphertext array".into());
    }
    Err("missing data (base64) or ciphertext".into())
}

async fn deliver_signal_push(
    hub: Option<&signal::SignalHub>,
    push_url: Option<&str>,
    http_client: &reqwest::Client,
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
        forward_signal_push(http_client, url, recipient_id, message_id, &data).await;
    }
}

/// Notify co-located signal server so online recipients get sub-100ms WS push.
async fn forward_signal_push(
    http_client: &reqwest::Client,
    url: &str,
    recipient_id: &str,
    message_id: &str,
    data_b64: &str,
) {
    let body = serde_json::json!({
        "to": recipient_id,
        "type": "message",
        "message_id": message_id,
        "data": data_b64,
    });
    match http_client
        .post(url)
        .json(&body)
        .timeout(Duration::from_secs(2))
        .send()
        .await
    {
        Ok(resp) if resp.status().is_success() => {
            log::debug!("⚡ WS push queued for {}", recipient_id);
        }
        Ok(resp) => {
            log::warn!("Signal push HTTP {} for msg {}", resp.status(), message_id);
        }
        Err(e) => log::debug!("Signal push skipped ({e})"),
    }
}

fn shard_ingest_payload(chunk: &EncryptedChunk, msg_id: &str) -> serde_json::Value {
    use sha2::{Digest, Sha256};
    let hash: [u8; 32] = Sha256::digest(&chunk.ciphertext).into();
    serde_json::json!({
        "msg_id": msg_id,
        "sender_id": "cipher-relay",
        "recipient_id": chunk.recipient_id,
        "ciphertext_hash": hash,
        "stored_at": chunk.stored_at,
    })
}

async fn forward_shard_ingest(
    http_client: &reqwest::Client,
    url: &str,
    chunk: &EncryptedChunk,
    msg_id: &str,
) {
    let body = shard_ingest_payload(chunk, msg_id);
    match http_client.post(url).json(&body).send().await {
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

async fn forward_shard_ingest_batch(
    http_client: &reqwest::Client,
    url: &str,
    chunks: &[EncryptedChunk],
) {
    if chunks.is_empty() {
        return;
    }
    let batch_url = if url.ends_with("/ingest") {
        format!("{url}/batch")
    } else {
        url.to_string()
    };

    if batch_url != url {
        let items: Vec<serde_json::Value> = chunks
            .iter()
            .map(|c| shard_ingest_payload(c, &c.message_id))
            .collect();
        if let Ok(resp) = http_client.post(&batch_url).json(&items).send().await {
            if resp.status().is_success() {
                return;
            }
        }
    }

    // Legacy single-message `/ingest` fallback if shard host doesn't expose `/ingest/batch`.
    for c in chunks {
        forward_shard_ingest(http_client, url, c, &c.message_id).await;
    }
}

async fn process_relay_request(
    req: RelayRequest,
    ctx: &RelayHttpContext,
    mesh_push_on_store: bool,
) -> RelayResponse {
    match req {
        RelayRequest::Store(chunk) => {
            let recipient = chunk.recipient_id.clone();
            match ctx.store.store(chunk.clone()).await {
                Ok(msg_id) => {
                    if let Some(ref h) = ctx.hybrid {
                        h.record_ingress();
                    }
                    debug!("📦 Stored message {} for {}", msg_id, recipient);
                    if let Some(ref url) = ctx.shard_ingest_url {
                        let url = url.clone();
                        let client = ctx.http_client.clone();
                        let mid = msg_id.clone();
                        let c = chunk.clone();
                        tokio::spawn(async move {
                            forward_shard_ingest(&client, &url, &c, &mid).await;
                        });
                    }
                    if ctx.signal_hub.is_some() || ctx.signal_push_url.is_some() {
                        let hub = ctx.signal_hub.clone();
                        let push_url = ctx.signal_push_url.clone();
                        let client = ctx.http_client.clone();
                        let mid = msg_id.clone();
                        let rid = recipient.clone();
                        let ct = chunk.ciphertext.clone();
                        tokio::spawn(async move {
                            deliver_signal_push(
                                hub.as_deref(),
                                push_url.as_deref(),
                                &client,
                                &rid,
                                &mid,
                                &ct,
                            )
                            .await;
                        });
                    }
                    if mesh_push_on_store {
                        mesh_signal_push(
                            recipient.clone(),
                            msg_id.clone(),
                            chunk.ciphertext.clone(),
                            MeshPushContext::from_relay_ctx(ctx),
                        );
                    }
                    if let Some(ref hs) = ctx.hyperswarm {
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
            if let Some(ref hs) = ctx.hyperswarm {
                let pulled = hs.pull_for_recipient(&recipient_id).await;
                if pulled > 0 {
                    debug!("🕸️ HyperSwarm pulled {pulled} chunk(s) for {recipient_id}");
                }
            }
            match ctx.store.fetch_or_register_notifier(&recipient_id).await {
                Ok(chunks) => {
                    debug!(
                        "📬 Fetch (Instant): {} chunks for {}",
                        chunks.len(),
                        recipient_id
                    );
                    RelayResponse::Chunks(chunks)
                }
                Err(notifier) => {
                    let _ =
                        tokio::time::timeout(Duration::from_secs(25), notifier.notified()).await;

                    if let Some(ref hs) = ctx.hyperswarm {
                        let _ = hs.pull_for_recipient(&recipient_id).await;
                    }

                    let chunks = ctx.store.fetch(&recipient_id).await;
                    if chunks.is_empty() {
                        debug!("💤 Fetch (Timeout): 0 chunks for {}", recipient_id);
                    } else {
                        debug!(
                            "📬 Fetch (Woken): {} chunks for {}",
                            chunks.len(),
                            recipient_id
                        );
                    }
                    RelayResponse::Chunks(chunks)
                }
            }
        }
        RelayRequest::Ack {
            recipient_id,
            message_ids,
        } => {
            let deleted = ctx.store.ack(&recipient_id, &message_ids).await;
            debug!("✓ Acked {} chunks for {}", deleted, recipient_id);
            RelayResponse::Acked { deleted }
        }
        RelayRequest::Register { evm_address } => {
            ctx.store.register(&evm_address).await;
            debug!("📋 Registered peer: {}", evm_address);
            RelayResponse::Registered
        }
        RelayRequest::RpcProxy { endpoint, body } => {
            debug!("🔄 Proxying RPC to {} ({} bytes)", endpoint, body.len());
            match ctx
                .http_client
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
    }
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
    let public_signal_ws = config.public_signal_ws.clone().or_else(|| {
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
    let federated_push_key = config.federated_push_key.clone().or_else(|| {
        std::env::var("CIPHER_FEDERATED_PUSH_KEY")
            .ok()
            .filter(|s| !s.is_empty())
    });

    info!("🚀 Cipher Storage Swarm Relay v0.1.5 (HyperSwarm chunk pull)");
    info!("📋 Blueprint §3.5: Encrypted chunk storage with TTL auto-delete");

    let store: SharedStore = Arc::new(ShardedChunkStore::new(100 * 1024 * 1024 * 1024));
    let http_client = build_shared_http_client();
    let discovery_cache = discovery::DiscoveryCache::new(Duration::from_secs(15));

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
            tokio::time::sleep(Duration::from_secs(300)).await;
            store_cleanup.cleanup_expired().await;
            let (chunks, bytes, peers) = store_cleanup.stats().await;
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
    info!("   Endpoints: /relay /store /store/batch /fetch /ack /register /signal/push /ws /stats /discovery");
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
        info!("   Embedded signal WS: port {signal_port} & relay /ws (/internal/push)");
    } else if let Some(ref push) = signal_push_url {
        info!("   WS instant push (external): {push}");
    }
    if let Some(ref ws) = public_signal_ws {
        info!("   Advertised signal WS: {ws}");
    }
    if federated_push_key.is_some() {
        info!("   Federated signal push: enabled (POST /signal/push + mesh fanout)");
    }

    let ctx = RelayHttpContext {
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
        http_client,
        discovery_cache,
    };

    serve_relay_http_listener(listener, ctx).await;
    Ok(())
}

fn build_shared_http_client() -> reqwest::Client {
    reqwest::Client::builder()
        .pool_max_idle_per_host(64)
        .pool_idle_timeout(Duration::from_secs(60))
        .tcp_nodelay(true)
        .connect_timeout(Duration::from_secs(2))
        .timeout(Duration::from_secs(5))
        .build()
        .unwrap_or_else(|_| reqwest::Client::new())
}

async fn serve_relay_http_listener(listener: tokio::net::TcpListener, ctx: RelayHttpContext) {
    loop {
        let Ok((stream, remote_addr)) = listener.accept().await else {
            continue;
        };
        let ctx = ctx.clone();
        debug!("Connection from {}", remote_addr);

        tokio::spawn(async move {
            let io = TokioIo::new(stream);
            let service = service_fn(move |req| {
                let ctx = ctx.clone();
                let peer = remote_addr;
                async move { handle_request(req, ctx, peer).await }
            });

            if let Err(e) = hyper::server::conn::http1::Builder::new()
                .serve_connection(io, service)
                .with_upgrades()
                .await
            {
                warn!("HTTP error: {}", e);
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_chunk(recipient: &str, msg_id: &str, size: usize, expires_at: u64) -> EncryptedChunk {
        EncryptedChunk {
            recipient_id: recipient.to_string(),
            message_id: msg_id.to_string(),
            ciphertext: vec![0xAB; size],
            stored_at: 100,
            expires_at,
        }
    }

    #[tokio::test]
    async fn sharded_store_concurrent_and_batch_ops() {
        let store = Arc::new(ShardedChunkStore::new(4096));

        let mut handles = Vec::new();
        for i in 0..16 {
            let s = store.clone();
            handles.push(tokio::spawn(async move {
                let rec = format!("0xrecipient_{i}");
                let id = format!("msg_{i}");
                s.store(test_chunk(&rec, &id, 64, u64::MAX)).await.unwrap();
                s.register(&rec).await;
            }));
        }
        for h in handles {
            h.await.unwrap();
        }

        let batch = vec![
            test_chunk("0xbatch_a", "b1", 128, u64::MAX),
            test_chunk("0xbatch_b", "b2", 128, u64::MAX),
            test_chunk("0xbatch_a", "b3", 128, 1), // expired timestamp (1 < now)
        ];
        let stored = store.store_batch(batch).await.unwrap();
        assert_eq!(stored.len(), 3);

        let (chunks, bytes, peers) = store.stats().await;
        assert_eq!(chunks, 19);
        assert_eq!(bytes, 16 * 64 + 3 * 128);
        assert_eq!(peers, 16);

        let deleted = store.ack("0xbatch_a", &["b1".to_string()]).await;
        assert_eq!(deleted, 1);

        store.cleanup_expired().await;
        let (chunks_after, _, _) = store.stats().await;
        assert_eq!(chunks_after, 17);

        // Over-capacity batch rejects cleanly without corrupting current_bytes.
        let small = ShardedChunkStore::new(100);
        let err = small
            .store_batch(vec![
                test_chunk("a", "1", 60, u64::MAX),
                test_chunk("b", "2", 60, u64::MAX),
            ])
            .await
            .unwrap_err();
        assert_eq!(err, "Storage full");
        assert_eq!(small.stats().await.1, 0);
    }

    #[tokio::test]
    async fn live_ws_upgrade_survives_101_and_delivers_push_and_batch() {
        let signal_hub = signal::SignalHub::new();
        let signal_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let signal_addr = signal_listener.local_addr().unwrap();
        let hub_bg = signal_hub.clone();
        let sig_task = tokio::spawn(async move {
            signal::serve_signal_listener(signal_listener, hub_bg).await;
        });

        let relay_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let relay_addr = relay_listener.local_addr().unwrap();
        let http_client = build_shared_http_client();
        let ctx = RelayHttpContext {
            store: Arc::new(ShardedChunkStore::new(1024 * 1024)),
            shard_ingest_url: None,
            hybrid: Some(Arc::new(HybridController::new(Default::default()))),
            public_relay_url: Some(format!("http://{relay_addr}")),
            public_signal_ws: Some(format!("ws://{signal_addr}/ws")),
            signal_push_url: Some(format!("http://{signal_addr}/internal/push")),
            chain_rpc_url: None,
            hyperswarm: None,
            build_info: None,
            federated_push_key: None,
            signal_hub: Some(signal_hub),
            http_client: http_client.clone(),
            discovery_cache: discovery::DiscoveryCache::new(Duration::from_secs(15)),
        };
        let relay_task = tokio::spawn(async move {
            serve_relay_http_listener(relay_listener, ctx).await;
        });

        // 1) Connect real WebSocket client on :signal_port/ws?addr=0xalice
        let alice_ws_url = format!("ws://{signal_addr}/ws?addr=0xAlice");
        let bob_ws_url = format!("ws://{relay_addr}/ws?addr=0xBob");
        let (mut ws_alice, resp_alice) = tokio::task::spawn_blocking(move || {
            hyper_tungstenite::tungstenite::connect(alice_ws_url).unwrap()
        })
        .await
        .unwrap();
        assert_eq!(resp_alice.status().as_u16(), 101);

        // 2) Connect real WebSocket client directly on :relay_port/ws?addr=0xBob
        let (mut ws_bob, resp_bob) = tokio::task::spawn_blocking(move || {
            hyper_tungstenite::tungstenite::connect(bob_ws_url).unwrap()
        })
        .await
        .unwrap();
        assert_eq!(resp_bob.status().as_u16(), 101);

        // 3) POST /store to relay -> should store AND push instant WS frame to 0xalice.
        let chunk = test_chunk("0xalice", "push_msg_1", 12, u64::MAX);
        let store_res: serde_json::Value = http_client
            .post(format!("http://{relay_addr}/store"))
            .json(&chunk)
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        assert_eq!(store_res["Stored"]["message_id"], "push_msg_1");

        let pushed_msg = tokio::task::spawn_blocking(move || {
            let msg = ws_alice
                .read()
                .expect("WS must stay open after 101 and receive push");
            (ws_alice, msg)
        })
        .await
        .unwrap();
        let mut ws_alice = pushed_msg.0;
        let text = pushed_msg.1.into_text().unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&text).unwrap();
        assert_eq!(parsed["type"], "message");
        assert_eq!(parsed["message_id"], "push_msg_1");

        // 4) Peer-to-peer WS relay from alice -> bob over open WS connections.
        let (mut ws_alice, ack_text) = tokio::task::spawn_blocking(move || {
            ws_alice
                .send(hyper_tungstenite::tungstenite::Message::Text(
                    serde_json::json!({
                        "to": "0xbob",
                        "payload": {"hello": "world"}
                    })
                    .to_string(),
                ))
                .unwrap();
            let ack = ws_alice.read().unwrap().into_text().unwrap();
            (ws_alice, ack)
        })
        .await
        .unwrap();
        let ack_json: serde_json::Value = serde_json::from_str(&ack_text).unwrap();
        assert_eq!(ack_json["ack"], true);
        assert_eq!(ack_json["delivered"], true);

        let bob_frame = tokio::task::spawn_blocking(move || {
            let frame = ws_bob.read().unwrap().into_text().unwrap();
            let _ = ws_bob.close(None);
            frame
        })
        .await
        .unwrap();
        let bob_json: serde_json::Value = serde_json::from_str(&bob_frame).unwrap();
        assert_eq!(bob_json["from"], "0xalice");
        assert_eq!(bob_json["payload"]["hello"], "world");
        let _ = tokio::task::spawn_blocking(move || ws_alice.close(None)).await;

        // 5) POST /store/batch + /fetch + /ack + /stats
        let batch_resp: serde_json::Value = http_client
            .post(format!("http://{relay_addr}/store/batch"))
            .json(&vec![
                test_chunk("0xcarol", "batch_1", 10, u64::MAX),
                test_chunk("0xcarol", "batch_2", 14, u64::MAX),
            ])
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        assert_eq!(batch_resp["stored"], 2);

        let fetched: serde_json::Value = http_client
            .post(format!("http://{relay_addr}/fetch"))
            .json(&serde_json::json!({"recipient_id": "0xcarol"}))
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        assert_eq!(fetched["Chunks"].as_array().unwrap().len(), 2);

        sig_task.abort();
        relay_task.abort();
    }
}
