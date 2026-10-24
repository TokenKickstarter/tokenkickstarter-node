//! HyperSwarm: libp2p gossip + torrent-style **pull** of ciphertext chunks.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::Duration;

use libp2p::futures::StreamExt;
use libp2p::gossipsub::{self, IdentTopic, MessageAuthenticity, ValidationMode};
use libp2p::identity::Keypair;
use libp2p::request_response::{self, json, ProtocolSupport};
use libp2p::swarm::{NetworkBehaviour, SwarmEvent};
use libp2p::{identify, ping, Multiaddr, PeerId, StreamProtocol, SwarmBuilder};
use log::{debug, info};
use serde::{Deserialize, Serialize};
use tokio::sync::{mpsc, oneshot, RwLock};

use crate::{EncryptedChunk, SharedStore};

pub const GOSSIP_RELAY_TOPIC: &str = "tks-hyperswarm-relay-v1";
pub const GOSSIP_CHUNK_TOPIC: &str = "tks-hyperswarm-chunk-v1";

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RelayAnnouncement {
    pub peer_id: String,
    pub http_base: String,
    pub libp2p_addrs: Vec<String>,
    pub region: Option<String>,
    pub ts: u64,
}

/// Gossip only metadata — **no plaintext, no keys** (torrent magnet-style).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ChunkAvailAnnouncement {
    pub message_id: String,
    pub recipient_id: String,
    pub ciphertext_sha256: String,
    pub size: u32,
    pub holder: String,
    pub ts: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ChunkRequest {
    pub recipient_id: String,
    pub message_id: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ChunkResponse {
    pub chunk: Option<EncryptedChunk>,
}

#[derive(NetworkBehaviour)]
struct HyperSwarmBehaviour {
    gossipsub: gossipsub::Behaviour,
    identify: identify::Behaviour,
    ping: ping::Behaviour,
    chunk: json::Behaviour<ChunkRequest, ChunkResponse>,
}

type AvailKey = (String, String);

#[derive(Default)]
struct AvailIndex {
    by_key: HashMap<AvailKey, ChunkAvailAnnouncement>,
}

#[derive(Clone)]
pub struct SwarmDirectory {
    inner: Arc<RwLock<DirectoryInner>>,
    local_peer_id: PeerId,
}

#[derive(Default)]
struct DirectoryInner {
    relays: HashMap<String, RelayAnnouncement>,
}

impl SwarmDirectory {
    pub fn local_peer_id(&self) -> PeerId {
        self.local_peer_id
    }

    pub fn snapshot(&self) -> Vec<RelayAnnouncement> {
        if let Ok(guard) = self.inner.try_read() {
            guard.relays.values().cloned().collect()
        } else {
            vec![]
        }
    }
}

enum SwarmCommand {
    AnnounceChunk(ChunkAvailAnnouncement),
    PullRecipient {
        recipient_id: String,
        reply: oneshot::Sender<u32>,
    },
}

pub struct HyperSwarmHandle {
    pub directory: Arc<SwarmDirectory>,
    cmd_tx: mpsc::UnboundedSender<SwarmCommand>,
}

impl HyperSwarmHandle {
    pub fn announce_chunk(&self, ann: ChunkAvailAnnouncement) {
        let _ = self.cmd_tx.send(SwarmCommand::AnnounceChunk(ann));
    }

    pub async fn pull_for_recipient(&self, recipient_id: &str) -> u32 {
        let (tx, rx) = oneshot::channel();
        if self
            .cmd_tx
            .send(SwarmCommand::PullRecipient {
                recipient_id: recipient_id.to_string(),
                reply: tx,
            })
            .is_err()
        {
            return 0;
        }
        rx.await.unwrap_or(0)
    }
}

pub struct HyperSwarmConfig {
    pub listen_port: u16,
    pub public_http_base: Option<String>,
    pub region: Option<String>,
    pub bootnodes: Vec<Multiaddr>,
}

pub async fn run_hyperswarm(
    store: SharedStore,
    cfg: HyperSwarmConfig,
) -> Result<HyperSwarmHandle, String> {
    let keypair = Keypair::generate_ed25519();
    let local_peer_id = PeerId::from(keypair.public());

    let gossipsub_config = gossipsub::ConfigBuilder::default()
        .validation_mode(ValidationMode::Permissive)
        .build()
        .map_err(|e| e.to_string())?;

    let mut gs = gossipsub::Behaviour::new(
        MessageAuthenticity::Signed(keypair.clone()),
        gossipsub_config,
    )
    .map_err(|e| e.to_string())?;
    gs.subscribe(&IdentTopic::new(GOSSIP_RELAY_TOPIC))
        .map_err(|e| e.to_string())?;
    gs.subscribe(&IdentTopic::new(GOSSIP_CHUNK_TOPIC))
        .map_err(|e| e.to_string())?;

    let chunk = json::Behaviour::<ChunkRequest, ChunkResponse>::new(
        [(
            StreamProtocol::new("/tks-hyperswarm/chunk/1.0.0"),
            ProtocolSupport::Full,
        )],
        request_response::Config::default(),
    );

    let behaviour = HyperSwarmBehaviour {
        gossipsub: gs,
        identify: identify::Behaviour::new(identify::Config::new(
            "/tks-hyperswarm/1.0.0".into(),
            keypair.public(),
        )),
        ping: ping::Behaviour::new(ping::Config::new()),
        chunk,
    };

    let mut swarm = SwarmBuilder::with_existing_identity(keypair)
        .with_tokio()
        .with_tcp(
            libp2p::tcp::Config::default(),
            libp2p::noise::Config::new,
            libp2p::yamux::Config::default,
        )
        .map_err(|e| e.to_string())?
        .with_behaviour(|_| behaviour)
        .map_err(|e| e.to_string())?
        .build();

    let listen: Multiaddr = format!("/ip4/0.0.0.0/tcp/{}", cfg.listen_port)
        .parse()
        .map_err(|e| format!("bad listen: {e}"))?;
    swarm.listen_on(listen).map_err(|e| e.to_string())?;

    for addr in &cfg.bootnodes {
        if let Some(peer) = extract_peer_id(addr) {
            let _ = swarm.dial(addr.clone().with(libp2p::multiaddr::Protocol::P2p(peer)));
        }
    }

    let directory = Arc::new(SwarmDirectory {
        inner: Arc::new(RwLock::new(DirectoryInner::default())),
        local_peer_id,
    });

    let (cmd_tx, mut cmd_rx) = mpsc::unbounded_channel::<SwarmCommand>();

    info!(
        "🕸️ HyperSwarm peer {} — gossip + chunk pull on tcp/{}",
        local_peer_id, cfg.listen_port
    );

    let dir = directory.clone();
    let http = cfg.public_http_base.clone();
    let region = cfg.region.clone();
    let relay_topic = IdentTopic::new(GOSSIP_RELAY_TOPIC);
    let chunk_topic = IdentTopic::new(GOSSIP_CHUNK_TOPIC);

    let mut avail = AvailIndex::default();
    let mut pull_targets: HashMap<
        libp2p::request_response::OutboundRequestId,
        ChunkRequest,
    > = HashMap::new();
    let mut pull_replies: HashMap<String, oneshot::Sender<u32>> = HashMap::new();
    let mut pull_counts: HashMap<String, u32> = HashMap::new();
    let mut pull_pending: HashMap<String, HashSet<AvailKey>> = HashMap::new();

    tokio::spawn(async move {
        let mut relay_tick = tokio::time::interval(Duration::from_secs(30));
        loop {
            tokio::select! {
                cmd = cmd_rx.recv() => {
                    let Some(cmd) = cmd else { break };
                    match cmd {
                        SwarmCommand::AnnounceChunk(ann) => {
                            if let Ok(bytes) = serde_json::to_vec(&ann) {
                                let _ = swarm.behaviour_mut().gossipsub.publish(chunk_topic.clone(), bytes);
                            }
                            avail.by_key.insert((ann.recipient_id.clone(), ann.message_id.clone()), ann);
                        }
                        SwarmCommand::PullRecipient { recipient_id, reply } => {
                            let keys: Vec<AvailKey> = avail.by_key.iter()
                                .filter(|((r, _), _)| r == &recipient_id)
                                .map(|(k, _)| k.clone())
                                .collect();
                            if keys.is_empty() {
                                let _ = reply.send(0);
                                continue;
                            }
                            pull_replies.insert(recipient_id.clone(), reply);
                            pull_counts.insert(recipient_id.clone(), 0);
                            pull_pending.insert(recipient_id.clone(), keys.iter().cloned().collect());
                            for key in keys {
                                let Some(ann) = avail.by_key.get(&key) else { continue };
                                let Ok(holder) = ann.holder.parse::<PeerId>() else { continue };
                                let req = ChunkRequest {
                                    recipient_id: key.0.clone(),
                                    message_id: key.1.clone(),
                                };
                                let id = swarm.behaviour_mut().chunk.send_request(&holder, req.clone());
                                pull_targets.insert(id, req);
                            }
                        }
                    }
                }
                event = swarm.select_next_some() => {
                    match event {
                        SwarmEvent::Behaviour(HyperSwarmBehaviourEvent::Gossipsub(
                            gossipsub::Event::Message { message, .. },
                        )) => {
                            if message.topic == chunk_topic.hash() {
                                if let Ok(ann) = serde_json::from_slice::<ChunkAvailAnnouncement>(&message.data) {
                                    avail.by_key.insert(
                                        (ann.recipient_id.clone(), ann.message_id.clone()),
                                        ann.clone(),
                                    );
                                    // Torrent seed: pull ciphertext into local cache
                                    if let Ok(holder) = ann.holder.parse::<PeerId>() {
                                        if holder != dir.local_peer_id {
                                            let req = ChunkRequest {
                                                recipient_id: ann.recipient_id,
                                                message_id: ann.message_id,
                                            };
                                            let id = swarm.behaviour_mut().chunk.send_request(&holder, req.clone());
                                            pull_targets.insert(id, req);
                                        }
                                    }
                                }
                            } else if let Ok(ann) = serde_json::from_slice::<RelayAnnouncement>(&message.data) {
                                let mut g = dir.inner.write().await;
                                g.relays.insert(ann.peer_id.clone(), ann);
                            }
                        }
                        SwarmEvent::Behaviour(HyperSwarmBehaviourEvent::Chunk(
                            request_response::Event::Message { message, .. },
                        )) => {
                            match message {
                                request_response::Message::Request { request, channel, .. } => {
                                    let chunk = {
                                        let s = store.read().await;
                                        s.get_chunk(&request.recipient_id, &request.message_id)
                                    };
                                    let _ = swarm.behaviour_mut().chunk.send_response(
                                        channel,
                                        ChunkResponse { chunk },
                                    );
                                }
                                request_response::Message::Response { request_id, response, .. } => {
                                    if let Some(req) = pull_targets.remove(&request_id) {
                                        if let Some(chunk) = response.chunk {
                                            let mut s = store.write().await;
                                            if s.store_replicate(chunk).is_ok() {
                                                for (rid, pending) in pull_pending.iter_mut() {
                                                    pending.remove(&(req.recipient_id.clone(), req.message_id.clone()));
                                                    if pending.is_empty() {
                                                        if let Some(n) = pull_counts.get_mut(rid) {
                                                            *n += 1;
                                                        }
                                                    }
                                                }
                                                if let Some(n) = pull_counts.get_mut(&req.recipient_id) {
                                                    *n += 1;
                                                }
                                            }
                                        }
                                    } else if let Some(chunk) = response.chunk {
                                        let mut s = store.write().await;
                                        let _ = s.store_replicate(chunk);
                                    }
                                }
                            }
                        }
                        SwarmEvent::ConnectionEstablished { peer_id, .. } => {
                            debug!("HyperSwarm connected: {peer_id}");
                        }
                        _ => {}
                    }
                }
                _ = relay_tick.tick() => {
                    if let Some(base) = http.as_ref() {
                        let ann = RelayAnnouncement {
                            peer_id: dir.local_peer_id.to_string(),
                            http_base: base.clone(),
                            libp2p_addrs: vec![format!("/p2p/{}", dir.local_peer_id)],
                            region: region.clone(),
                            ts: chrono::Utc::now().timestamp() as u64,
                        };
                        if let Ok(bytes) = serde_json::to_vec(&ann) {
                            let _ = swarm.behaviour_mut().gossipsub.publish(relay_topic.clone(), bytes);
                        }
                        let mut g = dir.inner.write().await;
                        g.relays.insert(ann.peer_id.clone(), ann);
                    }
                    // Complete pull batches with no pending keys
                    let done: Vec<String> = pull_pending.iter()
                        .filter(|(_, s)| s.is_empty())
                        .map(|(r, _)| r.clone())
                        .collect();
                    for rid in done {
                        pull_pending.remove(&rid);
                        if let Some(reply) = pull_replies.remove(&rid) {
                            let n = pull_counts.remove(&rid).unwrap_or(0);
                            let _ = reply.send(n);
                        }
                    }
                }
            }
        }
    });

    Ok(HyperSwarmHandle {
        directory,
        cmd_tx,
    })
}

fn extract_peer_id(addr: &Multiaddr) -> Option<PeerId> {
    use libp2p::multiaddr::Protocol;
    addr.iter().find_map(|p| {
        if let Protocol::P2p(id) = p {
            Some(id)
        } else {
            None
        }
    })
}

pub fn chunk_avail_from(
    chunk: &EncryptedChunk,
    holder_peer_id: &str,
) -> ChunkAvailAnnouncement {
    use sha2::{Digest, Sha256};
    let hash = Sha256::digest(&chunk.ciphertext);
    ChunkAvailAnnouncement {
        message_id: chunk.message_id.clone(),
        recipient_id: chunk.recipient_id.clone(),
        ciphertext_sha256: hex::encode(hash),
        size: chunk.ciphertext.len().min(u32::MAX as usize) as u32,
        holder: holder_peer_id.to_string(),
        ts: chrono::Utc::now().timestamp() as u64,
    }
}
