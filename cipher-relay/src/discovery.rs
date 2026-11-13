//! Merge on-chain validator endpoints with live libp2p gossip peers.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use cipher_hybrid::HybridController;
use serde::{Deserialize, Serialize};
use tokio::sync::{Mutex, RwLock};

use crate::hyperswarm::SwarmDirectory;

#[derive(Clone, Debug)]
struct CachedValidators {
    fetched_at: Instant,
    validators: Vec<ChainValidator>,
}

/// TTL cache for on-chain `tks_listSwarmValidators` RPC lookups so high-RPS `/store`
/// and `/discovery` paths never stampede the Substrate RPC or create `reqwest::Client::new()`.
#[derive(Clone)]
pub struct DiscoveryCache {
    ttl: Duration,
    entries: Arc<RwLock<HashMap<String, CachedValidators>>>,
    refresh_lock: Arc<Mutex<()>>,
}

impl DiscoveryCache {
    pub fn new(ttl: Duration) -> Self {
        Self {
            ttl,
            entries: Arc::new(RwLock::new(HashMap::new())),
            refresh_lock: Arc::new(Mutex::new(())),
        }
    }

    async fn get_validators(
        &self,
        rpc_url: &str,
        client: &reqwest::Client,
    ) -> Result<Vec<ChainValidator>, String> {
        {
            let guard = self.entries.read().await;
            if let Some(entry) = guard.get(rpc_url) {
                if entry.fetched_at.elapsed() < self.ttl {
                    return Ok(entry.validators.clone());
                }
            }
        }

        let _refresh_guard = self.refresh_lock.lock().await;
        // Re-check after acquiring refresh lock in case another task just refreshed.
        {
            let guard = self.entries.read().await;
            if let Some(entry) = guard.get(rpc_url) {
                if entry.fetched_at.elapsed() < self.ttl {
                    return Ok(entry.validators.clone());
                }
            }
        }

        match fetch_chain_validators(rpc_url, client).await {
            Ok(validators) => {
                let mut guard = self.entries.write().await;
                guard.insert(
                    rpc_url.to_string(),
                    CachedValidators {
                        fetched_at: Instant::now(),
                        validators: validators.clone(),
                    },
                );
                Ok(validators)
            }
            Err(e) => {
                // Serve stale entries on transient RPC error instead of dropping relay mesh visibility.
                let guard = self.entries.read().await;
                if let Some(entry) = guard.get(rpc_url) {
                    Ok(entry.validators.clone())
                } else {
                    Err(e)
                }
            }
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RelayEndpoint {
    pub source: String,
    pub peer_id: Option<String>,
    pub http_base: String,
    /// WebSocket push endpoint for online recipients (`wss://host/ws?addr=…`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub signal_ws: Option<String>,
    /// Federated instant push ingress (`POST …/signal/push`) on the relay host.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub federated_push: Option<String>,
    pub libp2p_addrs: Vec<String>,
    pub region: Option<String>,
    pub priority: u8,
    /// `true` only for cipher-relay mailbox APIs (`/store`, `/fetch`). Shard ingest hosts are `false`.
    #[serde(default = "default_message_relay")]
    pub message_relay: bool,
}

fn default_message_relay() -> bool {
    true
}

/// Shard executor / ingest vhosts are not store-and-forward mailboxes.
pub fn endpoint_is_mailbox_http(http: &str) -> bool {
    let h = http.to_lowercase();
    if h.contains("shard.") || h.contains(":4010") {
        return false;
    }
    true
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DiscoveryResponse {
    pub self_http: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub self_signal_ws: Option<String>,
    pub self_peer_id: Option<String>,
    pub relays: Vec<RelayEndpoint>,
    pub hybrid: Option<serde_json::Value>,
    pub chain_rpc: Option<String>,
}

/// Federated HTTP push ingress on a relay host (Phase 2 signal mesh).
pub fn federated_signal_push_url(http: &str) -> Option<String> {
    let base = http.trim().trim_end_matches('/');
    if base.is_empty() || !endpoint_is_mailbox_http(base) {
        return None;
    }
    Some(format!("{base}/signal/push"))
}

pub fn http_base_to_signal_ws(http: &str) -> Option<String> {
    let http = http.trim().trim_end_matches('/');
    if http.is_empty() {
        return None;
    }
    if let Some(rest) = http.strip_prefix("https://") {
        return Some(format!("wss://{rest}/ws"));
    }
    if let Some(rest) = http.strip_prefix("http://") {
        return Some(format!("ws://{rest}/ws"));
    }
    None
}

pub async fn build_discovery(
    public_http: Option<String>,
    public_signal_ws: Option<String>,
    self_peer_id: Option<String>,
    chain_rpc_url: Option<String>,
    swarm: Option<Arc<SwarmDirectory>>,
    hybrid: Option<Arc<HybridController>>,
) -> DiscoveryResponse {
    static FALLBACK_CLIENT: std::sync::OnceLock<reqwest::Client> = std::sync::OnceLock::new();
    let client = FALLBACK_CLIENT.get_or_init(reqwest::Client::new);
    build_discovery_inner(
        public_http,
        public_signal_ws,
        self_peer_id,
        chain_rpc_url,
        swarm,
        hybrid,
        client,
        None,
    )
    .await
}

pub async fn build_discovery_cached(
    public_http: Option<String>,
    public_signal_ws: Option<String>,
    self_peer_id: Option<String>,
    chain_rpc_url: Option<String>,
    swarm: Option<Arc<SwarmDirectory>>,
    hybrid: Option<Arc<HybridController>>,
    http_client: &reqwest::Client,
    cache: &DiscoveryCache,
) -> DiscoveryResponse {
    build_discovery_inner(
        public_http,
        public_signal_ws,
        self_peer_id,
        chain_rpc_url,
        swarm,
        hybrid,
        http_client,
        Some(cache),
    )
    .await
}

#[allow(clippy::too_many_arguments)]
async fn build_discovery_inner(
    public_http: Option<String>,
    public_signal_ws: Option<String>,
    self_peer_id: Option<String>,
    chain_rpc_url: Option<String>,
    swarm: Option<Arc<SwarmDirectory>>,
    hybrid: Option<Arc<HybridController>>,
    http_client: &reqwest::Client,
    cache: Option<&DiscoveryCache>,
) -> DiscoveryResponse {
    let mut relays = Vec::new();

    if let Some(http) = public_http.clone() {
        let signal_ws = public_signal_ws
            .clone()
            .or_else(|| http_base_to_signal_ws(&http));
        relays.push(RelayEndpoint {
            source: "self".into(),
            peer_id: self_peer_id.clone(),
            http_base: http.clone(),
            signal_ws,
            federated_push: federated_signal_push_url(&http),
            libp2p_addrs: vec![],
            region: None,
            priority: 255,
            message_relay: true,
        });
    }

    if let Some(url) = chain_rpc_url.as_ref() {
        let validators_res = match cache {
            Some(c) => c.get_validators(url, http_client).await,
            None => fetch_chain_validators(url, http_client).await,
        };
        if let Ok(chain) = validators_res {
            for v in chain {
                let http = multiaddr_to_http_hint(&v.endpoint);
                relays.push(RelayEndpoint {
                    source: "chain".into(),
                    peer_id: None,
                    http_base: http.clone(),
                    signal_ws: http_base_to_signal_ws(&http),
                    federated_push: federated_signal_push_url(&http),
                    libp2p_addrs: vec![v.endpoint.clone()],
                    region: Some(v.region),
                    priority: 200,
                    message_relay: endpoint_is_mailbox_http(&http),
                });
            }
        }
    }

    if let Some(dir) = swarm {
        for ann in dir.snapshot() {
            relays.push(RelayEndpoint {
                source: "gossip".into(),
                peer_id: Some(ann.peer_id),
                http_base: ann.http_base.clone(),
                signal_ws: ann
                    .signal_ws
                    .clone()
                    .or_else(|| http_base_to_signal_ws(&ann.http_base)),
                federated_push: ann
                    .federated_push
                    .clone()
                    .or_else(|| federated_signal_push_url(&ann.http_base)),
                libp2p_addrs: ann.libp2p_addrs,
                region: ann.region,
                priority: 180,
                message_relay: endpoint_is_mailbox_http(&ann.http_base),
            });
        }
    }

    relays.sort_by(|a, b| b.priority.cmp(&a.priority));
    relays.dedup_by(|a, b| a.http_base == b.http_base && a.http_base != "");

    let self_signal_ws =
        public_signal_ws.or_else(|| public_http.as_ref().and_then(|h| http_base_to_signal_ws(h)));

    DiscoveryResponse {
        self_http: public_http,
        self_signal_ws,
        self_peer_id,
        relays,
        hybrid: hybrid
            .as_ref()
            .map(|h| serde_json::to_value(h.snapshot()).unwrap_or_default()),
        chain_rpc: chain_rpc_url,
    }
}

#[derive(Deserialize)]
struct RpcEnvelope<T> {
    result: Option<T>,
}

#[derive(Clone, Debug, Deserialize)]
struct ChainValidator {
    #[allow(dead_code)]
    account_id: String,
    region: String,
    endpoint: String,
    active: bool,
}

async fn fetch_chain_validators(
    rpc_url: &str,
    client: &reqwest::Client,
) -> Result<Vec<ChainValidator>, String> {
    let body = serde_json::json!({
        "jsonrpc": "2.0",
        "method": "tks_listSwarmValidators",
        "params": [],
        "id": 1
    });
    let resp = client
        .post(rpc_url)
        .json(&body)
        .timeout(Duration::from_secs(3))
        .send()
        .await
        .map_err(|e| e.to_string())?;
    let parsed: RpcEnvelope<Vec<ChainValidator>> = resp.json().await.map_err(|e| e.to_string())?;
    Ok(parsed
        .result
        .unwrap_or_default()
        .into_iter()
        .filter(|v| v.active && !v.endpoint.is_empty())
        .collect())
}

/// Map common libp2p multiaddrs to HTTP relay hints for mobile clients.
fn multiaddr_to_http_hint(endpoint: &str) -> String {
    if endpoint.starts_with("http://") || endpoint.starts_with("https://") {
        return endpoint.trim_end_matches('/').to_string();
    }
    // /ip4/1.2.3.4/tcp/4002 → http://1.2.3.4:4002
    let parts: Vec<&str> = endpoint.split('/').filter(|s| !s.is_empty()).collect();
    let mut ip = None;
    let mut port = "4002";
    let mut i = 0;
    while i < parts.len() {
        match parts[i] {
            "ip4" | "ip6" if i + 1 < parts.len() => {
                ip = Some(parts[i + 1]);
                i += 2;
            }
            "tcp" if i + 1 < parts.len() => {
                port = parts[i + 1];
                i += 2;
            }
            _ => i += 1,
        }
    }
    if let Some(host) = ip {
        format!("http://{host}:{port}")
    } else {
        String::new()
    }
}
