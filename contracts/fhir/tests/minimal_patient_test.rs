use fhir::{FhirContract, FhirContractClient};
use soroban_sdk::{Env, String};

#[test]
fn only_maps_an_opaque_patient_id_and_active_state() {
    let env = Env::default();
    let contract = env.register(FhirContract, ());
    let client = FhirContractClient::new(&env, &contract);
    let id = String::from_str(&env, "pt-opaque.123");
    let patient = client.map_minimal_patient(&id, &false);
    assert_eq!(patient.id, id);
    assert!(!patient.active);
    assert!(client.try_map_minimal_patient(&String::from_str(&env, "name/email"), &true).is_err());
}