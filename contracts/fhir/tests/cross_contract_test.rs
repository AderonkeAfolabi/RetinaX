//! Integration tests for another contract reading versioned FHIR resources.

use fhir::{FhirContract, FhirContractClient, FhirError};
use soroban_sdk::{contract, contractimpl, testutils::Address as _, Address, Bytes, Env, String};

#[contract]
struct ResourceReader;

#[contractimpl]
impl ResourceReader {
    pub fn read(env: Env, fhir: Address, id: String) -> Bytes {
        FhirContractClient::new(&env, &fhir).get_resource(&id)
    }
}

#[test]
fn reads_exact_bytes_for_multiple_resources() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let fhir = env.register(FhirContract, ());
    let reader = env.register(ResourceReader, ());
    let registry = FhirContractClient::new(&env, &fhir);
    let reader = ResourceReaderClient::new(&env, &reader);
    registry.initialize(&admin);

    for (name, bytes) in [("record-1", b"first"), ("record-2", b"other")] {
        let id = String::from_str(&env, name);
        registry.register_resource(&admin, &id, &Bytes::from_slice(&env, bytes));
    }

    for (name, bytes) in [("record-2", b"other"), ("record-1", b"first")] {
        let id = String::from_str(&env, name);
        assert_eq!(reader.read(&fhir, &id), Bytes::from_slice(&env, bytes));
    }
}

#[test]
fn propagates_missing_resource_error() {
    let env = Env::default();
    let admin = Address::generate(&env);
    let fhir = env.register(FhirContract, ());
    let reader = env.register(ResourceReader, ());
    FhirContractClient::new(&env, &fhir).initialize(&admin);

    let result = ResourceReaderClient::new(&env, &reader)
        .try_read(&fhir, &String::from_str(&env, "missing"));
    assert_eq!(result, Err(Ok(FhirError::RecordNotFound.into())));
}
