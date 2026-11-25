//! Verifiable credential revocation registry.
//!
//! Each issuer owns a bitmap with one bit per credential, addressed by the `revocation_index` the
//! issuer put in the credential. The registry holds neither holders nor credential IDs: credential
//! IDs are linked to holders by `bind_credential`, so storing them here would expose who was
//! revoked.

use soroban_sdk::{symbol_short, Address, Env, Symbol};

const REVOCATIONS: Symbol = symbol_short!("VC_REVOKE");

/// Credentials covered by one ledger entry of the bitmap.
const WORD_BITS: u64 = u128::BITS as u64;

const TTL_THRESHOLD: u32 = 5_184_000;
const TTL_EXTEND_TO: u32 = 10_368_000;

/// Storage key of the bitmap word that holds `index`, and the bit for `index` within it.
fn locate(issuer: &Address, index: u64) -> ((Symbol, Address, u64), u128) {
    (
        (REVOCATIONS, issuer.clone(), index / WORD_BITS),
        1 << (index % WORD_BITS),
    )
}

/// Revokes `issuer`'s credential at `index`.
pub fn revoke(env: &Env, issuer: &Address, index: u64) {
    let (key, bit) = locate(issuer, index);
    let word: u128 = env.storage().persistent().get(&key).unwrap_or(0);
    env.storage().persistent().set(&key, &(word | bit));
    env.storage()
        .persistent()
        .extend_ttl(&key, TTL_THRESHOLD, TTL_EXTEND_TO);
}

/// Whether `issuer` has revoked its credential at `index`.
pub fn is_revoked(env: &Env, issuer: &Address, index: u64) -> bool {
    let (key, bit) = locate(issuer, index);
    let word: u128 = env.storage().persistent().get(&key).unwrap_or(0);
    word & bit != 0
}
