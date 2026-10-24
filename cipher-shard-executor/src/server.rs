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

use crate::batch::CipherBatchMessage;
use crate::engine::SharedEngine;

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
