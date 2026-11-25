//! Bounded per-tenant usage history (#532).
//!
//! The history is stored in the same ledger entry as the tenant's usage counters (see
//! [`crate::quota`]), so recording an operation stays a single write. That entry is rewritten on
//! every metered operation, which is why the number of entries is capped and their stored form is
//! compact.

use soroban_sdk::{contracttype, symbol_short, Env, Symbol, Vec};

use crate::OperationType;

const HISTORY_CAP: Symbol = symbol_short!("HIST_CAP");

/// Entries kept per tenant until the admin sets another cap.
pub const DEFAULT_HISTORY_CAP: u32 = 32;
/// Largest cap the admin can set.
pub const MAX_HISTORY_CAP: u32 = 200;

/// Page size when `limit` is not given.
pub const DEFAULT_LIMIT: u32 = 50;
/// Largest `limit` accepted.
pub const MAX_LIMIT: u32 = 200;

/// One metered operation.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UsageEntry {
    /// Position in the tenant's history, starting at 1. Never reused.
    pub id: u64,
    pub op_type: OperationType,
    /// Gas units charged, as priced by [`crate::GasCosts::cost_for`].
    pub units: u64,
    pub timestamp: u64,
}

/// One page of a tenant's usage history, newest first.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UsageHistoryPage {
    pub entries: Vec<UsageEntry>,
    /// Pass as `before` to fetch the next page; `None` on the last page.
    pub next_cursor: Option<u64>,
}

/// Stored form: `(op_type, units, timestamp)`, oldest first. IDs are consecutive, so only the
/// newest one is stored.
#[contracttype]
#[derive(Clone)]
pub(crate) struct UsageHistory {
    last_id: u64,
    entries: Vec<(OperationType, u64, u64)>,
}

impl UsageHistory {
    pub(crate) fn new(env: &Env) -> Self {
        UsageHistory {
            last_id: 0,
            entries: Vec::new(env),
        }
    }

    pub(crate) fn push(&mut self, op_type: &OperationType, units: u64, timestamp: u64) {
        self.last_id += 1;
        self.entries.push_back((op_type.clone(), units, timestamp));
    }

    /// Drops the oldest entries beyond `cap`.
    pub(crate) fn trim(&mut self, cap: u32) {
        let excess = self.entries.len().saturating_sub(cap);
        if excess > 0 {
            self.entries = self.entries.slice(excess..);
        }
    }

    /// Up to `limit` entries with `id < before` (all when `before` is `None`), newest first.
    pub(crate) fn page(&self, env: &Env, limit: u32, before: Option<u64>) -> UsageHistoryPage {
        let len = self.entries.len();
        let first_id = self.last_id - u64::from(len) + 1;
        // The window is `start..end`; the entry at index `i` has id `first_id + i`.
        let end = match before {
            Some(before) => before.saturating_sub(first_id).min(u64::from(len)) as u32,
            None => len,
        };
        let start = end.saturating_sub(limit);

        let mut entries = Vec::new(env);
        for i in (start..end).rev() {
            let (op_type, units, timestamp) = self.entries.get_unchecked(i);
            entries.push_back(UsageEntry {
                id: first_id + u64::from(i),
                op_type,
                units,
                timestamp,
            });
        }
        UsageHistoryPage {
            entries,
            next_cursor: (start > 0).then(|| first_id + u64::from(start)),
        }
    }
}

/// Entries kept per tenant.
pub(crate) fn cap(env: &Env) -> u32 {
    env.storage()
        .instance()
        .get(&HISTORY_CAP)
        .unwrap_or(DEFAULT_HISTORY_CAP)
}

pub(crate) fn set_cap(env: &Env, cap: u32) {
    env.storage().instance().set(&HISTORY_CAP, &cap);
}
