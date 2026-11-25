//! Admin authorization coverage for the FHIR resource lifecycle.

use fhir::{FhirContract, FhirContractClient, FhirError};
use soroban_sdk::{testutils::Address as _, Address, Bytes, Env, String};

#[test]
fn authenticated_non_admin_cannot_mutate_resources() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let other = Address::generate(&env);
    let id = env.register(FhirContract, ());
    let client = FhirContractClient::new(&env, &id);
    let resource = String::from_str(&env, "patient-1");
    let payload = Bytes::from_slice(&env, b"original");
    let updated = Bytes::from_slice(&env, b"updated");
    client.initialize(&admin);

    assert_eq!(
        client.try_register_resource(&other, &resource, &payload),
        Err(Ok(FhirError::Unauthorized.into()))
    );
    client.register_resource(&admin, &resource, &payload);
    assert_eq!(
        client.try_update_resource(&other, &resource, &updated),
        Err(Ok(FhirError::Unauthorized.into()))
    );
    assert_eq!(
        client.try_delete_resource(&other, &resource),
        Err(Ok(FhirError::Unauthorized.into()))
    );
    assert_eq!(
        client.try_migrate_resource(&other, &resource, &2),
        Err(Ok(FhirError::Unauthorized.into()))
    );
    assert_eq!(client.get_resource(&resource), payload);
}

#[test]
fn admin_address_without_authorization_cannot_register_a_resource() {
    let env = Env::default();
    let admin = Address::generate(&env);
    let id = env.register(FhirContract, ());
    let client = FhirContractClient::new(&env, &id);
    let resource = String::from_str(&env, "patient-1");
    let payload = Bytes::from_slice(&env, b"original");
    client.initialize(&admin);

    assert!(client
        .try_register_resource(&admin, &resource, &payload)
        .is_err());
    assert_eq!(
        client.try_get_resource(&resource),
        Err(Ok(FhirError::RecordNotFound.into()))
    );
}
