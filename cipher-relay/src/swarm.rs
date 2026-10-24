//! HyperSwarm libp2p gossip — relay presence + peer mesh (port 4001).

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use libp2p::futures::StreamExt;
use libp2p::gossipsub::{self, IdentTopic, MessageAuthenticity, ValidationMode};
use libp2p::identity::Keypair;
use libp2p::swarm::{NetworkBehaviour, SwarmEvent};
use libp2p::{identify, ping, Multiaddr, PeerId, SwarmBuilder};
use log::info;
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;

pub const GOSSIP_TOPIC: &str = "tks-hyperswarm-relay-v1";

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RelayAnnouncement {
    pub peer_id: String,
    pub http_base: String,
    pub libp2p_addrs: Vec<String>,
    pub region: Option<String>,
    pub ts: u64,
}

#[derive(NetworkBehaviour)]
struct HyperSwarmBehaviour {
    gossipsub: gossipsub::Behaviour,
    identify: identify::Behaviour,
    ping: ping::Behaviour,
}

#[derive(Default)]
struct DirectoryInner {
    peers: HashMap<String, RelayAnnouncement>,
}

#[derive(Clone)]
pub struct SwarmDirectory {
    inner: Arc<RwLock<DirectoryInner>>,
    local_peer_id: PeerId,
}

impl SwarmDirectory {
    pub fn local_peer_id(&self) -> PeerId {
        self.local_peer_id
    }

    pub fn snapshot(&self) -> Vec<RelayAnnouncement> {
        if let Ok(guard) = self.inner.try_read() {
            guard.peers.values().cloned().collect()
        } else {
            vec![]
        }
    }

    async fn upsert(&self, ann: RelayAnnouncement) {
        let mut guard = self.inner.write().await;
        guard.peers.insert(ann.peer_id.clone(), ann);
    }
}

pub struct SwarmConfig {
    pub listen_port: u16,
    pub public_http_base: Option<String>,
    pub region: Option<String>,
    pub bootnodes: Vec<Multiaddr>,
}

pub async fn run_hyperswarm_swarm(cfg: SwarmConfig) -> Result<Arc<SwarmDirectory>, String> {
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
    let topic = IdentTopic::new(GOSSIP_TOPIC);
    gs.subscribe(&topic).map_err(|e| e.to_string())?;

    let behaviour = HyperSwarmBehaviour {
        gossipsub: gs,
        identify: identify::Behaviour::new(identify::Config::new(
            "/tks-hyperswarm/1.0.0".into(),
            keypair.public(),
        )),
        ping: ping::Behaviour::new(ping::Config::new()),
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
        .map_err(|e| format!("bad listen addr: {e}"))?;
    swarm
        .listen_on(listen.clone())
        .map_err(|e| e.to_string())?;

    for addr in &cfg.bootnodes {
        if let Some(peer) = extract_peer_id(addr) {
            let _ = swarm.dial(addr.clone().with(libp2p::multiaddr::Protocol::P2p(peer)));
        }
    }

    let directory = Arc::new(SwarmDirectory {
        inner: Arc::new(RwLock::new(DirectoryInner::default())),
        local_peer_id,
    });

    info!(
        "🕸️ HyperSwarm libp2p peer {} listening on {}",
        local_peer_id, cfg.listen_port
    );

    let dir_events = directory.clone();
    let dir_announce = directory.clone();
    let http = cfg.public_http_base.clone();
    let region = cfg.region.clone();
    let topic_clone = topic.clone();

    tokio::spawn(async move {
        loop {
            tokio::select! {
                event = swarm.select_next_some() => {
                    match event {
                        SwarmEvent::Behaviour(HyperSwarmBehaviourEvent::Gossipsub(
                            gossipsub::Event::Message { message, .. },
                        )) => {
                            if let Ok(ann) = serde_json::from_slice::<RelayAnnouncement>(&message.data) {
                                dir_events.upsert(ann).await;
                            }
                        }
                        SwarmEvent::NewListenAddr { address, .. } => {
                            info!("HyperSwarm listen addr: {address}");
                        }
                        SwarmEvent::ConnectionEstablished { peer_id, .. } => {
                            info!("HyperSwarm connected: {peer_id}");
                        }
                        _ => {}
                    }
                }
                _ = tokio::time::sleep(Duration::from_secs(30)) => {
                    if let Some(base) = http.as_ref() {
                        let ann = RelayAnnouncement {
                            peer_id: dir_announce.local_peer_id.to_string(),
                            http_base: base.clone(),
                            libp2p_addrs: vec![format!("/p2p/{}", dir_announce.local_peer_id)],
                            region: region.clone(),
                            ts: chrono::Utc::now().timestamp() as u64,
                        };
                        if let Ok(bytes) = serde_json::to_vec(&ann) {
                            let _ = swarm.behaviour_mut().gossipsub.publish(topic_clone.clone(), bytes);
                        }
                        dir_announce.upsert(ann).await;
                    }
                }
            }
        }
    });

    Ok(directory)
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
