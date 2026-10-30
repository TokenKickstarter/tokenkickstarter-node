//! HTTP ingest API for shard messages.

use std::convert::Infallible;
use std::net::SocketAddr;
use bytes::Bytes;
use http_body_util::{BodyExt, Full};
use hyper::body::Incoming;
use hyper::server::conn::http1;
use hyper::service::service_fn;
use hyper::{Method, Request, Response, StatusCode};
use hyper_util::rt::TokioIo;
use tokio::net::TcpListener;

use std::sync::Arc;

use cipher_hybrid::HybridController;

use crate::batch::CipherBatchMessage;
use crate::build_info::SharedChainBuildInfo;
use crate::engine::SharedEngine;
use crate::multi_engine::SharedMultiEngine;

pub async fn run_http_ingest_hybrid(
    port: u16,
    engine: SharedMultiEngine,
    hybrid: Arc<HybridController>,
    build_info: Option<SharedChainBuildInfo>,
) -> Result<(), String> {
    let addr = SocketAddr::from(([0, 0, 0, 0], port));
    let listener = TcpListener::bind(addr).await.map_err(|e| e.to_string())?;
    log::info!("Cipher hybrid ingest on http://0.0.0.0:{port}/ingest /hybrid/stats");

    loop {
        let (stream, _) = listener.accept().await.map_err(|e| e.to_string())?;
        let engine = engine.clone();
        let hybrid = hybrid.clone();
        let build_info = build_info.clone();
        tokio::spawn(async move {
            let io = TokioIo::new(stream);
            let svc = service_fn(move |req| {
                let engine = engine.clone();
                let hybrid = hybrid.clone();
                let build_info = build_info.clone();
                async move { handle_hybrid(req, engine, hybrid, build_info).await }
            });
            if let Err(err) = http1::Builder::new().serve_connection(io, svc).await {
                log::debug!("connection error: {err}");
            }
        });
    }
}

async fn handle_hybrid(
    req: Request<Incoming>,
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
        (Method::GET, "/hybrid/stats") => {
            let snap = hybrid.snapshot();
            ok_json(&serde_json::to_string(&snap).unwrap_or_else(|_| "{}".into()))
        }
        (Method::POST, "/ingest") => {
            let bytes = match body.collect().await {
                Ok(collected) => collected.to_bytes(),
                Err(_) => return err(StatusCode::BAD_REQUEST, "body read failed"),
            };
            let msg: CipherBatchMessage = match serde_json::from_slice(&bytes) {
                Ok(m) => m,
                Err(e) => return err(StatusCode::BAD_REQUEST, &format!("invalid json: {e}")),
            };
            // Ingress is counted once at the relay `/store` when using embedded node + forward.
            let local = hybrid.shard_route_index(&msg.recipient_id);
            let mut guard = engine.lock().await;
            guard.ingest(local, msg);
            let n = guard.pending_total();
            ok_json(&format!(r#"{{"queued":{n}}}"#))
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
            let bytes = match body.collect().await {
                Ok(collected) => collected.to_bytes(),
                Err(_) => return err(StatusCode::BAD_REQUEST, "body read failed"),
            };
            let chunk: StoreChunk = match serde_json::from_slice(&bytes) {
                Ok(c) => c,
                Err(e) => return err(StatusCode::BAD_REQUEST, &format!("invalid json: {e}")),
            };
            let _ = chunk.expires_at;
            use sha2::{Digest, Sha256};
            let hash: [u8; 32] = Sha256::digest(&chunk.ciphertext).into();
            let msg = CipherBatchMessage {
                msg_id: chunk.message_id.clone(),
                sender_id: "shard-store".into(),
                recipient_id: chunk.recipient_id.clone(),
                ciphertext_hash: hash,
                stored_at: chunk.stored_at,
            };
            let local = hybrid.shard_route_index(&msg.recipient_id);
            let mut guard = engine.lock().await;
            guard.ingest(local, msg);
            let n = guard.pending_total();
            ok_json(&format!(
                r#"{{"Stored":{{"message_id":"{}"}},"queued":{n}}}"#,
                chunk.message_id
            ))
        }
        _ => err(StatusCode::NOT_FOUND, "not found"),
    }
}

pub async fn run_http_ingest(port: u16, engine: SharedEngine) -> Result<(), String> {
    let addr = SocketAddr::from(([0, 0, 0, 0], port));
    let listener = TcpListener::bind(addr).await.map_err(|e| e.to_string())?;
    log::info!("Cipher shard executor ingest listening on http://0.0.0.0:{port}/ingest");

    loop {
        let (stream, _) = listener.accept().await.map_err(|e| e.to_string())?;
        let engine = engine.clone();
        tokio::spawn(async move {
            let io = TokioIo::new(stream);
            let svc = service_fn(move |req| {
                let engine = engine.clone();
                async move { handle(req, engine).await }
            });
            if let Err(err) = http1::Builder::new().serve_connection(io, svc).await {
                log::debug!("connection error: {err}");
            }
        });
    }
}

async fn handle(
    req: Request<Incoming>,
    engine: SharedEngine,
) -> Result<Response<Full<Bytes>>, Infallible> {
    let (parts, body) = req.into_parts();
    match (parts.method, parts.uri.path()) {
        (Method::GET, "/health") => ok_json(r#"{"status":"ok"}"#),
        (Method::POST, "/ingest") => {
            let bytes = match body.collect().await {
                Ok(collected) => collected.to_bytes(),
                Err(_) => return err(StatusCode::BAD_REQUEST, "body read failed"),
            };
            let msg: CipherBatchMessage = match serde_json::from_slice(&bytes) {
                Ok(m) => m,
                Err(e) => return err(StatusCode::BAD_REQUEST, &format!("invalid json: {e}")),
            };
            let mut guard = engine.lock().await;
            guard.ingest(msg);
            let n = guard.pending_len();
            ok_json(&format!(r#"{{"queued":{n}}}"#))
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
    Ok(Response::builder()
        .status(status)
        .header("content-type", "application/json")
        .body(Full::new(Bytes::from(format!(r#"{{"error":"{msg}"}}"#))))
        .unwrap())
}
