//! Register a HyperSwarm validator and force shard rotation (sudo) via JSON-RPC.

use codec::{Decode, Encode};
use fp_account::AccountId20;

use crate::account_id20::from_secret_uri;
use frame_support::BoundedVec;
use pallet_shard_registry::{Call as ShardRegistryCall, GeoRegion};
use sp_core::{ecdsa, H256, Pair};
use tks_runtime::{Hash, RuntimeCall};

use crate::extrinsic_sign::encode_signed_extrinsic;

#[derive(Debug, clap::Parser)]
pub struct CipherRegisterShardCmd {
    /// JSON-RPC for reads (nonce, genesis). Defaults to `--submit-url`.
    #[arg(long)]
    pub rpc_url: Option<String>,

    /// JSON-RPC URL with `author_submitExtrinsic` (validator, `--rpc-methods=Unsafe`).
    #[arg(long, default_value = "http://127.0.0.1:9944")]
    pub submit_url: String,

    /// File with one line: ECDSA secret URI (required in production).
    #[arg(long)]
    pub seed_file: Option<std::path::PathBuf>,

    /// Dev-only URI override (conflicts with `--seed-file`; never use on mainnet).
    #[arg(long, hide = true)]
    pub seed: Option<String>,

    /// HyperSwarm endpoint stored on-chain (HTTPS URL for shard ingest).
    #[arg(long, default_value = "https://shard.tokenkickstarter.com")]
    pub endpoint: String,

    /// Print extrinsic hex only; do not submit.
    #[arg(long)]
    pub dry_run: bool,

    /// Skip `register_validator` (only run `force_rotate`).
    #[arg(long)]
    pub skip_register: bool,

    /// Override genesis hash (0x…) when `--rpc-url` is unavailable.
    #[arg(long)]
    pub genesis_hash: Option<String>,

    /// Override account nonce when `--rpc-url` is unavailable.
    #[arg(long)]
    pub nonce: Option<u32>,
}

impl CipherRegisterShardCmd {
    pub fn run(&self) -> sc_cli::Result<()> {
        let query_url = self.rpc_url.as_deref().unwrap_or(&self.submit_url);
        let seed = resolve_seed(
            self.seed_file.as_ref().map(|p| p.as_path()),
            self.seed.as_deref(),
        )?;
        let pair = ecdsa::Pair::from_legacy_string(seed.trim(), None);
        let account = from_secret_uri(&seed);
        let genesis_hash = match &self.genesis_hash {
            Some(h) => parse_hash(h).map_err(|e| sc_cli::Error::Input(e.into()))?,
            None => fetch_genesis_hash(query_url)?,
        };
        let mut nonce = match self.nonce {
            Some(n) => n,
            None => fetch_account_nonce(query_url, account)?,
        };

        println!("Signer (AccountId20): {account:?}");
        println!("Submit URL: {}", self.submit_url);
        println!("Genesis: {genesis_hash:?}");

        if !self.skip_register {
            let endpoint_bytes = self.endpoint.as_bytes().to_vec();
            let endpoint: BoundedVec<u8, tks_runtime::MaxEndpointLength> = endpoint_bytes
                .try_into()
                .map_err(|_| "endpoint too long")?;
            let call = RuntimeCall::ShardRegistry(ShardRegistryCall::register_validator {
                region: GeoRegion::NorthAmerica,
                endpoint,
            });
            let ext = encode_signed_extrinsic(call, &pair, account, nonce, genesis_hash)
                .map_err(|e| sc_cli::Error::Application(e.into()))?;
            submit_or_print(&self.submit_url, &ext, self.dry_run, "register_validator")?;
            nonce = nonce.saturating_add(1);
        }

        let call = RuntimeCall::ShardRegistry(ShardRegistryCall::force_rotate {});
        let ext = encode_signed_extrinsic(call, &pair, account, nonce, genesis_hash)
            .map_err(|e| sc_cli::Error::Application(e.into()))?;
        submit_or_print(&self.submit_url, &ext, self.dry_run, "force_rotate")?;

        Ok(())
    }
}

fn submit_or_print(
    url: &str,
    ext: &[u8],
    dry_run: bool,
    label: &str,
) -> sc_cli::Result<()> {
    let hex_ext = format!("0x{}", hex::encode(ext));
    if dry_run {
        println!("{label} extrinsic ({ } bytes): {hex_ext}", ext.len());
        return Ok(());
    }
    let hash = rpc_call(
        url,
        "author_submitExtrinsic",
        serde_json::json!([hex_ext]),
    )?;
    println!("{label} submitted: {hash}");
    Ok(())
}

fn resolve_seed(
    file: Option<&std::path::Path>,
    uri: Option<&str>,
) -> Result<String, sc_cli::Error> {
    match (file, uri) {
        (Some(path), None) => {
            let s = std::fs::read_to_string(path)
                .map_err(|e| sc_cli::Error::Input(format!("read {}: {e}", path.display())))?;
            Ok(s.lines().next().unwrap_or("").trim().to_string())
        }
        (None, Some(u)) => Ok(u.to_string()),
        (Some(_), Some(_)) => Err(sc_cli::Error::Input(
            "use either --seed-file or --seed, not both".into(),
        )),
        (None, None) => Err(sc_cli::Error::Input(
            "--seed-file is required (path to ECDSA secret URI)".into(),
        )),
    }
}

pub(crate) fn fetch_genesis_hash(url: &str) -> Result<Hash, sc_cli::Error> {
    let hex = rpc_call(url, "chain_getBlockHash", serde_json::json!([0]))?;
    let hex = hex.as_str().ok_or_else(|| sc_cli::Error::Input("bad genesis hash".into()))?;
    parse_hash(hex).map_err(|e| sc_cli::Error::Input(e.into()))
}

pub(crate) fn fetch_account_nonce(url: &str, account: AccountId20) -> Result<u32, sc_cli::Error> {
    let params = hex::encode(account.encode());
    let hex_params = format!("0x{}", params);
    let result = rpc_call(
        url,
        "state_call",
        serde_json::json!(["AccountNonceApi_account_nonce", hex_params, null]),
    )?;
    let hex = result
        .as_str()
        .ok_or_else(|| sc_cli::Error::Input("bad nonce response".into()))?;
    let bytes = hex::decode(hex.trim_start_matches("0x"))
        .map_err(|e| sc_cli::Error::Input(e.to_string()))?;
    u32::decode(&mut &bytes[..])
        .map_err(|e| sc_cli::Error::Input(format!("decode nonce: {e}")))
}

fn parse_hash(hex: &str) -> Result<Hash, String> {
    let hex = hex.strip_prefix("0x").unwrap_or(hex);
    let bytes = hex::decode(hex).map_err(|e| e.to_string())?;
    let bytes: [u8; 32] = bytes
        .try_into()
        .map_err(|_| "hash must be 32 bytes".to_string())?;
    Ok(Hash::from(H256::from(bytes)))
}

pub(crate) fn rpc_call(
    url: &str,
    method: &str,
    params: serde_json::Value,
) -> Result<serde_json::Value, sc_cli::Error> {
    let body = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": method,
        "params": params,
    });
    let resp: serde_json::Value = ureq::post(url)
        .set("Content-Type", "application/json")
        .send_json(body)
        .map_err(|e| sc_cli::Error::Application(format!("HTTP {method}: {e}").into()))?
        .into_json()
        .map_err(|e| sc_cli::Error::Application(format!("JSON {method}: {e}").into()))?;
    if let Some(err) = resp.get("error") {
        return Err(sc_cli::Error::Application(format!("RPC {method}: {err}").into()));
    }
    resp.get("result")
        .cloned()
        .ok_or_else(|| sc_cli::Error::Application(format!("RPC {method}: missing result").into()))
}
