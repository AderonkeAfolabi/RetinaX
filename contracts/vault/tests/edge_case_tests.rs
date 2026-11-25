//! Issue #424 — deposit/withdraw edge-case coverage for the vault crate.
//!
//! The current vault stores encryption-key shares rather than token balances
//! (there is no `deposit`/`withdraw` entry point and no `i128` amount in the
//! contract). These tests cover the equivalent boundaries on the real public
//! API: zero and maximum durations/amounts, repeated operations, and attempts
//! to take more than the configured quorum allows.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::arithmetic_side_effects
)]

extern crate std;

use identity::{IdentityContract, IdentityContractClient};
use soroban_sdk::{
    testutils::{Address as _, Ledger as _},
    Address, BytesN, Env, String, Vec,
};
use vault::{VaultContract, VaultContractClient, VaultError, VaultSnapshot};

fn setup_vault(env: &Env) -> (VaultContractClient<'_>, IdentityContractClient<'_>, Address) {
    env.mock_all_auths();

    let admin = Address::generate(env);
    let owner = Address::generate(env);

    let identity_id = env.register(IdentityContract, ());
    let identity = IdentityContractClient::new(env, &identity_id);
    identity.initialize(&owner);

    let vault_id = env.register(VaultContract, ());
    let vault = VaultContractClient::new(env, &vault_id);
    vault.initialize(&admin, &identity_id);

    (vault, identity, owner)
}

fn add_guardians(
    env: &Env,
    identity: &IdentityContractClient,
    owner: &Address,
    count: u32,
) -> std::vec::Vec<Address> {
    let mut guardians = std::vec::Vec::new();
    for _ in 0..count {
        let guardian = Address::generate(env);
        identity.add_guardian(owner, &guardian);
        guardians.push(guardian);
    }
    guardians
}

/// A zero inactivity timeout means the dead-man switch is immediately ready:
/// there is no grace duration to wait out.
#[test]
fn test_zero_inactivity_timeout_is_immediately_releasable() {
    let env = Env::default();
    env.ledger().set_timestamp(1_000);
    let (vault, identity, owner) = setup_vault(&env);
    add_guardians(&env, &identity, &owner, 3);

    let seed = BytesN::from_array(&env, &[1u8; 32]);
    let data_ref = String::from_str(&env, "ipfs://zero");
    let snapshot: VaultSnapshot = vault.configure_vault(&owner, &2, &3, &3, &0, &data_ref, &seed);
    let snapshot: VaultSnapshot =
        vault.configure_vault(&owner, &2, &3, &3, &0, &data_ref, &seed, &None);

    assert_eq!(snapshot.record.deadman_release_at, 1_000);
    assert!(vault.trigger_deadman_release(&owner));
}

/// A maximum-duration timeout must saturate instead of overflowing, and must
/// keep the release locked (no wrap-around to a small deadline).
#[test]
fn test_max_timeout_saturates_without_overflow() {
    let env = Env::default();
    env.ledger().set_timestamp(1_000);
    let (vault, identity, owner) = setup_vault(&env);
    add_guardians(&env, &identity, &owner, 3);

    let seed = BytesN::from_array(&env, &[2u8; 32]);
    let data_ref = String::from_str(&env, "ipfs://max");
    let snapshot: VaultSnapshot =
        vault.configure_vault(&owner, &2, &3, &3, &u64::MAX, &data_ref, &seed, &None);

    assert_eq!(snapshot.record.deadman_release_at, u64::MAX);
    assert_eq!(
        vault.try_trigger_deadman_release(&owner),
        Err(Ok(VaultError::DeadmanNotReady))
    );
}

/// A zeroed secret seed is a valid boundary input and must round-trip through
/// configuration and storage like any other seed.
#[test]
fn test_zero_seed_boundary() {
    let env = Env::default();
    let (vault, identity, owner) = setup_vault(&env);
    add_guardians(&env, &identity, &owner, 3);

    let zero_seed = BytesN::from_array(&env, &[0u8; 32]);
    let data_ref = String::from_str(&env, "ipfs://zero-seed");
    let snapshot = vault.configure_vault(&owner, &2, &3, &3, &3_600, &data_ref, &zero_seed, &None);

    assert_eq!(snapshot.record.owner, owner);
    assert_eq!(vault.get_vault(&owner), snapshot.record);
}

/// Re-submitting the same approval twice must not count as two approvals:
/// repeated values are idempotent and cannot unlock funds early.
#[test]
fn test_repeated_approval_does_not_increase_quorum() {
    let env = Env::default();
    let (vault, identity, owner) = setup_vault(&env);
    let guardians = add_guardians(&env, &identity, &owner, 3);

    let seed = BytesN::from_array(&env, &[3u8; 32]);
    let data_ref = String::from_str(&env, "ipfs://repeat");
    // 2-of-3 for normal reconstruction, 3-of-3 for emergency reconstruction.
    vault.configure_vault(&owner, &2, &3, &3, &3_600, &data_ref, &seed, &None);

    vault.submit_emergency_approval(&guardians[0], &owner);
    vault.submit_emergency_approval(&guardians[0], &owner);
    vault.submit_emergency_approval(&guardians[0], &owner);

    // Still only one distinct approval, so the quorum is not met.
    assert_eq!(
        vault.try_emergency_reconstruct(&owner),
        Err(Ok(VaultError::EmergencyThresholdNotMet))
    );

    vault.submit_emergency_approval(&guardians[1], &owner);
    vault.submit_emergency_approval(&guardians[2], &owner);
    assert_eq!(vault.emergency_reconstruct(&owner), seed);
}

/// Withdrawing/reconstructing more than is authorized must fail: fewer shares
/// than the threshold and fewer approvals than the emergency quorum are both
/// rejected.
#[test]
fn test_over_withdraw_equivalent_is_rejected() {
    let env = Env::default();
    let (vault, identity, owner) = setup_vault(&env);
    let _guardians = add_guardians(&env, &identity, &owner, 3);

    let seed = BytesN::from_array(&env, &[4u8; 32]);
    let data_ref = String::from_str(&env, "ipfs://over-withdraw");
    vault.configure_vault(&owner, &2, &3, &3, &3_600, &data_ref, &seed, &None);

    let no_shares = Vec::new(&env);
    assert_eq!(
        vault.try_reconstruct_key(&owner, &owner, &no_shares),
        Err(Ok(VaultError::InsufficientShares))
    );

    assert_eq!(
        vault.try_emergency_reconstruct(&owner),
        Err(Ok(VaultError::EmergencyThresholdNotMet))
    );
}

/// The vault crate has no token amounts today. These checks document the
/// checked-arithmetic semantics that any future deposit/withdraw
/// implementation must satisfy under `overflow-checks = true`: zero is a
/// valid amount, `i128::MAX` plus one overflows, repeated max deposits
/// overflow, and over-withdrawing underflows.
#[test]
fn test_i128_amount_boundaries() {
    let zero = 0i128;
    let max = i128::MAX;

    assert_eq!(zero.checked_add(zero), Some(0));
    assert_eq!(max.checked_add(1), None);
    assert_eq!(zero.checked_sub(1), None);
    assert_eq!(max.checked_sub(max), Some(0));

    let after_first_deposit = zero.checked_add(max).unwrap();
    assert_eq!(after_first_deposit.checked_add(max), None);

    let balance = 100i128;
    assert_eq!(balance.checked_sub(101), None);
    assert_eq!(balance.checked_sub(100), Some(0));
}
