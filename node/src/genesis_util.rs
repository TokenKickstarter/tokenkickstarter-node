//! Helpers for offline production genesis assembly (AccountId20 + multisig sudo).

use codec::{Decode, Encode};

#[derive(Debug, clap::Subcommand)]
pub enum ProductionCmd {
    /// Map a secret URI to the runtime `AccountId20` (H160) used on TKS.
    AccountId20(AccountId20FromUriCmd),
    /// Derive `pallet-multisig` account ID for sudo (2-of-N or higher).
    MultisigAccount(MultisigAccountCmd),
    /// Smoke test: 2-of-3 multisig dispatches `sudo(force_rotate)`.
    MultisigSudoForceRotate(crate::multisig_sudo_cmd::MultisigSudoForceRotateCmd),
}
use fp_account::AccountId20;
use sp_core::hashing::blake2_256;
use sp_runtime::traits::TrailingZeroInput;

use crate::chain_spec;

#[derive(Debug, clap::Parser)]
pub struct AccountId20FromUriCmd {
    /// Secret URI (mnemonic or `//` dev seed — production must use fresh mnemonics).
    pub uri: String,
}

impl AccountId20FromUriCmd {
    pub fn run(&self) -> sc_cli::Result<()> {
        let id = chain_spec::get_account_id_from_seed(self.uri.trim());
        println!("0x{}", hex::encode(id.0));
        Ok(())
    }
}

#[derive(Debug, clap::Parser)]
pub struct MultisigAccountCmd {
    /// Signatories as comma-separated `0x` H160 addresses (2–100).
    #[arg(long, value_delimiter = ',')]
    pub signatories: Vec<String>,

    /// M-of-N threshold (`pallet-multisig` requires ≥ 2).
    #[arg(long)]
    pub threshold: u16,
}

impl MultisigAccountCmd {
    pub fn run(&self) -> sc_cli::Result<()> {
        if self.threshold < 2 {
            return Err(sc_cli::Error::Input(
                "threshold must be >= 2 for on-chain multisig sudo".into(),
            ));
        }
        if self.signatories.len() < self.threshold as usize {
            return Err(sc_cli::Error::Input(
                "need at least `threshold` signatories".into(),
            ));
        }
        let mut accounts: Vec<AccountId20> = self
            .signatories
            .iter()
            .map(|s| parse_h160(s))
            .collect::<Result<Vec<_>, _>>()?;
        accounts.sort_by(|a, b| a.0.cmp(&b.0));
        let id = multisig_account_id(&accounts, self.threshold);
        println!("0x{}", hex::encode(id.0));
        let legacy = multisig_account_id_legacy_str_prefix(&accounts, self.threshold);
        if legacy != id {
            eprintln!(
                "Note: legacy genesis helper (SCALE `&str` prefix) was 0x{} — on-chain \
                 `sudo.key` on spec 412 may still match that until genesis regen.",
                hex::encode(legacy.0)
            );
        }
        Ok(())
    }
}

/// Same as `pallet_multisig::Pallet::multi_account_id` for `AccountId20` signatories.
pub fn multisig_account_id(signatories: &[AccountId20], threshold: u16) -> AccountId20 {
    let entropy = (b"modlpy/utilisuba", signatories, threshold).using_encoded(blake2_256);
    AccountId20::decode(&mut TrailingZeroInput::new(entropy.as_ref()))
        .expect("multisig AccountId20 decodes from blake2_256; qed")
}

/// Pre-v0.1.15 bug: used `("modlpy/utilisuba", …).encode()` (compact string prefix).
pub fn multisig_account_id_legacy_str_prefix(
    signatories: &[AccountId20],
    threshold: u16,
) -> AccountId20 {
    let entropy = blake2_256(&("modlpy/utilisuba", signatories, threshold).encode());
    AccountId20::decode(&mut TrailingZeroInput::new(entropy.as_ref()))
        .expect("legacy multisig AccountId20; qed")
}

fn parse_h160(hex: &str) -> Result<AccountId20, sc_cli::Error> {
    let hex = hex.strip_prefix("0x").unwrap_or(hex);
    let bytes = hex::decode(hex).map_err(|e| sc_cli::Error::Input(e.to_string()))?;
    let bytes: [u8; 20] = bytes
        .try_into()
        .map_err(|_| sc_cli::Error::Input("address must be 20 bytes".into()))?;
    Ok(AccountId20::from(bytes))
}
