//! Phase-1 relay load test — POST /store on cipher-relay (:4002).

use std::time::Instant;

use clap::Parser;
use serde::Serialize;
use sha2::{Digest, Sha256};

#[derive(Parser, Debug)]
#[command(name = "cipher-loadtest")]
struct Args {
    #[arg(long, default_value = "http://127.0.0.1:4002")]
    relay_url: String,

    #[arg(long, default_value_t = 1000)]
    messages: usize,

    #[arg(long, default_value_t = 50)]
    concurrency: usize,
}

#[derive(Serialize)]
struct EncryptedChunk {
    recipient_id: String,
    message_id: String,
    ciphertext: Vec<u8>,
    stored_at: u64,
    expires_at: u64,
}

#[tokio::main]
async fn main() -> Result<(), String> {
    let args = Args::parse();
    let client = reqwest::Client::new();
    let url = format!("{}/store", args.relay_url.trim_end_matches('/'));
    let started = Instant::now();
    let sem = std::sync::Arc::new(tokio::sync::Semaphore::new(args.concurrency));

    let mut handles = Vec::new();
    for i in 0..args.messages {
        let client = client.clone();
        let url = url.clone();
        let sem = sem.clone();
        handles.push(tokio::spawn(async move {
            let _permit = sem.acquire().await.unwrap();
            let recipient = format!("recipient_{}", i % 10);
            let message_id = uuid::Uuid::new_v4().to_string();
            let payload = format!("cipher-loadtest-{i}").into_bytes();
            let hash = Sha256::digest(&payload);
            let _ = hash;
            let now = chrono::Utc::now().timestamp() as u64;
            let body = EncryptedChunk {
                recipient_id: recipient,
                message_id: message_id.clone(),
                ciphertext: payload,
                stored_at: now,
                expires_at: now + 3600,
            };
            let resp = client
                .post(&url)
                .json(&body)
                .send()
                .await
                .map_err(|e| e.to_string())?;
            if !resp.status().is_success() {
                return Err(format!("http {}", resp.status()));
            }
            Ok(())
        }));
    }

    let mut ok = 0usize;
    let mut err = 0usize;
    for h in handles {
        match h.await {
            Ok(Ok(())) => ok += 1,
            _ => err += 1,
        }
    }

    let elapsed = started.elapsed().as_secs_f64().max(0.001);
    let result = serde_json::json!({
        "phase": "relay-only",
        "messages": args.messages,
        "ok": ok,
        "errors": err,
        "elapsed_secs": elapsed,
        "msg_per_sec": ok as f64 / elapsed,
    });
    println!("{}", serde_json::to_string_pretty(&result).unwrap());
    std::fs::write("cipher_load_results.json", serde_json::to_string_pretty(&result).unwrap())
        .map_err(|e| e.to_string())?;
    Ok(())
}
