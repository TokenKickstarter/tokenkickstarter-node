//! # TKS Chain Specification
//!
//! Defines the genesis state for the TKS blockchain.

use sc_service::ChainType;
use sp_consensus_aura::sr25519::AuthorityId as AuraId;
use sp_consensus_grandpa::AuthorityId as GrandpaId;
use sp_core::{sr25519, Pair, Public};
use sp_runtime::traits::{IdentifyAccount, Verify};
use tks_runtime::{AccountId, Balance, Signature, TKS};

/// Specialized `ChainSpec` for TKS (no extensions).
pub type ChainSpec = sc_service::GenericChainSpec;

/// Generate a crypto pair from seed.
pub fn get_from_seed<TPublic: Public>(seed: &str) -> <TPublic::Pair as Pair>::Public {
    TPublic::Pair::from_string(&format!("//{}", seed), None)
        .expect("static values are valid; qed")
        .public()
}

type AccountPublic = <Signature as Verify>::Signer;

/// Generate an account ID from seed.
pub fn get_account_id_from_seed<TPublic: Public>(seed: &str) -> AccountId
where
    AccountPublic: From<<TPublic::Pair as Pair>::Public>,
{
    AccountPublic::from(get_from_seed::<TPublic>(seed)).into_account()
}

/// Generate an Aura authority key.
pub fn authority_keys_from_seed(s: &str) -> (AuraId, GrandpaId) {
    (
        get_from_seed::<AuraId>(s),
        get_from_seed::<GrandpaId>(s),
    )
}

/// Development chain config (single-node, instant seal for testing).
pub fn development_config() -> Result<ChainSpec, String> {
    Ok(ChainSpec::builder(
        tks_runtime::WASM_BINARY.ok_or_else(|| "Development wasm not available".to_string())?,
        None,
    )
    .with_name("TKS Development")
    .with_id("tks_dev")
    .with_chain_type(ChainType::Development)
    .with_genesis_config_patch(testnet_genesis(
        // Initial Aura/GRANDPA authorities
        vec![authority_keys_from_seed("Alice")],
        // Sudo account
        get_account_id_from_seed::<sr25519::Public>("Alice"),
        // Pre-funded accounts
        vec![
            (get_account_id_from_seed::<sr25519::Public>("Alice"), 500_000_000 * TKS),
            (get_account_id_from_seed::<sr25519::Public>("Bob"), 200_000_000 * TKS),
            (get_account_id_from_seed::<sr25519::Public>("Charlie"), 100_000_000 * TKS),
            (get_account_id_from_seed::<sr25519::Public>("Dave"), 100_000_000 * TKS),
            (get_account_id_from_seed::<sr25519::Public>("Eve"), 50_000_000 * TKS),
            (get_account_id_from_seed::<sr25519::Public>("Ferdie"), 50_000_000 * TKS),
        ],
        // Pre-registered usernames
        vec![
            (b"alice".to_vec(), get_account_id_from_seed::<sr25519::Public>("Alice")),
            (b"bob".to_vec(), get_account_id_from_seed::<sr25519::Public>("Bob")),
        ],
    ))
    .build())
}

/// Local testnet config (two validators for multi-node testing).
pub fn local_testnet_config() -> Result<ChainSpec, String> {
    Ok(ChainSpec::builder(
        tks_runtime::WASM_BINARY.ok_or_else(|| "Local testnet wasm not available".to_string())?,
        None,
    )
    .with_name("TKS Local Testnet")
    .with_id("tks_local")
    .with_chain_type(ChainType::Local)
    .with_genesis_config_patch(testnet_genesis(
        // Two initial authorities
        vec![
            authority_keys_from_seed("Alice"),
            authority_keys_from_seed("Bob"),
        ],
        // Sudo account
        get_account_id_from_seed::<sr25519::Public>("Alice"),
        // Pre-funded accounts — 1 billion TKS total supply
        vec![
            (get_account_id_from_seed::<sr25519::Public>("Alice"), 400_000_000 * TKS),
            (get_account_id_from_seed::<sr25519::Public>("Bob"), 300_000_000 * TKS),
            (get_account_id_from_seed::<sr25519::Public>("Charlie"), 100_000_000 * TKS),
            (get_account_id_from_seed::<sr25519::Public>("Dave"), 100_000_000 * TKS),
            (get_account_id_from_seed::<sr25519::Public>("Eve"), 50_000_000 * TKS),
            (get_account_id_from_seed::<sr25519::Public>("Ferdie"), 50_000_000 * TKS),
        ],
        // Pre-registered usernames
        vec![
            (b"alice".to_vec(), get_account_id_from_seed::<sr25519::Public>("Alice")),
            (b"bob".to_vec(), get_account_id_from_seed::<sr25519::Public>("Bob")),
        ],
    ))
    .build())
}

/// Configure initial storage state for genesis.
fn testnet_genesis(
    initial_authorities: Vec<(AuraId, GrandpaId)>,
    root_key: AccountId,
    endowed_accounts: Vec<(AccountId, Balance)>,
    genesis_names: Vec<(Vec<u8>, AccountId)>,
) -> serde_json::Value {
    serde_json::json!({
        "balances": {
            "balances": endowed_accounts,
        },
        "aura": {
            "authorities": initial_authorities.iter().map(|x| x.0.clone()).collect::<Vec<_>>(),
        },
        "grandpa": {
            "authorities": initial_authorities
                .iter()
                .map(|x| (x.1.clone(), 1))
                .collect::<Vec<_>>(),
        },
        "sudo": {
            "key": Some(root_key),
        },
        "nameRegistry": {
            "names": genesis_names,
        },
    })
}
