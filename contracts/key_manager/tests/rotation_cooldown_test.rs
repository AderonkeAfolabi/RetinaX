//! Rotation cooldown tests (issue #435).
//!
//! A key with a non-zero `rotation_interval` acts as a cooldown: `rotate_key`
//! returns `RotationNotDue` until `interval` seconds have passed since the last
//! rotation (or since creation). These tests pin the boundary, the restart of
//! the cooldown after each rotation, and that rejected attempts change nothing.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use key_manager::{
    ContractError, KeyManagerContract, KeyManagerContractClient, KeyPolicy, KeyType,
};
use soroban_sdk::{testutils::Address as _, testutils::Ledger, Address, BytesN, Env, Vec};

const INTERVAL: u64 = 3_600;
const START: u64 = 1_000;

fn setup() -> (Env, KeyManagerContractClient<'static>, Address) {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().set_timestamp(START);
    let contract_id = env.register(KeyManagerContract, ());
    let client = KeyManagerContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let identity = Address::generate(&env);
    client.initialize(&admin, &identity);
    (env, client, admin)
}

fn unrestricted_policy(env: &Env) -> KeyPolicy {
    KeyPolicy {
        max_uses: 0,
        not_before: 0,
        not_after: 0,
        allowed_ops: Vec::new(env),
    }
}

fn make_master(
    env: &Env,
    client: &KeyManagerContractClient,
    admin: &Address,
    rotation_interval: u64,
    seed: u8,
) -> BytesN<32> {
    let key_bytes = BytesN::from_array(env, &[seed; 32]);
    client.create_master_key(
        admin,
        &KeyType::Encryption,
        &unrestricted_policy(env),
        &rotation_interval,
        &key_bytes,
    )
}

fn set_time(env: &Env, ts: u64) {
    env.ledger().set_timestamp(ts);
}

#[test]
fn test_rotation_rejected_one_second_before_cooldown_ends() {
    let (env, client, admin) = setup();
    let master = make_master(&env, &client, &admin, INTERVAL, 1);

    set_time(&env, START + INTERVAL - 1);
    assert_eq!(
        client.try_rotate_key(&admin, &master),
        Err(Ok(ContractError::RotationNotDue))
    );
}

#[test]
fn test_rotation_allowed_exactly_at_cooldown_boundary() {
    let (env, client, admin) = setup();
    let master = make_master(&env, &client, &admin, INTERVAL, 2);

    set_time(&env, START + INTERVAL);
    assert_eq!(client.rotate_key(&admin, &master), 2);
}

#[test]
fn test_successful_rotation_restarts_the_cooldown() {
    let (env, client, admin) = setup();
    let master = make_master(&env, &client, &admin, INTERVAL, 3);

    // First rotation at the boundary.
    set_time(&env, START + INTERVAL);
    assert_eq!(client.rotate_key(&admin, &master), 2);

    // The cooldown now counts from the rotation, not from creation.
    set_time(&env, START + 2 * INTERVAL - 1);
    assert_eq!(
        client.try_rotate_key(&admin, &master),
        Err(Ok(ContractError::RotationNotDue))
    );

    set_time(&env, START + 2 * INTERVAL);
    assert_eq!(client.rotate_key(&admin, &master), 3);

    let record = client.get_key_record(&master).unwrap();
    assert_eq!(record.last_rotated, START + 2 * INTERVAL);
}

#[test]
fn test_rejected_rotation_does_not_change_version_or_cooldown() {
    let (env, client, admin) = setup();
    let master = make_master(&env, &client, &admin, INTERVAL, 4);

    // Several rejected attempts inside the cooldown.
    for offset in [0u64, 1, INTERVAL / 2, INTERVAL - 1] {
        set_time(&env, START + offset);
        assert_eq!(
            client.try_rotate_key(&admin, &master),
            Err(Ok(ContractError::RotationNotDue))
        );
    }

    let record = client.get_key_record(&master).unwrap();
    assert_eq!(record.current_version, 1);
    assert_eq!(record.last_rotated, START);

    // The rejected attempts did not move the cooldown: the boundary still applies.
    set_time(&env, START + INTERVAL);
    assert_eq!(client.rotate_key(&admin, &master), 2);
}

#[test]
fn test_zero_interval_allows_back_to_back_rotations() {
    let (env, client, admin) = setup();
    let master = make_master(&env, &client, &admin, 0, 5);

    // No cooldown at all: rotations at the same timestamp all succeed.
    assert_eq!(client.rotate_key(&admin, &master), 2);
    assert_eq!(client.rotate_key(&admin, &master), 3);
    assert_eq!(client.rotate_key(&admin, &master), 4);
}

#[test]
fn test_cooldown_applies_to_stranger_as_unauthorized_not_as_timing() {
    let (env, client, admin) = setup();
    let master = make_master(&env, &client, &admin, INTERVAL, 6);
    let stranger = Address::generate(&env);

    // Before the cooldown, a stranger is refused for authorisation, not timing.
    set_time(&env, START + INTERVAL);
    assert_eq!(
        client.try_rotate_key(&stranger, &master),
        Err(Ok(ContractError::Unauthorized))
    );

    // The stranger's failed attempt does not consume the owner's rotation.
    assert_eq!(client.rotate_key(&admin, &master), 2);
}
