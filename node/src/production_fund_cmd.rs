//! Fund an H160 account via `Balances.transfer_allow_death` (Substrate extrinsic, not ETH JSON-RPC).

use fp_account::AccountId20;
use pallet_balances::Call as BalancesCall;
use sp_core::ecdsa;

use tks_runtime::{Balance, RuntimeCall, TKS};

use crate::extrinsic_sign::encode_signed_extrinsic;
use crate::shard_setup_cmd::{fetch_account_nonce, fetch_genesis_hash, rpc_call};

#[derive(Debug, clap::Parser)]
pub struct ProductionFundH160Cmd {
    #[arg(long, default_value = "http://127.0.0.1:9944")]
    pub rpc_url: String,

    #[arg(long)]
    pub seed_file: std::path::PathBuf,

    /// Recipient H160 (`0x…`).
    #[arg(long)]
    pub to: String,

    /// Whole TKS units (18 decimals).
    #[arg(long)]
    pub amount_tks: u64,
}

impl ProductionFundH160Cmd {
    pub fn run(&self) -> sc_cli::Result<()> {
        let phrase = read_seed(&self.seed_file)?;
        let (pair, from) = crate::ecdsa_seed::load_ecdsa_from_seed_text(&phrase)
            .map_err(|e| sc_cli::Error::Input(e.to_string()))?;
        let to = parse_h160_cli(&self.to)?;
        let value: Balance = (self.amount_tks as u128)
            .checked_mul(TKS)
            .ok_or_else(|| sc_cli::Error::Input("amount overflow".into()))?;

        let genesis_hash = fetch_genesis_hash(&self.rpc_url)?;
        let nonce = fetch_account_nonce(&self.rpc_url, from)?;
        let call = RuntimeCall::Balances(BalancesCall::transfer_allow_death {
            dest: to,
            value,
        });
        submit(&self.rpc_url, &pair, from, call, nonce, genesis_hash, &self.to)
    }
}

fn read_seed(path: &std::path::Path) -> Result<String, sc_cli::Error> {
    let s = std::fs::read_to_string(path)
        .map_err(|e| sc_cli::Error::Input(format!("read {}: {e}", path.display())))?;
    let trimmed = s.trim();
    if trimmed.starts_with('{') {
        let v: serde_json::Value = serde_json::from_str(trimmed)
            .map_err(|e| sc_cli::Error::Input(format!("json in {}: {e}", path.display())))?;
        return v
            .get("secretPhrase")
            .or_else(|| v.get("secretSeed"))
            .and_then(|x| x.as_str())
            .map(|x| x.to_string())
            .ok_or_else(|| sc_cli::Error::Input(format!("no secretPhrase in {}", path.display())));
    }
    Ok(trimmed.lines().next().unwrap_or("").trim().to_string())
}

fn submit(
    url: &str,
    pair: &ecdsa::Pair,
    account: AccountId20,
    call: RuntimeCall,
    nonce: u32,
    genesis_hash: tks_runtime::Hash,
    label: &str,
) -> sc_cli::Result<()> {
    let ext = encode_signed_extrinsic(call, pair, account, nonce, genesis_hash)
        .map_err(|e| sc_cli::Error::Application(e.into()))?;
    let hex_ext = format!("0x{}", hex::encode(ext));
    let hash = rpc_call(url, "author_submitExtrinsic", serde_json::json!([hex_ext]))?;
    println!("Funded {label}: {hash}");
    Ok(())
}

fn parse_h160_cli(hex: &str) -> Result<AccountId20, sc_cli::Error> {
    let hex = hex.strip_prefix("0x").unwrap_or(hex);
    let bytes = hex::decode(hex).map_err(|e| sc_cli::Error::Input(e.to_string()))?;
    let bytes: [u8; 20] = bytes
        .try_into()
        .map_err(|_| sc_cli::Error::Input("address must be 20 bytes".into()))?;
    Ok(AccountId20::from(bytes))
}
