//! # TKS Chain Specification
//!
//! Defines the genesis state for the TKS blockchain.
//! Includes NPoS staking, session keys, and treasury configuration.

use sc_service::ChainType;
use sp_consensus_aura::sr25519::AuthorityId as AuraId;
use sp_consensus_grandpa::AuthorityId as GrandpaId;
use sp_core::{sr25519, ByteArray, Pair, Public, H160};
use tks_runtime::{AccountId, Balance, SessionKeys, Signature, TKS};

/// Specialized `ChainSpec` for TKS (no extensions).
pub type ChainSpec = sc_service::GenericChainSpec;

/// Generate a crypto pair from seed.
pub fn get_from_seed<TPublic: Public>(seed: &str) -> <TPublic::Pair as Pair>::Public {
    TPublic::Pair::from_string(&format!("//{}", seed), None)
        .expect("static values are valid; qed")
        .public()
}

/// Helper to convert a hex string into an AccountId.
pub fn hex_to_account(hex: &str) -> AccountId {
    let hex = hex.strip_prefix("0x").unwrap_or(hex);
    let mut data = [0u8; 20];
    hex::decode_to_slice(hex, &mut data).expect("Invalid hex for AccountId20");
    AccountId::from(data)
}

/// Generate an account ID from seed (Mapping Substrate seeds to standard Ethereum H160).
pub fn get_account_id_from_seed(seed: &str) -> AccountId {
    match seed {
        "Alice" => hex_to_account("0xe04cc55ebee1cbce552f250e85c57b70b2e2625b"),
        "Bob" => hex_to_account("0x25451a4de12dccc2d166922fa938e900fcc4ed24"),
        "Charlie" => hex_to_account("0x5630a480727cd7799073b36472d9b1a6031f840b"),
        "Dave" => hex_to_account("0x4bb32a4263e369acbb6c020ffa89a41fd9722894"), 
        "Eve" => hex_to_account("0x362855f7c9c5c9d00a84157cdefe889fea436741"), 
        "Ferdie" => hex_to_account("0x0c8a57c77e50afc224f06caeeca12c46178b37c7"),
        _ => {
            let pubkey = get_from_seed::<sp_core::ecdsa::Public>(seed);
            let mut data = [0u8; 20];
            data.copy_from_slice(&pubkey.to_raw_vec()[0..20]);
            AccountId::from(data)
        }
    }
}

/// Generate Aura + GRANDPA authority keys AND a stash account from seed.
/// Returns (stash_account, aura_key, grandpa_key).
pub fn authority_keys_from_seed(s: &str) -> (AccountId, AuraId, GrandpaId) {
    (
        get_account_id_from_seed(s),
        get_from_seed::<AuraId>(s),
        get_from_seed::<GrandpaId>(s),
    )
}

/// Generate session keys struct from Aura + GRANDPA keys.
fn session_keys(aura: AuraId, grandpa: GrandpaId) -> SessionKeys {
    SessionKeys { aura, grandpa }
}

/// Development chain config (single-node, for testing).
pub fn development_config() -> Result<ChainSpec, String> {
    let mut properties = sc_service::Properties::new();
    properties.insert("tokenSymbol".into(), "TKS".into());
    properties.insert("tokenDecimals".into(), 18.into());
    properties.insert("ss58Format".into(), 42.into());

    Ok(ChainSpec::builder(
        tks_runtime::WASM_BINARY.ok_or_else(|| "Development wasm not available".to_string())?,
        None,
    )
    .with_name("TKS Development")
    .with_id("tks_dev")
    .with_chain_type(ChainType::Development)
    .with_properties(properties)
    .with_genesis_config_patch(testnet_genesis(
        // Initial validators (stash, aura, grandpa)
        vec![authority_keys_from_seed("Alice")],
        // Sudo account
        get_account_id_from_seed("Alice"),
        // ── DEV GENESIS — 20 Billion TKS total supply ──────────────────
        // Replace these with real wallet addresses for mainnet.
        // See network/GENESIS-AND-BRIDGE.md for mainnet allocation guide.
        vec![
            // Team / Foundation (10% = 2B TKS) — 4yr vest, 1yr cliff on mainnet
            (get_account_id_from_seed("Alice"),   2_000_000_000 * TKS),
            // Public Sale Reserve (50% = 10B TKS) — tiered presale rounds
            (get_account_id_from_seed("Bob"),    10_000_000_000 * TKS),
            // Bridge Reserve (15% = 3B TKS) — ETH/BSC migration pool
            (get_account_id_from_seed("Charlie"), 3_000_000_000 * TKS),
            // Ecosystem Fund (10% = 2B TKS) — grants, integrations
            (get_account_id_from_seed("Dave"),    2_000_000_000 * TKS),
            // Treasury on-chain (5% = 1B TKS) — governance controlled
            (get_account_id_from_seed("Eve"),     1_000_000_000 * TKS),
            // Airdrop + Community + Liquidity (10% = 2B TKS)
            (get_account_id_from_seed("Ferdie"),  2_000_000_000 * TKS),
        ],
        // ── Total: 20,000,000,000 TKS (20 Billion) ────────────────────
        vec![
            (b"alice".to_vec(), get_account_id_from_seed("Alice")),
            (b"bob".to_vec(),   get_account_id_from_seed("Bob")),
        ],
    ))
    .build())
}


/// Local testnet config (two validators for multi-node testing).
pub fn local_testnet_config() -> Result<ChainSpec, String> {
    let mut properties = sc_service::Properties::new();
    properties.insert("tokenSymbol".into(), "TKS".into());
    properties.insert("tokenDecimals".into(), 18.into());
    properties.insert("ss58Format".into(), 42.into());

    Ok(ChainSpec::builder(
        tks_runtime::WASM_BINARY.ok_or_else(|| "Local testnet wasm not available".to_string())?,
        None,
    )
    .with_name("TKS Local Testnet")
    .with_id("tks_local")
    .with_chain_type(ChainType::Local)
    .with_properties(properties)
    .with_genesis_config_patch(testnet_genesis(
        // Two initial validators
        vec![
            authority_keys_from_seed("Alice"),
            authority_keys_from_seed("Bob"),
        ],
        // Sudo account
        get_account_id_from_seed("Alice"),
        // Pre-funded accounts — 1 billion TKS total supply
        vec![
            (get_account_id_from_seed("Alice"), 400_000_000 * TKS),
            (get_account_id_from_seed("Bob"), 300_000_000 * TKS),
            (get_account_id_from_seed("Charlie"), 100_000_000 * TKS),
            (hex_to_account("0xEEb99F126Eb8C665675B8bd9652a969969696969"), 100_000_000 * TKS),
        ],
        // Pre-registered usernames
        vec![
            (b"alice".to_vec(), get_account_id_from_seed("Alice")),
            (b"bob".to_vec(), get_account_id_from_seed("Bob")),
        ],
    ))
    .build())
}

/// Configure initial storage state for genesis.
fn testnet_genesis(
    initial_authorities: Vec<(AccountId, AuraId, GrandpaId)>,
    root_key: AccountId,
    endowed_accounts: Vec<(AccountId, Balance)>,
    genesis_names: Vec<(Vec<u8>, AccountId)>,
) -> serde_json::Value {
    // Each validator bonds 100K TKS from their endowed balance
    let staking_bond: Balance = 100_000 * TKS;

    serde_json::json!({
        "balances": {
            "balances": endowed_accounts,
        },
        // Session pallet: map validators to their session keys
        "session": {
            "keys": initial_authorities
                .iter()
                .map(|(account, aura, grandpa)| {
                    (
                        account.clone(),                                    // validator stash
                        account.clone(),                                    // validator controller (same as stash)
                        session_keys(aura.clone(), grandpa.clone()),        // session keys
                    )
                })
                .collect::<Vec<_>>(),
        },
        // Staking pallet: bond validators with 100K TKS each
        "staking": {
            "validatorCount": initial_authorities.len() as u32,
            "minimumValidatorCount": 1,
            "stakers": initial_authorities
                .iter()
                .map(|(account, _, _)| {
                    (
                        account.clone(),        // stash account
                        account.clone(),        // controller account
                        staking_bond,           // bond amount: 100K TKS
                        "Validator",            // staker status
                    )
                })
                .collect::<Vec<_>>(),
            "invulnerables": initial_authorities
                .iter()
                .map(|(account, _, _)| account.clone())
                .collect::<Vec<_>>(),
            "forceEra": "NotForcing",
            // Validators must bond at least 100,000 TKS — enforced on-chain,
            // cannot be bypassed. Set here in genesis and updatable via sudo.
            "minValidatorBond": staking_bond,                  // 100,000 TKS
            // Nominators need at least 1,000 TKS to participate.
            "minNominatorBond": 1_000u128 * 1_000_000_000_000_000_000u128,  // 1,000 TKS
            "slashRewardFraction": 100_000_000u32, // 10% of slash goes to reporter (Perbill)
        },
        "sudo": {
            "key": Some(root_key),
        },
        "nameRegistry": {
            "names": genesis_names,
        },
        // EVM chain ID (for MetaMask: "Add Network" → Chain ID 7779)
        "evmChainId": {
            "chainId": 7779u64,
        },
        // Base fee (EIP-1559) — adaptive, starts at 1 Gwei
        "baseFee": {
            "baseFeePerGas": "0x3B9ACA00",
        },
        // ── EVM Genesis Accounts ────────────────────────────────────────
        // Pre-funded H160 accounts for EVM/MetaMask testing.
        // These use the standard Hardhat/Ganache dev private keys.
        //
        // ⚠️  DEVNET ONLY — replace with real addresses for mainnet.
        //
        // Account 0 — private key: 0xac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80
        //   MetaMask import → "Private Key" → paste the key above
        "evm": {
            "accounts": {
                // Hardhat dev account #0  (1,000 TKS)
                "0xf39Fd6e51aad88F6F4ce6aB8827279cffFb92266": {
                    "balance": "0x3635C9ADC5DEA00000",
                    "code": [],
                    "nonce": "0x0",
                    "storage": {}
                },
                // Hardhat dev account #1  (1,000 TKS)
                "0x70997970C51812dc3A010C7d01b50e0d17dc79C8": {
                    "balance": "0x3635C9ADC5DEA00000",
                    "code": [],
                    "nonce": "0x0",
                    "storage": {}
                },
                // Hardhat dev account #2  (1,000 TKS)
                "0x3C44CdDdB6a900fa2b585dd299e03d12FA4293BC": {
                    "balance": "0x3635C9ADC5DEA00000",
                    "code": [],
                    "nonce": "0x0",
                    "storage": {}
                },
                // TKS treasury EVM mirror  (100,000 TKS — for bridge testing)
                "0x15d34AAf54267DB7D7c367839AAf71A00a2C6A65": {
                    "balance": "0x152D02C7E14AF6800000",
                    "code": [],
                    "nonce": "0x0",
                    "storage": {}
                }
            }
        },
    })
}
#[cfg(test)]
mod tests {
    use super::*;
    use sp_core::{ecdsa, Pair, KeccakHasher, Hasher};

    #[test]
    fn calculate_eth_addresses() {
        let seeds = vec!["Alice", "Bob", "Charlie", "Dave", "Eve", "Ferdie"];
        for seed in seeds {
            let pair = ecdsa::Pair::from_string(&format!("//{}", seed), None).unwrap();
            let pubkey = pair.public();
            // Ethereum addresses are the last 20 bytes of the Keccak-256 hash of the uncompressed public key (minus the 0x04 prefix)
            let uncompressed = libsecp256k1::PublicKey::parse_slice(&pubkey.0, None).unwrap().serialize();
            let hash = KeccakHasher::hash(&uncompressed[1..65]);
            let address = &hash[12..];
            println!("{}: 0x{}", seed, hex::encode(address));
        }
    }
}
