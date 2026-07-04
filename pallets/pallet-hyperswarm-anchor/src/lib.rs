//! # TKS HyperSwarm Anchor Pallet
//!
//! Stores DAG Merkle root hashes on the TKS root chain, providing
//! cryptographic proof that messages existed and were processed by shard validators.
//!
//! ## Features
//! - Shard validators post batch anchor hashes every ~10 seconds
//! - Each anchor contains: shard_id, DAG Merkle root, message count, timestamp
//! - Fisherman protocol: any node can challenge a suspicious anchor
//! - Permanent proof-of-existence for dispute resolution
//! - Per-shard message counters for network health monitoring
//!
//! ## How It Works
//! 1. HyperSwarm shard validators reach BFT consensus on a batch of messages
//! 2. The shard leader computes a Merkle root over all DAG vertices in the batch
//! 3. Leader posts the anchor hash to this pallet via `post_anchor()`
//! 4. The anchor is stored on-chain permanently (only 32 bytes per batch)
//! 5. Any node can challenge a suspicious anchor via `submit_challenge()`
//! 6. Challenged anchors are resolved by root chain validators

#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;

pub use pallet::*;

#[frame_support::pallet]
pub mod pallet {
    use frame_support::pallet_prelude::*;
    use frame_system::pallet_prelude::*;
    use sp_runtime::Saturating;
    use codec::DecodeWithMemTracking;

    #[pallet::pallet]
    pub struct Pallet<T>(_);

    #[pallet::config]
    pub trait Config: frame_system::Config<RuntimeEvent: From<Event<Self>>> {
        /// Maximum age (in blocks) for submitting a challenge.
        /// Default: 7 days = 100800 blocks at 6s/block.
        #[pallet::constant]
        type ChallengeWindow: Get<u32>;
    }

    // ─── Types ─────────────────────────────────────────────────────────

    /// A batch anchor entry from a HyperSwarm shard.
    #[derive(Clone, Encode, Decode, DecodeWithMemTracking, TypeInfo, MaxEncodedLen, Debug)]
    pub struct AnchorEntry<BlockNumber> {
        /// Which shard posted this anchor
        pub shard_id: u16,
        /// Merkle root hash of all DAG vertices in this batch
        pub dag_root_hash: [u8; 32],
        /// Number of messages in this batch
        pub message_count: u32,
        /// Block number when posted
        pub posted_at: BlockNumber,
        /// Shard epoch when this batch was processed
        pub epoch: u32,
    }

    /// Challenge status
    #[derive(Clone, Encode, Decode, DecodeWithMemTracking, TypeInfo, MaxEncodedLen, Debug, PartialEq, Eq)]
    pub enum ChallengeStatus {
        /// Challenge is pending resolution
        Pending,
        /// Challenge resolved — anchor was valid
        ResolvedValid,
        /// Challenge resolved — anchor was invalid, shard validators slashed
        ResolvedInvalid,
    }

    /// A fisherman challenge against a suspicious anchor.
    #[derive(Clone, Encode, Decode, DecodeWithMemTracking, TypeInfo, MaxEncodedLen, Debug)]
    pub struct Challenge<AccountId, BlockNumber> {
        /// Who submitted the challenge
        pub challenger: AccountId,
        /// The shard being challenged
        pub shard_id: u16,
        /// The specific anchor hash being challenged
        pub anchor_hash: [u8; 32],
        /// Block number when challenge was submitted
        pub submitted_at: BlockNumber,
        /// Current status
        pub status: ChallengeStatus,
    }

    // ─── Storage ───────────────────────────────────────────────────────

    /// Shard → latest anchor entry.
    /// Only the most recent anchor per shard is stored for quick lookup.
    #[pallet::storage]
    #[pallet::getter(fn latest_anchor)]
    pub type LatestAnchors<T: Config> = StorageMap<
        _, Blake2_128Concat, u16, AnchorEntry<BlockNumberFor<T>>,
    >;

    /// Historical anchor index: (shard_id, epoch) → dag_root_hash.
    /// Allows querying past anchors for verification.
    #[pallet::storage]
    #[pallet::getter(fn anchor_history)]
    pub type AnchorHistory<T: Config> = StorageDoubleMap<
        _, Blake2_128Concat, u16, Blake2_128Concat, u32, [u8; 32],
    >;

    /// Total messages processed per shard (lifetime counter).
    #[pallet::storage]
    #[pallet::getter(fn shard_message_count)]
    pub type ShardMessageCount<T: Config> = StorageMap<
        _, Blake2_128Concat, u16, u64, ValueQuery,
    >;

    /// Total messages processed across all shards (lifetime counter).
    #[pallet::storage]
    #[pallet::getter(fn total_message_count)]
    pub type TotalMessageCount<T: Config> = StorageValue<_, u64, ValueQuery>;

    /// Total anchors posted (all shards, all time).
    #[pallet::storage]
    #[pallet::getter(fn total_anchors)]
    pub type TotalAnchors<T: Config> = StorageValue<_, u64, ValueQuery>;

    /// Active challenges: anchor_hash → Challenge.
    #[pallet::storage]
    #[pallet::getter(fn challenges)]
    pub type Challenges<T: Config> = StorageMap<
        _, Blake2_128Concat, [u8; 32], Challenge<T::AccountId, BlockNumberFor<T>>,
    >;

    /// Total number of challenges submitted (for metrics).
    #[pallet::storage]
    #[pallet::getter(fn total_challenges)]
    pub type TotalChallenges<T: Config> = StorageValue<_, u32, ValueQuery>;

    // ─── Events ────────────────────────────────────────────────────────

    #[pallet::event]
    #[pallet::generate_deposit(pub(super) fn deposit_event)]
    pub enum Event<T: Config> {
        /// New anchor posted by a shard.
        AnchorPosted {
            shard_id: u16,
            dag_root_hash: [u8; 32],
            message_count: u32,
            epoch: u32,
        },
        /// Fisherman challenge submitted against an anchor.
        ChallengeSubmitted {
            challenger: T::AccountId,
            shard_id: u16,
            anchor_hash: [u8; 32],
        },
        /// Challenge resolved.
        ChallengeResolved {
            anchor_hash: [u8; 32],
            valid: bool,
        },
    }

    // ─── Errors ────────────────────────────────────────────────────────

    #[pallet::error]
    pub enum Error<T> {
        /// Caller is not authorized to post anchors for this shard.
        NotShardValidator,
        /// An anchor with this hash already exists.
        AnchorAlreadyExists,
        /// Challenge for this anchor already exists.
        ChallengeAlreadyExists,
        /// The anchor being challenged doesn't exist.
        AnchorNotFound,
        /// Challenge window has expired (anchor is too old to challenge).
        ChallengeWindowExpired,
        /// Challenge not found.
        ChallengeNotFound,
        /// Challenge is not in pending status.
        ChallengeNotPending,
        /// Invalid message count (must be > 0).
        ZeroMessageCount,
    }

    // ─── Extrinsics ────────────────────────────────────────────────────

    #[pallet::call]
    impl<T: Config> Pallet<T> {
        /// Post a batch anchor hash from a HyperSwarm shard.
        ///
        /// Called by the shard leader after BFT consensus on a batch of messages.
        /// The dag_root_hash is a Merkle root over all MessageVertex IDs in the batch.
        ///
        /// Cost: This is a very small on-chain footprint (32 bytes hash + metadata).
        /// At 1024 shards posting every 10 seconds, this is ~100 anchor txs per
        /// 1-second block — well within root chain capacity.
        /// FREE: Pays::No — Cipher message anchoring is a core Cipher activity,
        /// not a financial transaction. No TKS required to anchor messages.
        #[pallet::call_index(0)]
        #[pallet::weight(Weight::from_parts(30_000_000, 0)
            .saturating_add(T::DbWeight::get().reads_writes(1, 4)))]
        pub fn post_anchor(
            origin: OriginFor<T>,
            shard_id: u16,
            dag_root_hash: [u8; 32],
            message_count: u32,
            epoch: u32,
        ) -> DispatchResultWithPostInfo {
            let _who = ensure_signed(origin)?;

            // TODO: Verify caller is an assigned validator for this shard
            // by reading from pallet-shard-registry. For now, open posting.

            ensure!(message_count > 0, Error::<T>::ZeroMessageCount);

            let now = frame_system::Pallet::<T>::block_number();

            let anchor = AnchorEntry {
                shard_id,
                dag_root_hash,
                message_count,
                posted_at: now,
                epoch,
            };

            // Update latest anchor for this shard
            LatestAnchors::<T>::insert(shard_id, anchor);

            // Store in history
            AnchorHistory::<T>::insert(shard_id, epoch, dag_root_hash);

            // Update counters
            ShardMessageCount::<T>::mutate(shard_id, |n| {
                *n = n.saturating_add(message_count as u64);
            });
            TotalMessageCount::<T>::mutate(|n| {
                *n = n.saturating_add(message_count as u64);
            });
            TotalAnchors::<T>::mutate(|n| *n = n.saturating_add(1));

            Self::deposit_event(Event::AnchorPosted {
                shard_id,
                dag_root_hash,
                message_count,
                epoch,
            });

            log::debug!(
                "⚓ Anchor posted: shard={}, epoch={}, msgs={}, root=0x{}",
                shard_id, epoch, message_count,
                hex_prefix(&dag_root_hash),
            );

            Ok(Pays::No.into())
        }

        /// Submit a fisherman challenge against a suspicious anchor.
        ///
        /// Any node can challenge an anchor if they believe the DAG root hash
        /// is invalid (e.g., messages were censored or forged).
        ///
        /// Challenges must be submitted within the ChallengeWindow (default: 7 days).
        /// FREE: Pays::No — challenging an anchor is a Cipher protocol action,
        /// not a financial transaction. Challengers should not pay to report fraud.
        #[pallet::call_index(1)]
        #[pallet::weight(Weight::from_parts(50_000_000, 0)
            .saturating_add(T::DbWeight::get().reads_writes(2, 2)))]
        pub fn submit_challenge(
            origin: OriginFor<T>,
            shard_id: u16,
            anchor_hash: [u8; 32],
        ) -> DispatchResultWithPostInfo {
            let who = ensure_signed(origin)?;

            // Verify the anchor exists
            let anchor = LatestAnchors::<T>::get(shard_id)
                .ok_or(Error::<T>::AnchorNotFound)?;

            // Check challenge window
            let now = frame_system::Pallet::<T>::block_number();
            let window: BlockNumberFor<T> = T::ChallengeWindow::get().into();
            let deadline = anchor.posted_at.saturating_add(window);
            ensure!(now <= deadline, Error::<T>::ChallengeWindowExpired);

            // Check no duplicate challenge
            ensure!(
                !Challenges::<T>::contains_key(&anchor_hash),
                Error::<T>::ChallengeAlreadyExists
            );

            Challenges::<T>::insert(anchor_hash, Challenge {
                challenger: who.clone(),
                shard_id,
                anchor_hash,
                submitted_at: now,
                status: ChallengeStatus::Pending,
            });

            TotalChallenges::<T>::mutate(|n| *n = n.saturating_add(1));

            Self::deposit_event(Event::ChallengeSubmitted {
                challenger: who,
                shard_id,
                anchor_hash,
            });

            Ok(Pays::No.into())
        }

        /// Sudo: Resolve a fisherman challenge.
        ///
        /// In production, this would be replaced by an automated verification
        /// process where root chain validators check the DAG proof.
        /// FREE: Pays::No — sudo governance action, no user fee.
        #[pallet::call_index(2)]
        #[pallet::weight(Weight::from_parts(30_000_000, 0)
            .saturating_add(T::DbWeight::get().reads_writes(1, 1)))]
        pub fn resolve_challenge(
            origin: OriginFor<T>,
            anchor_hash: [u8; 32],
            valid: bool,
        ) -> DispatchResultWithPostInfo {
            ensure_root(origin)?;

            Challenges::<T>::try_mutate(&anchor_hash, |maybe_challenge| -> DispatchResult {
                let challenge = maybe_challenge.as_mut().ok_or(Error::<T>::ChallengeNotFound)?;
                ensure!(
                    challenge.status == ChallengeStatus::Pending,
                    Error::<T>::ChallengeNotPending
                );

                challenge.status = if valid {
                    ChallengeStatus::ResolvedValid
                } else {
                    ChallengeStatus::ResolvedInvalid
                };

                Self::deposit_event(Event::ChallengeResolved { anchor_hash, valid });

                // TODO: If invalid, slash the shard validators via pallet-staking
                // TODO: If valid (false alarm), slash the challenger's deposit
                Ok(())
            })?;

            Ok(Pays::No.into())
        }
    }

    // ─── Helpers ───────────────────────────────────────────────────────

    /// Format first 4 bytes of a hash as hex for logging.
    fn hex_prefix(hash: &[u8; 32]) -> alloc::string::String {
        alloc::format!("{:02x}{:02x}{:02x}{:02x}...", hash[0], hash[1], hash[2], hash[3])
    }

    // ─── Genesis Config ────────────────────────────────────────────────

    #[pallet::genesis_config]
    #[derive(frame_support::DefaultNoBound)]
    pub struct GenesisConfig<T: Config> {
        /// Placeholder — no genesis anchors needed.
        pub _phantom: core::marker::PhantomData<T>,
    }

    #[pallet::genesis_build]
    impl<T: Config> BuildGenesisConfig for GenesisConfig<T> {
        fn build(&self) {
            TotalAnchors::<T>::put(0u64);
            TotalMessageCount::<T>::put(0u64);
            TotalChallenges::<T>::put(0u32);
            log::info!("⚓ HyperSwarm Anchor pallet initialized");
        }
    }
}
