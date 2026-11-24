//! Issue #425 — vault authorization coverage.
//!
//! Verifies that only the owner / approved guardians can mutate vault state,
//! that every mutating entry point enforces a signer, and that guardian
//! approvals are scoped to a single owner and stop counting once revoked.

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
use vault::{VaultContract, VaultContractClient, VaultError};

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

/// Every mutating function that carries a signer parameter must reject a call
/// with no authorization, even when the addresses themselves are valid.
#[test]
fn test_every_mutating_function_requires_a_signature() {
    let env = Env::default();
    let (vault, identity, owner) = setup_vault(&env);
    let guardians = add_guardians(&env, &identity, &owner, 3);

    let seed = BytesN::from_array(&env, &[10u8; 32]);
    let new_seed = BytesN::from_array(&env, &[11u8; 32]);
    let data_ref = String::from_str(&env, "ipfs://auth");
    vault.configure_vault(&owner, &2, &3, &3, &3_600, &data_ref, &seed, &None);

    // Remove all mocked signatures: requests now arrive unsigned.
    env.set_auths(&[]);

    assert!(vault
        .try_configure_vault(&owner, &2, &3, &3, &3_600, &data_ref, &seed, &None)
        .is_err());
    assert!(vault
        .try_reconstruct_key(&owner, &owner, &Vec::new(&env))
        .is_err());
    assert!(vault
        .try_submit_emergency_approval(&guardians[0], &owner)
        .is_err());
    assert!(vault.try_rotate_key(&owner, &new_seed).is_err());
    assert!(vault.try_touch_activity(&owner).is_err());

    // The read-only path and the time/quorum-gated paths need no signer by
    // design; they must still work after auths are cleared.
    assert_eq!(vault.get_vault(&owner).owner, owner);
    assert_eq!(
        vault.try_trigger_deadman_release(&owner),
        Err(Ok(VaultError::DeadmanNotReady))
    );
    assert_eq!(
        vault.try_emergency_reconstruct(&owner),
        Err(Ok(VaultError::EmergencyThresholdNotMet))
    );
}

/// A different, correctly signed address must not be able to reconfigure,
/// rotate, or touch someone else's vault.
#[test]
fn test_wrong_owner_cannot_mutate_vault() {
    let env = Env::default();
    let (vault, identity, owner) = setup_vault(&env);
    add_guardians(&env, &identity, &owner, 3);

    let seed = BytesN::from_array(&env, &[12u8; 32]);
    let new_seed = BytesN::from_array(&env, &[13u8; 32]);
    let data_ref = String::from_str(&env, "ipfs://wrong-owner");
    vault.configure_vault(&owner, &2, &3, &3, &3_600, &data_ref, &seed, &None);

    let attacker = Address::generate(&env);

    // The attacker is authenticated (auths are mocked) but owns no vault.
    assert_eq!(
        vault.try_rotate_key(&attacker, &new_seed),
        Err(Ok(VaultError::VaultNotFound))
    );
    assert_eq!(
        vault.try_touch_activity(&attacker),
        Err(Ok(VaultError::VaultNotFound))
    );
    assert_eq!(
        vault.try_configure_vault(&attacker, &1, &1, &1, &3_600, &data_ref, &seed, &None),
        Err(Ok(VaultError::InvalidConfig))
    );

    // The real owner's record is untouched.
    assert_eq!(vault.get_vault(&owner).epoch, 1);
}

/// Only guardians holding a share for a given owner may approve that owner's
/// emergency recovery. An approval for one owner never counts for another.
#[test]
fn test_approvals_are_guardian_and_owner_scoped() {
    let env = Env::default();
    let (vault, identity, owner) = setup_vault(&env);
    let guardians = add_guardians(&env, &identity, &owner, 3);

    let seed = BytesN::from_array(&env, &[14u8; 32]);
    let data_ref = String::from_str(&env, "ipfs://scoped");
    vault.configure_vault(&owner, &2, &3, &3, &3_600, &data_ref, &seed, &None);

    // A second owner with completely different guardians.
    let other_owner = Address::generate(&env);
    identity.initialize(&other_owner);
    add_guardians(&env, &identity, &other_owner, 3);
    vault.configure_vault(&other_owner, &2, &3, &3, &3_600, &data_ref, &seed, &None);

    // An owner-A guardian cannot approve owner B's vault.
    assert_eq!(
        vault.try_submit_emergency_approval(&guardians[0], &other_owner),
        Err(Ok(VaultError::Unauthorized))
    );

    // An approval for owner A does not help owner B reach quorum.
    vault.submit_emergency_approval(&guardians[0], &owner);
    assert_eq!(
        vault.try_emergency_reconstruct(&other_owner),
        Err(Ok(VaultError::EmergencyThresholdNotMet))
    );
    assert_eq!(vault.emergency_reconstruct(&owner), seed);
}

/// Removing a guardian from the identity contract revokes the practical
/// effect of their previously submitted approval: it no longer counts toward
/// the emergency quorum.
#[test]
fn test_revoked_guardian_approval_no_longer_counts() {
    let env = Env::default();
    let (vault, identity, owner) = setup_vault(&env);
    let guardians = add_guardians(&env, &identity, &owner, 3);

    let seed = BytesN::from_array(&env, &[15u8; 32]);
    let data_ref = String::from_str(&env, "ipfs://revoke");
    vault.configure_vault(&owner, &2, &3, &3, &3_600, &data_ref, &seed, &None);

    for guardian in &guardians {
        vault.submit_emergency_approval(guardian, &owner);
    }
    assert_eq!(vault.emergency_reconstruct(&owner), seed);

    identity.remove_guardian(&owner, &guardians[2]);

    // The stored approval flag remains, but the guardian is no longer in the
    // current guardian set, so the emergency quorum can no longer be met.
    assert_eq!(
        vault.try_emergency_reconstruct(&owner),
        Err(Ok(VaultError::EmergencyThresholdNotMet))
    );
}
