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
    use cipher_merkle::{count_is_false, merkle_root, MerkleLeaf};
    use codec::DecodeWithMemTracking;
    use frame_support::pallet_prelude::*;
    use frame_support::traits::Currency;
    use frame_system::pallet_prelude::*;
    use sp_runtime::traits::UniqueSaturatedInto;
    use sp_runtime::Saturating;

    #[pallet::pallet]
    pub struct Pallet<T>(_);

    #[pallet::config]
    pub trait Config:
        frame_system::Config<RuntimeEvent: From<Event<Self>>> + pallet_shard_registry::Config
    {
        /// Maximum age (in blocks) for submitting a challenge.
        /// Default: 7 days = 100800 blocks at 6s/block.
        #[pallet::constant]
        type ChallengeWindow: Get<u32>;

        /// Balance burned from the anchor poster on a proven false anchor.
        type Currency: Currency<Self::AccountId>;

        /// Slash amount in the smallest balance unit. The runtime sets 100 TKS.
        #[pallet::constant]
        type AnchorSlash: Get<u128>;
    }

    type BalanceOf<T> =
        <<T as Config>::Currency as Currency<<T as frame_system::Config>::AccountId>>::Balance;

    /// One batch leaf. Lengths match `cipher_merkle::MAX_LEAF_FIELD_LEN`.
    #[derive(Clone, Encode, Decode, DecodeWithMemTracking, TypeInfo, MaxEncodedLen, Debug, PartialEq, Eq)]
    pub struct AnchorLeaf {
        pub msg_id: BoundedVec<u8, ConstU32<128>>,
        pub sender_id: BoundedVec<u8, ConstU32<128>>,
        pub recipient_id: BoundedVec<u8, ConstU32<128>>,
        pub ciphertext_hash: [u8; 32],
        pub stored_at: u64,
    }

    /// Poster and declared count for a root. Separate from [`AnchorEntry`] so a
    /// runtime upgrade does not change anchors already stored on spec 412.
    #[derive(Clone, Encode, Decode, DecodeWithMemTracking, TypeInfo, MaxEncodedLen, Debug, PartialEq, Eq)]
    pub struct AnchorCommitment<AccountId, BlockNumber> {
        pub poster: AccountId,
        pub message_count: u32,
        pub posted_at: BlockNumber,
        pub slashed: bool,
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

    /// `dag_root_hash` → poster and declared count.
    #[pallet::storage]
    #[pallet::getter(fn anchor_by_root)]
    pub type AnchorByRoot<T: Config> = StorageMap<
        _,
        Blake2_128Concat,
        [u8; 32],
        AnchorCommitment<T::AccountId, BlockNumberFor<T>>,
    >;

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
        /// Poster balance was burned for a false anchor.
        AnchorSlashed {
            dag_root_hash: [u8; 32],
            poster: T::AccountId,
            declared_count: u32,
            proven_count: u32,
            burned: u128,
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
        /// Shard id is out of range for the active shard set.
        InvalidShardId,
        /// Supplied leaves do not hash to the committed root.
        InvalidMerkleProof,
        /// Leaves reproduce the root and the declared count. Not fraud.
        CountMatchesCommitment,
        /// This root was already slashed.
        AnchorAlreadySlashed,
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
            let who = ensure_signed(origin)?;

            ensure!(message_count > 0, Error::<T>::ZeroMessageCount);
            ensure!(
                shard_id < pallet_shard_registry::Pallet::<T>::active_shard_count(),
                Error::<T>::InvalidShardId,
            );
            ensure!(
                pallet_shard_registry::Pallet::<T>::is_shard_validator(&who, shard_id),
                Error::<T>::NotShardValidator,
            );
            ensure!(
                !AnchorByRoot::<T>::contains_key(dag_root_hash),
                Error::<T>::AnchorAlreadyExists,
            );

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
            AnchorByRoot::<T>::insert(
                dag_root_hash,
                AnchorCommitment {
                    poster: who.clone(),
                    message_count,
                    posted_at: now,
                    slashed: false,
                },
            );

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

                let poster = if !valid {
                    AnchorByRoot::<T>::get(anchor_hash).map(|c| (c.poster, c.slashed))
                } else {
                    None
                };

                challenge.status = if valid {
                    ChallengeStatus::ResolvedValid
                } else {
                    ChallengeStatus::ResolvedInvalid
                };

                Self::deposit_event(Event::ChallengeResolved { anchor_hash, valid });

                if let Some((poster, already)) = poster {
                    if !already {
                        AnchorByRoot::<T>::mutate(anchor_hash, |maybe| {
                            if let Some(commitment) = maybe {
                                commitment.slashed = true;
                            }
                        });
                        // Burns AnchorSlash. Challenges have no deposit, so a
                        // false alarm does not slash the challenger.
                        Self::slash_poster(anchor_hash, &poster, 0, 0);
                    }
                }
                Ok(())
            })?;

            Ok(Pays::No.into())
        }

        /// Prove `message_count` is false for a committed root.
        ///
        /// The runtime recomputes the `tks-leaf-v1` Merkle root. If it matches
        /// the commitment and `leaves.len() != message_count`, the poster's
        /// slash amount is burned. A leaf set that does not reproduce the root
        /// is rejected and slashes nobody.
        #[pallet::call_index(3)]
        #[pallet::weight(Weight::from_parts(500_000_000, 0)
            .saturating_add(T::DbWeight::get().reads_writes(2, 2)))]
        pub fn prove_false_anchor(
            origin: OriginFor<T>,
            dag_root_hash: [u8; 32],
            leaves: BoundedVec<AnchorLeaf, ConstU32<512>>,
        ) -> DispatchResultWithPostInfo {
            let _who = ensure_signed(origin)?;
            ensure!(!leaves.is_empty(), Error::<T>::ZeroMessageCount);

            let plain: alloc::vec::Vec<MerkleLeaf> = leaves
                .iter()
                .map(|leaf| MerkleLeaf {
                    msg_id: leaf.msg_id.to_vec(),
                    sender_id: leaf.sender_id.to_vec(),
                    recipient_id: leaf.recipient_id.to_vec(),
                    ciphertext_hash: leaf.ciphertext_hash,
                    stored_at: leaf.stored_at,
                })
                .collect();
            ensure!(merkle_root(&plain) == dag_root_hash, Error::<T>::InvalidMerkleProof);

            AnchorByRoot::<T>::try_mutate(dag_root_hash, |maybe| -> DispatchResult {
                let commitment = maybe.as_mut().ok_or(Error::<T>::AnchorNotFound)?;
                ensure!(!commitment.slashed, Error::<T>::AnchorAlreadySlashed);
                let declared = commitment.message_count;
                ensure!(
                    count_is_false(declared, leaves.len()),
                    Error::<T>::CountMatchesCommitment
                );
                let poster = commitment.poster.clone();
                let proven = leaves.len() as u32;
                commitment.slashed = true;
                Self::slash_poster(dag_root_hash, &poster, declared, proven);
                Ok(())
            })?;

            Ok(Pays::No.into())
        }
    }

    impl<T: Config> Pallet<T> {
        /// Burn `AnchorSlash` from `poster`. Dropping the negative imbalance
        /// reduces total issuance. Caller marks the commitment slashed.
        fn slash_poster(
            dag_root_hash: [u8; 32],
            poster: &T::AccountId,
            declared_count: u32,
            proven_count: u32,
        ) {
            let requested: BalanceOf<T> = T::AnchorSlash::get().unique_saturated_into();
            let (imbalance, missing) = <T as Config>::Currency::slash(poster, requested);
            let burned = requested.saturating_sub(missing);
            drop(imbalance);
            let burned_u128: u128 = burned.unique_saturated_into();
            Self::deposit_event(Event::AnchorSlashed {
                dag_root_hash,
                poster: poster.clone(),
                declared_count,
                proven_count,
                burned: burned_u128,
            });
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
