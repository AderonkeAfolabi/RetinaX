//! Issue #426 — timelock queue / cancel / expiry coverage.
//!
//! Ledger time is manipulated with `env.ledger().set_timestamp(...)` following
//! `docs/TIMESTAMP_ASSUMPTIONS.md`. In this contract `execute_after` is the
//! earliest execution time (the grace-period boundary), not a deadline:
//! there is no upper expiry bound, so "late" execution is always allowed.

use soroban_sdk::{
    testutils::{Address as _, Ledger},
    Address, BytesN, Env,
};
use timelock::{TimelockContract, TimelockContractClient, TimelockError};

fn setup(env: &Env, min_delay: u64) -> (TimelockContractClient<'_>, Address) {
    env.mock_all_auths();
    let admin = Address::generate(env);
    let contract_id = env.register(TimelockContract, ());
    let client = TimelockContractClient::new(env, &contract_id);
    client.initialize(&admin, &min_delay);
    (client, admin)
}

fn tx_id(env: &Env, value: u8) -> BytesN<32> {
    let mut bytes = [0u8; 32];
    bytes[0] = value;
    BytesN::from_array(env, &bytes)
}

/// A transaction cannot be queued with less than the configured grace delay,
/// and a queued id cannot be queued twice.
#[test]
fn test_queue_grace_period_boundaries() {
    let env = Env::default();
    let (client, admin) = setup(&env, 100);
    env.ledger().set_timestamp(1_000);

    let id = tx_id(&env, 1);
    assert_eq!(
        client.try_queue_tx(&admin, &id, &99),
        Err(Ok(TimelockError::DelayTooShort))
    );
    assert_eq!(client.queue_tx(&admin, &id, &100), 1_100);
    assert_eq!(
        client.try_queue_tx(&admin, &id, &200),
        Err(Ok(TimelockError::TxAlreadyQueued))
    );

    let queued = client.get_tx(&id);
    assert_eq!(queued.id, id);
    assert_eq!(queued.execute_after, 1_100);
    assert!(!queued.priority);
}

/// Execution before the grace period elapses is rejected and leaves the
/// queue untouched; at the exact boundary it succeeds and removes the tx.
#[test]
fn test_execute_early_is_rejected_and_boundary_is_inclusive() {
    let env = Env::default();
    let (client, admin) = setup(&env, 100);
    env.ledger().set_timestamp(1_000);
    let id = tx_id(&env, 1);
    client.queue_tx(&admin, &id, &100);

    env.ledger().set_timestamp(1_099);
    assert_eq!(
        client.try_execute_tx(&admin, &id),
        Err(Ok(TimelockError::TooEarly))
    );
    assert_eq!(client.get_queue().len(), 1);

    env.ledger().set_timestamp(1_100);
    client.execute_tx(&admin, &id);
    assert_eq!(client.get_queue().len(), 0);
    assert_eq!(client.try_get_tx(&id), Err(Ok(TimelockError::TxNotFound)));
}

/// There is no upper expiry: a transaction remains executable long after its
/// `execute_after` timestamp.
#[test]
fn test_execute_late_after_expiry_is_allowed() {
    let env = Env::default();
    let (client, admin) = setup(&env, 100);
    env.ledger().set_timestamp(1_000);
    let id = tx_id(&env, 1);
    client.queue_tx(&admin, &id, &100);

    env.ledger().set_timestamp(10_000_000);
    client.execute_tx(&admin, &id);
    assert_eq!(client.get_queue().len(), 0);
}

/// Cancellation and queuing are restricted to the proposer/admin. A different
/// (even correctly authenticated) address cannot touch the queued tx.
#[test]
fn test_cancel_by_non_proposer_is_unauthorized() {
    let env = Env::default();
    let (client, admin) = setup(&env, 100);
    env.ledger().set_timestamp(1_000);
    let id = tx_id(&env, 1);
    client.queue_tx(&admin, &id, &100);

    let outsider = Address::generate(&env);
    assert_eq!(
        client.try_cancel_tx(&outsider, &id),
        Err(Ok(TimelockError::Unauthorized))
    );
    assert_eq!(
        client.try_queue_tx(&outsider, &tx_id(&env, 2), &100),
        Err(Ok(TimelockError::Unauthorized))
    );
    assert_eq!(
        client.try_prioritize_tx(&outsider, &id),
        Err(Ok(TimelockError::Unauthorized))
    );

    env.ledger().set_timestamp(2_000);
    assert_eq!(
        client.try_execute_tx(&outsider, &id),
        Err(Ok(TimelockError::Unauthorized))
    );
    assert_eq!(client.get_queue().len(), 1);
}

/// The proposer may cancel at any point — both inside the grace period and
/// after `execute_after` has passed — and cancelling twice fails.
#[test]
fn test_cancel_before_and_after_expiry() {
    let env = Env::default();
    let (client, admin) = setup(&env, 100);
    env.ledger().set_timestamp(1_000);

    let early = tx_id(&env, 1);
    client.queue_tx(&admin, &early, &100);
    client.cancel_tx(&admin, &early);
    assert_eq!(client.get_queue().len(), 0);
    assert_eq!(
        client.try_cancel_tx(&admin, &early),
        Err(Ok(TimelockError::TxNotFound))
    );

    let late = tx_id(&env, 2);
    client.queue_tx(&admin, &late, &100);
    env.ledger().set_timestamp(5_000);
    client.cancel_tx(&admin, &late);
    assert_eq!(client.get_queue().len(), 0);
}

/// A prioritized transaction bypasses the grace period and can be executed
/// early by the proposer.
#[test]
fn test_prioritized_tx_executes_inside_grace_period() {
    let env = Env::default();
    let (client, admin) = setup(&env, 100);
    env.ledger().set_timestamp(1_000);
    let id = tx_id(&env, 1);
    client.queue_tx(&admin, &id, &1_000);
    client.prioritize_tx(&admin, &id);

    assert!(client.get_tx(&id).priority);
    env.ledger().set_timestamp(1_001);
    client.execute_tx(&admin, &id);
    assert_eq!(client.get_queue().len(), 0);
}

/// Timestamp boundaries from `docs/TIMESTAMP_ASSUMPTIONS.md`: time 0, the
/// inclusive `execute_after` boundary, and saturation at `u64::MAX`.
#[test]
fn test_timestamp_boundaries_at_zero_and_max() {
    let env = Env::default();
    let (client, admin) = setup(&env, 0);

    env.ledger().set_timestamp(0);
    let zero_id = tx_id(&env, 1);
    assert_eq!(client.queue_tx(&admin, &zero_id, &100), 100);

    env.ledger().set_timestamp(99);
    assert_eq!(
        client.try_execute_tx(&admin, &zero_id),
        Err(Ok(TimelockError::TooEarly))
    );
    env.ledger().set_timestamp(100);
    client.execute_tx(&admin, &zero_id);

    let max_id = tx_id(&env, 2);
    env.ledger().set_timestamp(u64::MAX);
    assert_eq!(client.queue_tx(&admin, &max_id, &10), u64::MAX);
    client.execute_tx(&admin, &max_id);
}
