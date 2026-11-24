#![no_std]
#![allow(clippy::too_many_arguments)]

extern crate alloc;
use alloc::vec::Vec as StdVec;

pub mod accounting;
pub mod encryption;
pub mod key_rotation;
pub mod sss;

#[cfg(test)]
mod test;

pub use accounting::{AssetBalance, LedgerEntry, LedgerEntryKind, TransferReceipt};
pub use common::{VaultPolicy, VaultRecord, VaultShare};
use identity::IdentityContractClient;
use key_rotation::RotationEvent;
use soroban_sdk::{
    contract, contracterror, contractimpl, contracttype, symbol_short, token, Address, BytesN, Env,
    String, Symbol, Vec,
};

const ADMIN: Symbol = symbol_short!("ADMIN");
const IDENTITY: Symbol = symbol_short!("IDENTITY");
const INIT: Symbol = symbol_short!("INIT");
const VAULT: Symbol = symbol_short!("VAULT");
const SHARE: Symbol = symbol_short!("SHARE");
const APR: Symbol = symbol_short!("EM_APR");

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum VaultError {
    NotInitialized = 1,
    AlreadyInitialized = 2,
    Unauthorized = 3,
    InvalidConfig = 4,
    VaultNotFound = 5,
    InsufficientShares = 6,
    InvalidShare = 7,
    EmergencyThresholdNotMet = 8,
    DeadmanNotReady = 9,
    /// The token has not been approved by the admin, or has been delisted.
    UnsupportedAsset = 10,
    /// Amount must be positive and must not overflow a balance.
    InvalidAmount = 11,
    /// The vault's balance in the asset is lower than the requested amount.
    InsufficientBalance = 12,
    /// Source and destination vault are the same.
    SelfTransfer = 13,
    /// The supported-asset registry is full (`accounting::MAX_SUPPORTED_ASSETS`).
    AssetLimitReached = 14,
    /// The token reported no increase in custody for a deposit.
    DepositNotReceived = 15,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VaultSnapshot {
    pub record: VaultRecord,
    pub shard_holders: Vec<Address>,
}

#[contract]
pub struct VaultContract;

#[contractimpl]
impl VaultContract {
    pub fn initialize(
        env: Env,
        admin: Address,
        identity_contract: Address,
    ) -> Result<(), VaultError> {
        if env.storage().instance().has(&INIT) {
            return Err(VaultError::AlreadyInitialized);
        }
        admin.require_auth();
        env.storage().instance().set(&ADMIN, &admin);
        env.storage().instance().set(&IDENTITY, &identity_contract);
        env.storage().instance().set(&INIT, &true);
        Ok(())
    }

    pub fn configure_vault(
        env: Env,
        owner: Address,
        threshold: u32,
        shard_count: u32,
        emergency_threshold: u32,
        inactivity_timeout_secs: u64,
        data_ref_hash: String,
        secret_seed: BytesN<32>,
    ) -> Result<VaultSnapshot, VaultError> {
        Self::require_init(&env)?;
        owner.require_auth();

        if threshold == 0
            || shard_count == 0
            || threshold > shard_count
            || emergency_threshold < threshold
            || emergency_threshold > shard_count
        {
            return Err(VaultError::InvalidConfig);
        }

        let identity: Address = env
            .storage()
            .instance()
            .get(&IDENTITY)
            .ok_or(VaultError::NotInitialized)?;
        let id_client = IdentityContractClient::new(&env, &identity);
        let guardians = id_client.get_guardians(&owner);
        if guardians.len() < shard_count {
            return Err(VaultError::InvalidConfig);
        }

        let policy = VaultPolicy {
            threshold,
            shard_count,
            emergency_threshold,
            inactivity_timeout_secs,
        };

        let now = env.ledger().timestamp();
        let record = VaultRecord {
            owner: owner.clone(),
            epoch: 1,
            policy: policy.clone(),
            data_ref_hash,
            created_at: now,
            last_activity_at: now,
            deadman_release_at: now.saturating_add(inactivity_timeout_secs),
        };
        env.storage()
            .persistent()
            .set(&(VAULT, owner.clone()), &record);

        let shares = sss::split(
            secret_seed.to_array(),
            threshold as u8,
            shard_count as u8,
            env.crypto().sha256(&secret_seed.into()).to_array(),
        );

        let mut holders = Vec::new(&env);
        for i in 0..shard_count {
            let guardian = guardians.get(i).ok_or(VaultError::InvalidConfig)?;
            let share = shares.get(i as usize).ok_or(VaultError::InvalidConfig)?;
            let s = VaultShare {
                guardian: guardian.clone(),
                x: share.x as u32,
                y: BytesN::from_array(&env, &share.y),
            };
            env.storage()
                .persistent()
                .set(&(SHARE, owner.clone(), record.epoch, guardian.clone()), &s);
            holders.push_back(guardian);
        }

        Ok(VaultSnapshot {
            record,
            shard_holders: holders,
        })
    }

    pub fn reconstruct_key(
        env: Env,
        requester: Address,
        owner: Address,
        shares: Vec<VaultShare>,
    ) -> Result<BytesN<32>, VaultError> {
        Self::require_init(&env)?;
        requester.require_auth();

        let record: VaultRecord = env
            .storage()
            .persistent()
            .get(&(VAULT, owner.clone()))
            .ok_or(VaultError::VaultNotFound)?;
        if shares.len() < record.policy.threshold {
            return Err(VaultError::InsufficientShares);
        }

        let mut verified = StdVec::new();
        for i in 0..record.policy.threshold {
            let s = shares.get(i).ok_or(VaultError::InsufficientShares)?;
            let stored: VaultShare = env
                .storage()
                .persistent()
                .get(&(SHARE, owner.clone(), record.epoch, s.guardian.clone()))
                .ok_or(VaultError::InvalidShare)?;
            if stored.x != s.x || stored.y != s.y {
                return Err(VaultError::InvalidShare);
            }
            verified.push(sss::Share {
                x: s.x as u8,
                y: s.y.to_array(),
            });
        }

        let secret = sss::reconstruct(&verified, record.policy.threshold as u8)
            .ok_or(VaultError::InsufficientShares)?;
        Ok(BytesN::from_array(&env, &secret))
    }

    pub fn submit_emergency_approval(
        env: Env,
        guardian: Address,
        owner: Address,
    ) -> Result<(), VaultError> {
        Self::require_init(&env)?;
        guardian.require_auth();

        let record: VaultRecord = env
            .storage()
            .persistent()
            .get(&(VAULT, owner.clone()))
            .ok_or(VaultError::VaultNotFound)?;
        let stored: Option<VaultShare> =
            env.storage()
                .persistent()
                .get(&(SHARE, owner.clone(), record.epoch, guardian.clone()));
        if stored.is_none() {
            return Err(VaultError::Unauthorized);
        }
        env.storage()
            .persistent()
            .set(&(APR, owner, guardian), &true);
        Ok(())
    }

    pub fn emergency_reconstruct(env: Env, owner: Address) -> Result<BytesN<32>, VaultError> {
        Self::require_init(&env)?;

        let record: VaultRecord = env
            .storage()
            .persistent()
            .get(&(VAULT, owner.clone()))
            .ok_or(VaultError::VaultNotFound)?;
        let identity: Address = env
            .storage()
            .instance()
            .get(&IDENTITY)
            .ok_or(VaultError::NotInitialized)?;
        let id_client = IdentityContractClient::new(&env, &identity);
        let guardians = id_client.get_guardians(&owner);

        let mut approved = 0u32;
        let mut shares = StdVec::new();
        for i in 0..guardians.len() {
            if let Some(g) = guardians.get(i) {
                let ok: bool = env
                    .storage()
                    .persistent()
                    .get(&(APR, owner.clone(), g.clone()))
                    .unwrap_or(false);
                if ok {
                    approved = approved.saturating_add(1);
                    if let Some(s) = env.storage().persistent().get::<_, VaultShare>(&(
                        SHARE,
                        owner.clone(),
                        record.epoch,
                        g.clone(),
                    )) {
                        shares.push(sss::Share {
                            x: s.x as u8,
                            y: s.y.to_array(),
                        });
                    }
                }
            }
        }

        if approved < record.policy.emergency_threshold {
            return Err(VaultError::EmergencyThresholdNotMet);
        }

        let secret = sss::reconstruct(&shares, record.policy.threshold as u8)
            .ok_or(VaultError::InsufficientShares)?;
        Ok(BytesN::from_array(&env, &secret))
    }

    pub fn rotate_key(
        env: Env,
        owner: Address,
        new_seed: BytesN<32>,
    ) -> Result<RotationEvent, VaultError> {
        Self::require_init(&env)?;
        owner.require_auth();

        let mut record: VaultRecord = env
            .storage()
            .persistent()
            .get(&(VAULT, owner.clone()))
            .ok_or(VaultError::VaultNotFound)?;
        let identity: Address = env
            .storage()
            .instance()
            .get(&IDENTITY)
            .ok_or(VaultError::NotInitialized)?;
        let id_client = IdentityContractClient::new(&env, &identity);
        let guardians = id_client.get_guardians(&owner);

        let old_epoch = record.epoch;
        let new_epoch = old_epoch.saturating_add(1);
        let shares = sss::split(
            new_seed.to_array(),
            record.policy.threshold as u8,
            record.policy.shard_count as u8,
            env.crypto().sha256(&new_seed.into()).to_array(),
        );

        for i in 0..record.policy.shard_count {
            let g = guardians.get(i).ok_or(VaultError::InvalidConfig)?;
            let share = shares.get(i as usize).ok_or(VaultError::InvalidConfig)?;
            env.storage().persistent().set(
                &(SHARE, owner.clone(), new_epoch, g.clone()),
                &VaultShare {
                    guardian: g,
                    x: share.x as u32,
                    y: BytesN::from_array(&env, &share.y),
                },
            );
        }

        record.epoch = new_epoch;
        record.last_activity_at = env.ledger().timestamp();
        record.deadman_release_at = record
            .last_activity_at
            .saturating_add(record.policy.inactivity_timeout_secs);
        env.storage()
            .persistent()
            .set(&(VAULT, owner.clone()), &record);

        Ok(RotationEvent {
            owner,
            previous_epoch: old_epoch,
            new_epoch,
            rotated_at: record.last_activity_at,
        })
    }

    pub fn touch_activity(env: Env, owner: Address) -> Result<(), VaultError> {
        owner.require_auth();
        let mut record: VaultRecord = env
            .storage()
            .persistent()
            .get(&(VAULT, owner.clone()))
            .ok_or(VaultError::VaultNotFound)?;
        record.last_activity_at = env.ledger().timestamp();
        record.deadman_release_at = record
            .last_activity_at
            .saturating_add(record.policy.inactivity_timeout_secs);
        env.storage().persistent().set(&(VAULT, owner), &record);
        Ok(())
    }

    pub fn trigger_deadman_release(env: Env, owner: Address) -> Result<bool, VaultError> {
        let record: VaultRecord = env
            .storage()
            .persistent()
            .get(&(VAULT, owner))
            .ok_or(VaultError::VaultNotFound)?;
        if env.ledger().timestamp() < record.deadman_release_at {
            return Err(VaultError::DeadmanNotReady);
        }
        Ok(true)
    }

    pub fn get_vault(env: Env, owner: Address) -> Result<VaultRecord, VaultError> {
        env.storage()
            .persistent()
            .get(&(VAULT, owner))
            .ok_or(VaultError::VaultNotFound)
    }

    // ── Multi-asset accounting ─────────────────────────────────────────────

    /// Approve a token contract for deposits and vault-to-vault transfers.
    /// Idempotent. Admin only.
    pub fn add_supported_asset(env: Env, admin: Address, asset: Address) -> Result<(), VaultError> {
        Self::require_admin(&env, &admin)?;
        if accounting::add_supported(&env, &asset)? {
            #[allow(deprecated)]
            env.events()
                .publish((symbol_short!("VLT_ASTA"), asset), true);
        }
        Ok(())
    }

    /// Delist a token. Blocks new deposits and transfers in that token;
    /// withdrawals stay open so balances are never trapped. Admin only.
    pub fn remove_supported_asset(
        env: Env,
        admin: Address,
        asset: Address,
    ) -> Result<(), VaultError> {
        Self::require_admin(&env, &admin)?;
        if accounting::remove_supported(&env, &asset) {
            #[allow(deprecated)]
            env.events()
                .publish((symbol_short!("VLT_ASTA"), asset), false);
        }
        Ok(())
    }

    pub fn get_supported_assets(env: Env) -> Vec<Address> {
        accounting::supported_assets(&env)
    }

    pub fn is_supported_asset(env: Env, asset: Address) -> bool {
        accounting::is_supported(&env, &asset)
    }

    /// Move `amount` of `asset` from `owner` into custody and credit their
    /// vault. The credited amount is the contract's measured balance increase,
    /// so a token that delivers less than requested cannot inflate balances.
    /// Returns the audit entry.
    pub fn deposit(
        env: Env,
        owner: Address,
        asset: Address,
        amount: i128,
    ) -> Result<LedgerEntry, VaultError> {
        Self::require_init(&env)?;
        owner.require_auth();
        Self::require_vault(&env, &owner)?;
        Self::require_supported(&env, &asset)?;
        if amount <= 0 {
            return Err(VaultError::InvalidAmount);
        }

        let tok = token::Client::new(&env, &asset);
        let custody = env.current_contract_address();
        let before = tok.balance(&custody);
        tok.transfer(&owner, &custody, &amount);
        let received = tok
            .balance(&custody)
            .checked_sub(before)
            .ok_or(VaultError::InvalidAmount)?;
        if received <= 0 {
            return Err(VaultError::DepositNotReceived);
        }

        let entry = accounting::credit(
            &env,
            &owner,
            &asset,
            received,
            LedgerEntryKind::Deposit,
            &owner,
        )?;
        Self::emit_entry(&env, &owner, &entry);
        Ok(entry)
    }

    /// Debit `amount` of `asset` from `owner`'s vault and send the tokens to
    /// `to`. Works for delisted assets. Returns the audit entry.
    pub fn withdraw(
        env: Env,
        owner: Address,
        asset: Address,
        amount: i128,
        to: Address,
    ) -> Result<LedgerEntry, VaultError> {
        Self::require_init(&env)?;
        owner.require_auth();
        if amount <= 0 {
            return Err(VaultError::InvalidAmount);
        }

        // Effects before the external call.
        let entry = accounting::debit(
            &env,
            &owner,
            &asset,
            amount,
            LedgerEntryKind::Withdrawal,
            &to,
        )?;
        token::Client::new(&env, &asset).transfer(&env.current_contract_address(), &to, &amount);
        Self::emit_entry(&env, &owner, &entry);
        Ok(entry)
    }

    /// Move `amount` of `asset` from `from`'s vault to `to`'s vault. Both
    /// vaults must be configured. No tokens leave custody; both trails get an
    /// entry.
    pub fn transfer_between_vaults(
        env: Env,
        from: Address,
        to: Address,
        asset: Address,
        amount: i128,
    ) -> Result<TransferReceipt, VaultError> {
        Self::require_init(&env)?;
        from.require_auth();
        if from == to {
            return Err(VaultError::SelfTransfer);
        }
        if amount <= 0 {
            return Err(VaultError::InvalidAmount);
        }
        Self::require_vault(&env, &from)?;
        Self::require_vault(&env, &to)?;
        Self::require_supported(&env, &asset)?;

        let out = accounting::debit(
            &env,
            &from,
            &asset,
            amount,
            LedgerEntryKind::TransferOut,
            &to,
        )?;
        let inc = accounting::credit(
            &env,
            &to,
            &asset,
            amount,
            LedgerEntryKind::TransferIn,
            &from,
        )?;
        Self::emit_entry(&env, &from, &out);
        Self::emit_entry(&env, &to, &inc);

        Ok(TransferReceipt {
            asset,
            amount,
            from_balance_after: out.balance_after,
            to_balance_after: inc.balance_after,
            from_seq: out.seq,
            to_seq: inc.seq,
        })
    }

    pub fn get_balance(env: Env, owner: Address, asset: Address) -> i128 {
        accounting::balance(&env, &owner, &asset)
    }

    /// Every asset with a non-zero balance in `owner`'s vault.
    pub fn get_balances(env: Env, owner: Address) -> Vec<AssetBalance> {
        accounting::balances(&env, &owner)
    }

    /// Sum of all vault balances in `asset`. Equals the tokens this contract
    /// should hold in custody for that asset.
    pub fn get_total_balance(env: Env, asset: Address) -> i128 {
        accounting::total(&env, &asset)
    }

    pub fn get_ledger_entry_count(env: Env, owner: Address) -> u64 {
        accounting::entry_count(&env, &owner)
    }

    pub fn get_ledger_entry(env: Env, owner: Address, seq: u64) -> Option<LedgerEntry> {
        accounting::entry(&env, &owner, seq)
    }

    /// Page through `owner`'s audit trail from `start` (1-based), oldest
    /// first. `limit` is capped at `accounting::MAX_LEDGER_PAGE`.
    pub fn get_ledger_entries(
        env: Env,
        owner: Address,
        start: u64,
        limit: u32,
    ) -> Vec<LedgerEntry> {
        accounting::get_entries(&env, &owner, start, limit)
    }

    fn require_admin(env: &Env, admin: &Address) -> Result<(), VaultError> {
        Self::require_init(env)?;
        let stored: Address = env
            .storage()
            .instance()
            .get(&ADMIN)
            .ok_or(VaultError::NotInitialized)?;
        if stored != *admin {
            return Err(VaultError::Unauthorized);
        }
        admin.require_auth();
        Ok(())
    }

    fn require_vault(env: &Env, owner: &Address) -> Result<(), VaultError> {
        if !env.storage().persistent().has(&(VAULT, owner.clone())) {
            return Err(VaultError::VaultNotFound);
        }
        Ok(())
    }

    fn require_supported(env: &Env, asset: &Address) -> Result<(), VaultError> {
        if !accounting::is_supported(env, asset) {
            return Err(VaultError::UnsupportedAsset);
        }
        Ok(())
    }

    /// Topic: `("VLT_ACC", owner, asset)`; data: `(kind, amount, balance_after, seq)`.
    fn emit_entry(env: &Env, owner: &Address, entry: &LedgerEntry) {
        #[allow(deprecated)]
        env.events().publish(
            (symbol_short!("VLT_ACC"), owner.clone(), entry.asset.clone()),
            (entry.kind, entry.amount, entry.balance_after, entry.seq),
        );
    }

    fn require_init(env: &Env) -> Result<(), VaultError> {
        if !env.storage().instance().has(&INIT) {
            return Err(VaultError::NotInitialized);
        }
        Ok(())
    }
}
