//! Merge on-chain validator endpoints with live libp2p gossip peers.

use std::sync::Arc;

use cipher_hybrid::HybridController;
use serde::{Deserialize, Serialize};

use crate::hyperswarm::SwarmDirectory;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RelayEndpoint {
    pub source: String,
    pub peer_id: Option<String>,
    pub http_base: String,
    pub libp2p_addrs: Vec<String>,
    pub region: Option<String>,
    pub priority: u8,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DiscoveryResponse {
    pub self_http: Option<String>,
    pub self_peer_id: Option<String>,
    pub relays: Vec<RelayEndpoint>,
    pub hybrid: Option<serde_json::Value>,
    pub chain_rpc: Option<String>,
}

pub async fn build_discovery(
    public_http: Option<String>,
    self_peer_id: Option<String>,
    chain_rpc_url: Option<String>,
    swarm: Option<Arc<SwarmDirectory>>,
    hybrid: Option<Arc<HybridController>>,
) -> DiscoveryResponse {
    let mut relays = Vec::new();

    if let Some(http) = public_http.clone() {
        relays.push(RelayEndpoint {
            source: "self".into(),
            peer_id: self_peer_id.clone(),
            http_base: http,
            libp2p_addrs: vec![],
            region: None,
            priority: 255,
        });
    }

    if let Some(url) = chain_rpc_url.as_ref() {
        if let Ok(chain) = fetch_chain_validators(url).await {
            for v in chain {
                let http = multiaddr_to_http_hint(&v.endpoint);
                relays.push(RelayEndpoint {
                    source: "chain".into(),
                    peer_id: None,
                    http_base: http.clone(),
                    libp2p_addrs: vec![v.endpoint.clone()],
                    region: Some(v.region),
                    priority: 200,
                });
            }
        }
    }

    if let Some(dir) = swarm {
        for ann in dir.snapshot() {
            relays.push(RelayEndpoint {
                source: "gossip".into(),
                peer_id: Some(ann.peer_id),
                http_base: ann.http_base,
                libp2p_addrs: ann.libp2p_addrs,
                region: ann.region,
                priority: 180,
            });
        }
    }

    relays.sort_by(|a, b| b.priority.cmp(&a.priority));
    relays.dedup_by(|a, b| a.http_base == b.http_base && a.http_base != "");

    DiscoveryResponse {
        self_http: public_http,
        self_peer_id,
        relays,
        hybrid: hybrid.as_ref().map(|h| serde_json::to_value(h.snapshot()).unwrap_or_default()),
        chain_rpc: chain_rpc_url,
    }
}

#[derive(Deserialize)]
struct RpcEnvelope<T> {
    result: Option<T>,
}

#[derive(Deserialize)]
struct ChainValidator {
    account_id: String,
    region: String,
    endpoint: String,
    active: bool,
}

async fn fetch_chain_validators(rpc_url: &str) -> Result<Vec<ChainValidator>, String> {
    let body = serde_json::json!({
        "jsonrpc": "2.0",
        "method": "tks_listSwarmValidators",
        "params": [],
        "id": 1
    });
    let client = reqwest::Client::new();
    let resp = client
        .post(rpc_url)
        .json(&body)
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
