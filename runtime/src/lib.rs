//! # TKS Runtime
//!
//! The TokenKickstarter (TKS) Substrate runtime.
//! - **Consensus**: Aura (block authoring) + GRANDPA (finality)
//! - **Token**: TKS with 18 decimals, 1 billion total supply at genesis
//! - **Custom pallet**: `pallet-name-registry` for @username registration
//! - **Governance**: Sudo (will be replaced by on-chain governance in Phase 5)

#![cfg_attr(not(feature = "std"), no_std)]
#![recursion_limit = "256"]

// Re-export the WASM binary (built by build.rs)
#[cfg(feature = "std")]
include!(concat!(env!("OUT_DIR"), "/wasm_binary.rs"));

extern crate alloc;
use alloc::vec::Vec;

use frame_support::{
    construct_runtime,
    derive_impl,
    parameter_types,
    traits::{ConstBool, ConstU32, ConstU64, ConstU8},
    weights::{
        constants::RocksDbWeight,
        IdentityFee, Weight,
    },
};
use pallet_grandpa::AuthorityId as GrandpaId;
use sp_api::impl_runtime_apis;
use sp_consensus_aura::sr25519::AuthorityId as AuraId;
use sp_core::{crypto::KeyTypeId, OpaqueMetadata};
use alloc::borrow::Cow;
use sp_runtime::{
    generic, impl_opaque_keys,
    traits::{AccountIdLookup, BlakeTwo256, Block as BlockT, IdentifyAccount, Verify},
    transaction_validity::{TransactionSource, TransactionValidity},
    ApplyExtrinsicResult, MultiSignature,
};
use sp_version::RuntimeVersion;

/// Alias for the signature type used by accounts.
pub type Signature = MultiSignature;

/// Account ID type (derived from public key).
pub type AccountId = <<Signature as Verify>::Signer as IdentifyAccount>::AccountId;

/// Balance type — u128 to match TKS ERC-20 (18 decimals).
pub type Balance = u128;

/// Index/nonce of a transaction.
pub type Nonce = u32;

/// Block header hash type.
pub type Hash = sp_core::H256;

/// Block number type.
pub type BlockNumber = u32;

/// The address format for the runtime (AccountId-based).
pub type Address = sp_runtime::MultiAddress<AccountId, ()>;

/// Block header type.
pub type Header = generic::Header<BlockNumber, BlakeTwo256>;

/// Opaque block type (used by the node).
pub type Block = generic::Block<Header, UncheckedExtrinsic>;

/// The SignedExtension to the basic transaction logic.
pub type SignedExtra = (
    frame_system::CheckNonZeroSender<Runtime>,
    frame_system::CheckSpecVersion<Runtime>,
    frame_system::CheckTxVersion<Runtime>,
    frame_system::CheckGenesis<Runtime>,
    frame_system::CheckEra<Runtime>,
    frame_system::CheckNonce<Runtime>,
    frame_system::CheckWeight<Runtime>,
    pallet_transaction_payment::ChargeTransactionPayment<Runtime>,
);

/// Unchecked extrinsic type.
pub type UncheckedExtrinsic =
    generic::UncheckedExtrinsic<Address, RuntimeCall, Signature, SignedExtra>;

/// Executive type (dispatches extrinsics).
pub type Executive = frame_executive::Executive<
    Runtime,
    Block,
    frame_system::ChainContext<Runtime>,
    Runtime,
    AllPalletsWithSystem,
>;

// ─── Runtime Version ───────────────────────────────────────────────

#[sp_version::runtime_version]
pub const VERSION: RuntimeVersion = RuntimeVersion {
    spec_name: Cow::Borrowed("tks-chain"),
    impl_name: Cow::Borrowed("tks-chain"),
    authoring_version: 1,
    spec_version: 100,
    impl_version: 1,
    apis: RUNTIME_API_VERSIONS,
    transaction_version: 1,
    system_version: 1,
};

// ─── Constants ─────────────────────────────────────────────────────

/// 1 TKS = 10^18 smallest units (same as ETH/ERC-20).
pub const TKS: Balance = 1_000_000_000_000_000_000;
pub const MILLI_TKS: Balance = TKS / 1_000;
pub const MICRO_TKS: Balance = TKS / 1_000_000;

/// Block time: 6 seconds (same as Polkadot).
pub const MILLISECS_PER_BLOCK: u64 = 6000;
pub const SLOT_DURATION: u64 = MILLISECS_PER_BLOCK;
pub const MINUTES: BlockNumber = 60_000 / (MILLISECS_PER_BLOCK as BlockNumber);
pub const HOURS: BlockNumber = MINUTES * 60;
pub const DAYS: BlockNumber = HOURS * 24;

/// The version for the genesis state.
#[cfg(feature = "std")]
pub fn native_version() -> sp_version::NativeVersion {
    sp_version::NativeVersion {
        runtime_version: VERSION,
        can_author_with: Default::default(),
    }
}

// ─── Frame System ──────────────────────────────────────────────────

const NORMAL_DISPATCH_RATIO: sp_runtime::Perbill = sp_runtime::Perbill::from_percent(75);

parameter_types! {
    pub const BlockHashCount: BlockNumber = 2400;
    pub const Version: RuntimeVersion = VERSION;
    pub BlockWeights: frame_system::limits::BlockWeights =
        frame_system::limits::BlockWeights::with_sensible_defaults(
            Weight::from_parts(2u64 * frame_support::weights::constants::WEIGHT_REF_TIME_PER_SECOND, u64::MAX),
            NORMAL_DISPATCH_RATIO,
        );
    pub BlockLength: frame_system::limits::BlockLength =
        frame_system::limits::BlockLength::max_with_normal_ratio(
            5 * 1024 * 1024,
            NORMAL_DISPATCH_RATIO,
        );
    pub const SS58Prefix: u8 = 42; // Generic Substrate SS58 prefix
}

#[derive_impl(frame_system::config_preludes::SolochainDefaultConfig)]
impl frame_system::Config for Runtime {
    type BaseCallFilter = frame_support::traits::Everything;
    type BlockWeights = BlockWeights;
    type BlockLength = BlockLength;
    type RuntimeOrigin = RuntimeOrigin;
    type RuntimeCall = RuntimeCall;
    type Nonce = Nonce;
    type Hash = Hash;
    type Hashing = BlakeTwo256;
    type AccountId = AccountId;
    type Lookup = AccountIdLookup<AccountId, ()>;
    type Block = Block;
    type RuntimeEvent = RuntimeEvent;
    type BlockHashCount = BlockHashCount;
    type DbWeight = RocksDbWeight;
    type Version = Version;
    type PalletInfo = PalletInfo;
    type AccountData = pallet_balances::AccountData<Balance>;
    type OnNewAccount = ();
    type OnKilledAccount = ();
    type SystemWeightInfo = ();
    type SS58Prefix = SS58Prefix;
    type OnSetCode = ();
    type MaxConsumers = ConstU32<16>;
}

// ─── Aura (Block Authoring) ────────────────────────────────────────

impl pallet_aura::Config for Runtime {
    type AuthorityId = AuraId;
    type DisabledValidators = ();
    type MaxAuthorities = ConstU32<32>;
    type AllowMultipleBlocksPerSlot = ConstBool<false>;
    type SlotDuration = pallet_aura::MinimumPeriodTimesTwo<Runtime>;
}

// ─── GRANDPA (Finality) ────────────────────────────────────────────

impl pallet_grandpa::Config for Runtime {
    type RuntimeEvent = RuntimeEvent;
    type WeightInfo = ();
    type MaxAuthorities = ConstU32<32>;
    type MaxNominators = ConstU32<0>;
    type MaxSetIdSessionEntries = ConstU64<0>;
    type KeyOwnerProof = sp_core::Void;
    type EquivocationReportSystem = ();
}

// ─── Timestamp ─────────────────────────────────────────────────────

parameter_types! {
    pub const MinimumPeriod: u64 = SLOT_DURATION / 2;
}

impl pallet_timestamp::Config for Runtime {
    type Moment = u64;
    type OnTimestampSet = Aura;
    type MinimumPeriod = MinimumPeriod;
    type WeightInfo = ();
}

// ─── Balances (TKS Token) ──────────────────────────────────────────

parameter_types! {
    pub const ExistentialDeposit: Balance = MILLI_TKS; // 0.001 TKS
}

impl pallet_balances::Config for Runtime {
    type MaxLocks = ConstU32<50>;
    type MaxReserves = ConstU32<50>;
    type ReserveIdentifier = [u8; 8];
    type Balance = Balance;
    type RuntimeEvent = RuntimeEvent;
    type DustRemoval = ();
    type ExistentialDeposit = ExistentialDeposit;
    type AccountStore = System;
    type WeightInfo = pallet_balances::weights::SubstrateWeight<Runtime>;
    type FreezeIdentifier = ();
    type MaxFreezes = ConstU32<0>;
    type RuntimeHoldReason = ();
    type RuntimeFreezeReason = ();
    type DoneSlashHandler = ();
}

// ─── Transaction Payment ───────────────────────────────────────────

impl pallet_transaction_payment::Config for Runtime {
    type RuntimeEvent = RuntimeEvent;
    type OnChargeTransaction =
        pallet_transaction_payment::FungibleAdapter<Balances, ()>;
    type OperationalFeeMultiplier = ConstU8<5>;
    type WeightToFee = IdentityFee<Balance>;
    type LengthToFee = IdentityFee<Balance>;
    type FeeMultiplierUpdate = ();
    type WeightInfo = pallet_transaction_payment::weights::SubstrateWeight<Runtime>;
}

// ─── Sudo (Temporary Governance) ───────────────────────────────────

impl pallet_sudo::Config for Runtime {
    type RuntimeEvent = RuntimeEvent;
    type RuntimeCall = RuntimeCall;
    type WeightInfo = pallet_sudo::weights::SubstrateWeight<Runtime>;
}

// ─── Name Registry (TKS Username Pallet) ───────────────────────────

parameter_types! {
    pub const MaxNameLength: u32 = 32;
    pub const MinNameLength: u32 = 3;
    pub const RegistrationFee: Balance = 10 * TKS; // 10 TKS per username
    pub TreasuryAccountId: Option<AccountId> = None; // Fees are burned for now
}

impl pallet_name_registry::Config for Runtime {
    type Currency = Balances;
    type MaxNameLength = MaxNameLength;
    type MinNameLength = MinNameLength;
    type RegistrationFee = RegistrationFee;
    type TreasuryAccount = TreasuryAccountId;
}

// ─── Construct Runtime ─────────────────────────────────────────────

construct_runtime!(
    pub struct Runtime {
        // Core
        System: frame_system,
        Timestamp: pallet_timestamp,

        // Consensus
        Aura: pallet_aura,
        Grandpa: pallet_grandpa,

        // Monetary
        Balances: pallet_balances,
        TransactionPayment: pallet_transaction_payment,

        // Governance (temporary)
        Sudo: pallet_sudo,

        // TKS Custom
        NameRegistry: pallet_name_registry,
    }
);

// ─── Session Keys ──────────────────────────────────────────────────

impl_opaque_keys! {
    pub struct SessionKeys {
        pub aura: Aura,
        pub grandpa: Grandpa,
    }
}

// ─── Runtime APIs ──────────────────────────────────────────────────

impl_runtime_apis! {
    impl sp_api::Core<Block> for Runtime {
        fn version() -> RuntimeVersion {
            VERSION
        }
        fn execute_block(block: <Block as BlockT>::LazyBlock) {
            Executive::execute_block(block.into());
        }
        fn initialize_block(header: &<Block as BlockT>::Header) -> sp_runtime::ExtrinsicInclusionMode {
            Executive::initialize_block(header)
        }
    }

    impl sp_api::Metadata<Block> for Runtime {
        fn metadata() -> OpaqueMetadata {
            OpaqueMetadata::new(Runtime::metadata().into())
        }
        fn metadata_at_version(version: u32) -> Option<OpaqueMetadata> {
            Runtime::metadata_at_version(version)
        }
        fn metadata_versions() -> alloc::vec::Vec<u32> {
            Runtime::metadata_versions()
        }
    }

    impl sp_block_builder::BlockBuilder<Block> for Runtime {
        fn apply_extrinsic(extrinsic: <Block as BlockT>::Extrinsic) -> ApplyExtrinsicResult {
            Executive::apply_extrinsic(extrinsic)
        }
        fn finalize_block() -> <Block as BlockT>::Header {
            Executive::finalize_block()
        }
        fn inherent_extrinsics(data: sp_inherents::InherentData) -> Vec<<Block as BlockT>::Extrinsic> {
            data.create_extrinsics()
        }
        fn check_inherents(
            block: <Block as BlockT>::LazyBlock,
            data: sp_inherents::InherentData,
        ) -> sp_inherents::CheckInherentsResult {
            data.check_extrinsics(&block.into())
        }
    }

    impl sp_transaction_pool::runtime_api::TaggedTransactionQueue<Block> for Runtime {
        fn validate_transaction(
            source: TransactionSource,
            tx: <Block as BlockT>::Extrinsic,
            block_hash: <Block as BlockT>::Hash,
        ) -> TransactionValidity {
            Executive::validate_transaction(source, tx, block_hash)
        }
    }

    impl sp_offchain::OffchainWorkerApi<Block> for Runtime {
        fn offchain_worker(header: &<Block as BlockT>::Header) {
            Executive::offchain_worker(header)
        }
    }

    impl sp_consensus_aura::AuraApi<Block, AuraId> for Runtime {
        fn slot_duration() -> sp_consensus_aura::SlotDuration {
            sp_consensus_aura::SlotDuration::from_millis(Aura::slot_duration())
        }
        fn authorities() -> Vec<AuraId> {
            pallet_aura::Authorities::<Runtime>::get().into_inner()
        }
    }

    impl sp_session::SessionKeys<Block> for Runtime {
        fn generate_session_keys(seed: Option<Vec<u8>>) -> Vec<u8> {
            SessionKeys::generate(seed)
        }
        fn decode_session_keys(encoded: Vec<u8>) -> Option<Vec<(Vec<u8>, KeyTypeId)>> {
            SessionKeys::decode_into_raw_public_keys(&encoded)
        }
    }

    impl sp_consensus_grandpa::GrandpaApi<Block> for Runtime {
        fn grandpa_authorities() -> sp_consensus_grandpa::AuthorityList {
            Grandpa::grandpa_authorities()
        }
        fn current_set_id() -> sp_consensus_grandpa::SetId {
            Grandpa::current_set_id()
        }
        fn submit_report_equivocation_unsigned_extrinsic(
            _equivocation_proof: sp_consensus_grandpa::EquivocationProof<
                <Block as BlockT>::Hash,
                sp_runtime::traits::NumberFor<Block>,
            >,
            _key_owner_proof: sp_consensus_grandpa::OpaqueKeyOwnershipProof,
        ) -> Option<()> {
            None
        }
        fn generate_key_ownership_proof(
            _set_id: sp_consensus_grandpa::SetId,
            _authority_id: GrandpaId,
        ) -> Option<sp_consensus_grandpa::OpaqueKeyOwnershipProof> {
            None
        }
    }

    impl frame_system_rpc_runtime_api::AccountNonceApi<Block, AccountId, Nonce> for Runtime {
        fn account_nonce(account: AccountId) -> Nonce {
            System::account_nonce(account)
        }
    }

    impl pallet_transaction_payment_rpc_runtime_api::TransactionPaymentApi<Block, Balance> for Runtime {
        fn query_info(
            uxt: <Block as BlockT>::Extrinsic,
            len: u32,
        ) -> pallet_transaction_payment_rpc_runtime_api::RuntimeDispatchInfo<Balance> {
            TransactionPayment::query_info(uxt, len)
        }
        fn query_fee_details(
            uxt: <Block as BlockT>::Extrinsic,
            len: u32,
        ) -> pallet_transaction_payment::FeeDetails<Balance> {
            TransactionPayment::query_fee_details(uxt, len)
        }
        fn query_weight_to_fee(weight: Weight) -> Balance {
            TransactionPayment::weight_to_fee(weight)
        }
        fn query_length_to_fee(length: u32) -> Balance {
            TransactionPayment::length_to_fee(length)
        }
    }

    impl sp_genesis_builder::GenesisBuilder<Block> for Runtime {
        fn build_state(config: Vec<u8>) -> sp_genesis_builder::Result {
            frame_support::genesis_builder_helper::build_state::<RuntimeGenesisConfig>(config)
        }
        fn get_preset(id: &Option<sp_genesis_builder::PresetId>) -> Option<Vec<u8>> {
            frame_support::genesis_builder_helper::get_preset::<RuntimeGenesisConfig>(id, |_| None)
        }
        fn preset_names() -> Vec<sp_genesis_builder::PresetId> {
            Default::default()
        }
    }
}
