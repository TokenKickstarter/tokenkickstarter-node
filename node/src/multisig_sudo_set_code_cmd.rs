//! Submit `sudo.sudo(system.set_code(wasm))` via 2-of-N multisig (runtime upgrade).

use codec::Encode;
use fp_account::AccountId20;
use frame_system::Call as SystemCall;
use pallet_multisig::Call as MultisigCall;
use pallet_sudo::Call as SudoCall;
use sp_core::{ecdsa, hashing::blake2_256, Pair};
use tks_runtime::{Hash, RuntimeCall, VERSION};

use crate::extrinsic_sign::encode_signed_extrinsic;
use crate::genesis_util::multisig_account_id;
use crate::multisig_storage::wait_for_multisig_timepoint;
use crate::shard_setup_cmd::{
    fetch_account_nonce, fetch_genesis_hash, fetch_runtime_version, rpc_call,
};

#[derive(Debug, clap::Parser)]
pub struct MultisigSudoSetCodeCmd {
    #[arg(long, default_value = "http://127.0.0.1:9944")]
    pub rpc_url: String,

    /// Compressed WASM produced by `cargo build -p tks-chain-node` (`.compact.compressed.wasm`).
    #[arg(long)]
    pub wasm_file: std::path::PathBuf,

    #[arg(long)]
    pub member1_seed_file: std::path::PathBuf,

    #[arg(long)]
    pub member2_seed_file: std::path::PathBuf,

    #[arg(long)]
    pub member3_seed_file: Option<std::path::PathBuf>,

    #[arg(long, default_value_t = 2)]
    pub threshold: u16,
}

impl MultisigSudoSetCodeCmd {
    pub fn run(&self) -> sc_cli::Result<()> {
        let wasm = std::fs::read(&self.wasm_file).map_err(|e| {
            sc_cli::Error::Input(format!("read wasm {}: {e}", self.wasm_file.display()))
        })?;
        if wasm.is_empty() {
            return Err(sc_cli::Error::Input("wasm file is empty".into()));
        }
        println!(
            "WASM {} bytes from {}",
            wasm.len(),
            self.wasm_file.display()
        );
        println!(
            "Built-in runtime spec_version={} (must match on-chain after upgrade).",
            VERSION.spec_version
        );

        let m1 = read_seed(&self.member1_seed_file)?;
        let m2 = read_seed(&self.member2_seed_file)?;
        let m3 = match &self.member3_seed_file {
            Some(path) => Some(read_seed(path)?),
            None => None,
        };

        let (pair1, acc1) = crate::ecdsa_seed::load_ecdsa_from_seed_text(&m1)
            .map_err(|e| sc_cli::Error::Input(e.to_string()))?;
        let (pair2, acc2) = crate::ecdsa_seed::load_ecdsa_from_seed_text(&m2)
            .map_err(|e| sc_cli::Error::Input(e.to_string()))?;
        let acc3 = if let Some(s) = m3.as_ref() {
            Some(
                crate::ecdsa_seed::load_ecdsa_from_seed_text(s)
                    .map_err(|e| sc_cli::Error::Input(e.to_string()))?
                    .1,
            )
        } else {
            None
        };

        let mut signers = vec![acc1, acc2];
        if let Some(a) = acc3 {
            signers.push(a);
        }
        signers.sort_by(|a, b| a.0.cmp(&b.0));

        let multisig_addr = multisig_account_id(&signers, self.threshold);
        println!("Multisig account: {multisig_addr:?}");

        let genesis_hash = fetch_genesis_hash(&self.rpc_url)?;
        let (spec_version, transaction_version) = fetch_runtime_version(&self.rpc_url)?;
        println!(
            "On-chain spec_version={spec_version} tx_version={transaction_version} (signing with these, not binary VERSION)."
        );
        let inner = RuntimeCall::Sudo(SudoCall::sudo {
            call: Box::new(RuntimeCall::System(SystemCall::set_code { code: wasm })),
        });
        let call_bytes = inner.encode();
        let call_hash: [u8; 32] = blake2_256(&call_bytes);
        // set_code can be heavy; allow large ref-time budget.
        let max_weight = frame_support::weights::Weight::from_parts(2_000_000_000_000, 0);

        let other_for = |who: AccountId20| -> Vec<AccountId20> {
            signers.iter().copied().filter(|a| *a != who).collect()
        };

        let nonce1 = fetch_account_nonce(&self.rpc_url, acc1)?;
        let call1 = RuntimeCall::Multisig(MultisigCall::as_multi {
            threshold: self.threshold,
            other_signatories: other_for(acc1),
            maybe_timepoint: None,
            call: Box::new(inner.clone()),
            max_weight,
        });
        submit(
            &self.rpc_url,
            &pair1,
            acc1,
            call1,
            nonce1,
            genesis_hash,
            spec_version,
            transaction_version,
            "member1 as_multi set_code",
        )?;

        let timepoint = wait_for_multisig_timepoint(&self.rpc_url, multisig_addr, &call_hash, 90)?;
        let nonce2 = fetch_account_nonce(&self.rpc_url, acc2)?;
        let call2 = RuntimeCall::Multisig(MultisigCall::approve_as_multi {
            threshold: self.threshold,
            other_signatories: other_for(acc2),
            maybe_timepoint: Some(timepoint),
            call_hash,
            max_weight,
        });
        submit(
            &self.rpc_url,
            &pair2,
            acc2,
            call2,
            nonce2,
            genesis_hash,
            spec_version,
            transaction_version,
            "member2 approve_as_multi set_code",
        )?;

        println!("Multisig sudo set_code submitted.");
        Ok(())
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
    genesis_hash: Hash,
    spec_version: u32,
    transaction_version: u32,
    label: &str,
) -> sc_cli::Result<()> {
    let ext = encode_signed_extrinsic(
        call,
        pair,
        account,
        nonce,
        genesis_hash,
        spec_version,
        transaction_version,
    )
    .map_err(|e| sc_cli::Error::Application(e.into()))?;
    let hex_ext = format!("0x{}", hex::encode(ext));
    let hash = rpc_call(url, "author_submitExtrinsic", serde_json::json!([hex_ext]))?;
    println!("{label}: {hash}");
    Ok(())
}
