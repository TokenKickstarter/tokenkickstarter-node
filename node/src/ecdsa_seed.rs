//! Load ECDSA signers from operator seed files (hex seed or JSON `secretPhrase`).

use sc_cli::Error as CliError;
use sp_core::{ecdsa, Pair};
use std::path::Path;

use crate::account_id20::from_secret_uri;

/// Read a one-line seed file or JSON key export and return `(pair, AccountId20)`.
pub fn load_ecdsa_from_seed_file(path: &Path) -> Result<(ecdsa::Pair, fp_account::AccountId20), CliError> {
    let raw = std::fs::read_to_string(path).map_err(|e| {
        CliError::Input(format!("cannot read {}: {e}", path.display()))
    })?;
    load_ecdsa_from_seed_text(&raw)
}

/// Parse seed text: JSON `{ "secretPhrase": "..." }`, `//URI`, or raw hex secret seed.
pub fn load_ecdsa_from_seed_text(raw: &str) -> Result<(ecdsa::Pair, fp_account::AccountId20), CliError> {
    let trimmed = raw.trim();
    let uri = if trimmed.starts_with('{') {
        let v: serde_json::Value = serde_json::from_str(trimmed).map_err(|e| {
            CliError::Input(format!("invalid JSON seed: {e}"))
        })?;
        v.get("secretPhrase")
            .or_else(|| v.get("secretSeed"))
            .and_then(|x| x.as_str())
            .map(|s| s.to_string())
            .ok_or_else(|| CliError::Input("seed JSON missing secretPhrase/secretSeed".into()))?
    } else {
        trimmed.to_string()
    };

    let account = from_secret_uri(&uri);
    let pair = if uri.starts_with("//") {
        ecdsa::Pair::from_string(&uri, None).map_err(|e| CliError::Input(format!("ecdsa URI: {e:?}")))?
    } else {
        ecdsa::Pair::from_legacy_string(&uri, None)
    };

    Ok((pair, account))
}
