//! Tests for the patient data portability export (issue #321).

#![allow(clippy::unwrap_used, clippy::arithmetic_side_effects)]

use super::{
    examination::{OptPhysicalMeasurement, PhysicalMeasurement},
    portability::{iso8601, PortabilityFormat},
    AccessLevel, ContractError, IntraocularPressure, OptFundusPhotography, OptRetinalImaging,
    OptVisualField, RecordType, Role, SlitLampFindings, VisionRecordsContract,
    VisionRecordsContractClient, VisualAcuity,
};
use soroban_sdk::{
    testutils::{Address as _, Ledger as _},
    Address, Bytes, Env, String,
};

const VALID_HASH: &str = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";

fn setup() -> (Env, VisionRecordsContractClient<'static>, Address) {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(VisionRecordsContract, ());
    let client = VisionRecordsContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    client.initialize(&admin);

    (env, client, admin)
}

fn register_user(
    env: &Env,
    client: &VisionRecordsContractClient,
    admin: &Address,
    role: Role,
    name: &str,
) -> Address {
    let user = Address::generate(env);
    client.register_user(admin, &user, &role, &String::from_str(env, name));
    user
}

/// Create an examination record for `patient` with a fully populated exam.
fn add_examination(
    env: &Env,
    client: &VisionRecordsContractClient,
    provider: &Address,
    patient: &Address,
) -> u64 {
    let record_id = client.add_record(
        provider,
        patient,
        provider,
        &RecordType::Examination,
        &String::from_str(env, VALID_HASH),
    );
    client.add_eye_examination(
        provider,
        &record_id,
        &VisualAcuity {
            uncorrected: PhysicalMeasurement {
                left_eye: String::from_str(env, "20/40"),
                right_eye: String::from_str(env, "20/25"),
            },
            corrected: OptPhysicalMeasurement::Some(PhysicalMeasurement {
                left_eye: String::from_str(env, "20/20"),
                right_eye: String::from_str(env, "20/20"),
            }),
        },
        &IntraocularPressure {
            left_eye: 16,
            right_eye: 22,
            method: String::from_str(env, "Goldmann"),
            timestamp: env.ledger().timestamp(),
        },
        &SlitLampFindings {
            cornea: String::from_str(env, "Clear"),
            anterior_chamber: String::from_str(env, "Deep and quiet"),
            iris: String::from_str(env, "Normal"),
            lens: String::from_str(env, "Clear"),
        },
        &OptVisualField::None,
        &OptRetinalImaging::None,
        &OptFundusPhotography::None,
        &String::from_str(env, "Follow-up in six months"),
    );
    record_id
}

fn bytes_to_text(bytes: &Bytes) -> alloc::string::String {
    let mut buf = alloc::vec![0u8; bytes.len() as usize];
    bytes.copy_into_slice(&mut buf);
    alloc::string::String::from_utf8(buf).unwrap()
}

fn soroban_to_text(value: &String) -> alloc::string::String {
    let mut buf = alloc::vec![0u8; value.len() as usize];
    value.copy_into_slice(&mut buf);
    alloc::string::String::from_utf8(buf).unwrap()
}

#[test]
fn fhir_export_by_patient_includes_exam_and_withholds_internal_ids() {
    let (env, client, admin) = setup();
    let patient = register_user(&env, &client, &admin, Role::Patient, "Patient");
    let provider = register_user(&env, &client, &admin, Role::Optometrist, "Provider");
    let record_id = add_examination(&env, &client, &provider, &patient);

    let bytes =
        client.export_patient_data(&patient, &patient, &PortabilityFormat::FhirJson, &0, &50);
    let text = bytes_to_text(&bytes);

    assert!(text.contains("\"resourceType\":\"Bundle\""));
    assert!(text.contains("\"resourceType\":\"Observation\""));
    assert!(text.contains("Intraocular pressure left eye"));
    assert!(text.contains("\"unit\":\"mm[Hg]\""));
    assert!(text.contains("Follow-up in six months"));

    // Internal identifiers must never appear in the export.
    assert!(!text.contains("\"id\""));
    assert!(!text.contains(&alloc::format!("\"record_id\":{}", record_id)));
    assert!(!text.contains(&soroban_to_text(&patient.to_string())));
    assert!(!text.contains(&soroban_to_text(&provider.to_string())));
}

#[test]
fn csv_export_includes_grants_from_the_patient_grantee_list() {
    let (env, client, admin) = setup();
    let patient = register_user(&env, &client, &admin, Role::Patient, "Patient");
    let provider = register_user(&env, &client, &admin, Role::Optometrist, "Provider");
    let grantee = Address::generate(&env);

    add_examination(&env, &client, &provider, &patient);
    client.grant_access(&patient, &patient, &grantee, &AccessLevel::Read, &3600);

    let bytes = client.export_patient_data(&patient, &patient, &PortabilityFormat::Csv, &0, &50);
    let text = bytes_to_text(&bytes);

    assert!(text.starts_with("section,record_type,date,field,value\n"));
    assert!(text.contains("record,Examination,"));
    assert!(text.contains("Intraocular pressure left eye,16 mmHg"));
    assert!(text.contains("access_grant,Consent,"));
    assert!(text.contains(",level,Read"));
    assert!(text.contains(&soroban_to_text(&grantee.to_string())));
}

#[test]
fn stranger_without_grant_is_denied() {
    let (env, client, admin) = setup();
    let patient = register_user(&env, &client, &admin, Role::Patient, "Patient");
    let stranger = Address::generate(&env);

    let result =
        client.try_export_patient_data(&stranger, &patient, &PortabilityFormat::FhirJson, &0, &50);
    assert_eq!(result, Err(Ok(ContractError::AccessDenied)));
}

#[test]
fn export_rejects_zero_page_size() {
    let (env, client, admin) = setup();
    let patient = register_user(&env, &client, &admin, Role::Patient, "Patient");

    let result =
        client.try_export_patient_data(&patient, &patient, &PortabilityFormat::Csv, &0, &0);
    assert_eq!(result, Err(Ok(ContractError::InvalidInput)));
}

#[test]
fn export_pages_records_by_offset_and_limit() {
    let (env, client, admin) = setup();
    let patient = register_user(&env, &client, &admin, Role::Patient, "Patient");
    let provider = register_user(&env, &client, &admin, Role::Optometrist, "Provider");
    add_examination(&env, &client, &provider, &patient);
    add_examination(&env, &client, &provider, &patient);
    add_examination(&env, &client, &provider, &patient);

    let first = client.export_patient_data(&patient, &patient, &PortabilityFormat::Csv, &0, &2);
    let second = client.export_patient_data(&patient, &patient, &PortabilityFormat::Csv, &2, &2);

    // Each examination contributes exactly one IOP left-eye row.
    let iop_row = "Intraocular pressure left eye";
    assert_eq!(bytes_to_text(&first).matches(iop_row).count(), 2);
    assert_eq!(bytes_to_text(&second).matches(iop_row).count(), 1);
}

#[test]
fn grantee_with_active_grant_can_export_and_grant_reads_as_active() {
    let (env, client, admin) = setup();
    let patient = register_user(&env, &client, &admin, Role::Patient, "Patient");
    let grantee = Address::generate(&env);

    client.grant_access(&patient, &patient, &grantee, &AccessLevel::Read, &3600);

    let bytes =
        client.export_patient_data(&grantee, &patient, &PortabilityFormat::FhirJson, &0, &50);
    let text = bytes_to_text(&bytes);
    assert!(text.contains("\"resourceType\":\"Consent\""));
    assert!(text.contains("\"status\":\"active\""));
}

#[test]
fn expired_grant_is_reported_as_inactive_consent() {
    let (env, client, admin) = setup();
    let patient = register_user(&env, &client, &admin, Role::Patient, "Patient");
    let grantee = Address::generate(&env);

    client.grant_access(&patient, &patient, &grantee, &AccessLevel::Read, &60);

    // The patient still exports after expiry; the consent is marked inactive.
    env.ledger().with_mut(|li| li.timestamp += 3600);

    let bytes =
        client.export_patient_data(&patient, &patient, &PortabilityFormat::FhirJson, &0, &50);
    let text = bytes_to_text(&bytes);
    assert!(text.contains("\"resourceType\":\"Consent\""));
    assert!(text.contains("\"status\":\"inactive\""));
}

#[test]
fn cursor_export_pages_over_one_hundred_and_twenty_records() {
    let (env, client, admin) = setup();
    let patient = register_user(&env, &client, &admin, Role::Patient, "Patient");
    let provider = register_user(&env, &client, &admin, Role::Optometrist, "Provider");
    for _ in 0..120 {
        add_examination(&env, &client, &provider, &patient);
    }

    let iop_row = "Intraocular pressure left eye";
    let mut cursor = Some(0u32);
    let mut pages = 0;
    let mut rows = 0;
    while let Some(at) = cursor {
        let page =
            client.export_patient_data_page(&patient, &patient, &PortabilityFormat::Csv, &at, &50);
        assert_eq!(page.total, 120);
        rows += bytes_to_text(&page.data).matches(iop_row).count();
        pages += 1;
        cursor = page.next_cursor;
    }

    assert_eq!(pages, 3);
    assert_eq!(rows, 120);
}

#[test]
fn cursor_export_pages_grants_past_the_old_fifty_entry_limit() {
    let (env, client, admin) = setup();
    let patient = register_user(&env, &client, &admin, Role::Patient, "Patient");
    let provider = register_user(&env, &client, &admin, Role::Optometrist, "Provider");
    add_examination(&env, &client, &provider, &patient);
    let grantees: alloc::vec::Vec<Address> = (0..60).map(|_| Address::generate(&env)).collect();
    for grantee in &grantees {
        client.grant_access(&patient, &patient, grantee, &AccessLevel::Read, &3600);
    }

    // The offset-based export refuses a grantee list longer than one page.
    let refused =
        client.try_export_patient_data(&patient, &patient, &PortabilityFormat::Csv, &0, &50);
    assert_eq!(refused, Err(Ok(ContractError::InvalidInput)));

    // One record, then 60 grants: the first page holds the record and grants 0..=48.
    let first =
        client.export_patient_data_page(&patient, &patient, &PortabilityFormat::Csv, &0, &50);
    let second =
        client.export_patient_data_page(&patient, &patient, &PortabilityFormat::Csv, &50, &50);
    assert_eq!(first.total, 61);
    assert_eq!(first.next_cursor, Some(50));
    assert_eq!(second.next_cursor, None);

    let first_text = bytes_to_text(&first.data);
    let second_text = bytes_to_text(&second.data);
    for (index, grantee) in grantees.iter().enumerate() {
        let address = soroban_to_text(&grantee.to_string());
        if index < 49 {
            assert!(first_text.contains(&address));
            assert!(!second_text.contains(&address));
        } else {
            assert!(second_text.contains(&address));
            assert!(!first_text.contains(&address));
        }
    }
}

#[test]
fn cursor_past_the_end_is_rejected_and_zero_limit_is_refused() {
    let (env, client, admin) = setup();
    let patient = register_user(&env, &client, &admin, Role::Patient, "Patient");
    let provider = register_user(&env, &client, &admin, Role::Optometrist, "Provider");
    add_examination(&env, &client, &provider, &patient);

    let past_end =
        client.try_export_patient_data_page(&patient, &patient, &PortabilityFormat::Csv, &2, &50);
    assert_eq!(past_end, Err(Ok(ContractError::InvalidInput)));

    let zero_limit =
        client.try_export_patient_data_page(&patient, &patient, &PortabilityFormat::Csv, &0, &0);
    assert_eq!(zero_limit, Err(Ok(ContractError::InvalidInput)));

    let at_end =
        client.export_patient_data_page(&patient, &patient, &PortabilityFormat::Csv, &1, &50);
    assert_eq!(at_end.next_cursor, None);
    assert_eq!(at_end.total, 1);
}

#[test]
fn iso8601_formats_known_timestamps() {
    assert_eq!(iso8601(0), "1970-01-01T00:00:00Z");
    assert_eq!(iso8601(951_782_400), "2000-02-29T00:00:00Z");
    assert_eq!(iso8601(1_704_067_200), "2024-01-01T00:00:00Z");
}
