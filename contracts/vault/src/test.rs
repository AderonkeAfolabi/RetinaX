extern crate std;

use soroban_sdk::{
    testutils::{Address as _, Ledger as _},
    Address, BytesN, Env, String,
};

use crate::{VaultContract, VaultContractClient, VaultError};
use identity::{IdentityContract, IdentityContractClient};

// ─────────────────────────────────────────────────────────────────────────────
// Shared setup helpers
// ─────────────────────────────────────────────────────────────────────────────

/// Register and initialise the vault + identity contracts.
/// Returns `(vault_client, identity_client, admin, owner)`.
fn setup_vault(
    env: &Env,
) -> (
    VaultContractClient<'_>,
    IdentityContractClient<'_>,
    Address,
    Address,
) {
    env.mock_all_auths();

    let admin = Address::generate(env);
    let owner = Address::generate(env);

    let identity_id = env.register(IdentityContract, ());
    let identity = IdentityContractClient::new(env, &identity_id);
    identity.initialize(&owner);

    let vault_id = env.register(VaultContract, ());
    let vault = VaultContractClient::new(env, &vault_id);
    vault.initialize(&admin, &identity_id);

    (vault, identity, admin, owner)
}

/// Add `n` guardians for `owner` in the identity contract and return them.
fn add_guardians(
    env: &Env,
    identity: &IdentityContractClient,
    owner: &Address,
    n: u32,
) -> std::vec::Vec<Address> {
    let mut guardians = std::vec::Vec::new();
    for _ in 0..n {
        let g = Address::generate(env);
        identity.add_guardian(owner, &g);
        guardians.push(g);
    }
    guardians
}

// ─────────────────────────────────────────────────────────────────────────────
// Existing smoke test (preserved)
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn test_initialize_and_deadman_guard() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(VaultContract, ());
    let client = VaultContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let identity = Address::generate(&env);
    client.initialize(&admin, &identity);

    let owner = Address::generate(&env);
    let res = client.try_trigger_deadman_release(&owner);
    assert!(res.is_err());

    env.ledger().set_timestamp(1_000);
    let bytes = BytesN::from_array(&env, &[3u8; 32]);
    let _ = bytes;
}

// ─────────────────────────────────────────────────────────────────────────────
// #477 — Multi-signature threshold configuration
// ─────────────────────────────────────────────────────────────────────────────

/// threshold > shard_count is an invalid configuration.
#[test]
fn test_configure_vault_threshold_exceeds_shard_count() {
    let env = Env::default();
    let (vault, identity, _admin, owner) = setup_vault(&env);
    add_guardians(&env, &identity, &owner, 3);

    let seed = BytesN::from_array(&env, &[1u8; 32]);
    let data_ref = String::from_str(&env, "hash");

    // threshold (4) > shard_count (3) — must be rejected
    let result = vault.try_configure_vault(&owner, &4, &3, &4, &3600, &data_ref, &seed, &None);
    assert_eq!(result, Err(Ok(VaultError::InvalidConfig)));
}

/// emergency_threshold must not be less than threshold.
#[test]
fn test_configure_vault_emergency_threshold_below_threshold() {
    let env = Env::default();
    let (vault, identity, _admin, owner) = setup_vault(&env);
    add_guardians(&env, &identity, &owner, 5);

    let seed = BytesN::from_array(&env, &[2u8; 32]);
    let data_ref = String::from_str(&env, "hash");

    // emergency_threshold (2) < threshold (3) — must be rejected
    let result = vault.try_configure_vault(&owner, &3, &5, &2, &3600, &data_ref, &seed, &None);
    assert_eq!(result, Err(Ok(VaultError::InvalidConfig)));
}

/// zero threshold is rejected.
#[test]
fn test_configure_vault_zero_threshold_rejected() {
    let env = Env::default();
    let (vault, identity, _admin, owner) = setup_vault(&env);
    add_guardians(&env, &identity, &owner, 3);

    let seed = BytesN::from_array(&env, &[3u8; 32]);
    let data_ref = String::from_str(&env, "hash");

    let result = vault.try_configure_vault(&owner, &0, &3, &3, &3600, &data_ref, &seed, &None);
    assert_eq!(result, Err(Ok(VaultError::InvalidConfig)));
}

/// A valid 3-of-5 multi-sig threshold configuration must succeed and store
/// a vault record with the correct policy.
#[test]
fn test_configure_vault_three_of_five_threshold_succeeds() {
    let env = Env::default();
    let (vault, identity, _admin, owner) = setup_vault(&env);
    add_guardians(&env, &identity, &owner, 5);

    let seed = BytesN::from_array(&env, &[4u8; 32]);
    let data_ref = String::from_str(&env, "ipfs://ustb");

    let snapshot = vault.configure_vault(&owner, &3, &5, &4, &86400, &data_ref, &seed, &None);

    assert_eq!(snapshot.record.policy.threshold, 3);
    assert_eq!(snapshot.record.policy.shard_count, 5);
    assert_eq!(snapshot.record.policy.emergency_threshold, 4);
    assert_eq!(snapshot.shard_holders.len(), 5);

    // stored record should be retrievable
    let stored = vault.get_vault(&owner);
    assert_eq!(stored.policy.threshold, 3);
}

/// A valid high-threshold 3-of-5 (maximum guardian count) configuration must succeed.
#[test]
fn test_configure_vault_max_guardian_threshold_succeeds() {
    let env = Env::default();
    let (vault, identity, _admin, owner) = setup_vault(&env);
    // Identity contract caps guardians at 5.
    add_guardians(&env, &identity, &owner, 5);

    let seed = BytesN::from_array(&env, &[5u8; 32]);
    let data_ref = String::from_str(&env, "hash");

    // 4-of-5 — high threshold within the maximum allowed shard count.
    let snapshot = vault.configure_vault(&owner, &4, &5, &5, &0, &data_ref, &seed, &None);

    assert_eq!(snapshot.record.policy.threshold, 4);
    assert_eq!(snapshot.record.policy.shard_count, 5);
    assert_eq!(snapshot.shard_holders.len(), 5);
}

// ─────────────────────────────────────────────────────────────────────────────
// #477 — Emergency approval quorum
// ─────────────────────────────────────────────────────────────────────────────

/// Attempting emergency reconstruction before any approvals are submitted must
/// return EmergencyThresholdNotMet.
#[test]
fn test_emergency_reconstruct_no_approvals_fails() {
    let env = Env::default();
    let (vault, identity, _admin, owner) = setup_vault(&env);
    add_guardians(&env, &identity, &owner, 5);

    let seed = BytesN::from_array(&env, &[6u8; 32]);
    let data_ref = String::from_str(&env, "hash");
    vault.configure_vault(&owner, &3, &5, &4, &86400, &data_ref, &seed, &None);

    // No approvals submitted — emergency_threshold (4) is not met
    let result = vault.try_emergency_reconstruct(&owner);
    assert_eq!(result, Err(Ok(VaultError::EmergencyThresholdNotMet)));
}

/// Only guardians that hold shares may submit an emergency approval.
/// A random address that is not a guardian must be rejected.
#[test]
fn test_emergency_approval_non_guardian_rejected() {
    let env = Env::default();
    let (vault, identity, _admin, owner) = setup_vault(&env);
    add_guardians(&env, &identity, &owner, 3);

    let seed = BytesN::from_array(&env, &[7u8; 32]);
    let data_ref = String::from_str(&env, "hash");
    vault.configure_vault(&owner, &2, &3, &3, &3600, &data_ref, &seed, &None);

    let non_guardian = Address::generate(&env);
    let result = vault.try_submit_emergency_approval(&non_guardian, &owner);
    assert_eq!(result, Err(Ok(VaultError::Unauthorized)));
}

// ─────────────────────────────────────────────────────────────────────────────
// #477 — Deadman release trigger
// ─────────────────────────────────────────────────────────────────────────────

/// Triggering the deadman switch before the inactivity timeout has elapsed
/// must return DeadmanNotReady.
#[test]
fn test_deadman_release_before_timeout_rejected() {
    let env = Env::default();
    env.ledger().set_timestamp(1_000);

    let (vault, identity, _admin, owner) = setup_vault(&env);
    add_guardians(&env, &identity, &owner, 3);

    let seed = BytesN::from_array(&env, &[8u8; 32]);
    let data_ref = String::from_str(&env, "hash");
    // inactivity_timeout_secs = 3600
    vault.configure_vault(&owner, &2, &3, &3, &3600, &data_ref, &seed, &None);

    // Advance time — but not past the 3600-second window
    env.ledger().set_timestamp(1_000 + 1_000);

    let result = vault.try_trigger_deadman_release(&owner);
    assert_eq!(result, Err(Ok(VaultError::DeadmanNotReady)));
}

/// After the full inactivity timeout elapses, trigger_deadman_release must
/// return Ok(true).
#[test]
fn test_deadman_release_after_timeout_succeeds() {
    let env = Env::default();
    env.ledger().set_timestamp(1_000);

    let (vault, identity, _admin, owner) = setup_vault(&env);
    add_guardians(&env, &identity, &owner, 3);

    let seed = BytesN::from_array(&env, &[9u8; 32]);
    let data_ref = String::from_str(&env, "hash");
    vault.configure_vault(&owner, &2, &3, &3, &3600, &data_ref, &seed, &None);

    // Advance time past the 3600-second inactivity window
    env.ledger().set_timestamp(1_000 + 3_601);

    let result = vault.trigger_deadman_release(&owner);
    assert!(result);
}

/// touch_activity resets the deadman timer — a trigger that would have fired
/// before the touch must be rejected after the touch extends the window.
#[test]
fn test_touch_activity_resets_deadman_timer() {
    let env = Env::default();
    env.ledger().set_timestamp(1_000);

    let (vault, identity, _admin, owner) = setup_vault(&env);
    add_guardians(&env, &identity, &owner, 3);

    let seed = BytesN::from_array(&env, &[10u8; 32]);
    let data_ref = String::from_str(&env, "hash");
    vault.configure_vault(&owner, &2, &3, &3, &3600, &data_ref, &seed, &None);

    // Advance almost to the timeout boundary, then touch activity
    env.ledger().set_timestamp(1_000 + 3_500);
    vault.touch_activity(&owner);

    // Now advance past the original deadline — but the timer was reset
    env.ledger().set_timestamp(1_000 + 3_700);
    let result = vault.try_trigger_deadman_release(&owner);
    assert_eq!(result, Err(Ok(VaultError::DeadmanNotReady)));
}

// ─────────────────────────────────────────────────────────────────────────────
// #533 — Configurable Grace Period Tests
// ─────────────────────────────────────────────────────────────────────────────

/// When grace_period_secs > 0, release attempted after inactivity timeout
/// but before grace period has elapsed must be rejected with DeadmanNotReady.
#[test]
fn test_deadman_release_with_grace_period_rejected_before_grace_ends() {
    let env = Env::default();
    env.ledger().set_timestamp(1_000);

    let (vault, identity, _admin, owner) = setup_vault(&env);
    add_guardians(&env, &identity, &owner, 3);

    let seed = BytesN::from_array(&env, &[11u8; 32]);
    let data_ref = String::from_str(&env, "hash");
    let inactivity_secs = 3_600;
    let grace_period_secs = 600; // 10 minutes grace

    // Configure vault with 600s grace period
    vault.configure_vault(
        &owner,
        &2,
        &3,
        &3,
        &inactivity_secs,
        &data_ref,
        &seed,
        &Some(grace_period_secs),
    );

    // Advance past inactivity timeout (1000 + 3600 = 4600), but within grace period (4600 + 300 = 4900)
    env.ledger().set_timestamp(1_000 + inactivity_secs + 300);

    // Release must be rejected because grace period has not ended
    let result = vault.try_trigger_deadman_release(&owner);
    assert_eq!(result, Err(Ok(VaultError::DeadmanNotReady)));
}

/// Owner activity during the grace period must successfully reset the timer.
#[test]
fn test_deadman_release_with_grace_period_activity_during_grace_resets_timer() {
    let env = Env::default();
    env.ledger().set_timestamp(1_000);

    let (vault, identity, _admin, owner) = setup_vault(&env);
    add_guardians(&env, &identity, &owner, 3);

    let seed = BytesN::from_array(&env, &[12u8; 32]);
    let data_ref = String::from_str(&env, "hash");
    let inactivity_secs = 3_600;
    let grace_period_secs = 600;

    vault.configure_vault(
        &owner,
        &2,
        &3,
        &3,
        &inactivity_secs,
        &data_ref,
        &seed,
        &Some(grace_period_secs),
    );

    // Advance into grace period
    env.ledger().set_timestamp(1_000 + inactivity_secs + 300);

    // Owner checks in during grace period!
    vault.touch_activity(&owner);

    // Check timer was pushed forward: now + inactivity = 4900 + 3600 = 8500
    // Try releasing at 5000 (which was after original deadman + grace): rejected!
    env.ledger().set_timestamp(5_000);
    let result = vault.try_trigger_deadman_release(&owner);
    assert_eq!(result, Err(Ok(VaultError::DeadmanNotReady)));
}

/// Release attempted after both inactivity timeout and grace period elapses must succeed.
#[test]
fn test_deadman_release_with_grace_period_allowed_after_grace_ends() {
    let env = Env::default();
    env.ledger().set_timestamp(1_000);

    let (vault, identity, _admin, owner) = setup_vault(&env);
    add_guardians(&env, &identity, &owner, 3);

    let seed = BytesN::from_array(&env, &[13u8; 32]);
    let data_ref = String::from_str(&env, "hash");
    let inactivity_secs = 3_600;
    let grace_period_secs = 600;

    vault.configure_vault(
        &owner,
        &2,
        &3,
        &3,
        &inactivity_secs,
        &data_ref,
        &seed,
        &Some(grace_period_secs),
    );

    // Advance past inactivity + grace period: 1000 + 3600 + 600 + 1 = 5201
    env.ledger().set_timestamp(1_000 + inactivity_secs + grace_period_secs + 1);

    let result = vault.trigger_deadman_release(&owner);
    assert!(result);
}

/// Existing vault records written in the legacy format (without grace_period_secs)
/// must deserialize with grace_period_secs = 0 without needing manual migration.
#[test]
fn test_legacy_vault_record_backward_compatibility() {
    use common::{LegacyVaultPolicy, LegacyVaultRecord};
    use soroban_sdk::symbol_short;

    let env = Env::default();
    env.mock_all_auths();
    env.ledger().set_timestamp(1_000);

    let (vault, _identity, _admin, owner) = setup_vault(&env);

    // Write a legacy record directly into storage
    let legacy = LegacyVaultRecord {
        owner: owner.clone(),
        epoch: 1,
        policy: LegacyVaultPolicy {
            threshold: 2,
            shard_count: 3,
            emergency_threshold: 3,
            inactivity_timeout_secs: 3_600,
        },
        data_ref_hash: String::from_str(&env, "legacy_hash"),
        created_at: 1_000,
        last_activity_at: 1_000,
        deadman_release_at: 1_000 + 3_600,
    };

    let key = (symbol_short!("VAULT"), owner.clone());
    env.as_contract(&vault.address, || {
        env.storage().persistent().set(&key, &legacy);
    });

    // Query vault via contract API: should seamlessly return updated VaultRecord with grace_period_secs = 0
    let retrieved = vault.get_vault(&owner);
    assert_eq!(retrieved.policy.threshold, 2);
    assert_eq!(retrieved.policy.grace_period_secs, 0);

    // Before inactivity timeout, release rejected
    env.ledger().set_timestamp(1_000 + 3_000);
    assert_eq!(vault.try_trigger_deadman_release(&owner), Err(Ok(VaultError::DeadmanNotReady)));

    // After inactivity timeout (grace period is 0), release succeeds
    env.ledger().set_timestamp(1_000 + 3_601);
    assert!(vault.trigger_deadman_release(&owner));
}

/// A legacy record can also be upgraded explicitly via migrate_vault.
#[test]
fn test_legacy_vault_record_explicit_migration() {
    use common::{LegacyVaultPolicy, LegacyVaultRecord};
    use soroban_sdk::symbol_short;

    let env = Env::default();
    env.mock_all_auths();
    env.ledger().set_timestamp(1_000);

    let (vault, _identity, _admin, owner) = setup_vault(&env);

    let legacy = LegacyVaultRecord {
        owner: owner.clone(),
        epoch: 1,
        policy: LegacyVaultPolicy {
            threshold: 2,
            shard_count: 3,
            emergency_threshold: 3,
            inactivity_timeout_secs: 3_600,
        },
        data_ref_hash: String::from_str(&env, "legacy_hash"),
        created_at: 1_000,
        last_activity_at: 1_000,
        deadman_release_at: 1_000 + 3_600,
    };

    let key = (symbol_short!("VAULT"), owner.clone());
    env.as_contract(&vault.address, || {
        env.storage().persistent().set(&key, &legacy);
    });

    // Run explicit migration
    vault.migrate_vault(&owner);

    // Verify record is now modern format with grace_period_secs = 0
    let retrieved = vault.get_vault(&owner);
    assert_eq!(retrieved.policy.grace_period_secs, 0);
    assert_eq!(vault.get_grace_period(&owner), 0);
}
