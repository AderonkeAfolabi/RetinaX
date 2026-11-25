//! Verify that loading a FHIR resource emits no application event.

use fhir::{FhirContract, FhirContractClient};
use soroban_sdk::{
    testutils::{Address as _, Events},
    Address, Bytes, Env, String,
};

#[test]
fn get_resource_does_not_publish_a_contract_event() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let contract = env.register(FhirContract, ());
    let client = FhirContractClient::new(&env, &contract);
    let id = String::from_str(&env, "patient-1");
    let payload = Bytes::from_slice(&env, b"patient payload");
    client.initialize(&admin);
    client.register_resource(&admin, &id, &payload);

    assert_eq!(client.get_resource(&id), payload);
    assert!(env.events().all().events().is_empty());
}
