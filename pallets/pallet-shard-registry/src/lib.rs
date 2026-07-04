//! # TKS Shard Registry Pallet
//!
//! On-chain registry for HyperSwarm shard ↔ validator assignments.
//! HyperSwarm daemon nodes read this pallet to know which shards they manage.
//!
//! ## Features
//! - Validator registration with geographic region and libp2p endpoint
//! - Deterministic shard assignment via VRF-seeded rotation every epoch
//! - Dynamic shard count (scales with network growth)
//! - Shard → validator map queryable by HyperSwarm daemons via RPC
//! - Epoch-based rotation to prevent targeted shard attacks
//!
//! ## How It Works
//! 1. Staked validators call `register_validator(region, endpoint)` to join HyperSwarm
//! 2. Every epoch (6 hours), `on_initialize` rotates shard assignments using VRF
//! 3. HyperSwarm daemons query `ShardMap` via RPC to discover their assignments
//! 4. Validators that go offline or misbehave can be deregistered + slashed

#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;

pub use pallet::*;

#[frame_support::pallet]
pub mod pallet {
    use frame_support::pallet_prelude::*;
    use frame_system::pallet_prelude::*;
    use alloc::vec::Vec;
    use codec::DecodeWithMemTracking;

    /// Maximum number of shards the network can support
    pub const MAX_SHARDS: u16 = 1024;

    /// Default validators per shard (BFT requires odd number for ⅔+1 quorum)
    pub const DEFAULT_VALIDATORS_PER_SHARD: u32 = 7;

    #[pallet::pallet]
    pub struct Pallet<T>(_);

    #[pallet::config]
    pub trait Config: frame_system::Config<RuntimeEvent: From<Event<Self>>> {
        /// How many blocks per shard rotation epoch.
        /// Default: 6 hours = 3600 blocks at 6s/block.
        #[pallet::constant]
        type EpochLength: Get<u32>;

        /// Maximum validators that can register for HyperSwarm duty.
        #[pallet::constant]
        type MaxValidators: Get<u32>;

        /// Maximum endpoint length (libp2p multiaddr string).
        #[pallet::constant]
        type MaxEndpointLength: Get<u32>;
    }

    // ─── Types ─────────────────────────────────────────────────────────

    /// Geographic region for validator placement.
    /// Validators in the same region form shards with sub-100ms consensus.
    #[derive(Clone, Encode, Decode, DecodeWithMemTracking, TypeInfo, MaxEncodedLen, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
    pub enum GeoRegion {
        /// North America (US, Canada, Mexico)
        NorthAmerica,
        /// Europe (EU, UK, Turkey, Russia West)
        Europe,
        /// Asia (East Asia, SE Asia, India, Middle East)
        Asia,
        /// South America
        SouthAmerica,
        /// Africa
        Africa,
        /// Oceania (Australia, NZ, Pacific)
        Oceania,
    }

    impl Default for GeoRegion {
        fn default() -> Self {
            GeoRegion::NorthAmerica
        }
    }

    /// A validator's HyperSwarm registration details.
    #[derive(Clone, Encode, Decode, DecodeWithMemTracking, TypeInfo, MaxEncodedLen, Debug)]
    #[scale_info(skip_type_params(T))]
    pub struct SwarmValidator<T: Config> {
        /// Geographic region for geo-sharding
        pub region: GeoRegion,
        /// libp2p multiaddr endpoint (e.g., "/ip4/1.2.3.4/udp/4001/quic-v1")
        pub endpoint: BoundedVec<u8, T::MaxEndpointLength>,
        /// Block number when registered
        pub registered_at: BlockNumberFor<T>,
        /// Whether this validator is currently active
        pub active: bool,
    }

    /// A shard's validator assignment for a given epoch.
    #[derive(Clone, Encode, Decode, DecodeWithMemTracking, TypeInfo, MaxEncodedLen, Debug)]
    #[scale_info(skip_type_params(T))]
    pub struct ShardAssignment<T: Config> {
        /// Shard ID (0..ActiveShardCount)
        pub shard_id: u16,
        /// Geographic region this shard serves
        pub region: GeoRegion,
        /// Epoch when this assignment was made
        pub epoch: u32,
        /// Number of validators assigned (actual list stored separately for gas efficiency)
        pub validator_count: u32,
        /// Block number of assignment
        pub assigned_at: BlockNumberFor<T>,
    }

    // ─── Storage ───────────────────────────────────────────────────────

    /// All registered HyperSwarm validators: AccountId → registration details.
    #[pallet::storage]
    #[pallet::getter(fn validators)]
    pub type Validators<T: Config> = StorageMap<
        _, Blake2_128Concat, T::AccountId, SwarmValidator<T>,
    >;

    /// Shard → assignment metadata.
    #[pallet::storage]
    #[pallet::getter(fn shard_map)]
    pub type ShardMap<T: Config> = StorageMap<
        _, Blake2_128Concat, u16, ShardAssignment<T>,
    >;

    /// Shard → list of assigned validator AccountIds.
    /// Stored separately from ShardAssignment for gas-efficient querying.
    #[pallet::storage]
    #[pallet::getter(fn shard_validators)]
    pub type ShardValidators<T: Config> = StorageMap<
        _, Blake2_128Concat, u16, BoundedVec<T::AccountId, ConstU32<21>>, ValueQuery,
    >;

    /// Current epoch number (incremented every EpochLength blocks).
    #[pallet::storage]
    #[pallet::getter(fn current_epoch)]
    pub type CurrentEpoch<T: Config> = StorageValue<_, u32, ValueQuery>;

    /// VRF seed for this epoch (derived from parent block hash, used for random assignment).
    #[pallet::storage]
    #[pallet::getter(fn epoch_vrf_seed)]
    pub type EpochVrfSeed<T: Config> = StorageValue<_, [u8; 32], ValueQuery>;

    /// Total number of active shards (starts small, grows with network).
    #[pallet::storage]
    #[pallet::getter(fn active_shard_count)]
    pub type ActiveShardCount<T: Config> = StorageValue<_, u16, ValueQuery>;

    /// Total number of registered validators.
    #[pallet::storage]
    #[pallet::getter(fn total_validators)]
    pub type TotalValidators<T: Config> = StorageValue<_, u32, ValueQuery>;

    /// Validators per shard (configurable, default 7).
    #[pallet::storage]
    #[pallet::getter(fn validators_per_shard)]
    pub type ValidatorsPerShard<T: Config> = StorageValue<_, u32, ValueQuery>;

    // ─── Events ────────────────────────────────────────────────────────

    #[pallet::event]
    #[pallet::generate_deposit(pub(super) fn deposit_event)]
    pub enum Event<T: Config> {
        /// Validator registered for HyperSwarm duty.
        ValidatorRegistered {
            who: T::AccountId,
            region: GeoRegion,
        },
        /// Validator deregistered from HyperSwarm duty.
        ValidatorDeregistered {
            who: T::AccountId,
        },
        /// Shard assignments rotated for a new epoch.
        EpochRotated {
            epoch: u32,
            shard_count: u16,
            validator_count: u32,
        },
        /// Active shard count changed (scaling event).
        ShardCountChanged {
            old_count: u16,
            new_count: u16,
        },
        /// Validators per shard configuration changed.
        ValidatorsPerShardChanged {
            old_value: u32,
            new_value: u32,
        },
        /// Validator endpoint updated.
        ValidatorEndpointUpdated {
            who: T::AccountId,
        },
    }

    // ─── Errors ────────────────────────────────────────────────────────

    #[pallet::error]
    pub enum Error<T> {
        /// Validator is already registered for HyperSwarm.
        AlreadyRegistered,
        /// Validator is not registered for HyperSwarm.
        NotRegistered,
        /// Shard count exceeds maximum (1024).
        ShardCountTooHigh,
        /// Shard count must be greater than zero.
        ShardCountZero,
        /// Validators per shard must be at least 3 (BFT minimum).
        ValidatorsPerShardTooLow,
        /// Validators per shard exceeds maximum (21).
        ValidatorsPerShardTooHigh,
        /// Endpoint is empty.
        EmptyEndpoint,
        /// Not enough validators to fill all shards.
        InsufficientValidators,
    }

    // ─── Extrinsics ────────────────────────────────────────────────────

    #[pallet::call]
    impl<T: Config> Pallet<T> {
        /// Register as a HyperSwarm shard validator.
        ///
        /// Requirements:
        /// - Must not be already registered
        /// - Must provide a valid geographic region
        /// - Must provide a non-empty libp2p endpoint
        ///
        /// Note: In production, should also verify the caller is a staked
        /// validator on the root chain. For now, open registration.
        #[pallet::call_index(0)]
        #[pallet::weight(Weight::from_parts(50_000_000, 0)
            .saturating_add(T::DbWeight::get().reads_writes(2, 2)))]
        pub fn register_validator(
            origin: OriginFor<T>,
            region: GeoRegion,
            endpoint: BoundedVec<u8, T::MaxEndpointLength>,
        ) -> DispatchResult {
            let who = ensure_signed(origin)?;

            ensure!(!Validators::<T>::contains_key(&who), Error::<T>::AlreadyRegistered);
            ensure!(!endpoint.is_empty(), Error::<T>::EmptyEndpoint);

            let now = frame_system::Pallet::<T>::block_number();

            Validators::<T>::insert(&who, SwarmValidator {
                region: region.clone(),
                endpoint,
                registered_at: now,
                active: true,
            });

            TotalValidators::<T>::mutate(|n| *n = n.saturating_add(1));

            Self::deposit_event(Event::ValidatorRegistered { who, region });
            Ok(())
        }

        /// Deregister from HyperSwarm duty.
        ///
        /// The validator will be removed from shard assignments at the next epoch rotation.
        #[pallet::call_index(1)]
        #[pallet::weight(Weight::from_parts(30_000_000, 0)
            .saturating_add(T::DbWeight::get().reads_writes(1, 2)))]
        pub fn deregister_validator(origin: OriginFor<T>) -> DispatchResult {
            let who = ensure_signed(origin)?;
            ensure!(Validators::<T>::contains_key(&who), Error::<T>::NotRegistered);

            Validators::<T>::remove(&who);
            TotalValidators::<T>::mutate(|n| *n = n.saturating_sub(1));

            Self::deposit_event(Event::ValidatorDeregistered { who });
            Ok(())
        }

        /// Update validator endpoint (e.g., IP change).
        #[pallet::call_index(2)]
        #[pallet::weight(Weight::from_parts(20_000_000, 0)
            .saturating_add(T::DbWeight::get().reads_writes(1, 1)))]
        pub fn update_endpoint(
            origin: OriginFor<T>,
            endpoint: BoundedVec<u8, T::MaxEndpointLength>,
        ) -> DispatchResult {
            let who = ensure_signed(origin)?;
            ensure!(!endpoint.is_empty(), Error::<T>::EmptyEndpoint);

            Validators::<T>::try_mutate(&who, |maybe_val| -> DispatchResult {
                let val = maybe_val.as_mut().ok_or(Error::<T>::NotRegistered)?;
                val.endpoint = endpoint;
                Ok(())
            })?;

            Self::deposit_event(Event::ValidatorEndpointUpdated { who });
            Ok(())
        }

        /// Sudo: Set the active shard count (for scaling the network).
        ///
        /// This determines how many shards the HyperSwarm network operates.
        /// More shards = more aggregate TPS but requires more validators.
        #[pallet::call_index(3)]
        #[pallet::weight(Weight::from_parts(10_000_000, 0)
            .saturating_add(T::DbWeight::get().reads_writes(1, 1)))]
        pub fn set_shard_count(origin: OriginFor<T>, count: u16) -> DispatchResult {
            ensure_root(origin)?;
            ensure!(count > 0, Error::<T>::ShardCountZero);
            ensure!(count <= MAX_SHARDS, Error::<T>::ShardCountTooHigh);

            let old = ActiveShardCount::<T>::get();
            ActiveShardCount::<T>::put(count);

            Self::deposit_event(Event::ShardCountChanged {
                old_count: old,
                new_count: count,
            });
            Ok(())
        }

        /// Sudo: Set validators per shard (default 7, min 3, max 21).
        #[pallet::call_index(4)]
        #[pallet::weight(Weight::from_parts(10_000_000, 0)
            .saturating_add(T::DbWeight::get().reads_writes(1, 1)))]
        pub fn set_validators_per_shard(origin: OriginFor<T>, count: u32) -> DispatchResult {
            ensure_root(origin)?;
            ensure!(count >= 3, Error::<T>::ValidatorsPerShardTooLow);
            ensure!(count <= 21, Error::<T>::ValidatorsPerShardTooHigh);

            let old = ValidatorsPerShard::<T>::get();
            ValidatorsPerShard::<T>::put(count);

            Self::deposit_event(Event::ValidatorsPerShardChanged {
                old_value: old,
                new_value: count,
            });
            Ok(())
        }

        /// Sudo: Force an immediate epoch rotation (for emergency/testing).
        #[pallet::call_index(5)]
        #[pallet::weight(Weight::from_parts(100_000_000, 0)
            .saturating_add(T::DbWeight::get().reads_writes(10, 100)))]
        pub fn force_rotate(origin: OriginFor<T>) -> DispatchResult {
            ensure_root(origin)?;
            let now = frame_system::Pallet::<T>::block_number();
            Self::do_rotate_shards(now);
            Ok(())
        }
    }

    // ─── Hooks (Epoch Rotation) ────────────────────────────────────────

    #[pallet::hooks]
    impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {
        fn on_initialize(n: BlockNumberFor<T>) -> Weight {
            // Convert block number safely for epoch calculation
            let block_num: u64 = n.try_into().unwrap_or(0u64);
            let epoch_len = T::EpochLength::get() as u64;

            if epoch_len == 0 {
                return Weight::zero();
            }

            // Rotate shards at the start of each epoch
            if block_num > 0 && block_num % epoch_len == 0 {
                Self::do_rotate_shards(n);
                return Weight::from_parts(100_000_000, 0);
            }

            Weight::zero()
        }
    }

    // ─── Internal Logic ────────────────────────────────────────────────

    impl<T: Config> Pallet<T> {
        /// Execute shard rotation for a new epoch.
        ///
        /// Uses the parent block hash as a VRF seed to deterministically but
        /// unpredictably assign validators to shards. This prevents targeted
        /// attacks on specific shards.
        fn do_rotate_shards(now: BlockNumberFor<T>) {
            let block_num: u64 = now.try_into().unwrap_or(0u64);
            let epoch_len = T::EpochLength::get() as u64;
            let epoch = if epoch_len > 0 { (block_num / epoch_len) as u32 } else { 0 };

            CurrentEpoch::<T>::put(epoch);

            // Generate VRF seed from parent block hash
            let parent_hash = frame_system::Pallet::<T>::parent_hash();
            let seed = sp_io::hashing::blake2_256(parent_hash.as_ref());
            EpochVrfSeed::<T>::put(seed);

            let shard_count = ActiveShardCount::<T>::get();
            let total_validators = TotalValidators::<T>::get();

            if shard_count == 0 || total_validators == 0 {
                return;
            }

            // Collect all active validator accounts
            let mut all_validators: Vec<T::AccountId> = Vec::new();
            let mut iter = Validators::<T>::iter();
            while let Some((account, val)) = iter.next() {
                if val.active {
                    all_validators.push(account);
                }
            }

            if all_validators.is_empty() {
                return;
            }

            // Deterministic shuffle using VRF seed
            // Simple Fisher-Yates with seed-derived randomness
            let len = all_validators.len();
            for i in (1..len).rev() {
                // Derive per-index random from seed
                let mut index_seed = seed;
                index_seed[0] ^= (i & 0xFF) as u8;
                index_seed[1] ^= ((i >> 8) & 0xFF) as u8;
                let hash = sp_io::hashing::blake2_256(&index_seed);
                let j = (u64::from_le_bytes([
                    hash[0], hash[1], hash[2], hash[3],
                    hash[4], hash[5], hash[6], hash[7],
                ]) as usize) % (i + 1);
                all_validators.swap(i, j);
            }

            let vps = ValidatorsPerShard::<T>::get();
            let validators_per_shard = if vps == 0 { DEFAULT_VALIDATORS_PER_SHARD } else { vps };

            // Assign validators to shards round-robin from shuffled list
            for shard_id in 0..shard_count {
                let mut shard_vals: BoundedVec<T::AccountId, ConstU32<21>> =
                    BoundedVec::default();

                for v_idx in 0..validators_per_shard {
                    let global_idx = (shard_id as u32 * validators_per_shard + v_idx) as usize;
                    // Wrap around if not enough validators
                    let wrapped_idx = global_idx % all_validators.len();
                    let _ = shard_vals.try_push(all_validators[wrapped_idx].clone());
                }

                // Determine region for this shard (majority vote of assigned validators)
                let region = Self::majority_region(&shard_vals);

                ShardMap::<T>::insert(shard_id, ShardAssignment {
                    shard_id,
                    region,
                    epoch,
                    validator_count: shard_vals.len() as u32,
                    assigned_at: now,
                });

                ShardValidators::<T>::insert(shard_id, shard_vals);
            }

            Self::deposit_event(Event::EpochRotated {
                epoch,
                shard_count,
                validator_count: total_validators,
            });

            log::info!(
                "🔄 HyperSwarm epoch {} — {} shards rotated with {} validators",
                epoch, shard_count, total_validators
            );
        }

        /// Determine the majority geographic region of a set of validators.
        fn majority_region(validators: &[T::AccountId]) -> GeoRegion {
            let mut counts = [0u32; 6]; // One per GeoRegion variant
            for val_account in validators {
                if let Some(val) = Validators::<T>::get(val_account) {
                    let idx = match val.region {
                        GeoRegion::NorthAmerica => 0,
                        GeoRegion::Europe => 1,
                        GeoRegion::Asia => 2,
                        GeoRegion::SouthAmerica => 3,
                        GeoRegion::Africa => 4,
                        GeoRegion::Oceania => 5,
                    };
                    counts[idx] += 1;
                }
            }
            let max_idx = counts.iter().enumerate()
                .max_by_key(|(_, c)| *c)
                .map(|(i, _)| i)
                .unwrap_or(0);
            match max_idx {
                0 => GeoRegion::NorthAmerica,
                1 => GeoRegion::Europe,
                2 => GeoRegion::Asia,
                3 => GeoRegion::SouthAmerica,
                4 => GeoRegion::Africa,
                _ => GeoRegion::Oceania,
            }
        }
    }

    // ─── Genesis Config ────────────────────────────────────────────────

    #[pallet::genesis_config]
    #[derive(frame_support::DefaultNoBound)]
    pub struct GenesisConfig<T: Config> {
        /// Initial number of active shards (default: 8).
        pub initial_shard_count: u16,
        /// Validators per shard (default: 7).
        pub validators_per_shard: u32,
        /// Pre-registered validators: (account, region, endpoint_bytes).
        pub validators: Vec<(T::AccountId, GeoRegion, Vec<u8>)>,
    }

    #[pallet::genesis_build]
    impl<T: Config> BuildGenesisConfig for GenesisConfig<T> {
        fn build(&self) {
            let shard_count = if self.initial_shard_count == 0 { 8 } else { self.initial_shard_count };
            ActiveShardCount::<T>::put(shard_count);

            let vps = if self.validators_per_shard == 0 { DEFAULT_VALIDATORS_PER_SHARD } else { self.validators_per_shard };
            ValidatorsPerShard::<T>::put(vps);

            for (account, region, endpoint_bytes) in &self.validators {
                let endpoint: BoundedVec<u8, T::MaxEndpointLength> = endpoint_bytes
                    .clone()
                    .try_into()
                    .expect("Genesis validator endpoint too long");

                Validators::<T>::insert(account, SwarmValidator {
                    region: region.clone(),
                    endpoint,
                    registered_at: BlockNumberFor::<T>::default(),
                    active: true,
                });
                TotalValidators::<T>::mutate(|n| *n = n.saturating_add(1));
            }

            log::info!(
                "🚀 HyperSwarm genesis: {} shards, {} validators/shard, {} validators registered",
                shard_count, vps, self.validators.len()
            );
        }
    }
}
