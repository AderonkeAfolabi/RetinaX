//! FHIR R4 Observation validation tests for eye examinations (issue #316).
//!
//! Verifies that:
//! * Required FHIR R4 fields (`status`, `code`, `subject`, `effective[x]`)
//!   are enforced.
//! * Coding systems reference LOINC (`http://loinc.org`) or SNOMED CT
//!   (`http://snomed.info/sct`).
//! * Eye-examination observations accept only the documented optometry codes.

#![allow(clippy::unwrap_used)]

use fhir::types::{FhirError, Observation, ObservationStatus};
use fhir::validation::{
    is_supported_coding_system, validate_eye_examination_observation, validate_observation_r4,
    LOINC_EYE_PHYSICAL_EXAM, LOINC_IOP, LOINC_SYSTEM, LOINC_VISUAL_ACUITY_BEST_CORR_LEFT,
    LOINC_VISUAL_ACUITY_BEST_CORR_RIGHT, LOINC_VISUAL_ACUITY_PANEL,
    LOINC_VISUAL_ACUITY_UNCORR_LEFT, LOINC_VISUAL_ACUITY_UNCORR_RIGHT,
    SNOMED_DISTANCE_VISUAL_ACUITY, SNOMED_SYSTEM, SNOMED_UNCORRECTED_VISUAL_ACUITY,
    SNOMED_VISUAL_ACUITY,
};
use fhir::{FhirContract, FhirContractClient};
use soroban_sdk::{testutils::Address as _, Address, Env, String};

// ── Helpers ───────────────────────────────────────────────────────────────────

fn setup() -> (Env, FhirContractClient<'static>) {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(FhirContract, ());
    let client = FhirContractClient::new(&env, &contract_id);
    client.initialize(&Address::generate(&env));
    (env, client)
}

/// A fully valid eye-examination observation (LOINC IOP, all required fields).
fn valid_observation(env: &Env) -> Observation {
    Observation {
        id: String::from_str(env, "obs-1"),
        status: ObservationStatus::Final,
        code_system: String::from_str(env, LOINC_SYSTEM),
        code_value: String::from_str(env, LOINC_IOP),
        subject_id: String::from_str(env, "patient-42"),
        value: String::from_str(env, "L:16 R:15 (Goldmann)"),
        effective_datetime: 1_700_000_000,
    }
}

// ── Required R4 fields ────────────────────────────────────────────────────────

#[test]
fn test_valid_observation_passes_r4_validation() {
    let (env, client) = setup();
    let obs = valid_observation(&env);
    assert!(client.try_validate_observation_r4(&obs).is_ok());
    assert!(client.try_validate_eye_exam_observation(&obs).is_ok());
}

#[test]
fn test_missing_code_fails_r4_validation() {
    let (env, client) = setup();
    let mut obs = valid_observation(&env);
    obs.code_system = String::from_str(&env, "");
    assert_eq!(
        client.try_validate_observation_r4(&obs),
        Err(Ok(FhirError::InvalidRecordData))
    );

    let mut obs2 = valid_observation(&env);
    obs2.code_value = String::from_str(&env, "");
    assert_eq!(
        client.try_validate_observation_r4(&obs2),
        Err(Ok(FhirError::InvalidRecordData))
    );
}

#[test]
fn test_missing_subject_fails_r4_validation() {
    let (env, client) = setup();
    let mut obs = valid_observation(&env);
    obs.subject_id = String::from_str(&env, "");
    assert_eq!(
        client.try_validate_observation_r4(&obs),
        Err(Ok(FhirError::InvalidRecordData))
    );
}

#[test]
fn test_missing_effective_datetime_fails_r4_validation() {
    let (env, client) = setup();
    let mut obs = valid_observation(&env);
    obs.effective_datetime = 0;
    assert_eq!(
        client.try_validate_observation_r4(&obs),
        Err(Ok(FhirError::InvalidRecordData))
    );
}

#[test]
fn test_unknown_coding_system_fails_r4_validation() {
    let (env, client) = setup();
    let mut obs = valid_observation(&env);
    obs.code_system = String::from_str(&env, "http://example.com/codes");
    assert_eq!(
        client.try_validate_observation_r4(&obs),
        Err(Ok(FhirError::InvalidPayload))
    );
}

// ── Coding system references ─────────────────────────────────────────────────

#[test]
fn test_canonical_loinc_and_snomed_uris_supported() {
    let (env, _) = setup();
    let loinc = String::from_str(&env, LOINC_SYSTEM);
    let snomed = String::from_str(&env, SNOMED_SYSTEM);
    assert!(is_supported_coding_system(&loinc));
    assert!(is_supported_coding_system(&snomed));
}

#[test]
fn test_legacy_shorthand_labels_still_supported() {
    let (env, _) = setup();
    assert!(is_supported_coding_system(&String::from_str(&env, "LOINC")));
    assert!(is_supported_coding_system(&String::from_str(
        &env, "SNOMED"
    )));
    assert!(is_supported_coding_system(&String::from_str(
        &env,
        "SNOMED CT"
    )));
}

#[test]
fn test_non_loinc_snomed_system_rejected() {
    let (env, _) = setup();
    let rxnorm = String::from_str(&env, "http://www.nlm.nih.gov/research/umls/rxnorm");
    assert!(!is_supported_coding_system(&rxnorm));
}

// ── Eye-examination codes ────────────────────────────────────────────────────

#[test]
fn test_all_eye_exam_loinc_codes_accepted() {
    let (env, client) = setup();
    let codes = [
        LOINC_EYE_PHYSICAL_EXAM,
        LOINC_IOP,
        LOINC_VISUAL_ACUITY_UNCORR_RIGHT,
        LOINC_VISUAL_ACUITY_UNCORR_LEFT,
        LOINC_VISUAL_ACUITY_BEST_CORR_RIGHT,
        LOINC_VISUAL_ACUITY_BEST_CORR_LEFT,
        LOINC_VISUAL_ACUITY_PANEL,
    ];
    for code in codes {
        let mut obs = valid_observation(&env);
        obs.code_value = String::from_str(&env, code);
        assert!(
            client.try_validate_eye_exam_observation(&obs).is_ok(),
            "LOINC {code} must be accepted as an eye-examination code"
        );
    }
}

#[test]
fn test_eye_exam_snomed_codes_accepted() {
    let (env, client) = setup();
    let codes = [
        SNOMED_VISUAL_ACUITY,
        SNOMED_DISTANCE_VISUAL_ACUITY,
        SNOMED_UNCORRECTED_VISUAL_ACUITY,
    ];
    for code in codes {
        let mut obs = valid_observation(&env);
        obs.code_system = String::from_str(&env, SNOMED_SYSTEM);
        obs.code_value = String::from_str(&env, code);
        assert!(
            client.try_validate_eye_exam_observation(&obs).is_ok(),
            "SNOMED {code} must be accepted as an eye-examination code"
        );
    }
}

#[test]
fn test_non_eye_exam_loinc_code_rejected_by_eye_exam_validation() {
    let (env, client) = setup();
    // 2160-0 (creatinine) is a valid LOINC code but not an eye-exam code.
    let mut obs = valid_observation(&env);
    obs.code_value = String::from_str(&env, "2160-0");
    assert!(client.try_validate_observation_r4(&obs).is_ok());
    assert_eq!(
        client.try_validate_eye_exam_observation(&obs),
        Err(Ok(FhirError::InvalidPayload))
    );
}

#[test]
fn test_eye_exam_validation_enforces_required_fields() {
    let (env, client) = setup();
    let mut obs = valid_observation(&env);
    obs.subject_id = String::from_str(&env, "");
    assert_eq!(
        client.try_validate_eye_exam_observation(&obs),
        Err(Ok(FhirError::InvalidRecordData))
    );
}

// ── Status field ─────────────────────────────────────────────────────────────

#[test]
fn test_all_observation_statuses_are_present_and_valid() {
    let (env, client) = setup();
    let statuses = [
        ObservationStatus::Registered,
        ObservationStatus::Preliminary,
        ObservationStatus::Final,
        ObservationStatus::Amended,
        ObservationStatus::Corrected,
        ObservationStatus::Cancelled,
        ObservationStatus::EnteredInError,
        ObservationStatus::Unknown,
    ];
    for status in statuses {
        let mut obs = valid_observation(&env);
        obs.status = status;
        // status is a required field: every variant must be present and pass.
        assert!(client.try_validate_observation_r4(&obs).is_ok());
    }
}

// ── Pure-function API ────────────────────────────────────────────────────────

#[test]
fn test_free_functions_match_contract_entrypoints() {
    let (env, _) = setup();
    let obs = valid_observation(&env);
    assert!(validate_observation_r4(&obs).is_ok());
    assert!(validate_eye_examination_observation(&obs).is_ok());

    let mut bad = obs.clone();
    bad.subject_id = String::from_str(&env, "");
    assert_eq!(
        validate_observation_r4(&bad),
        Err(FhirError::InvalidRecordData)
    );
}

// ── Boundary tests for each eye-examination rule (issue #531) ────────────────

/// The smallest observation that satisfies every rule: a one-character
/// subject, the earliest non-zero timestamp, and the fields no rule checks
/// (`id`, `value`) left empty.
fn minimal_observation(env: &Env) -> Observation {
    Observation {
        id: String::from_str(env, ""),
        subject_id: String::from_str(env, "p"),
        value: String::from_str(env, ""),
        effective_datetime: 1,
        ..valid_observation(env)
    }
}

#[test]
fn test_minimal_eye_exam_observation_passes() {
    let (env, client) = setup();
    assert_eq!(
        client.try_validate_eye_exam_observation(&minimal_observation(&env)),
        Ok(Ok(()))
    );
}

#[test]
fn test_eye_exam_missing_code_system_is_invalid_record_data() {
    let (env, client) = setup();
    let mut obs = minimal_observation(&env);
    obs.code_system = String::from_str(&env, "");
    assert_eq!(
        client.try_validate_eye_exam_observation(&obs),
        Err(Ok(FhirError::InvalidRecordData))
    );
}

#[test]
fn test_eye_exam_missing_code_value_is_invalid_record_data() {
    let (env, client) = setup();
    let mut obs = minimal_observation(&env);
    obs.code_value = String::from_str(&env, "");
    assert_eq!(
        client.try_validate_eye_exam_observation(&obs),
        Err(Ok(FhirError::InvalidRecordData))
    );
}

#[test]
fn test_eye_exam_zero_effective_datetime_is_invalid_record_data() {
    let (env, client) = setup();
    let mut obs = minimal_observation(&env);
    obs.effective_datetime = 0;
    assert_eq!(
        client.try_validate_eye_exam_observation(&obs),
        Err(Ok(FhirError::InvalidRecordData))
    );
}

#[test]
fn test_eye_exam_unsupported_code_system_is_invalid_payload() {
    let (env, client) = setup();
    let mut obs = minimal_observation(&env);
    // One character away from the canonical LOINC URI.
    obs.code_system = String::from_str(&env, "http://loinc.org/");
    assert_eq!(
        client.try_validate_eye_exam_observation(&obs),
        Err(Ok(FhirError::InvalidPayload))
    );
}

#[test]
fn test_eye_exam_unrecognised_snomed_code_is_invalid_payload() {
    let (env, client) = setup();
    let mut obs = minimal_observation(&env);
    obs.code_system = String::from_str(&env, SNOMED_SYSTEM);
    // One digit away from SNOMED_VISUAL_ACUITY.
    obs.code_value = String::from_str(&env, "363983008");
    assert_eq!(
        client.try_validate_eye_exam_observation(&obs),
        Err(Ok(FhirError::InvalidPayload))
    );
}
