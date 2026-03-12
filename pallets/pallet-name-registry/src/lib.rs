//! # TKS Name Registry Pallet
//!
//! Provides optional on-chain global username registration for Cipher identities.
//! Users register a human-readable `@username` that maps to their Substrate AccountId.
//!
//! ## Features
//! - Register a unique username (3–32 chars, alphanumeric + underscore)
//! - Release a username
//! - Reverse lookup (AccountId → username)
//! - Registration fee burned or sent to treasury
//! - Username transfer support

#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;

pub use pallet::*;

#[frame_support::pallet]
pub mod pallet {
    use frame_support::{
        pallet_prelude::*,
        traits::{Currency, ExistenceRequirement, ReservableCurrency},
    };
    use frame_system::pallet_prelude::*;
    use alloc::vec::Vec;
    use sp_runtime::traits::Zero;

    type BalanceOf<T> =
        <<T as Config>::Currency as Currency<<T as frame_system::Config>::AccountId>>::Balance;

    #[pallet::pallet]
    pub struct Pallet<T>(_);

    #[pallet::config]
    pub trait Config: frame_system::Config<RuntimeEvent: From<Event<Self>>> {

        /// Currency used for registration fees.
        type Currency: Currency<Self::AccountId> + ReservableCurrency<Self::AccountId>;

        /// Maximum length for a username (default: 32).
        #[pallet::constant]
        type MaxNameLength: Get<u32>;

        /// Minimum length for a username (default: 3).
        #[pallet::constant]
        type MinNameLength: Get<u32>;

        /// Fee charged for registering a username (in TKS).
        #[pallet::constant]
        type RegistrationFee: Get<BalanceOf<Self>>;

        /// Treasury account that receives registration fees.
        /// If None, the fee is burned.
        type TreasuryAccount: Get<Option<Self::AccountId>>;
    }

    // ─── Storage ───────────────────────────────────────────────────────

    /// Maps username bytes → owner account.
    #[pallet::storage]
    #[pallet::getter(fn names)]
    pub type Names<T: Config> = StorageMap<
        _,
        Blake2_128Concat,
        BoundedVec<u8, T::MaxNameLength>,
        T::AccountId,
    >;

    /// Reverse lookup: account → username.
    #[pallet::storage]
    #[pallet::getter(fn reverse_lookup)]
    pub type ReverseLookup<T: Config> = StorageMap<
        _,
        Blake2_128Concat,
        T::AccountId,
        BoundedVec<u8, T::MaxNameLength>,
    >;

    /// Total number of registered usernames.
    #[pallet::storage]
    #[pallet::getter(fn total_names)]
    pub type TotalNames<T: Config> = StorageValue<_, u64, ValueQuery>;

    // ─── Events ────────────────────────────────────────────────────────

    #[pallet::event]
    #[pallet::generate_deposit(pub(super) fn deposit_event)]
    pub enum Event<T: Config> {
        /// A username was registered. \[name, owner\]
        NameRegistered {
            name: BoundedVec<u8, T::MaxNameLength>,
            owner: T::AccountId,
        },
        /// A username was released. \[name, owner\]
        NameReleased {
            name: BoundedVec<u8, T::MaxNameLength>,
            owner: T::AccountId,
        },
        /// A username was transferred to a new owner. \[name, old_owner, new_owner\]
        NameTransferred {
            name: BoundedVec<u8, T::MaxNameLength>,
            old_owner: T::AccountId,
            new_owner: T::AccountId,
        },
    }

    // ─── Errors ────────────────────────────────────────────────────────

    #[pallet::error]
    pub enum Error<T> {
        /// Username is too short (< MinNameLength).
        NameTooShort,
        /// Username is too long (> MaxNameLength).
        NameTooLong,
        /// Username is already taken.
        NameTaken,
        /// Username contains invalid characters (only a-z, 0-9, _ allowed).
        InvalidCharacter,
        /// Caller does not own this username.
        NotOwner,
        /// Insufficient balance to pay the registration fee.
        InsufficientBalance,
        /// Account already has a registered username.
        AlreadyRegistered,
        /// Username does not exist.
        NameNotFound,
    }

    // ─── Extrinsics ────────────────────────────────────────────────────

    #[pallet::call]
    impl<T: Config> Pallet<T> {
        /// Register a global username (optional, costs a small fee).
        ///
        /// - Username must be 3–32 characters, lowercase alphanumeric + underscore only.
        /// - Registration fee is sent to the treasury (or burned if no treasury configured).
        /// - Each account can only have ONE username at a time.
        #[pallet::call_index(0)]
        #[pallet::weight(Weight::from_parts(50_000_000, 0).saturating_add(T::DbWeight::get().reads_writes(3, 3)))]
        pub fn register(
            origin: OriginFor<T>,
            name: BoundedVec<u8, T::MaxNameLength>,
        ) -> DispatchResult {
            let who = ensure_signed(origin)?;

            // Validate name length
            ensure!(
                name.len() >= T::MinNameLength::get() as usize,
                Error::<T>::NameTooShort
            );

            // Validate characters (lowercase a-z, 0-9, underscore only)
            for &byte in name.iter() {
                ensure!(
                    byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_',
                    Error::<T>::InvalidCharacter
                );
            }

            // Check name availability
            ensure!(!Names::<T>::contains_key(&name), Error::<T>::NameTaken);

            // Check account doesn't already have a username
            ensure!(
                !ReverseLookup::<T>::contains_key(&who),
                Error::<T>::AlreadyRegistered
            );

            // Charge registration fee
            let fee = T::RegistrationFee::get();
            if !fee.is_zero() {
                match T::TreasuryAccount::get() {
                    Some(treasury) => {
                        // Transfer fee to treasury
                        T::Currency::transfer(
                            &who,
                            &treasury,
                            fee,
                            ExistenceRequirement::KeepAlive,
                        )?;
                    }
                    None => {
                        // Burn the fee (slash from free balance)
                        let _ = T::Currency::withdraw(
                            &who,
                            fee,
                            frame_support::traits::WithdrawReasons::FEE,
                            ExistenceRequirement::KeepAlive,
                        )?;
                    }
                }
            }

            // Store the registration
            Names::<T>::insert(&name, &who);
            ReverseLookup::<T>::insert(&who, &name);
            TotalNames::<T>::mutate(|n| *n = n.saturating_add(1));

            Self::deposit_event(Event::NameRegistered {
                name,
                owner: who,
            });

            Ok(())
        }

        /// Release your username, freeing it for others to register.
        #[pallet::call_index(1)]
        #[pallet::weight(Weight::from_parts(30_000_000, 0).saturating_add(T::DbWeight::get().reads_writes(2, 3)))]
        pub fn release(
            origin: OriginFor<T>,
            name: BoundedVec<u8, T::MaxNameLength>,
        ) -> DispatchResult {
            let who = ensure_signed(origin)?;

            let owner = Names::<T>::get(&name).ok_or(Error::<T>::NameNotFound)?;
            ensure!(owner == who, Error::<T>::NotOwner);

            Names::<T>::remove(&name);
            ReverseLookup::<T>::remove(&who);
            TotalNames::<T>::mutate(|n| *n = n.saturating_sub(1));

            Self::deposit_event(Event::NameReleased {
                name,
                owner: who,
            });

            Ok(())
        }

        /// Transfer your username to another account.
        ///
        /// The new owner must NOT already have a username registered.
        #[pallet::call_index(2)]
        #[pallet::weight(Weight::from_parts(40_000_000, 0).saturating_add(T::DbWeight::get().reads_writes(3, 4)))]
        pub fn transfer(
            origin: OriginFor<T>,
            name: BoundedVec<u8, T::MaxNameLength>,
            new_owner: T::AccountId,
        ) -> DispatchResult {
            let who = ensure_signed(origin)?;

            let owner = Names::<T>::get(&name).ok_or(Error::<T>::NameNotFound)?;
            ensure!(owner == who, Error::<T>::NotOwner);

            // New owner must not already have a username
            ensure!(
                !ReverseLookup::<T>::contains_key(&new_owner),
                Error::<T>::AlreadyRegistered
            );

            // Update ownership
            Names::<T>::insert(&name, &new_owner);
            ReverseLookup::<T>::remove(&who);
            ReverseLookup::<T>::insert(&new_owner, &name);

            Self::deposit_event(Event::NameTransferred {
                name,
                old_owner: who,
                new_owner,
            });

            Ok(())
        }
    }

    // ─── Genesis Config ────────────────────────────────────────────────

    #[pallet::genesis_config]
    #[derive(frame_support::DefaultNoBound)]
    pub struct GenesisConfig<T: Config> {
        /// Pre-registered usernames at genesis: (name_bytes, owner_account).
        pub names: Vec<(Vec<u8>, T::AccountId)>,
    }

    #[pallet::genesis_build]
    impl<T: Config> BuildGenesisConfig for GenesisConfig<T> {
        fn build(&self) {
            for (name_bytes, owner) in &self.names {
                let name: BoundedVec<u8, T::MaxNameLength> = name_bytes
                    .clone()
                    .try_into()
                    .expect("Genesis name too long");
                Names::<T>::insert(&name, owner);
                ReverseLookup::<T>::insert(owner, &name);
                TotalNames::<T>::mutate(|n| *n = n.saturating_add(1));
            }
        }
    }
}
