//! # TKS Runtime
//!
//! The TokenKickstarter (TKS) Substrate runtime.
//! - **Consensus**: NPoS (Aura block authoring + GRANDPA finality)
//! - **Token**: TKS with 18 decimals, 1 billion total supply at genesis
//! - **Staking**: 100K TKS validator bond, 50K TKS nominator bond
//! - **Treasury**: Receives all tx fees + 20% of block rewards
//! - **EVM**: Full Ethereum compatibility (MetaMask, Solidity, Web3)
//! - **Custom pallet**: `pallet-name-registry` for @username registration
//! - **Governance**: Sudo (will be replaced by on-chain governance in Phase 5)

#![cfg_attr(not(feature = "std"), no_std)]
#![recursion_limit = "512"]

// Re-export the WASM binary (built by build.rs)
#[cfg(feature = "std")]
include!(concat!(env!("OUT_DIR"), "/wasm_binary.rs"));

extern crate alloc;

mod precompiles;
use precompiles::FrontierPrecompiles;

use alloc::vec;
use alloc::vec::Vec;
use alloc::borrow::Cow;
use core::marker::PhantomData;

use frame_support::{
    construct_runtime,
    derive_impl,
    parameter_types,
    traits::{
        ConstBool, ConstU32, ConstU64, ConstU8, FindAuthor,
        fungible, Nothing, OnUnbalanced,
    },
    weights::{
        constants::RocksDbWeight,
        IdentityFee, Weight,
    },
    PalletId,
};
use frame_election_provider_support::{
    onchain, SequentialPhragmen,
};
use pallet_grandpa::AuthorityId as GrandpaId;
use pallet_ethereum::PostLogContent;
use pallet_evm::IdentityAddressMapping;
use sp_api::impl_runtime_apis;
use sp_consensus_aura::sr25519::AuthorityId as AuraId;
use sp_core::{crypto::{ByteArray, KeyTypeId}, OpaqueMetadata, H160, U256};
use sp_runtime::{
    generic, impl_opaque_keys,
    traits::{
        AccountIdLookup, BlakeTwo256, Block as BlockT, Dispatchable, DispatchInfoOf,
        IdentifyAccount, PostDispatchInfoOf, UniqueSaturatedInto, Verify,
    },
    transaction_validity::{TransactionSource, TransactionValidity, TransactionValidityError},
    ApplyExtrinsicResult, ConsensusEngineId, Perbill, Permill,
};
use fp_account::{AccountId20, EthereumSignature};
use sp_staking::SessionIndex;
use sp_version::RuntimeVersion;

/// Alias for the signature type used by accounts.
pub type Signature = EthereumSignature;

/// Account ID type (Ethereum-native H160).
pub type AccountId = AccountId20;

/// Balance type — u128 to match TKS ERC-20 (18 decimals).
pub type Balance = u128;

/// Index/nonce of a transaction.
pub type Nonce = u32;

/// Block header hash type.
pub type Hash = sp_core::H256;

/// Block number type.
pub type BlockNumber = u32;

/// The address format for the runtime (AccountId-based).
pub type Address = AccountId;

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
    fp_self_contained::UncheckedExtrinsic<Address, RuntimeCall, Signature, SignedExtra>;

/// Executive type (dispatches extrinsics).
pub type Executive = frame_executive::Executive<
    Runtime,
    Block,
    frame_system::ChainContext<Runtime>,
    Runtime,
    AllPalletsWithSystem,
    (Ethereum,),
>;

// ─── Runtime Version ───────────────────────────────────────────────

#[sp_version::runtime_version]
pub const VERSION: RuntimeVersion = RuntimeVersion {
    /// Chain identity — must NEVER change (breaks peer discovery if changed)
    spec_name: Cow::Borrowed("tks-chain"),
    impl_name: Cow::Borrowed("tks-chain"),
    /// Bump when block authoring logic changes (rare)
    authoring_version: 1,
    /// *** BUMP THIS on every runtime upgrade ***
    /// v400: 1-second block time, zero base fee (free messages + username), free NameRegistry.
    /// Nodes with different spec_version CANNOT sync with each other.
    /// A node with lower spec_version will refuse blocks from higher.
    /// Tolerance: ±4 versions (396–404 will still sync with warning).
    spec_version: 403,
    /// Bump when node implementation changes but state transition is same
    impl_version: 1,
    apis: RUNTIME_API_VERSIONS,
    /// *** BUMP THIS when transaction format changes ***
    /// Wallets cache this — if changed, they must re-fetch transaction format.
    /// Increment whenever extrinsic encoding format changes.
    transaction_version: 1,
    system_version: 1,
};

// ─── Version Tolerance ─────────────────────────────────────────────
//
// Defines how far behind a node binary can be before it is considered
// incompatible with the live TKS network.
//
// Behaviour:
//   Within tolerance (≤ 4 versions behind) → node syncs, logs a WARNING
//   Beyond tolerance  (> 4 versions behind) → node refuses to start / logs ERROR
//
// Example (current spec_version = 301):
//   spec_version 300, 299, 298, 297  → ✅ sync OK (warning shown)
//   spec_version 296 and below       → ❌ too old, refuses to start
//
// When you ship a runtime upgrade (bump spec_version to 302+), old nodes on
// 297 and below will be automatically rejected. This gives users a 4-release
// grace window to update their binary.

/// How many spec_version steps behind the current on-chain version a node
/// binary may be before it is considered incompatible.
pub const SPEC_VERSION_TOLERANCE: u32 = 4;

/// The oldest spec_version this binary can work with.
/// Recomputed automatically — never edit this directly.
pub const MIN_SUPPORTED_SPEC_VERSION: u32 =
    VERSION.spec_version.saturating_sub(SPEC_VERSION_TOLERANCE);

// ─── Constants ─────────────────────────────────────────────────────

/// 1 TKS = 10^18 smallest units (same as ETH/ERC-20).
pub const TKS: Balance = 1_000_000_000_000_000_000;
pub const MILLI_TKS: Balance = TKS / 1_000;
pub const MICRO_TKS: Balance = TKS / 1_000_000;

/// Block time: 1 second — optimised for Cipher real-time messaging.
/// Validators need < 200ms latency between each other for stable 1s blocks.
pub const MILLISECS_PER_BLOCK: u64 = 1000;
pub const SLOT_DURATION: u64 = MILLISECS_PER_BLOCK;
pub const MINUTES: BlockNumber = 60_000 / (MILLISECS_PER_BLOCK as BlockNumber); // 60 blocks
pub const HOURS: BlockNumber = MINUTES * 60;                                     // 3,600 blocks
pub const DAYS: BlockNumber = HOURS * 24;                                        // 86,400 blocks

/// Staking era: 6 hours = 21,600 blocks (was 360 at 6s blocks).
pub const EPOCH_DURATION_IN_BLOCKS: BlockNumber = HOURS * 6;

/// The version for the genesis state.
#[cfg(feature = "std")]
pub fn native_version() -> sp_version::NativeVersion {
    sp_version::NativeVersion {
        runtime_version: VERSION,
        can_author_with: Default::default(),
    }
}

// ─── Treasury Fee Handler ──────────────────────────────────────────

/// Routes all transaction fees to the treasury using the modern fungible API.
pub struct DealWithFees;
impl OnUnbalanced<fungible::Credit<<Runtime as frame_system::Config>::AccountId, Balances>>
    for DealWithFees
{
    fn on_unbalanceds(
        mut fees_then_tips: impl Iterator<
            Item = fungible::Credit<<Runtime as frame_system::Config>::AccountId, Balances>,
        >,
    ) {
        use frame_support::traits::tokens::fungible::Balanced;
        if let Some(fees) = fees_then_tips.next() {
            let _ = Balances::resolve(&Treasury::account_id(), fees);
            if let Some(tips) = fees_then_tips.next() {
                let _ = Balances::resolve(&Treasury::account_id(), tips);
            }
        }
    }
}

// ─── Frame System ──────────────────────────────────────────────────

// ─── TPS Capacity Tiers ────────────────────────────────────────────
//
//  Phase 1 (now):   ~10,000 TPS  — single chain, 1s blocks, 500M gas limit
//  Phase 2 (6mo):   ~50,000 TPS  — tuned validators, larger blocks
//  Phase 3 (12mo):  ~500,000 TPS — 8-shard HyperSwarm (62,500 TPS × 8 shards)
//
// Adaptive TPS: block fullness is monitored; when >80% full for 10 consecutive
// blocks, governance can vote to increase BLOCK_GAS_LIMIT via on-chain upgrade.
// No hard-fork needed — spec_version bump + WASM upgrade.

/// 700ms compute budget per 1-second block (70% of block time for EVM).
/// Leaves 300ms for P2P propagation + GRANDPA voting at 10k TPS.
/// Validators need >= 1Gbps bandwidth and >= 8 CPU cores to sustain this.
const WEIGHT_MILLISECS_PER_BLOCK: u64 = 700;
const NORMAL_DISPATCH_RATIO: Perbill = Perbill::from_percent(75);

parameter_types! {
    pub const BlockHashCount: BlockNumber = 2400;
    pub const Version: RuntimeVersion = VERSION;
    pub BlockWeights: frame_system::limits::BlockWeights =
        frame_system::limits::BlockWeights::with_sensible_defaults(
            // 7× WEIGHT_REF_TIME_PER_SECOND matches 700ms compute budget
            Weight::from_parts(7u64 * frame_support::weights::constants::WEIGHT_REF_TIME_PER_SECOND, u64::MAX),
            NORMAL_DISPATCH_RATIO,
        );
    pub BlockLength: frame_system::limits::BlockLength =
        frame_system::limits::BlockLength::max_with_normal_ratio(
            // 10MB blocks — needed for 10k TPS (10,000 × ~250 bytes ≈ 2.5MB raw,
            // 10MB gives headroom for large EVM call data and Cipher message anchors)
            10 * 1024 * 1024,
            NORMAL_DISPATCH_RATIO,
        );
    pub const SS58Prefix: u8 = 42;
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
    type Lookup = sp_runtime::traits::IdentityLookup<AccountId>;
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
    type MaxAuthorities = ConstU32<50>;
    type AllowMultipleBlocksPerSlot = ConstBool<false>;
    type SlotDuration = pallet_aura::MinimumPeriodTimesTwo<Runtime>;
}

// ─── GRANDPA (Finality) ────────────────────────────────────────────

impl pallet_grandpa::Config for Runtime {
    type RuntimeEvent = RuntimeEvent;
    type WeightInfo = ();
    type MaxAuthorities = ConstU32<50>;
    type MaxNominators = ConstU32<256>;
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

// ─── Authorship ────────────────────────────────────────────────────

impl pallet_authorship::Config for Runtime {
    type FindAuthor = pallet_session::FindAccountFromAuthorIndex<Self, Aura>;
    type EventHandler = (Staking,);
}

// ─── Balances (TKS Token) ──────────────────────────────────────────
//
// Total supply: 20,000,000,000 TKS (20 Billion) — set in genesis.
// Existing ERC-20/BEP-20 supply on ETH/BSC: 1,000,000,000 TKS (1 Billion).
// When ETH/BSC holders bridge to TKS Network, their ERC-20 tokens are
// burned on ETH/BSC via TKSBridge.burnAndBridge(), and native TKS is
// released from the Bridge Reserve (3B allocation in genesis).
// This reduces effective circulating supply over time.
//
// Burn mechanism:
//   EVM layer:  transfer to BURN_ADDRESS (0x000...dEaD) — standard EVM burn
//   Substrate:  use pallet_balances::burn() dispatchable (removes from total issuance)
//   EIP-1559:   base fee is burned each block (deflationary pressure)

parameter_types! {
    pub const ExistentialDeposit: Balance = MILLI_TKS; // 0.001 TKS

    /// Standard EVM burn address (0x000...dEaD). Any TKS sent here is permanently
    /// removed from circulating supply. Widely recognised by wallets and explorers.
    /// Same address used by ETH, BNB, MATIC and most EVM chains as the canonical burn address.
    pub BurnAddress: AccountId = AccountId::from(
        H160::from_slice(&[
            0x00,0x00,0x00,0x00,0x00,0x00,0x00,0x00,0x00,0x00,
            0x00,0x00,0x00,0x00,0x00,0x00,0x00,0x00,0xde,0xad,
        ])
    );
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
    type FreezeIdentifier = RuntimeFreezeReason;
    type MaxFreezes = ConstU32<8>;
    type RuntimeHoldReason = RuntimeHoldReason;
    type RuntimeFreezeReason = RuntimeFreezeReason;
    type DoneSlashHandler = ();
}

// ─── Transaction Payment ───────────────────────────────────────────

impl pallet_transaction_payment::Config for Runtime {
    type RuntimeEvent = RuntimeEvent;
    type OnChargeTransaction =
        pallet_transaction_payment::FungibleAdapter<Balances, DealWithFees>;
    type OperationalFeeMultiplier = ConstU8<5>;
    type WeightToFee = IdentityFee<Balance>;
    type LengthToFee = IdentityFee<Balance>;
    type FeeMultiplierUpdate = ();
    type WeightInfo = pallet_transaction_payment::weights::SubstrateWeight<Runtime>;
}

// ─── Session ───────────────────────────────────────────────────────

parameter_types! {
    pub const Period: BlockNumber = EPOCH_DURATION_IN_BLOCKS; // 6 hours
    pub const Offset: BlockNumber = 0;
}

parameter_types! {
    pub const SessionKeyDeposit: Balance = 0; // Free session key registration in dev
}

impl pallet_session::Config for Runtime {
    type RuntimeEvent = RuntimeEvent;
    type ValidatorId = AccountId;
    type ValidatorIdOf = sp_runtime::traits::ConvertInto;
    type ShouldEndSession = pallet_session::PeriodicSessions<Period, Offset>;
    type NextSessionRotation = pallet_session::PeriodicSessions<Period, Offset>;
    type SessionManager = pallet_session::historical::NoteHistoricalRoot<Runtime, Staking>;
    type SessionHandler = <SessionKeys as sp_runtime::traits::OpaqueKeys>::KeyTypeIdProviders;
    type Keys = SessionKeys;
    type DisablingStrategy = pallet_session::disabling::UpToLimitDisablingStrategy;
    type Currency = Balances;
    type KeyDeposit = SessionKeyDeposit;
    type WeightInfo = pallet_session::weights::SubstrateWeight<Runtime>;
}

impl pallet_session::historical::Config for Runtime {
    type RuntimeEvent = RuntimeEvent;
    type FullIdentification = pallet_staking::Exposure<AccountId, Balance>;
    type FullIdentificationOf = pallet_staking::DefaultExposureOf<Runtime>;
}

// ─── Staking (NPoS) ───────────────────────────────────────────────

parameter_types! {
    pub const SessionsPerEra: SessionIndex = 1;
    pub const BondingDuration: u32 = 28 * 4; // 28 days × 4 eras/day = 112 eras
    pub const SlashDeferDuration: u32 = 27 * 4; // 27 days
    pub const MaxNominations: u32 = 16;
    pub const HistoryDepth: u32 = 84; // ~21 days at 6-hour eras
    pub const OffendingValidatorsThreshold: Perbill = Perbill::from_percent(17);
    /// Minimum TKS a user must bond to become a validator.
    /// Enforced on-chain by pallet-staking — cannot be bypassed.
    pub const MinValidatorBond: Balance = 100_000 * TKS;   // 100,000 TKS
    /// Minimum TKS a nominator must bond to back a validator.
    pub const MinNominatorBond: Balance = 1_000 * TKS;     // 1,000 TKS
    /// Maximum number of active validators in any given era.
    pub const MaxValidatorCount: u32 = 500;
}

/// Simple era payout: ~15 TKS per block.
/// 80% to stakers, 20% to treasury.
pub struct TksEraPayout;
impl pallet_staking::EraPayout<Balance> for TksEraPayout {
    fn era_payout(
        _total_staked: Balance,
        _total_issuance: Balance,
        era_duration_millis: u64,
    ) -> (Balance, Balance) {
        let blocks_in_era = era_duration_millis / (MILLISECS_PER_BLOCK as u64);
        let per_block_reward = 15 * TKS;
        let total_reward = per_block_reward * (blocks_in_era as u128);
        let staker_payout = total_reward * 80 / 100;
        let treasury_payout = total_reward - staker_payout;
        (staker_payout, treasury_payout)
    }
}

/// Wrapper to route staking slashes/remainder to treasury.
pub struct StakingRewardToTreasury;
impl OnUnbalanced<fungible::Credit<AccountId, Balances>> for StakingRewardToTreasury {
    fn on_nonzero_unbalanced(credit: fungible::Credit<AccountId, Balances>) {
        use frame_support::traits::tokens::fungible::Balanced;
        let _ = Balances::resolve(&Treasury::account_id(), credit);
    }
}

/// Staking benchmarking configuration.
pub struct StakingBenchmarkConfig;
impl pallet_staking::BenchmarkingConfig for StakingBenchmarkConfig {
    type MaxValidators = ConstU32<1000>;
    type MaxNominators = ConstU32<1000>;
}

impl pallet_staking::Config for Runtime {
    type Currency = Balances;
    type CurrencyBalance = Balance;
    type OldCurrency = Balances;
    type UnixTime = Timestamp;
    type CurrencyToVote = sp_staking::currency_to_vote::U128CurrencyToVote;
    type RewardRemainder = StakingRewardToTreasury;
    type RuntimeEvent = RuntimeEvent;
    type RuntimeHoldReason = RuntimeHoldReason;
    type Slash = StakingRewardToTreasury;
    type Reward = ();
    type SessionsPerEra = SessionsPerEra;
    type BondingDuration = BondingDuration;
    type SlashDeferDuration = SlashDeferDuration;
    type AdminOrigin = frame_system::EnsureRoot<AccountId>;
    type SessionInterface = Self;
    type EraPayout = TksEraPayout;
    type NextNewSession = Session;
    type HistoryDepth = HistoryDepth;
    type MaxExposurePageSize = ConstU32<256>;
    type MaxValidatorSet = ConstU32<5000>;
    type MaxControllersInDeprecationBatch = ConstU32<100>;
    type ElectionProvider = ElectionProviderMultiPhase;
    type GenesisElectionProvider = onchain::OnChainExecution<OnChainSeqPhragmen>;
    type VoterList = BagsList;
    type TargetList = pallet_staking::UseValidatorsMap<Runtime>;
    type NominationsQuota = pallet_staking::FixedNominationsQuota<{ MaxNominations::get() }>;
    type MaxUnlockingChunks = ConstU32<32>;
    type EventListeners = ();
    type Filter = Nothing;
    type BenchmarkingConfig = StakingBenchmarkConfig;
    type WeightInfo = pallet_staking::weights::SubstrateWeight<Runtime>;
    // NOTE: MinValidatorBond (100,000 TKS) and MinNominatorBond (1,000 TKS)
    // are set as storage values in the genesis config below (not trait types
    // in this pallet-staking version). They can also be updated at runtime
    // via staking.updateStakingMinimums (sudo only).
}

// ─── Election Provider (multi-phase for 500-5K validators) ─────────

/// Deposit amount per signed submission (converts submission count to balance).
pub struct SignedDepositBaseFn;
impl sp_runtime::traits::Convert<usize, Balance> for SignedDepositBaseFn {
    fn convert(_c: usize) -> Balance {
        TKS // 1 TKS base deposit per signed submission
    }
}

parameter_types! {
    pub const SignedDepositByteAmount: Balance = MICRO_TKS;
}

/// Benchmarking config for multi-phase election provider.
pub struct ElectionBenchmarkConfig;
impl pallet_election_provider_multi_phase::BenchmarkingConfig for ElectionBenchmarkConfig {
    const VOTERS: [u32; 2] = [1000, 2000];
    const TARGETS: [u32; 2] = [500, 1000];
    const ACTIVE_VOTERS: [u32; 2] = [500, 800];
    const DESIRED_TARGETS: [u32; 2] = [200, 400];
    const SNAPSHOT_MAXIMUM_VOTERS: u32 = 1000;
    const MINER_MAXIMUM_VOTERS: u32 = 1000;
    const MAXIMUM_TARGETS: u32 = 300;
}

parameter_types! {
    pub const SignedMaxSubmissions: u32 = 10;
    pub const SignedRewardBase: Balance = TKS;
    pub const StakingUnsignedPriority: u64 = u64::MAX / 2;
    pub const MultiPhaseUnsignedPriority: u64 = StakingUnsignedPriority::get() - 1;
    pub MinerMaxWeight: Weight = BlockWeights::get()
        .get(frame_support::dispatch::DispatchClass::Normal)
        .max_extrinsic
        .unwrap_or(Weight::MAX);
    pub MinerMaxLength: u32 = Perbill::from_percent(90) * *BlockLength::get().max.get(
        frame_support::dispatch::DispatchClass::Normal,
    );
    pub ElectionBoundsOnChain: frame_election_provider_support::bounds::ElectionBounds =
        frame_election_provider_support::bounds::ElectionBoundsBuilder::default()
            .voters_count(5_000.into())
            .targets_count(1_250.into())
            .build();
    pub ElectionBoundsMultiPhase: frame_election_provider_support::bounds::ElectionBounds =
        frame_election_provider_support::bounds::ElectionBoundsBuilder::default()
            .voters_count(10_000.into())
            .targets_count(1_500.into())
            .build();
}

/// On-chain sequential Phragmén solver.
pub struct OnChainSeqPhragmen;
impl onchain::Config for OnChainSeqPhragmen {
    type System = Runtime;
    type Solver = SequentialPhragmen<AccountId, sp_runtime::PerU16>;
    type DataProvider = Staking;
    type WeightInfo = frame_election_provider_support::weights::SubstrateWeight<Runtime>;
    type MaxWinnersPerPage = ConstU32<50>;
    type MaxBackersPerWinner = ConstU32<256>;
    type Sort = ConstBool<true>;
    type Bounds = ElectionBoundsOnChain;
}

frame_election_provider_support::generate_solution_type!(
    #[compact]
    pub struct NposSolution16::<
        VoterIndex = u32,
        TargetIndex = u16,
        Accuracy = sp_runtime::PerU16,
        MaxVoters = ConstU32<22500>,
    >(16)
);

impl pallet_election_provider_multi_phase::Config for Runtime {
    type RuntimeEvent = RuntimeEvent;
    type Currency = Balances;
    type EstimateCallFee = pallet_transaction_payment::Pallet<Runtime>;
    type SignedPhase = ConstU32<{ EPOCH_DURATION_IN_BLOCKS / 4 }>;
    type UnsignedPhase = ConstU32<{ EPOCH_DURATION_IN_BLOCKS / 4 }>;
    type BetterSignedThreshold = ();
    type OffchainRepeat = ConstU32<5>;
    type MinerTxPriority = MultiPhaseUnsignedPriority;
    type MinerConfig = Self;
    type SignedMaxSubmissions = SignedMaxSubmissions;
    type SignedRewardBase = SignedRewardBase;
    type SignedDepositBase = SignedDepositBaseFn;
    type SignedDepositByte = SignedDepositByteAmount;
    type SignedMaxRefunds = ConstU32<3>;
    type SignedDepositWeight = ();
    type SignedMaxWeight = MinerMaxWeight;
    type SlashHandler = Treasury;
    type RewardHandler = ();
    type DataProvider = Staking;
    type Fallback = onchain::OnChainExecution<OnChainSeqPhragmen>;
    type MaxBackersPerWinner = ConstU32<256>;
    type GovernanceFallback = onchain::OnChainExecution<OnChainSeqPhragmen>;
    type Solver = SequentialPhragmen<AccountId, sp_runtime::PerU16>;
    type ForceOrigin = frame_system::EnsureRoot<AccountId>;
    type MaxWinners = ConstU32<50>;
    type ElectionBounds = ElectionBoundsMultiPhase;
    type BenchmarkingConfig = ElectionBenchmarkConfig;
    type WeightInfo = pallet_election_provider_multi_phase::weights::SubstrateWeight<Runtime>;
}

impl pallet_election_provider_multi_phase::MinerConfig for Runtime {
    type AccountId = AccountId;
    type MaxLength = MinerMaxLength;
    type MaxWeight = MinerMaxWeight;
    type MaxVotesPerVoter = ConstU32<16>;
    type MaxWinners = ConstU32<50>;
    type MaxBackersPerWinner = ConstU32<256>;
    type Solution = NposSolution16;

    fn solution_weight(v: u32, t: u32, a: u32, d: u32) -> Weight {
        <pallet_election_provider_multi_phase::weights::SubstrateWeight<Runtime>
            as pallet_election_provider_multi_phase::WeightInfo>::submit_unsigned(v, t, a, d)
    }
}

// ─── Bags List (voter list for staking) ────────────────────────────

parameter_types! {
    pub const BagThresholds: &'static [u64] = &[];
}

impl pallet_bags_list::Config for Runtime {
    type RuntimeEvent = RuntimeEvent;
    type ScoreProvider = Staking;
    type BagThresholds = BagThresholds;
    type Score = u64;
    type MaxAutoRebagPerBlock = ConstU32<0>;
    type WeightInfo = pallet_bags_list::weights::SubstrateWeight<Runtime>;
}

// ─── Treasury ──────────────────────────────────────────────────────

parameter_types! {
    pub const TreasuryPalletId: PalletId = PalletId(*b"py/trsry");
    pub const ProposalBond: Permill = Permill::from_percent(5);
    pub const ProposalBondMinimum: Balance = 100 * TKS;
    pub const SpendPeriod: BlockNumber = 24 * HOURS; // 1 day
    pub const MaxApprovals: u32 = 100;
    pub const TreasuryBurn: Permill = Permill::from_percent(0); // No burn — keep all funds for Cipher infra
    pub TreasuryAccount: AccountId = Treasury::account_id();
}

impl pallet_treasury::Config for Runtime {
    type PalletId = TreasuryPalletId;
    type Currency = Balances;
    type RejectOrigin = frame_system::EnsureRoot<AccountId>;
    type RuntimeEvent = RuntimeEvent;
    type SpendPeriod = SpendPeriod;
    type Burn = TreasuryBurn;
    type BurnDestination = ();
    type BlockNumberProvider = System;
    type SpendFunds = ();
    type WeightInfo = pallet_treasury::weights::SubstrateWeight<Runtime>;
    type MaxApprovals = MaxApprovals;
    type SpendOrigin = frame_support::traits::NeverEnsureOrigin<Balance>;
    type AssetKind = ();
    type Beneficiary = AccountId;
    type BeneficiaryLookup = AccountIdLookup<AccountId, ()>;
    type Paymaster = frame_support::traits::tokens::PayFromAccount<Balances, TreasuryAccount>;
    type BalanceConverter = frame_support::traits::tokens::UnityAssetBalanceConversion;
    type PayoutPeriod = ConstU32<10>;
}

// ─── Sudo (Temporary Governance) ───────────────────────────────────

impl pallet_sudo::Config for Runtime {
    type RuntimeEvent = RuntimeEvent;
    type RuntimeCall = RuntimeCall;
    type WeightInfo = pallet_sudo::weights::SubstrateWeight<Runtime>;
}

// ─── Name Registry (TKS Username Pallet — 100% FREE) ──────────────

parameter_types! {
    pub const MaxNameLength: u32 = 32;
    pub const MinNameLength: u32 = 3;
}

impl pallet_name_registry::Config for Runtime {
    type MaxNameLength = MaxNameLength;
    type MinNameLength = MinNameLength;
}

// ─── Shard Registry (HyperSwarm) ───────────────────────────────────

parameter_types! {
    pub const ShardEpochLength: u32 = HOURS * 6;  // Rotate shards every 6 hours
    pub const MaxSwarmValidators: u32 = 5000;      // Max registered HyperSwarm validators
    pub const MaxEndpointLength: u32 = 256;        // Max libp2p multiaddr length
}

impl pallet_shard_registry::Config for Runtime {
    type EpochLength = ShardEpochLength;
    type MaxValidators = MaxSwarmValidators;
    type MaxEndpointLength = MaxEndpointLength;
}

// ─── HyperSwarm Anchor ─────────────────────────────────────────────

parameter_types! {
    /// Challenges must be submitted within 7 days of anchor posting.
    /// 7 days × 24 hours × 600 blocks/hour = 100800 blocks.
    pub const ChallengeWindow: u32 = 7 * 24 * HOURS;
}

impl pallet_hyperswarm_anchor::Config for Runtime {
    type ChallengeWindow = ChallengeWindow;
}

// ─── EVM / Frontier ────────────────────────────────────────────────

impl pallet_evm_chain_id::Config for Runtime {}

/// Find the author of the block for EVM mining rewards.
pub struct FindAuthorTruncated<F>(PhantomData<F>);
impl<F: FindAuthor<u32>> FindAuthor<H160> for FindAuthorTruncated<F> {
    fn find_author<'a, I>(digests: I) -> Option<H160>
    where
        I: 'a + IntoIterator<Item = (ConsensusEngineId, &'a [u8])>,
    {
        if let Some(author_index) = F::find_author(digests) {
            let authority_id =
                pallet_aura::Authorities::<Runtime>::get()[author_index as usize].clone();
            return Some(H160::from_slice(&authority_id.to_raw_vec()[4..24]));
        }
        None
    }
}

// 500M gas/block @ 1s = ~10,000 TPS (simple ERC-20 transfer ≈ 50k gas)
// Upgrade path: 1.5B gas → ~30k TPS | 4B gas + sharding → 500k TPS
//
// Adaptive adjustment (via on-chain upgrade, no hard-fork):
//   If avg_block_fullness > 80% over 100 blocks → propose gas limit increase
//   If avg_block_fullness < 20% over 100 blocks → propose gas limit decrease
//   Each adjustment: ±25% of current limit, max 4B, min 75M
// The INITIAL gas limit — pallet-adaptive-gas will change this each block.
// This constant is only used as the genesis/default value.
const BLOCK_GAS_LIMIT: u64 = 500_000_000;  // 500M — starts at ~10,000 TPS
const MAX_POV_SIZE: u64 = 10 * 1024 * 1024; // 10MB to match BlockLength
// NOTE: GasLimitPovSizeRatio and GasLimitStorageGrowthRatio control how much gas
// is charged for Merkle proof size and storage growth respectively. These are needed
// for PARACHAINS (relay-chain limits proof-of-validity size). For STANDALONE chains
// like TKS there is no PoV budget to protect, so we disable them by setting to 0.
// With ratio=4 (non-zero), a 25KB state proof would cost 25000*4=100K gas — far
// more than a 21K-gas transfer, causing OOG reverts on basic operations.
const GAS_POV_RATIO: u64 = 0;         // 0 = disabled (standalone chain, no relay PoV budget)
const GAS_STORAGE_RATIO: u64 = 0;     // 0 = disabled (storage growth uncapped per-tx)

// Adaptive gas pallet constants
const MIN_GAS_LIMIT: u64  =    75_000_000; //  75M →  ~1,500 TPS (floor)
const MAX_GAS_LIMIT: u64  = 4_000_000_000; //   4B → ~80,000 TPS (ceiling)

/// Calculate weight per gas unit.
fn weight_per_gas(gas_limit: u64, ratio: Perbill, millis: u64) -> u64 {
    let max_weight = ratio * Weight::from_parts(
        frame_support::weights::constants::WEIGHT_REF_TIME_PER_MILLIS * millis,
        0,
    );
    max_weight.ref_time() / gas_limit
}

parameter_types! {
    /// The LIVE gas limit is read dynamically from AdaptiveGas::CurrentGasLimit storage
    /// via AdaptiveGasLimitGet — no static constant needed here.
    pub const GasLimitPovSizeRatio: u64 = GAS_POV_RATIO;
    pub const GasLimitStorageGrowthRatio: u64 = GAS_STORAGE_RATIO;
    pub PrecompilesValue: FrontierPrecompiles<Runtime> = FrontierPrecompiles::<_>::new();
    pub WeightPerGas: Weight = Weight::from_parts(
        // Use MAX gas limit for weight-per-gas so any valid tx is accepted;
        // the actual cap is enforced dynamically by AdaptiveGas::CurrentGasLimit.
        weight_per_gas(MAX_GAS_LIMIT, NORMAL_DISPATCH_RATIO, WEIGHT_MILLISECS_PER_BLOCK),
        0,
    );
}

// ─── Adaptive Gas Pallet Config ────────────────────────────────────

parameter_types! {
    pub const AdaptiveMinGasLimit: u64 = MIN_GAS_LIMIT;
    pub const AdaptiveMaxGasLimit: u64 = MAX_GAS_LIMIT;
    pub const AdaptiveInitialGasLimit: u64 = BLOCK_GAS_LIMIT;
    /// Target 50% block fullness — below this limit shrinks, above it grows.
    pub const AdaptiveTargetFillPercent: u32 = 50;
    /// Max ±12.5% adjustment per block (EIP-1559 elasticity).
    pub const AdaptiveMaxAdjustmentPermill: u32 = 125;
}

impl pallet_adaptive_gas::Config for Runtime {
    type RuntimeEvent = RuntimeEvent;
    type MinGasLimit = AdaptiveMinGasLimit;
    type MaxGasLimit = AdaptiveMaxGasLimit;
    type InitialGasLimit = AdaptiveInitialGasLimit;
    type TargetFillPercent = AdaptiveTargetFillPercent;
    type MaxAdjustmentPermill = AdaptiveMaxAdjustmentPermill;
}

impl pallet_evm::Config for Runtime {
    type AccountProvider = pallet_evm::FrameSystemAccountProvider<Self>;
    type FeeCalculator = BaseFee;
    type GasWeightMapping = pallet_evm::FixedGasWeightMapping<Self>;
    type WeightPerGas = WeightPerGas;
    type BlockHashMapping = pallet_ethereum::EthereumBlockHashMapping<Self>;
    type CallOrigin = pallet_evm::EnsureAccountId20;
    type WithdrawOrigin = pallet_evm::EnsureAccountId20;
    type AddressMapping = IdentityAddressMapping;
    type Currency = Balances;
    type PrecompilesType = FrontierPrecompiles<Self>;
    type PrecompilesValue = PrecompilesValue;
    type ChainId = EVMChainId;
    type BlockGasLimit = pallet_adaptive_gas::AdaptiveGasLimitGet<Runtime>;
    type Runner = pallet_evm::runner::stack::Runner<Self>;
    type OnChargeTransaction = ();
    type OnCreate = ();
    type FindAuthor = FindAuthorTruncated<Aura>;
    type GasLimitPovSizeRatio = GasLimitPovSizeRatio;
    type GasLimitStorageGrowthRatio = GasLimitStorageGrowthRatio;
    type Timestamp = Timestamp;
    type CreateOriginFilter = ();
    type CreateInnerOriginFilter = ();
    type WeightInfo = pallet_evm::weights::SubstrateWeight<Self>;
}

parameter_types! {
    pub const PostBlockAndTxnHashes: PostLogContent = PostLogContent::BlockAndTxnHashes;
    pub const AllowUnprotectedTxs: bool = false;
}

impl pallet_ethereum::Config for Runtime {
    type StateRoot = pallet_ethereum::IntermediateStateRoot<Self::Version>;
    type PostLogContent = PostBlockAndTxnHashes;
    type ExtraDataLength = ConstU32<30>;
    type AllowUnprotectedTxs = AllowUnprotectedTxs;
}

parameter_types! {
    pub BoundDivision: U256 = U256::from(1024);
}

impl pallet_dynamic_fee::Config for Runtime {
    type MinGasPriceBoundDivisor = BoundDivision;
}

parameter_types! {
    // ─── Two-Tier Fee Model ────────────────────────────────────────────
    //
    //  FREE (Pays::No on Substrate extrinsics — no gas at all):
    //    • Username register / transfer / release  (pallet-name-registry)
    //    • Cipher message anchors                  (pallet-hyperswarm-anchor)
    //    • Shard coordination messages             (pallet-shard-registry)
    //    These NEVER touch the EVM fee system.
    //
    //  ADAPTIVE FEE (EVM base fee, EIP-1559 style):
    //    • TKS token transfers (ERC-20, native)
    //    • Smart contract deployment
    //    • DApp interactions (NinjaSwap DEX, etc.)
    //    • NFT minting / trading
    //    • Any other EVM transaction
    //
    //  Fee calculation:
    //    cost = gas_used × (base_fee + optional_priority_tip)
    //
    //  Starting fee: 1 Gwei (0.000000001 TKS per gas unit)
    //  Transfer (21k gas) @ 1 Gwei = 0.000021 TKS ≈ $0.000001 at $0.05/TKS
    //  Contract deploy (1M gas) @ 1 Gwei = 0.001 TKS ≈ $0.00005 at $0.05/TKS
    //
    //  Auto-adjustment (EIP-1559 elasticity = 12.5% per block):
    //    Block > 50% full  → base fee rises  up to +12.5%/block
    //    Block < 50% full  → base fee drops  up to -12.5%/block
    //    Block = 50% full  → base fee stable
    //    Min fee           → approaches 0 when network is idle
    //    Max fee           → capped by block weight, not artificially
    //
    //  Combined with adaptive block SIZE (pallet-adaptive-gas):
    //    Busy network      → bigger blocks (more TPS) + higher fee
    //    Idle network      → smaller blocks (saves disk) + near-zero fee
    pub DefaultBaseFeePerGas: U256 = U256::from(1_000_000_000u64); // 1 Gwei starting point
    pub DefaultElasticity: Permill = Permill::from_parts(125_000);  // 12.5% per block (EIP-1559)
}

pub struct BaseFeeThreshold;
impl pallet_base_fee::BaseFeeThreshold for BaseFeeThreshold {
    fn lower() -> Permill {
        Permill::from_parts(0)        // below 0% fullness → fee can drop to near 0
    }
    fn ideal() -> Permill {
        Permill::from_parts(500_000)  // target: 50% block fullness → fee stays stable
    }
    fn upper() -> Permill {
        Permill::from_parts(1_000_000) // above 100% → fee rises (but blocks expand first)
    }
}

impl pallet_base_fee::Config for Runtime {
    type Threshold = BaseFeeThreshold;
    type DefaultBaseFeePerGas = DefaultBaseFeePerGas;
    type DefaultElasticity = DefaultElasticity;
}

impl pallet_hotfix_sufficients::Config for Runtime {
    type AddressMapping = IdentityAddressMapping;
    type WeightInfo = pallet_hotfix_sufficients::weights::SubstrateWeight<Self>;
}

// ─── pallet-nfts ────────────────────────────────────────────────────
// Substrate-native NFT pallet — provides collections and items
// without requiring EVM. Gas-free for holders, deposits set to
// zero for the TKS free-to-use model.

parameter_types! {
    pub const NftsCollectionDeposit: Balance = 0;
    pub const NftsItemDeposit: Balance       = 0;
    pub const NftsMetadataDepositBase: Balance = 0;
    pub const NftsAttributeDepositBase: Balance = 0;
    pub const NftsDepositPerByte: Balance    = 0;
    pub const NftsStringLimit: u32           = 256;
    pub const NftsKeyLimit: u32              = 64;
    pub const NftsValueLimit: u32            = 256;
    pub const NftsApprovalsLimit: u32        = 20;
    pub const NftsItemAttributesApprovalsLimit: u32 = 30;
    pub const NftsMaxTips: u32               = 10;
    pub const NftsMaxDeadlineDuration: u32   = 604_800; // ~7 days in blocks (@1s blocks)
    pub const NftsMaxAttributesPerCall: u32  = 10;
    pub NftsFeatures: pallet_nfts::PalletFeatures =
        pallet_nfts::PalletFeatures::all_enabled();
}

impl pallet_nfts::Config for Runtime {
    type RuntimeEvent                   = RuntimeEvent;
    type CollectionId                   = u32;
    type ItemId                         = u32;
    type Currency                       = Balances;
    // Anyone can create a collection
    type CreateOrigin = frame_support::traits::AsEnsureOriginWithArg<
        frame_system::EnsureSigned<AccountId>,
    >;
    type ForceOrigin                    = frame_system::EnsureRoot<AccountId>;
    type Locker                         = ();
    type CollectionDeposit              = NftsCollectionDeposit;
    type ItemDeposit                    = NftsItemDeposit;
    type MetadataDepositBase            = NftsMetadataDepositBase;
    type AttributeDepositBase           = NftsAttributeDepositBase;
    type DepositPerByte                 = NftsDepositPerByte;
    type StringLimit                    = ConstU32<256>;
    type KeyLimit                       = ConstU32<64>;
    type ValueLimit                     = ConstU32<256>;
    type ApprovalsLimit                 = ConstU32<20>;
    type ItemAttributesApprovalsLimit   = ConstU32<30>;
    type MaxTips                        = ConstU32<10>;
    type MaxDeadlineDuration            = NftsMaxDeadlineDuration;
    type MaxAttributesPerCall           = ConstU32<10>;
    type Features                       = NftsFeatures;
    type OffchainSignature              = EthereumSignature;
    type OffchainPublic                 = fp_account::EthereumSigner;
    type BlockNumberProvider            = System;
    type WeightInfo                     = pallet_nfts::weights::SubstrateWeight<Runtime>;
    #[cfg(feature = "runtime-benchmarks")]
    type Helper                         = ();
}

// ─── Construct Runtime ─────────────────────────────────────────────

construct_runtime!(
    pub struct Runtime {
        // Core
        System: frame_system,
        Timestamp: pallet_timestamp,

        // Monetary
        Balances: pallet_balances,
        TransactionPayment: pallet_transaction_payment,
        Treasury: pallet_treasury,

        // Consensus & Staking
        Aura: pallet_aura,
        Grandpa: pallet_grandpa,
        Authorship: pallet_authorship,
        Session: pallet_session,
        SessionHistorical: pallet_session::historical,
        Staking: pallet_staking,
        ElectionProviderMultiPhase: pallet_election_provider_multi_phase,
        BagsList: pallet_bags_list,

        // EVM / Ethereum
        Ethereum: pallet_ethereum,
        EVM: pallet_evm,
        EVMChainId: pallet_evm_chain_id,
        BaseFee: pallet_base_fee,
        DynamicFee: pallet_dynamic_fee,
        HotfixSufficients: pallet_hotfix_sufficients,

        // Governance (temporary)
        Sudo: pallet_sudo,

        // TKS Custom
        NameRegistry: pallet_name_registry,

        // TKS HyperSwarm (Layer 2 support pallets)
        ShardRegistry: pallet_shard_registry,
        HyperSwarmAnchor: pallet_hyperswarm_anchor,

        // Adaptive block gas limit — auto-scales TPS from 1.5k to 80k based on load
        AdaptiveGas: pallet_adaptive_gas,

        // Native NFTs (Substrate-layer, no EVM required)
        Nfts: pallet_nfts,
    }
);

// ─── Ethereum Transaction Converter ────────────────────────────────


// ─── Ethereum Transaction Converter ────────────────────────────────

/// Alias for Ethereum transaction type (TransactionV3 with EIP-7702).
pub type EthereumTransaction = pallet_ethereum::Transaction;

#[derive(Clone)]
pub struct TransactionConverter;

impl fp_rpc::ConvertTransaction<UncheckedExtrinsic> for TransactionConverter {
    fn convert_transaction(
        &self,
        transaction: EthereumTransaction,
    ) -> UncheckedExtrinsic {
        let extrinsic: UncheckedExtrinsic = fp_self_contained::UncheckedExtrinsic(generic::UncheckedExtrinsic::new_bare(
            pallet_ethereum::Call::<Runtime>::transact { transaction }.into(),
        ));
        let encoded = codec::Encode::encode(&extrinsic);
        codec::Decode::decode(&mut &encoded[..])
            .expect("Encoded extrinsic is always valid")
    }
}

// ─── Self-contained Ethereum transactions ──────────────────────────

impl fp_self_contained::SelfContainedCall for RuntimeCall {
    type SignedInfo = H160;

    fn is_self_contained(&self) -> bool {
        match self {
            RuntimeCall::Ethereum(call) => call.is_self_contained(),
            _ => false,
        }
    }

    fn check_self_contained(&self) -> Option<Result<Self::SignedInfo, TransactionValidityError>> {
        match self {
            RuntimeCall::Ethereum(call) => call.check_self_contained(),
            _ => None,
        }
    }

    fn validate_self_contained(
        &self,
        info: &Self::SignedInfo,
        dispatch_info: &DispatchInfoOf<RuntimeCall>,
        len: usize,
    ) -> Option<TransactionValidity> {
        match self {
            RuntimeCall::Ethereum(call) => call.validate_self_contained(info, dispatch_info, len),
            _ => None,
        }
    }

    fn pre_dispatch_self_contained(
        &self,
        info: &Self::SignedInfo,
        dispatch_info: &DispatchInfoOf<RuntimeCall>,
        len: usize,
    ) -> Option<Result<(), TransactionValidityError>> {
        match self {
            RuntimeCall::Ethereum(call) => {
                call.pre_dispatch_self_contained(info, dispatch_info, len)
            }
            _ => None,
        }
    }

    fn apply_self_contained(
        self,
        info: Self::SignedInfo,
    ) -> Option<sp_runtime::DispatchResultWithInfo<PostDispatchInfoOf<Self>>> {
        match self {
            call @ RuntimeCall::Ethereum(pallet_ethereum::Call::transact { .. }) => {
                Some(call.dispatch(RuntimeOrigin::from(
                    pallet_ethereum::RawOrigin::EthereumTransaction(info),
                )))
            }
            _ => None,
        }
    }
}

// ─── Offchain (bare extrinsic creation for election unsigned submissions) ──

use frame_system::offchain::{CreateTransactionBase, CreateBare};

impl CreateTransactionBase<pallet_election_provider_multi_phase::Call<Runtime>> for Runtime {
    type Extrinsic = UncheckedExtrinsic;
    type RuntimeCall = RuntimeCall;
}

impl CreateBare<pallet_election_provider_multi_phase::Call<Runtime>> for Runtime {
    fn create_bare(call: Self::RuntimeCall) -> Self::Extrinsic {
        fp_self_contained::UncheckedExtrinsic(generic::UncheckedExtrinsic::new_bare(call))
    }
}

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

    // ─── Frontier Ethereum Runtime APIs ────────────────────────────────

    impl fp_rpc::EthereumRuntimeRPCApi<Block> for Runtime {
        fn chain_id() -> u64 {
            use frame_support::traits::Get;
            <Runtime as pallet_evm::Config>::ChainId::get()
        }

        fn account_basic(address: H160) -> fp_evm::Account {
            let (account, _) = pallet_evm::Pallet::<Runtime>::account_basic(&address);
            account
        }

        fn gas_price() -> U256 {
            use fp_evm::FeeCalculator;
            let (gas_price, _) = <Runtime as pallet_evm::Config>::FeeCalculator::min_gas_price();
            gas_price
        }

        fn account_code_at(address: H160) -> Vec<u8> {
            pallet_evm::AccountCodes::<Runtime>::get(address)
        }

        fn author() -> H160 {
            <pallet_evm::Pallet<Runtime>>::find_author()
        }

        fn storage_at(address: H160, index: U256) -> sp_core::H256 {
            let tmp = index.to_big_endian();
            pallet_evm::AccountStorages::<Runtime>::get(address, sp_core::H256::from(tmp))
        }

        fn call(
            from: H160,
            to: H160,
            data: Vec<u8>,
            value: U256,
            gas_limit: U256,
            max_fee_per_gas: Option<U256>,
            max_priority_fee_per_gas: Option<U256>,
            nonce: Option<U256>,
            estimate: bool,
            access_list: Option<Vec<(H160, Vec<sp_core::H256>)>>,
            authorization_list: Option<ethereum::AuthorizationList>,
        ) -> Result<fp_evm::ExecutionInfoV2<Vec<u8>>, sp_runtime::DispatchError> {
            use pallet_evm::GasWeightMapping as _;
            use codec::Encode as _;
            use pallet_evm::runner::Runner as _;

            let config = if estimate {
                let mut config = <Runtime as pallet_evm::Config>::config().clone();
                config.estimate = true;
                Some(config)
            } else {
                None
            };

            let mut estimated_transaction_len = data.len() + 259;
            if access_list.is_some() {
                estimated_transaction_len += access_list.encoded_size();
            }
            if authorization_list.is_some() {
                estimated_transaction_len += authorization_list.encoded_size();
            }

            let gas_limit = if gas_limit > U256::from(u64::MAX) {
                u64::MAX
            } else {
                gas_limit.low_u64()
            };
            let without_base_extrinsic_weight = true;

            let (weight_limit, proof_size_base_cost) =
                match <Runtime as pallet_evm::Config>::GasWeightMapping::gas_to_weight(
                    gas_limit,
                    without_base_extrinsic_weight,
                ) {
                    weight_limit if weight_limit.proof_size() > 0 => {
                        (Some(weight_limit), Some(estimated_transaction_len as u64))
                    }
                    _ => (None, None),
                };

            <Runtime as pallet_evm::Config>::Runner::call(
                from,
                to,
                data,
                value,
                gas_limit.unique_saturated_into(),
                max_fee_per_gas,
                max_priority_fee_per_gas,
                nonce,
                access_list.unwrap_or_default(),
                authorization_list.unwrap_or_default(),
                false,
                true,
                weight_limit,
                proof_size_base_cost,
                config.as_ref().unwrap_or(<Runtime as pallet_evm::Config>::config()),
            )
            .map_err(|err| err.error.into())
        }

        fn create(
            from: H160,
            data: Vec<u8>,
            value: U256,
            gas_limit: U256,
            max_fee_per_gas: Option<U256>,
            max_priority_fee_per_gas: Option<U256>,
            nonce: Option<U256>,
            estimate: bool,
            access_list: Option<Vec<(H160, Vec<sp_core::H256>)>>,
            authorization_list: Option<ethereum::AuthorizationList>,
        ) -> Result<fp_evm::ExecutionInfoV2<H160>, sp_runtime::DispatchError> {
            use pallet_evm::GasWeightMapping as _;
            use codec::Encode as _;
            use pallet_evm::runner::Runner as _;

            let config = if estimate {
                let mut config = <Runtime as pallet_evm::Config>::config().clone();
                config.estimate = true;
                Some(config)
            } else {
                None
            };

            let mut estimated_transaction_len = data.len() + 190;
            if max_fee_per_gas.is_some() {
                estimated_transaction_len += 32;
            }
            if max_priority_fee_per_gas.is_some() {
                estimated_transaction_len += 32;
            }
            if access_list.is_some() {
                estimated_transaction_len += access_list.encoded_size();
            }
            if authorization_list.is_some() {
                estimated_transaction_len += authorization_list.encoded_size();
            }

            let gas_limit = if gas_limit > U256::from(u64::MAX) {
                u64::MAX
            } else {
                gas_limit.low_u64()
            };
            let without_base_extrinsic_weight = true;

            let (weight_limit, proof_size_base_cost) =
                match <Runtime as pallet_evm::Config>::GasWeightMapping::gas_to_weight(
                    gas_limit,
                    without_base_extrinsic_weight,
                ) {
                    weight_limit if weight_limit.proof_size() > 0 => {
                        (Some(weight_limit), Some(estimated_transaction_len as u64))
                    }
                    _ => (None, None),
                };

            <Runtime as pallet_evm::Config>::Runner::create(
                from,
                data,
                value,
                gas_limit.unique_saturated_into(),
                max_fee_per_gas,
                max_priority_fee_per_gas,
                nonce,
                access_list.unwrap_or_default(),
                authorization_list.unwrap_or_default(),
                false,
                true,
                weight_limit,
                proof_size_base_cost,
                config.as_ref().unwrap_or(<Runtime as pallet_evm::Config>::config()),
            )
            .map_err(|err| err.error.into())
        }

        fn current_transaction_statuses() -> Option<Vec<fp_rpc::TransactionStatus>> {
            pallet_ethereum::CurrentTransactionStatuses::<Runtime>::get()
        }

        fn current_block() -> Option<pallet_ethereum::Block> {
            pallet_ethereum::CurrentBlock::<Runtime>::get()
        }

        fn current_receipts() -> Option<Vec<pallet_ethereum::Receipt>> {
            pallet_ethereum::CurrentReceipts::<Runtime>::get()
        }

        fn current_all() -> (
            Option<pallet_ethereum::Block>,
            Option<Vec<pallet_ethereum::Receipt>>,
            Option<Vec<fp_rpc::TransactionStatus>>,
        ) {
            (
                pallet_ethereum::CurrentBlock::<Runtime>::get(),
                pallet_ethereum::CurrentReceipts::<Runtime>::get(),
                pallet_ethereum::CurrentTransactionStatuses::<Runtime>::get(),
            )
        }

        fn extrinsic_filter(
            xts: Vec<<Block as BlockT>::Extrinsic>,
        ) -> Vec<EthereumTransaction> {
            xts.into_iter().filter_map(|xt| match xt.0.function {
                RuntimeCall::Ethereum(pallet_ethereum::Call::transact { transaction }) => Some(transaction),
                _ => None,
            }).collect::<Vec<EthereumTransaction>>()
        }

        fn elasticity() -> Option<Permill> {
            Some(pallet_base_fee::Elasticity::<Runtime>::get())
        }

        fn gas_limit_multiplier_support() {}

        fn pending_block(
            xts: Vec<<Block as BlockT>::Extrinsic>,
        ) -> (Option<pallet_ethereum::Block>, Option<Vec<fp_rpc::TransactionStatus>>) {
            for ext in xts.into_iter() {
                let _ = Executive::apply_extrinsic(ext);
            }

            {
                use frame_support::traits::Hooks;
                pallet_ethereum::Pallet::<Runtime>::on_finalize(System::block_number() + 1);
            }

            (
                pallet_ethereum::CurrentBlock::<Runtime>::get(),
                pallet_ethereum::CurrentTransactionStatuses::<Runtime>::get(),
            )
        }

        fn initialize_pending_block(header: &<Block as BlockT>::Header) {
            Executive::initialize_block(header);
        }
    }

    impl fp_rpc::ConvertTransactionRuntimeApi<Block> for Runtime {
        fn convert_transaction(
            transaction: EthereumTransaction,
        ) -> <Block as BlockT>::Extrinsic {
            fp_self_contained::UncheckedExtrinsic(generic::UncheckedExtrinsic::new_bare(
                pallet_ethereum::Call::<Runtime>::transact { transaction }.into(),
            ))
        }
    }
}
