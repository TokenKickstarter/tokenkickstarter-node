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
use http_body_util::{BodyExt, Full, LengthLimitError, Limited};
use hyper::body::Incoming;
use hyper::server::conn::http1;
use hyper::service::service_fn;
use hyper::{Method, Request, Response, StatusCode};
use hyper_tungstenite::tungstenite::Message;
use hyper_tungstenite::WebSocketStream;
use hyper_util::rt::TokioIo;
use log::{debug, info, warn};
use std::net::{IpAddr, Ipv4Addr};
use std::time::{Duration, Instant};
use tokio::sync::{mpsc, Mutex, RwLock};

use crate::security;

const MAX_SIGNAL_PUSH_BYTES: usize = 1024 * 1024;
const MAX_WS_TEXT_BYTES: usize = 256 * 1024;
const MAX_WS_CONN_PER_IP: usize = 32;
const MAX_SESSIONS_TOTAL: usize = 50_000;
const MAX_SIGNAL_QUEUES_TOTAL: usize = 50_000;
const MAX_WS_MSG_PER_10S: u32 = 120;

pub(crate) enum BodyReadError {
    TooLarge,
    InvalidBody,
}

pub(crate) async fn collect_bounded_body(
    body: Incoming,
    max_bytes: usize,
) -> Result<Bytes, BodyReadError> {
    Limited::new(body, max_bytes)
        .collect()
        .await
        .map(|c| c.to_bytes())
        .map_err(|e| {
            if e.is::<LengthLimitError>() {
                BodyReadError::TooLarge
            } else {
                BodyReadError::InvalidBody
            }
        })
}

pub(crate) fn resolve_bind_ip() -> IpAddr {
    match std::env::var("CIPHER_BIND_ADDR") {
        Ok(raw) if !raw.trim().is_empty() => match raw.trim().parse::<IpAddr>() {
            Ok(ip) => ip,
            Err(e) => {
                warn!("Invalid CIPHER_BIND_ADDR '{raw}' ({e}); defaulting to 127.0.0.1");
                IpAddr::V4(Ipv4Addr::LOCALHOST)
            }
        },
        _ => IpAddr::V4(Ipv4Addr::LOCALHOST),
    }
}

pub(crate) fn is_valid_signal_addr(addr: &str) -> bool {
    !addr.is_empty()
        && addr.len() <= 128
        && addr
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-' | b'.'))
}

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
    conns_per_ip: Arc<Mutex<HashMap<IpAddr, usize>>>,
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
            conns_per_ip: Arc::new(Mutex::new(HashMap::new())),
        })
    }

    pub async fn try_acquire_ws_slot(self: &Arc<Self>, ip: IpAddr) -> Option<WsConnGuard> {
        let mut conns = self.conns_per_ip.lock().await;
        let current = *conns.get(&ip).unwrap_or(&0);
        if current >= MAX_WS_CONN_PER_IP {
            return None;
        }
        if self.sessions.read().await.len() >= MAX_SESSIONS_TOTAL {
            return None;
        }
        conns.insert(ip, current + 1);
        Some(WsConnGuard {
            hub: self.clone(),
            ip,
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
        if queues.len() >= MAX_SIGNAL_QUEUES_TOTAL && !queues.contains_key(&recipient) {
            let cutoff = now_ms().saturating_sub(QUEUE_TTL_MS);
            queues.retain(|_, q| {
                q.retain(|m| m.ts > cutoff);
                !q.is_empty()
            });
            if queues.len() >= MAX_SIGNAL_QUEUES_TOTAL {
                return PushOutcome {
                    delivered: false,
                    queued: false,
                };
            }
        }
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
        info!(
            "[Signal] {addr} connected ({} peers)",
            self.sessions.read().await.len()
        );

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

pub struct WsConnGuard {
    hub: Arc<SignalHub>,
    ip: IpAddr,
}

impl Drop for WsConnGuard {
    fn drop(&mut self) {
        let hub = self.hub.clone();
        let ip = self.ip;
        tokio::spawn(async move {
            let mut conns = hub.conns_per_ip.lock().await;
            if let Some(count) = conns.get_mut(&ip) {
                if *count <= 1 {
                    conns.remove(&ip);
                } else {
                    *count -= 1;
                }
            }
        });
    }
}

pub async fn run_signal_server(
    port: u16,
    hub: Arc<SignalHub>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let bind_ip = resolve_bind_ip();
    let addr = SocketAddr::new(bind_ip, port);
    let listener = tokio::net::TcpListener::bind(addr).await?;
    info!("⚡ Embedded Cipher signal on http://{addr} (/ws, /internal/push)");
    serve_signal_listener(listener, hub).await;
    Ok(())
}

pub(crate) async fn serve_signal_listener(listener: tokio::net::TcpListener, hub: Arc<SignalHub>) {
    serve_signal_listener_with_auth(listener, hub, security::require_client_sig_enabled()).await;
}

pub(crate) async fn serve_signal_listener_with_auth(
    listener: tokio::net::TcpListener,
    hub: Arc<SignalHub>,
    enforce_sig: bool,
) {
    hub.spawn_cleanup();
    let rate_limiter = Arc::new(security::IpRateLimiter::from_env());

    loop {
        let Ok((stream, remote)) = listener.accept().await else {
            continue;
        };
        let hub = hub.clone();
        let rl = rate_limiter.clone();
        tokio::spawn(async move {
            let io = TokioIo::new(stream);
            let service = service_fn(move |req| {
                let hub = hub.clone();
                let rl = rl.clone();
                async move { handle_signal_http(req, remote, hub, &rl, enforce_sig).await }
            });
            // `.with_upgrades()` is required in hyper 1 so WebSocket connections survive 101 Switching Protocols.
            if let Err(e) = http1::Builder::new()
                .serve_connection(io, service)
                .with_upgrades()
                .await
            {
                debug!("Signal HTTP connection ended: {e}");
            }
        });
    }
}

/// Attempts to handle `GET /ws?addr=<id>&ts=<ms>&sig=<hex>` WebSocket upgrade on `:4003` or `:4002`.
pub(crate) async fn try_upgrade_ws_with_auth(
    req: Request<Incoming>,
    remote: SocketAddr,
    hub: &Arc<SignalHub>,
    enforce_sig: bool,
) -> Result<Response<Full<Bytes>>, Request<Incoming>> {
    if !hyper_tungstenite::is_upgrade_request(&req) || req.uri().path() != "/ws" {
        return Err(req);
    }

    let addr = req
        .uri()
        .query()
        .and_then(|q| {
            q.split('&').find_map(|pair| {
                let (k, v) = pair.split_once('=')?;
                if k == "addr" {
                    Some(v.trim().to_lowercase())
                } else {
                    None
                }
            })
        })
        .filter(|a| is_valid_signal_addr(a));

    let Some(addr) = addr else {
        return Ok(Response::builder()
            .status(StatusCode::BAD_REQUEST)
            .body(Full::new(Bytes::from("Missing or invalid addr parameter")))
            .unwrap());
    };

    if let Err((status, msg)) =
        security::authorize_ws_mailbox(&addr, req.uri().query(), enforce_sig)
    {
        return Ok(Response::builder()
            .status(status)
            .body(Full::new(Bytes::from(msg)))
            .unwrap());
    }

    let client_ip = security::extract_client_ip(remote, req.headers());
    let Some(guard) = hub.try_acquire_ws_slot(client_ip).await else {
        return Ok(Response::builder()
            .status(StatusCode::TOO_MANY_REQUESTS)
            .body(Full::new(Bytes::from("Too many WebSocket connections")))
            .unwrap());
    };

    match hyper_tungstenite::upgrade(req, None) {
        Ok((resp, ws)) => {
            let hub = hub.clone();
            tokio::spawn(async move {
                let _guard = guard;
                match ws.await {
                    Ok(ws) => run_ws_session(ws, addr, hub).await,
                    Err(e) => warn!("[Signal] WebSocket upgrade failed: {e}"),
                }
            });
            Ok(resp)
        }
        Err(e) => {
            drop(guard);
            warn!("[Signal] Rejected invalid WebSocket upgrade: {e}");
            Ok(Response::builder()
                .status(StatusCode::BAD_REQUEST)
                .body(Full::new(Bytes::from("Invalid WebSocket upgrade")))
                .unwrap())
        }
    }
}

async fn handle_signal_http(
    req: Request<Incoming>,
    remote: SocketAddr,
    hub: Arc<SignalHub>,
    rate_limiter: &security::IpRateLimiter,
    enforce_sig: bool,
) -> Result<Response<Full<Bytes>>, Infallible> {
    let client_ip = security::extract_client_ip(remote, req.headers());
    if !rate_limiter.allow(client_ip, 1) {
        return Ok(Response::builder()
            .status(StatusCode::TOO_MANY_REQUESTS)
            .body(Full::new(Bytes::from("Rate limit exceeded")))
            .unwrap());
    }

    let req = match try_upgrade_ws_with_auth(req, remote, &hub, enforce_sig).await {
        Ok(resp) => return Ok(resp),
        Err(req) => req,
    };

    let path = req.uri().path().to_string();

    if req.method() == Method::GET && (path == "/" || path == "/health") {
        return Ok(Response::builder()
            .status(StatusCode::OK)
            .body(Full::new(Bytes::from("Cipher embedded signal online")))
            .unwrap());
    }

    if req.method() == Method::POST && path == "/internal/push" {
        if !remote.ip().is_loopback()
            || req.headers().contains_key("x-forwarded-for")
            || req.headers().contains_key("x-real-ip")
            || req.headers().contains_key("forwarded")
        {
            return Ok(Response::builder()
                .status(StatusCode::FORBIDDEN)
                .body(Full::new(Bytes::from("Forbidden")))
                .unwrap());
        }
        let body = match collect_bounded_body(req.into_body(), MAX_SIGNAL_PUSH_BYTES).await {
            Ok(collected) => collected,
            Err(BodyReadError::TooLarge) => {
                return Ok(Response::builder()
                    .status(StatusCode::PAYLOAD_TOO_LARGE)
                    .body(Full::new(Bytes::from("Payload too large")))
                    .unwrap());
            }
            Err(BodyReadError::InvalidBody) => {
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
    let ack_tx = to_client.clone();
    let session_id = hub.register_session(addr.clone(), to_client).await;

    let writer = tokio::spawn(async move {
        while let Some(msg) = from_hub.recv().await {
            if sink.send(msg).await.is_err() {
                break;
            }
        }
    });

    let mut rl_start = Instant::now();
    let mut rl_count: u32 = 0;

    while let Some(frame) = stream.next().await {
        match frame {
            Ok(Message::Text(text)) => {
                if rl_start.elapsed() >= Duration::from_secs(10) {
                    rl_start = Instant::now();
                    rl_count = 0;
                }
                rl_count = rl_count.saturating_add(1);
                if rl_count > MAX_WS_MSG_PER_10S {
                    let _ = ack_tx.send(Message::Text(
                        r#"{"error":"Rate limit exceeded"}"#.to_string(),
                    ));
                    continue;
                }
                if let Some(reply) = handle_ws_text(&addr, &text, &hub).await {
                    let _ = ack_tx.send(Message::Text(reply));
                }
            }
            Ok(Message::Close(_)) => break,
            Ok(Message::Ping(p)) => {
                let _ = ack_tx.send(Message::Pong(p));
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
    if text.len() > MAX_WS_TEXT_BYTES {
        return Some(r#"{"error":"Message too large"}"#.to_string());
    }
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
    if !is_valid_signal_addr(&recipient) || payload.is_none() {
        return Some(r#"{"error":"Missing or invalid to/payload fields"}"#.to_string());
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
