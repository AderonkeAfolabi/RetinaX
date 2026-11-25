//! Zero-Value Parameter Passing Edge Case tests for the `fhir` crate.
//!
//! Sends inputs of zero, empty strings, or blank/zero addresses to every
//! critical state-modifying function to verify consistent validation and
//! correct revert behaviour — preventing silent acceptance of degenerate data.

use soroban_sdk::{testutils::Address as _, Address, Bytes, Env, String};

use fhir::{FhirContract, FhirContractClient, FhirError};

// ── Helpers ───────────────────────────────────────────────────────────────────

fn setup() -> (Env, FhirContractClient<'static>, Address) {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(FhirContract, ());
    let client = FhirContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    client.initialize(&admin);

    (env, client, admin)
}

// ── Empty resource ID ─────────────────────────────────────────────────────────

#[test]
fn test_register_resource_with_empty_id_fails() {
    let (env, client, admin) = setup();

    let empty_id = String::from_str(&env, "");
    let payload = Bytes::from_slice(&env, br#"{"resourceType":"Patient"}"#);

    let result = client.try_register_resource(&admin, &empty_id, &payload);

    assert_eq!(
        result.err().unwrap().unwrap(),
        soroban_sdk::Error::from(FhirError::InvalidPayload),
        "register_resource must reject an empty resource ID"
    );
}

// ── Empty payload ─────────────────────────────────────────────────────────────

#[test]
fn test_register_resource_with_empty_payload_fails() {
    let (env, client, admin) = setup();

    let resource_id = String::from_str(&env, "res-empty-payload");
    let empty_payload = Bytes::new(&env);

    let result = client.try_register_resource(&admin, &resource_id, &empty_payload);

    assert_eq!(
        result.err().unwrap().unwrap(),
        soroban_sdk::Error::from(FhirError::InvalidPayload),
        "register_resource must reject an empty payload"
    );
}

// ── Both resource ID and payload empty ────────────────────────────────────────

#[test]
fn test_register_resource_with_all_empty_inputs_fails() {
    let (env, client, admin) = setup();

    let empty = String::from_str(&env, "");
    let result = client.try_register_resource(&admin, &empty, &Bytes::new(&env));

    assert_eq!(
        result.err().unwrap().unwrap(),
        soroban_sdk::Error::from(FhirError::InvalidPayload),
        "register_resource must reject when both ID and payload are empty"
    );
}

// ── Update non-existent resource (zero / unknown ID) ─────────────────────────

#[test]
fn test_update_nonexistent_resource_fails() {
    let (env, client, admin) = setup();

    let ghost_id = String::from_str(&env, "does-not-exist");
    let payload = Bytes::from_slice(&env, br#"{"resourceType":"Medication"}"#);

    let result = client.try_update_resource(&admin, &ghost_id, &payload);

    assert_eq!(
        result.err().unwrap().unwrap(),
        soroban_sdk::Error::from(FhirError::RecordNotFound),
        "update_resource must return NotFound for an unknown resource ID"
    );
}

// ── Delete non-existent resource ──────────────────────────────────────────────

#[test]
fn test_delete_nonexistent_resource_fails() {
    let (env, client, admin) = setup();

    let ghost_id = String::from_str(&env, "ghost-res");
    let result = client.try_delete_resource(&admin, &ghost_id);

    assert_eq!(
        result.err().unwrap().unwrap(),
        soroban_sdk::Error::from(FhirError::RecordNotFound),
        "delete_resource must return NotFound for a non-existent resource"
    );
}

// ── Double-initialize guard ───────────────────────────────────────────────────

#[test]
fn test_double_initialize_fails() {
    let (env, client, _admin) = setup();

    let new_admin = Address::generate(&env);
    let result = client.try_initialize(&new_admin);

    assert_eq!(
        result.err().unwrap().unwrap(),
        soroban_sdk::Error::from(FhirError::AlreadyInitialized),
        "initialize must revert if the contract has already been initialised"
    );
}
