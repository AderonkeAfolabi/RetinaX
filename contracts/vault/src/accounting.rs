//! Multi-asset accounting for the vault contract.
//!
//! Each vault (keyed by its owner) can hold balances in any number of
//! admin-approved Soroban tokens. Balances are internal ledger entries backed
//! by tokens held in custody by the vault contract, so a transfer between two
//! vaults moves no tokens, only accounting.
//!
//! Every balance change appends an immutable [`LedgerEntry`] to the vault's
//! audit trail, and the contract keeps a per-asset total of all vault balances
//! so custody can be reconciled against the contract's token balance.

#![allow(clippy::arithmetic_side_effects)]

use soroban_sdk::{contracttype, symbol_short, Address, Env, Symbol, Vec};

use crate::VaultError;

/// Supported asset registry (instance storage).
const ASSET_REG: Symbol = symbol_short!("AST_REG");
/// `(BAL, owner, asset) -> i128` per-vault, per-asset balance.
const BALANCE: Symbol = symbol_short!("BAL");
/// `(V_ASSETS, owner) -> Vec<Address>` assets with a non-zero balance.
const VAULT_ASSETS: Symbol = symbol_short!("V_ASSETS");
/// `(AST_TOT, asset) -> i128` sum of all vault balances for an asset.
const ASSET_TOTAL: Symbol = symbol_short!("AST_TOT");
/// `(ACC_SEQ, owner) -> u64` number of audit entries for a vault.
const LEDGER_SEQ: Symbol = symbol_short!("ACC_SEQ");
/// `(ACC_LOG, owner, seq) -> LedgerEntry` audit trail entry.
const LEDGER_LOG: Symbol = symbol_short!("ACC_LOG");

/// Maximum number of distinct assets the admin can approve.
pub const MAX_SUPPORTED_ASSETS: u32 = 32;
/// Maximum page size for [`get_entries`].
pub const MAX_LEDGER_PAGE: u32 = 50;

const TTL_THRESHOLD: u32 = 5_184_000;
const TTL_EXTEND_TO: u32 = 10_368_000;

/// The kind of balance movement an audit entry records.
#[contracttype]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum LedgerEntryKind {
    /// Tokens entered custody and were credited to this vault.
    Deposit = 0,
    /// Tokens left custody and were debited from this vault.
    Withdrawal = 1,
    /// Balance credited from another vault (no tokens moved).
    TransferIn = 2,
    /// Balance debited to another vault (no tokens moved).
    TransferOut = 3,
}

/// One immutable line in a vault's accounting audit trail.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LedgerEntry {
    /// Position in this vault's trail, starting at 1.
    pub seq: u64,
    pub kind: LedgerEntryKind,
    pub asset: Address,
    /// Always positive; the direction is given by `kind`.
    pub amount: i128,
    /// Deposit: the funding address. Withdrawal: the recipient.
    /// Transfers: the other vault's owner.
    pub counterparty: Address,
    /// This vault's balance in `asset` after the entry was applied.
    pub balance_after: i128,
    pub timestamp: u64,
    pub ledger: u32,
}

/// A vault's balance in one asset.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AssetBalance {
    pub asset: Address,
    pub balance: i128,
}

/// Result of a vault-to-vault transfer.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TransferReceipt {
    pub asset: Address,
    pub amount: i128,
    pub from_balance_after: i128,
    pub to_balance_after: i128,
    /// Sequence number of the `TransferOut` entry in the sender's trail.
    pub from_seq: u64,
    /// Sequence number of the `TransferIn` entry in the recipient's trail.
    pub to_seq: u64,
}

// ── Supported asset registry ────────────────────────────────────────────────

pub fn supported_assets(env: &Env) -> Vec<Address> {
    env.storage()
        .instance()
        .get(&ASSET_REG)
        .unwrap_or_else(|| Vec::new(env))
}

pub fn is_supported(env: &Env, asset: &Address) -> bool {
    supported_assets(env).contains(asset)
}

/// Adds `asset` to the registry. Returns `false` if it was already present.
pub fn add_supported(env: &Env, asset: &Address) -> Result<bool, VaultError> {
    let mut assets = supported_assets(env);
    if assets.contains(asset) {
        return Ok(false);
    }
    if assets.len() >= MAX_SUPPORTED_ASSETS {
        return Err(VaultError::AssetLimitReached);
    }
    assets.push_back(asset.clone());
    env.storage().instance().set(&ASSET_REG, &assets);
    Ok(true)
}

/// Removes `asset` from the registry. Returns `false` if it was not present.
pub fn remove_supported(env: &Env, asset: &Address) -> bool {
    let assets = supported_assets(env);
    match assets.first_index_of(asset) {
        Some(idx) => {
            let mut assets = assets;
            assets.remove(idx);
            env.storage().instance().set(&ASSET_REG, &assets);
            true
        }
        None => false,
    }
}

// ── Balances ────────────────────────────────────────────────────────────────

fn balance_key(owner: &Address, asset: &Address) -> (Symbol, Address, Address) {
    (BALANCE, owner.clone(), asset.clone())
}

pub fn balance(env: &Env, owner: &Address, asset: &Address) -> i128 {
    env.storage()
        .persistent()
        .get(&balance_key(owner, asset))
        .unwrap_or(0)
}

pub fn vault_assets(env: &Env, owner: &Address) -> Vec<Address> {
    env.storage()
        .persistent()
        .get(&(VAULT_ASSETS, owner.clone()))
        .unwrap_or_else(|| Vec::new(env))
}

pub fn balances(env: &Env, owner: &Address) -> Vec<AssetBalance> {
    let mut out = Vec::new(env);
    for asset in vault_assets(env, owner).iter() {
        let bal = balance(env, owner, &asset);
        out.push_back(AssetBalance {
            asset,
            balance: bal,
        });
    }
    out
}

pub fn total(env: &Env, asset: &Address) -> i128 {
    env.storage()
        .persistent()
        .get(&(ASSET_TOTAL, asset.clone()))
        .unwrap_or(0)
}

fn set_balance(env: &Env, owner: &Address, asset: &Address, value: i128) {
    let key = balance_key(owner, asset);
    let assets_key = (VAULT_ASSETS, owner.clone());
    let mut assets = vault_assets(env, owner);

    if value == 0 {
        env.storage().persistent().remove(&key);
        if let Some(idx) = assets.first_index_of(asset) {
            assets.remove(idx);
        }
    } else {
        env.storage().persistent().set(&key, &value);
        env.storage()
            .persistent()
            .extend_ttl(&key, TTL_THRESHOLD, TTL_EXTEND_TO);
        if !assets.contains(asset) {
            assets.push_back(asset.clone());
        }
    }

    if assets.is_empty() {
        env.storage().persistent().remove(&assets_key);
    } else {
        env.storage().persistent().set(&assets_key, &assets);
        env.storage()
            .persistent()
            .extend_ttl(&assets_key, TTL_THRESHOLD, TTL_EXTEND_TO);
    }
}

fn adjust_total(env: &Env, asset: &Address, delta: i128) -> Result<(), VaultError> {
    let key = (ASSET_TOTAL, asset.clone());
    let next = total(env, asset)
        .checked_add(delta)
        .ok_or(VaultError::InvalidAmount)?;
    env.storage().persistent().set(&key, &next);
    env.storage()
        .persistent()
        .extend_ttl(&key, TTL_THRESHOLD, TTL_EXTEND_TO);
    Ok(())
}

/// Credits `amount` to a vault and records the entry. Returns the new entry.
pub fn credit(
    env: &Env,
    owner: &Address,
    asset: &Address,
    amount: i128,
    kind: LedgerEntryKind,
    counterparty: &Address,
) -> Result<LedgerEntry, VaultError> {
    let next = balance(env, owner, asset)
        .checked_add(amount)
        .ok_or(VaultError::InvalidAmount)?;
    set_balance(env, owner, asset, next);
    if kind == LedgerEntryKind::Deposit {
        adjust_total(env, asset, amount)?;
    }
    Ok(append_entry(
        env,
        owner,
        kind,
        asset,
        amount,
        counterparty,
        next,
    ))
}

/// Debits `amount` from a vault and records the entry. Returns the new entry.
pub fn debit(
    env: &Env,
    owner: &Address,
    asset: &Address,
    amount: i128,
    kind: LedgerEntryKind,
    counterparty: &Address,
) -> Result<LedgerEntry, VaultError> {
    let current = balance(env, owner, asset);
    if current < amount {
        return Err(VaultError::InsufficientBalance);
    }
    let next = current - amount;
    set_balance(env, owner, asset, next);
    if kind == LedgerEntryKind::Withdrawal {
        adjust_total(env, asset, -amount)?;
    }
    Ok(append_entry(
        env,
        owner,
        kind,
        asset,
        amount,
        counterparty,
        next,
    ))
}

// ── Audit trail ─────────────────────────────────────────────────────────────

pub fn entry_count(env: &Env, owner: &Address) -> u64 {
    env.storage()
        .persistent()
        .get(&(LEDGER_SEQ, owner.clone()))
        .unwrap_or(0)
}

pub fn entry(env: &Env, owner: &Address, seq: u64) -> Option<LedgerEntry> {
    env.storage()
        .persistent()
        .get(&(LEDGER_LOG, owner.clone(), seq))
}

/// Returns up to `limit` entries (capped at [`MAX_LEDGER_PAGE`]) starting at
/// sequence number `start` (1-based), oldest first.
pub fn get_entries(env: &Env, owner: &Address, start: u64, limit: u32) -> Vec<LedgerEntry> {
    let mut out = Vec::new(env);
    let count = entry_count(env, owner);
    let limit = limit.min(MAX_LEDGER_PAGE) as u64;
    let mut seq = start.max(1);
    while seq <= count && (out.len() as u64) < limit {
        if let Some(e) = entry(env, owner, seq) {
            out.push_back(e);
        }
        seq += 1;
    }
    out
}

fn append_entry(
    env: &Env,
    owner: &Address,
    kind: LedgerEntryKind,
    asset: &Address,
    amount: i128,
    counterparty: &Address,
    balance_after: i128,
) -> LedgerEntry {
    let seq_key = (LEDGER_SEQ, owner.clone());
    let seq = entry_count(env, owner) + 1;
    env.storage().persistent().set(&seq_key, &seq);
    env.storage()
        .persistent()
        .extend_ttl(&seq_key, TTL_THRESHOLD, TTL_EXTEND_TO);

    let e = LedgerEntry {
        seq,
        kind,
        asset: asset.clone(),
        amount,
        counterparty: counterparty.clone(),
        balance_after,
        timestamp: env.ledger().timestamp(),
        ledger: env.ledger().sequence(),
    };
    let log_key = (LEDGER_LOG, owner.clone(), seq);
    env.storage().persistent().set(&log_key, &e);
    env.storage()
        .persistent()
        .extend_ttl(&log_key, TTL_THRESHOLD, TTL_EXTEND_TO);
    e
}
