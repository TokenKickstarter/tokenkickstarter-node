//! HTTP ingest API for shard messages.

use bytes::Bytes;
use http_body_util::{BodyExt, Full, LengthLimitError, Limited};
use hyper::body::Incoming;
use hyper::header::HeaderMap;
use hyper::server::conn::http1;
use hyper::service::service_fn;
use hyper::{Method, Request, Response, StatusCode};
use hyper_util::rt::TokioIo;
use sha2::{Digest, Sha256};
use std::convert::Infallible;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use tokio::net::TcpListener;

use std::sync::Arc;

use cipher_hybrid::HybridController;

use crate::batch::CipherBatchMessage;
use crate::build_info::SharedChainBuildInfo;
use crate::engine::SharedEngine;
use crate::multi_engine::SharedMultiEngine;

const MAX_SHARD_BODY_BYTES: usize = 16 * 1024 * 1024;

fn resolve_bind_ip() -> IpAddr {
    match std::env::var("CIPHER_BIND_ADDR") {
        Ok(raw) if !raw.trim().is_empty() => match raw.trim().parse::<IpAddr>() {
            Ok(ip) => ip,
            Err(e) => {
                log::warn!("Invalid CIPHER_BIND_ADDR '{raw}' ({e}); defaulting to 127.0.0.1");
                IpAddr::V4(Ipv4Addr::LOCALHOST)
            }
        },
        _ => IpAddr::V4(Ipv4Addr::LOCALHOST),
    }
}

enum BodyReadError {
    TooLarge,
    InvalidBody,
}

async fn collect_bounded_body(body: Incoming, max_bytes: usize) -> Result<Bytes, BodyReadError> {
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

fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    let da = Sha256::digest(a);
    let db = Sha256::digest(b);
    let mut diff = 0u8;
    for (x, y) in da.iter().zip(db.iter()) {
        diff |= x ^ y;
    }
    diff == 0 && a.len() == b.len()
}

fn authorize_shard_ingest(remote: SocketAddr, headers: &HeaderMap) -> bool {
    let is_proxied = headers.contains_key("x-forwarded-for")
        || headers.contains_key("x-real-ip")
        || headers.contains_key("forwarded");
    if let Ok(expected) = std::env::var("CIPHER_SHARD_INGEST_KEY") {
        if !expected.is_empty() {
            return headers
                .get("X-TKS-Shard-Ingest-Key")
                .and_then(|v| v.to_str().ok())
                .map(|got| constant_time_eq(got.as_bytes(), expected.as_bytes()))
                .unwrap_or(false);
        }
    }
    remote.ip().is_loopback() && !is_proxied
}

pub async fn run_http_ingest_hybrid(
    port: u16,
    engine: SharedMultiEngine,
    hybrid: Arc<HybridController>,
    build_info: Option<SharedChainBuildInfo>,
) -> Result<(), String> {
    let addr = SocketAddr::new(resolve_bind_ip(), port);
    let listener = TcpListener::bind(addr).await.map_err(|e| e.to_string())?;
    log::info!("Cipher hybrid ingest on http://{addr}/ingest /hybrid/stats");

    loop {
        let (stream, remote) = listener.accept().await.map_err(|e| e.to_string())?;
        let engine = engine.clone();
        let hybrid = hybrid.clone();
        let build_info = build_info.clone();
        tokio::spawn(async move {
            let io = TokioIo::new(stream);
            let svc = service_fn(move |req| {
                let engine = engine.clone();
                let hybrid = hybrid.clone();
                let build_info = build_info.clone();
                async move { handle_hybrid(req, remote, engine, hybrid, build_info).await }
            });
            if let Err(err) = http1::Builder::new().serve_connection(io, svc).await {
                log::debug!("connection error: {err}");
            }
        });
    }
}

async fn handle_hybrid(
    req: Request<Incoming>,
    remote: SocketAddr,
    engine: SharedMultiEngine,
    hybrid: Arc<HybridController>,
    build_info: Option<SharedChainBuildInfo>,
) -> Result<Response<Full<Bytes>>, Infallible> {
    let (parts, body) = req.into_parts();
    match (parts.method, parts.uri.path()) {
        (Method::GET, "/health") => ok_json(r#"{"status":"ok"}"#),
        (Method::GET, "/version") => {
            if let Some(info) = build_info {
                ok_json(&serde_json::to_string(info.as_ref()).unwrap_or_else(|_| "{}".into()))
            } else {
                ok_json(r#"{"error":"build_info unavailable"}"#)
            }
        }
        (Method::GET, "/hybrid/stats") | (Method::GET, "/stats") => {
            let snap = hybrid.snapshot();
            ok_json(&serde_json::to_string(&snap).unwrap_or_else(|_| "{}".into()))
        }
        (Method::GET, "/discovery") => {
            let body = serde_json::json!({
                "role": "shard_ingest",
                "note": "Ciphertext storage and POST /ack live on relay; this host batches leaves for L1 anchors.",
                "preferred_store": "https://relay.tokenkickstarter.com/store",
                "preferred_ack": "https://relay.tokenkickstarter.com/ack",
                "endpoints": ["/ingest", "/ingest/batch", "/store", "/health", "/version", "/hybrid/stats", "/stats"]
            });
            ok_json(&body.to_string())
        }
        (Method::POST, "/ack") => ok_json(
            r#"{"deleted":0,"note":"Shard does not retain ciphertext; POST /ack to relay.tokenkickstarter.com"}"#,
        ),
        (Method::POST, "/ingest") => {
            if !authorize_shard_ingest(remote, &parts.headers) {
                return err(StatusCode::FORBIDDEN, "shard ingest forbidden");
            }
            let bytes = match collect_bounded_body(body, MAX_SHARD_BODY_BYTES).await {
                Ok(collected) => collected,
                Err(BodyReadError::TooLarge) => {
                    return err(StatusCode::PAYLOAD_TOO_LARGE, "payload too large");
                }
                Err(BodyReadError::InvalidBody) => {
                    return err(StatusCode::BAD_REQUEST, "body read failed");
                }
            };
            let msg: CipherBatchMessage = match serde_json::from_slice(&bytes) {
                Ok(m) => m,
                Err(e) => return err(StatusCode::BAD_REQUEST, &format!("invalid json: {e}")),
            };
            // Ingress is counted once at the relay `/store` when using embedded node + forward.
            let local = hybrid.shard_route_index(&msg.recipient_id);
            let n = engine.ingest(local, msg).await;
            ok_json(&serde_json::json!({ "queued": n }).to_string())
        }
        (Method::POST, "/ingest/batch") => {
            if !authorize_shard_ingest(remote, &parts.headers) {
                return err(StatusCode::FORBIDDEN, "shard ingest forbidden");
            }
            let bytes = match collect_bounded_body(body, MAX_SHARD_BODY_BYTES).await {
                Ok(collected) => collected,
                Err(BodyReadError::TooLarge) => {
                    return err(StatusCode::PAYLOAD_TOO_LARGE, "payload too large");
                }
                Err(BodyReadError::InvalidBody) => {
                    return err(StatusCode::BAD_REQUEST, "body read failed");
                }
            };
            let msgs: Vec<CipherBatchMessage> = match serde_json::from_slice(&bytes) {
                Ok(m) => m,
                Err(e) => return err(StatusCode::BAD_REQUEST, &format!("invalid json: {e}")),
            };
            let accepted = msgs.len();
            let routed: Vec<(u16, CipherBatchMessage)> = msgs
                .into_iter()
                .map(|m| (hybrid.shard_route_index(&m.recipient_id), m))
                .collect();
            let queued = engine.ingest_batch(routed).await;
            ok_json(&serde_json::json!({ "accepted": accepted, "queued": queued }).to_string())
        }
        (Method::POST, "/store") => {
            // Same JSON shape as cipher-relay `/store` (auditors often hit the shard vhost).
            #[derive(serde::Deserialize)]
            struct StoreChunk {
                recipient_id: String,
                message_id: String,
                ciphertext: Vec<u8>,
                stored_at: u64,
                #[serde(default)]
                expires_at: u64,
            }
            let bytes = match collect_bounded_body(body, MAX_SHARD_BODY_BYTES).await {
                Ok(collected) => collected,
                Err(BodyReadError::TooLarge) => {
                    return err(StatusCode::PAYLOAD_TOO_LARGE, "payload too large");
                }
                Err(BodyReadError::InvalidBody) => {
                    return err(StatusCode::BAD_REQUEST, "body read failed");
                }
            };
            let chunk: StoreChunk = match serde_json::from_slice(&bytes) {
                Ok(c) => c,
                Err(e) => return err(StatusCode::BAD_REQUEST, &format!("invalid json: {e}")),
            };
            let _ = chunk.expires_at;
            let hash: [u8; 32] = Sha256::digest(&chunk.ciphertext).into();
            let msg = CipherBatchMessage {
                msg_id: chunk.message_id.clone(),
                sender_id: "shard-store".into(),
                recipient_id: chunk.recipient_id.clone(),
                ciphertext_hash: hash,
                stored_at: chunk.stored_at,
            };
            let local = hybrid.shard_route_index(&msg.recipient_id);
            let n = engine.ingest(local, msg).await;
            let body_json = serde_json::json!({
                "Stored": { "message_id": chunk.message_id },
                "queued": n,
            });
            ok_json(&body_json.to_string())
        }
        _ => err(StatusCode::NOT_FOUND, "not found"),
    }
}

pub async fn run_http_ingest(port: u16, engine: SharedEngine) -> Result<(), String> {
    let addr = SocketAddr::new(resolve_bind_ip(), port);
    let listener = TcpListener::bind(addr).await.map_err(|e| e.to_string())?;
    log::info!("Cipher shard executor ingest listening on http://{addr}/ingest");

    loop {
        let (stream, remote) = listener.accept().await.map_err(|e| e.to_string())?;
        let engine = engine.clone();
        tokio::spawn(async move {
            let io = TokioIo::new(stream);
            let svc = service_fn(move |req| {
                let engine = engine.clone();
                async move { handle(req, remote, engine).await }
            });
            if let Err(err) = http1::Builder::new().serve_connection(io, svc).await {
                log::debug!("connection error: {err}");
            }
        });
    }
}

async fn handle(
    req: Request<Incoming>,
    remote: SocketAddr,
    engine: SharedEngine,
) -> Result<Response<Full<Bytes>>, Infallible> {
    let (parts, body) = req.into_parts();
    match (parts.method, parts.uri.path()) {
        (Method::GET, "/health") => ok_json(r#"{"status":"ok"}"#),
        (Method::POST, "/ingest") => {
            if !authorize_shard_ingest(remote, &parts.headers) {
                return err(StatusCode::FORBIDDEN, "shard ingest forbidden");
            }
            let bytes = match collect_bounded_body(body, MAX_SHARD_BODY_BYTES).await {
                Ok(collected) => collected,
                Err(BodyReadError::TooLarge) => {
                    return err(StatusCode::PAYLOAD_TOO_LARGE, "payload too large");
                }
                Err(BodyReadError::InvalidBody) => {
                    return err(StatusCode::BAD_REQUEST, "body read failed");
                }
            };
            let msg: CipherBatchMessage = match serde_json::from_slice(&bytes) {
                Ok(m) => m,
                Err(e) => return err(StatusCode::BAD_REQUEST, &format!("invalid json: {e}")),
            };
            let mut guard = engine.lock().await;
            guard.ingest(msg);
            let n = guard.pending_len();
            ok_json(&serde_json::json!({ "queued": n }).to_string())
        }
        _ => err(StatusCode::NOT_FOUND, "not found"),
    }
}

fn ok_json(body: &str) -> Result<Response<Full<Bytes>>, Infallible> {
    Ok(Response::builder()
        .status(StatusCode::OK)
        .header("content-type", "application/json")
        .body(Full::new(Bytes::from(body.to_string())))
        .unwrap())
}

fn err(status: StatusCode, msg: &str) -> Result<Response<Full<Bytes>>, Infallible> {
    let body = serde_json::json!({ "error": msg }).to_string();
    Ok(Response::builder()
        .status(status)
        .header("content-type", "application/json")
        .body(Full::new(Bytes::from(body)))
        .unwrap())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shard_ingest_auth_blocks_proxied_and_external_requests() {
        let loopback: SocketAddr = "127.0.0.1:41000".parse().unwrap();
        let remote: SocketAddr = "198.51.100.4:41000".parse().unwrap();

        let empty = HeaderMap::new();
        assert!(authorize_shard_ingest(loopback, &empty));
        assert!(!authorize_shard_ingest(remote, &empty));

        let mut proxied = HeaderMap::new();
        proxied.insert("x-forwarded-for", "203.0.113.5".parse().unwrap());
        assert!(!authorize_shard_ingest(loopback, &proxied));
    }
}
