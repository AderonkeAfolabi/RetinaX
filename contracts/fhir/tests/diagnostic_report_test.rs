use fhir::types::{DiagnosticReport, DiagnosticReportStatus};
use fhir::{FhirContract, FhirContractClient};
use soroban_sdk::{vec, Env, String};

#[test]
fn maps_report_coding_and_opaque_result_references() {
    let env = Env::default();
    let contract = env.register(FhirContract, ());
    let client = FhirContractClient::new(&env, &contract);
    let mut report = DiagnosticReport {
        id: String::from_str(&env, "report-1"),
        status: DiagnosticReportStatus::Final,
        code_system: String::from_str(&env, "http://loinc.org"),
        code_value: String::from_str(&env, "29271-4"),
        subject_id: String::from_str(&env, "patient-1"),
        effective_datetime: 100,
        result_ids: vec![&env, String::from_str(&env, "observation-1")],
    };
    assert_eq!(client.map_diagnostic_report(&report), report);
    report.result_ids = vec![
        &env,
        String::from_str(&env, "http://example.com/observation"),
    ];
    assert!(client.try_map_diagnostic_report(&report).is_err());
}
