#![cfg(test)]

use identity::recovery::RecoveryError;
use identity::{IdentityContract, IdentityContractClient};
use soroban_sdk::{
    testutils::{Address as _, Ledger as _, LedgerInfo},
    Address, Env,
};

const COOLDOWN_SECONDS: u64 = 172_800; // 48 hours

fn setup() -> (Env, IdentityContractClient<'static>, Address) {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(IdentityContract, ());
    let client = IdentityContractClient::new(&env, &contract_id);
    let owner = Address::generate(&env);
    client.initialize(&owner);
    (env, client, owner)
}

fn add_n_guardians(
    env: &Env,
    client: &IdentityContractClient,
    owner: &Address,
    count: usize,
) -> std::vec::Vec<Address> {
    let mut guardians = std::vec::Vec::with_capacity(count);
    for _ in 0..count {
        let g = Address::generate(env);
        client.add_guardian(owner, &g);
        guardians.push(g);
    }
    guardians
}

#[test]
fn test_duplicate_guardian_and_max_guardian_limit() {
    let (env, client, owner) = setup();
    let g1 = Address::generate(&env);
    client.add_guardian(&owner, &g1);

    // Adding the same guardian twice fails with DuplicateGuardian.
    assert_eq!(
        client.try_add_guardian(&owner, &g1),
        Err(Ok(RecoveryError::DuplicateGuardian))
    );

    // Fill up to maximum of 5 guardians.
    for _ in 0..4 {
        let g = Address::generate(&env);
        client.add_guardian(&owner, &g);
    }
    assert_eq!(client.get_guardians(&owner).len(), 5);

    // A 6th guardian is rejected with MaxGuardiansReached.
    let g6 = Address::generate(&env);
    assert_eq!(
        client.try_add_guardian(&owner, &g6),
        Err(Ok(RecoveryError::MaxGuardiansReached))
    );
}

#[test]
fn test_guardian_threshold_validation_and_quorum_enforcement() {
    let (env, client, owner) = setup();
    let g1 = Address::generate(&env);
    let g2 = Address::generate(&env);
    client.add_guardian(&owner, &g1);
    client.add_guardian(&owner, &g2);

    // Cannot initiate recovery with fewer than 3 guardians.
    let proposed = Address::generate(&env);
    assert_eq!(
        client.try_initiate_recovery(&g1, &owner, &proposed),
        Err(Ok(RecoveryError::InsufficientGuardians))
    );

    // Add 3rd guardian and verify threshold bounds (0 and > 3 rejected).
    let g3 = Address::generate(&env);
    client.add_guardian(&owner, &g3);
    assert_eq!(
        client.try_set_recovery_threshold(&owner, &0),
        Err(Ok(RecoveryError::InvalidThreshold))
    );
    assert_eq!(
        client.try_set_recovery_threshold(&owner, &4),
        Err(Ok(RecoveryError::InvalidThreshold))
    );

    // Set 2-of-3 threshold.
    client.set_recovery_threshold(&owner, &2);
    assert_eq!(client.get_recovery_threshold(&owner), 2);

    // Non-guardian cannot initiate or approve recovery.
    let outsider = Address::generate(&env);
    assert_eq!(
        client.try_initiate_recovery(&outsider, &owner, &proposed),
        Err(Ok(RecoveryError::NotAGuardian))
    );

    env.ledger().set(LedgerInfo {
        timestamp: 1_000,
        protocol_version: 25,
        sequence_number: 10,
        network_id: Default::default(),
        base_reserve: 10,
        min_temp_entry_ttl: 16,
        min_persistent_entry_ttl: 16,
        max_entry_ttl: 6_312_000,
    });

    // Guardian 1 initiates recovery (1 approval so far).
    client.initiate_recovery(&g1, &owner, &proposed);
    assert_eq!(
        client.try_initiate_recovery(&g2, &owner, &proposed),
        Err(Ok(RecoveryError::RecoveryAlreadyActive))
    );
    assert_eq!(
        client.try_approve_recovery(&g1, &owner),
        Err(Ok(RecoveryError::AlreadyApproved))
    );
    assert_eq!(
        client.try_approve_recovery(&outsider, &owner),
        Err(Ok(RecoveryError::NotAGuardian))
    );

    // Only 1-of-2 approvals -> InsufficientApprovals even after cooldown.
    env.ledger().set_timestamp(1_000 + COOLDOWN_SECONDS + 1);
    assert_eq!(
        client.try_execute_recovery(&g1, &owner),
        Err(Ok(RecoveryError::InsufficientApprovals))
    );

    // Reset timestamp before cooldown and add 2nd approval -> CooldownNotExpired.
    env.ledger().set_timestamp(1_000 + COOLDOWN_SECONDS - 1);
    client.approve_recovery(&g2, &owner);
    assert_eq!(
        client.try_execute_recovery(&g1, &owner),
        Err(Ok(RecoveryError::CooldownNotExpired))
    );

    // Advance past cooldown -> recovery executes and transfers ownership.
    env.ledger().set_timestamp(1_000 + COOLDOWN_SECONDS);
    let recovered = client.execute_recovery(&g1, &owner);
    assert_eq!(recovered, proposed);
    assert!(!client.is_owner_active(&owner));
    assert!(client.is_owner_active(&proposed));
    assert_eq!(client.get_guardians(&proposed).len(), 3);
    assert_eq!(client.get_recovery_threshold(&proposed), 2);
    assert!(client.get_recovery_request(&owner).is_none());
}

#[test]
fn test_cancelled_recovery_clears_request_and_blocks_execution() {
    let (env, client, owner) = setup();
    let guardians = add_n_guardians(&env, &client, &owner, 3);
    client.set_recovery_threshold(&owner, &2);

    // Cancelling when no recovery is active returns NoActiveRecovery.
    assert_eq!(
        client.try_cancel_recovery(&owner),
        Err(Ok(RecoveryError::NoActiveRecovery))
    );

    let proposed = Address::generate(&env);
    env.ledger().set_timestamp(5_000);
    client.initiate_recovery(&guardians[0], &owner, &proposed);
    client.approve_recovery(&guardians[1], &owner);
    assert!(client.get_recovery_request(&owner).is_some());

    // Owner vetoes / cancels the recovery request.
    client.cancel_recovery(&owner);
    assert!(client.get_recovery_request(&owner).is_none());

    // Execution after cancellation fails with NoActiveRecovery.
    env.ledger().set_timestamp(5_000 + COOLDOWN_SECONDS + 10);
    assert_eq!(
        client.try_execute_recovery(&guardians[0], &owner),
        Err(Ok(RecoveryError::NoActiveRecovery))
    );
    assert!(client.is_owner_active(&owner));
    assert!(!client.is_owner_active(&proposed));

    // A fresh recovery can still be initiated afterwards.
    let second_proposed = Address::generate(&env);
    client.initiate_recovery(&guardians[2], &owner, &second_proposed);
    assert_eq!(
        client.get_recovery_request(&owner).unwrap().new_address,
        second_proposed
    );
}
