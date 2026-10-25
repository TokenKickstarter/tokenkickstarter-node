//! Submit `sudo.sudo(force_rotate)` via 2-of-N multisig (production ops / smoke tests).

use codec::Encode;
use fp_account::AccountId20;

use crate::account_id20::from_secret_uri;
use pallet_multisig::Call as MultisigCall;
use pallet_shard_registry::Call as ShardRegistryCall;
use pallet_sudo::Call as SudoCall;
use sp_core::{ecdsa, hashing::blake2_256, Pair};
use tks_runtime::{Hash, RuntimeCall, VERSION};

use crate::extrinsic_sign::encode_signed_extrinsic;
use crate::genesis_util::multisig_account_id;
use crate::shard_setup_cmd::{fetch_account_nonce, fetch_genesis_hash, rpc_call};

#[derive(Debug, clap::Parser)]
pub struct MultisigSudoForceRotateCmd {
    #[arg(long, default_value = "http://127.0.0.1:9944")]
    pub rpc_url: String,

    #[arg(long)]
    pub member1_seed_file: std::path::PathBuf,

    #[arg(long)]
    pub member2_seed_file: std::path::PathBuf,

    #[arg(long)]
    pub member3_seed_file: Option<std::path::PathBuf>,

    #[arg(long, default_value_t = 2)]
    pub threshold: u16,
}

impl MultisigSudoForceRotateCmd {
    pub fn run(&self) -> sc_cli::Result<()> {
        let m1 = read_seed(&self.member1_seed_file)?;
        let m2 = read_seed(&self.member2_seed_file)?;
        let m3 = match &self.member3_seed_file {
            Some(path) => Some(read_seed(path)?),
            None => None,
        };

        let pair1 = ecdsa::Pair::from_legacy_string(&m1, None);
        let pair2 = ecdsa::Pair::from_legacy_string(&m2, None);
        let acc1 = from_secret_uri(&m1);
        let acc2 = from_secret_uri(&m2);
        let acc3 = m3.as_ref().map(|s| from_secret_uri(s));

        let mut signers = vec![acc1, acc2];
        if let Some(a) = acc3 {
            signers.push(a);
        }
        signers.sort_by(|a, b| a.0.cmp(&b.0));

        let multisig_addr = multisig_account_id(&signers, self.threshold);
        println!("Multisig account: {multisig_addr:?}");

        let genesis_hash = fetch_genesis_hash(&self.rpc_url)?;
        let inner = RuntimeCall::Sudo(SudoCall::sudo {
            call: Box::new(RuntimeCall::ShardRegistry(
                ShardRegistryCall::force_rotate {},
            )),
        });
        let call_bytes = inner.encode();
        let call_hash: [u8; 32] = blake2_256(&call_bytes);
        let max_weight = frame_support::weights::Weight::from_parts(5_000_000_000, 0);

        let other_for = |who: AccountId20| -> Vec<AccountId20> {
            signers
                .iter()
                .copied()
                .filter(|a| *a != who)
                .collect()
        };

        // Member 1: initial as_multi (stores pending approval).
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
            "member1 as_multi",
        )?;

        // Member 2: approve pending multisig (executes when threshold met).
        std::thread::sleep(std::time::Duration::from_secs(6));
        let nonce2 = fetch_account_nonce(&self.rpc_url, acc2)?;
        let call2 = RuntimeCall::Multisig(MultisigCall::approve_as_multi {
            threshold: self.threshold,
            other_signatories: other_for(acc2),
            maybe_timepoint: None,
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
            "member2 approve_as_multi",
        )?;

        println!(
            "Multisig sudo force_rotate submitted (spec_version={}).",
            VERSION.spec_version
        );
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
    label: &str,
) -> sc_cli::Result<()> {
    let ext = encode_signed_extrinsic(call, pair, account, nonce, genesis_hash)
        .map_err(|e| sc_cli::Error::Application(e.into()))?;
    let hex_ext = format!("0x{}", hex::encode(ext));
    let hash = rpc_call(
        url,
        "author_submitExtrinsic",
        serde_json::json!([hex_ext]),
    )?;
    println!("{label}: {hash}");
    Ok(())
}
