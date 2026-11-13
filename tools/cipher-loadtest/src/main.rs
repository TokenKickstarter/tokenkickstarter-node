//! Cipher relay & shard ingest load test — supports single `POST /store`,
//! batched `POST /store/batch` (default benchmark on `:4002`), and optional
//! shard `POST /ingest` / `POST /ingest/batch` benchmarking.

use std::path::PathBuf;
use std::time::Instant;

use clap::Parser;
use serde::Serialize;

#[derive(Parser, Debug)]
#[command(name = "cipher-loadtest")]
struct Args {
    #[arg(long, default_value = "http://127.0.0.1:4002")]
    relay_url: String,

    /// Optional shard executor HTTP base URL (when set alongside --batch-size, also benchmarks /ingest or /ingest/batch).
    #[arg(long)]
    shard_url: Option<String>,

    #[arg(long, default_value_t = 1000)]
    messages: usize,

    #[arg(long, default_value_t = 50)]
    concurrency: usize,

    /// Messages per HTTP request (`1` uses `/store`, `>1` uses `/store/batch`).
    #[arg(long, default_value_t = 1)]
    batch_size: usize,

    /// Optional JSON output file path.
    #[arg(long)]
    out: Option<PathBuf>,
}

#[derive(Serialize, Clone)]
struct EncryptedChunk {
    recipient_id: String,
    message_id: String,
    ciphertext: Vec<u8>,
    stored_at: u64,
    expires_at: u64,
}

#[derive(Serialize, Clone)]
struct BatchIngestMsg {
    msg_id: String,
    sender_id: String,
    recipient_id: String,
    ciphertext_hash: [u8; 32],
    stored_at: u64,
}

async fn run_relay_load(
    client: &reqwest::Client,
    base: &str,
    messages: usize,
    concurrency: usize,
    batch_size: usize,
) -> serde_json::Value {
    let batch_size = batch_size.max(1);
    let endpoint = if batch_size > 1 {
        format!("{}/store/batch", base.trim_end_matches('/'))
    } else {
        format!("{}/store", base.trim_end_matches('/'))
    };
    let started = Instant::now();
    let sem = std::sync::Arc::new(tokio::sync::Semaphore::new(concurrency.max(1)));
    let now = chrono::Utc::now().timestamp() as u64;

    let mut handles = Vec::new();
    let mut offset = 0usize;
    while offset < messages {
        let count = (messages - offset).min(batch_size);
        let client = client.clone();
        let endpoint = endpoint.clone();
        let sem = sem.clone();
        handles.push(tokio::spawn(async move {
            let _permit = sem.acquire().await.unwrap();
            if batch_size > 1 {
                let mut chunks = Vec::with_capacity(count);
                for j in 0..count {
                    let idx = offset + j;
                    chunks.push(EncryptedChunk {
                        recipient_id: format!("recipient_{}", idx % 64),
                        message_id: uuid::Uuid::new_v4().to_string(),
                        ciphertext: format!("cipher-loadtest-{idx}").into_bytes(),
                        stored_at: now,
                        expires_at: now + 3600,
                    });
                }
                let resp = client
                    .post(&endpoint)
                    .json(&chunks)
                    .send()
                    .await
                    .map_err(|e| e.to_string())?;
                if !resp.status().is_success() {
                    return Err(format!("http {}", resp.status()));
                }
                Ok(count)
            } else {
                let body = EncryptedChunk {
                    recipient_id: format!("recipient_{}", offset % 64),
                    message_id: uuid::Uuid::new_v4().to_string(),
                    ciphertext: format!("cipher-loadtest-{offset}").into_bytes(),
                    stored_at: now,
                    expires_at: now + 3600,
                };
                let resp = client
                    .post(&endpoint)
                    .json(&body)
                    .send()
                    .await
                    .map_err(|e| e.to_string())?;
                if !resp.status().is_success() {
                    return Err(format!("http {}", resp.status()));
                }
                Ok(1)
            }
        }));
        offset += count;
    }

    let mut ok = 0usize;
    let mut err = 0usize;
    for h in handles {
        match h.await {
            Ok(Ok(n)) => ok += n,
            _ => err += 1,
        }
    }

    let elapsed = started.elapsed().as_secs_f64().max(0.0001);
    serde_json::json!({
        "phase": if batch_size > 1 { "relay-batch" } else { "relay-single" },
        "endpoint": endpoint,
        "batch_size": batch_size,
        "messages": messages,
        "ok": ok,
        "errors": err,
        "elapsed_secs": elapsed,
        "msg_per_sec": ok as f64 / elapsed,
    })
}

async fn run_shard_load(
    client: &reqwest::Client,
    base: &str,
    messages: usize,
    concurrency: usize,
    batch_size: usize,
) -> serde_json::Value {
    let batch_size = batch_size.max(1);
    let endpoint = if batch_size > 1 {
        format!("{}/ingest/batch", base.trim_end_matches('/'))
    } else {
        format!("{}/ingest", base.trim_end_matches('/'))
    };
    let started = Instant::now();
    let sem = std::sync::Arc::new(tokio::sync::Semaphore::new(concurrency.max(1)));
    let now = chrono::Utc::now().timestamp() as u64;

    let mut handles = Vec::new();
    let mut offset = 0usize;
    while offset < messages {
        let count = (messages - offset).min(batch_size);
        let client = client.clone();
        let endpoint = endpoint.clone();
        let sem = sem.clone();
        handles.push(tokio::spawn(async move {
            let _permit = sem.acquire().await.unwrap();
            if batch_size > 1 {
                let mut items = Vec::with_capacity(count);
                for j in 0..count {
                    let idx = offset + j;
                    items.push(BatchIngestMsg {
                        msg_id: uuid::Uuid::new_v4().to_string(),
                        sender_id: "loadtest".into(),
                        recipient_id: format!("recipient_{}", idx % 64),
                        ciphertext_hash: [((idx & 0xff) as u8); 32],
                        stored_at: now,
                    });
                }
                let resp = client
                    .post(&endpoint)
                    .json(&items)
                    .send()
                    .await
                    .map_err(|e| e.to_string())?;
                if !resp.status().is_success() {
                    return Err(format!("http {}", resp.status()));
                }
                Ok(count)
            } else {
                let item = BatchIngestMsg {
                    msg_id: uuid::Uuid::new_v4().to_string(),
                    sender_id: "loadtest".into(),
                    recipient_id: format!("recipient_{}", offset % 64),
                    ciphertext_hash: [((offset & 0xff) as u8); 32],
                    stored_at: now,
                };
                let resp = client
                    .post(&endpoint)
                    .json(&item)
                    .send()
                    .await
                    .map_err(|e| e.to_string())?;
                if !resp.status().is_success() {
                    return Err(format!("http {}", resp.status()));
                }
                Ok(1)
            }
        }));
        offset += count;
    }

    let mut ok = 0usize;
    let mut err = 0usize;
    for h in handles {
        match h.await {
            Ok(Ok(n)) => ok += n,
            _ => err += 1,
        }
    }

    let elapsed = started.elapsed().as_secs_f64().max(0.0001);
    serde_json::json!({
        "phase": if batch_size > 1 { "shard-batch" } else { "shard-single" },
        "endpoint": endpoint,
        "batch_size": batch_size,
        "messages": messages,
        "ok": ok,
        "errors": err,
        "elapsed_secs": elapsed,
        "msg_per_sec": ok as f64 / elapsed,
    })
}

#[tokio::main]
async fn main() -> Result<(), String> {
    let args = Args::parse();
    let client = reqwest::Client::builder()
        .pool_max_idle_per_host(args.concurrency.max(32))
        .tcp_nodelay(true)
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .map_err(|e| e.to_string())?;

    let relay_res = run_relay_load(
        &client,
        &args.relay_url,
        args.messages,
        args.concurrency,
        args.batch_size,
    )
    .await;

    let output = if let Some(ref shard_url) = args.shard_url {
        let shard_res = run_shard_load(
            &client,
            shard_url,
            args.messages,
            args.concurrency,
            args.batch_size,
        )
        .await;
        serde_json::json!({
            "relay": relay_res,
            "shard": shard_res,
        })
    } else {
        relay_res
    };

    let pretty = serde_json::to_string_pretty(&output).unwrap();
    println!("{pretty}");
    if let Some(ref out_path) = args.out {
        std::fs::write(out_path, pretty).map_err(|e| e.to_string())?;
    }
    Ok(())
}
