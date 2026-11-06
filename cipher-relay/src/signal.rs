//! Embedded WebSocket signal server (replaces `vps-signal-server` sidecar).
//!
//! Same protocol: `GET /ws?addr=…`, `POST /internal/push` (localhost), health on `/`.

use std::collections::HashMap;
use std::convert::Infallible;
use std::net::SocketAddr;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use bytes::Bytes;
use futures_util::{SinkExt, StreamExt};
use http_body_util::{BodyExt, Full};
use hyper::body::Incoming;
use hyper::server::conn::http1;
use hyper::service::service_fn;
use hyper::{Method, Request, Response, StatusCode};
use hyper_tungstenite::tungstenite::Message;
use hyper_tungstenite::WebSocketStream;
use hyper_util::rt::TokioIo;
use log::{debug, info, warn};
use tokio::sync::{mpsc, RwLock};

const QUEUE_TTL_MS: u64 = 5 * 60 * 1000;
const MAX_QUEUE: usize = 50;

type SessionTx = mpsc::UnboundedSender<Message>;

static SESSION_ID: AtomicU64 = AtomicU64::new(1);

struct LiveSession {
    id: u64,
    tx: SessionTx,
}

#[derive(Clone)]
pub struct SignalHub {
    sessions: Arc<RwLock<HashMap<String, LiveSession>>>,
    queues: Arc<RwLock<HashMap<String, Vec<QueuedMsg>>>>,
}

#[derive(Clone)]
struct QueuedMsg {
    json: String,
    ts: u64,
}

impl SignalHub {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            sessions: Arc::new(RwLock::new(HashMap::new())),
            queues: Arc::new(RwLock::new(HashMap::new())),
        })
    }

    pub fn spawn_cleanup(self: &Arc<Self>) {
        let hub = self.clone();
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(std::time::Duration::from_secs(300)).await;
                hub.cleanup_queues_async().await;
            }
        });
    }

    async fn cleanup_queues_async(&self) {
        let cutoff = now_ms().saturating_sub(QUEUE_TTL_MS);
        let mut queues = self.queues.write().await;
        queues.retain(|_, q| {
            q.retain(|m| m.ts > cutoff);
            !q.is_empty()
        });
    }

    /// Push JSON payload to an online session or queue for offline delivery.
    pub async fn push_json(&self, recipient: &str, payload: serde_json::Value) -> PushOutcome {
        let recipient = recipient.to_lowercase();
        let json = payload.to_string();
        let msg = QueuedMsg {
            json: json.clone(),
            ts: now_ms(),
        };

        if let Some(session) = self.sessions.read().await.get(&recipient) {
            if session.tx.send(Message::Text(json)).is_ok() {
                return PushOutcome {
                    delivered: true,
                    queued: false,
                };
            }
        }

        let mut queues = self.queues.write().await;
        let q = queues.entry(recipient).or_default();
        q.push(msg);
        let cutoff = now_ms().saturating_sub(QUEUE_TTL_MS);
        q.retain(|m| m.ts > cutoff);
        if q.len() > MAX_QUEUE {
            let drop = q.len() - MAX_QUEUE;
            q.drain(0..drop);
        }
        PushOutcome {
            delivered: false,
            queued: true,
        }
    }

    async fn register_session(&self, addr: String, tx: SessionTx) -> u64 {
        let addr = addr.to_lowercase();
        let id = SESSION_ID.fetch_add(1, Ordering::Relaxed);
        if let Some(old) = self
            .sessions
            .write()
            .await
            .insert(addr.clone(), LiveSession { id, tx })
        {
            let _ = old.tx.send(Message::Close(None));
        }
        info!("[Signal] {addr} connected ({} peers)", self.sessions.read().await.len());

        let queued = self.queues.write().await.remove(&addr);
        if let Some(messages) = queued {
            let n = messages.len();
            if let Some(session) = self.sessions.read().await.get(&addr) {
                for m in messages {
                    let _ = session.tx.send(Message::Text(m.json));
                }
                info!("[Signal] Delivered {n} queued message(s) to {addr}");
            }
        }
        id
    }

    async fn unregister_session(&self, addr: &str, session_id: u64) {
        let addr = addr.to_lowercase();
        let mut sessions = self.sessions.write().await;
        if sessions
            .get(&addr)
            .map(|s| s.id == session_id)
            .unwrap_or(false)
        {
            sessions.remove(&addr);
            info!("[Signal] {addr} disconnected ({} peers)", sessions.len());
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct PushOutcome {
    pub delivered: bool,
    pub queued: bool,
}

pub async fn run_signal_server(
    port: u16,
    hub: Arc<SignalHub>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    hub.spawn_cleanup();
    let addr: SocketAddr = format!("0.0.0.0:{port}").parse()?;
    let listener = tokio::net::TcpListener::bind(addr).await?;
    info!("⚡ Embedded Cipher signal on http://0.0.0.0:{port} (/ws, /internal/push)");

    loop {
        let (stream, remote) = listener.accept().await?;
        let hub = hub.clone();
        tokio::spawn(async move {
            let io = TokioIo::new(stream);
            let service = service_fn(move |req| {
                let hub = hub.clone();
                async move { handle_signal_http(req, remote, hub).await }
            });
            if let Err(e) = http1::Builder::new()
                .serve_connection(io, service)
                .await
            {
                debug!("Signal HTTP connection ended: {e}");
            }
        });
    }
}

async fn handle_signal_http(
    req: Request<Incoming>,
    remote: SocketAddr,
    hub: Arc<SignalHub>,
) -> Result<Response<Full<Bytes>>, Infallible> {
    let path = req.uri().path().to_string();

    if hyper_tungstenite::is_upgrade_request(&req) && path == "/ws" {
        let addr = req
            .uri()
            .query()
            .and_then(|q| {
                q.split('&').find_map(|pair| {
                    let mut it = pair.splitn(2, '=');
                    let k = it.next()?;
                    let v = it.next()?;
                    if k == "addr" {
                        Some(v.to_lowercase())
                    } else {
                        None
                    }
                })
            })
            .filter(|a| !a.is_empty());

        let Some(addr) = addr else {
            return Ok(Response::builder()
                .status(StatusCode::BAD_REQUEST)
                .body(Full::new(Bytes::from("Missing addr parameter")))
                .unwrap());
        };

        let (resp, ws) = hyper_tungstenite::upgrade(req, None).unwrap();
        let hub = hub.clone();
        tokio::spawn(async move {
            match ws.await {
                Ok(ws) => run_ws_session(ws, addr, hub).await,
                Err(e) => warn!("[Signal] WebSocket upgrade failed: {e}"),
            }
        });
        return Ok(resp);
    }

    if req.method() == Method::GET && (path == "/" || path == "/health") {
        return Ok(Response::builder()
            .status(StatusCode::OK)
            .body(Full::new(Bytes::from("Cipher embedded signal online")))
            .unwrap());
    }

    if req.method() == Method::POST && path == "/internal/push" {
        if !remote.ip().is_loopback() {
            return Ok(Response::builder()
                .status(StatusCode::FORBIDDEN)
                .body(Full::new(Bytes::from("Forbidden")))
                .unwrap());
        }
        let body = match req.collect().await {
            Ok(collected) => collected.to_bytes(),
            Err(_) => {
                return Ok(Response::builder()
                    .status(StatusCode::BAD_REQUEST)
                    .body(Full::new(Bytes::from("Bad body")))
                    .unwrap());
            }
        };
        return Ok(handle_internal_push(&body, hub).await);
    }

    Ok(Response::builder()
        .status(StatusCode::NOT_FOUND)
        .body(Full::new(Bytes::from("Not found")))
        .unwrap())
}

async fn handle_internal_push(body: &[u8], hub: Arc<SignalHub>) -> Response<Full<Bytes>> {
    let val: serde_json::Value = match serde_json::from_slice(body) {
        Ok(v) => v,
        Err(_) => {
            return Response::builder()
                .status(StatusCode::BAD_REQUEST)
                .body(Full::new(Bytes::from("Invalid JSON")))
                .unwrap();
        }
    };
    let recipient = val
        .get("to")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_lowercase();
    if recipient.is_empty() {
        return Response::builder()
            .status(StatusCode::BAD_REQUEST)
            .body(Full::new(Bytes::from("Missing to")))
            .unwrap();
    }
    let msg_type = val
        .get("type")
        .and_then(|v| v.as_str())
        .unwrap_or("message");
    let payload = serde_json::json!({
        "type": msg_type,
        "message_id": val.get("message_id"),
        "data": val.get("data"),
        "ts": now_ms(),
    });
    let outcome = hub.push_json(&recipient, payload).await;
    let body = serde_json::json!({
        "delivered": outcome.delivered,
        "queued": outcome.queued,
    });
    Response::builder()
        .status(StatusCode::OK)
        .header("Content-Type", "application/json")
        .body(Full::new(Bytes::from(body.to_string())))
        .unwrap()
}

async fn run_ws_session(
    ws: WebSocketStream<TokioIo<hyper::upgrade::Upgraded>>,
    addr: String,
    hub: Arc<SignalHub>,
) {
    let (mut sink, mut stream) = ws.split();
    let (to_client, mut from_hub) = mpsc::unbounded_channel();
    let session_id = hub.register_session(addr.clone(), to_client).await;

    let writer = tokio::spawn(async move {
        while let Some(msg) = from_hub.recv().await {
            if sink.send(msg).await.is_err() {
                break;
            }
        }
    });

    let ack_tx = hub.sessions.read().await.get(&addr.to_lowercase()).map(|s| s.tx.clone());

    while let Some(frame) = stream.next().await {
        match frame {
            Ok(Message::Text(text)) => {
                if let Some(reply) = handle_ws_text(&addr, &text, &hub).await {
                    if let Some(ref tx) = ack_tx {
                        let _ = tx.send(Message::Text(reply));
                    }
                }
            }
            Ok(Message::Close(_)) => break,
            Ok(Message::Ping(p)) => {
                if let Some(ref tx) = ack_tx {
                    let _ = tx.send(Message::Pong(p));
                }
            }
            Ok(_) => {}
            Err(e) => {
                debug!("[Signal] WS read error for {addr}: {e}");
                break;
            }
        }
    }

    writer.abort();
    hub.unregister_session(&addr, session_id).await;
}

/// Returns optional JSON text to send back to the client.
async fn handle_ws_text(from_addr: &str, text: &str, hub: &SignalHub) -> Option<String> {
    let data: serde_json::Value = match serde_json::from_str(text) {
        Ok(v) => v,
        Err(_) => return Some(r#"{"error":"Invalid message format"}"#.to_string()),
    };

    if data.get("type").and_then(|t| t.as_str()) == Some("ping") {
        return None;
    }

    let recipient = data
        .get("to")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_lowercase();
    let payload = data.get("payload");
    if recipient.is_empty() || payload.is_none() {
        return Some(r#"{"error":"Missing to/payload fields"}"#.to_string());
    }

    let signal_payload = serde_json::json!({
        "from": from_addr,
        "payload": payload,
        "ts": now_ms(),
    });
    let outcome = hub.push_json(&recipient, signal_payload).await;
    Some(
        serde_json::json!({
            "ack": true,
            "delivered": outcome.delivered,
            "queued": outcome.queued,
        })
        .to_string(),
    )
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}
