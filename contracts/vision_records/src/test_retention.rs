//! Tests for data class retention policy and automatic expiry (Issue #447, coordinating with Issue #325).

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use super::{
    ContractError, RecordType, Role, VisionRecordsContract, VisionRecordsContractClient,
};
use soroban_sdk::{testutils::Address as _, testutils::Ledger as _, Address, Env, String, Vec};

const VALID_HASH: &str = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";

struct Ctx {
    env: Env,
    client: VisionRecordsContractClient<'static>,
    admin: Address,
    provider: Address,
    patient: Address,
}

fn setup() -> Ctx {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(VisionRecordsContract, ());
    let client = VisionRecordsContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    client.initialize(&admin);

    let provider = Address::generate(&env);
    client.register_user(
        &admin,
        &provider,
        &Role::Optometrist,
        &String::from_str(&env, "Dr. Retention"),
    );

    let patient = Address::generate(&env);
    client.register_user(
        &admin,
        &patient,
        &Role::Patient,
        &String::from_str(&env, "Patient Alice"),
    );

    Ctx {
        env,
        client,
        admin,
        provider,
        patient,
    }
}

fn add_record(ctx: &Ctx, record_type: RecordType) -> u64 {
    ctx.client.add_record(
        &ctx.provider,
        &ctx.patient,
        &ctx.provider,
        &record_type,
        &String::from_str(&ctx.env, VALID_HASH),
    )
}

#[test]
fn test_admin_configures_retention_period_per_data_class() {
    let ctx = setup();

    // Default retention period should be 0 (indefinite)
    assert_eq!(
        ctx.client.get_retention_period(&RecordType::Examination),
        0
    );
    assert_eq!(
        ctx.client.get_retention_period(&RecordType::Prescription),
        0
    );

    // Admin configures 30 days (2,592,000s) for Examination and 7 years (220,752,000s) for Surgery
    let exam_retention = 2_592_000u64;
    let surgery_retention = 220_752_000u64;

    ctx.client.set_retention_period(
        &ctx.admin,
        &RecordType::Examination,
        &exam_retention,
    );
    ctx.client.set_retention_period(
        &ctx.admin,
        &RecordType::Surgery,
        &surgery_retention,
    );

    assert_eq!(
        ctx.client.get_retention_period(&RecordType::Examination),
        exam_retention
    );
    assert_eq!(
        ctx.client.get_retention_period(&RecordType::Surgery),
        surgery_retention
    );
    // Unset type remains 0
    assert_eq!(
        ctx.client.get_retention_period(&RecordType::Prescription),
        0
    );

    // Setting retention period back to 0 resets it to indefinite
    ctx.client.set_retention_period(
        &ctx.admin,
        &RecordType::Examination,
        &0,
    );
    assert_eq!(
        ctx.client.get_retention_period(&RecordType::Examination),
        0
    );
}

#[test]
fn test_non_admin_cannot_set_retention_period() {
    let ctx = setup();
    let stranger = Address::generate(&ctx.env);

    let res = ctx.client.try_set_retention_period(
        &stranger,
        &RecordType::Examination,
        &86_400,
    );
    assert!(res.is_err());
    assert_eq!(res.unwrap_err(), Ok(ContractError::Unauthorized));

    let res_patient = ctx.client.try_set_retention_period(
        &ctx.patient,
        &RecordType::Examination,
        &86_400,
    );
    assert!(res_patient.is_err());
    assert_eq!(res_patient.unwrap_err(), Ok(ContractError::Unauthorized));
}

#[test]
fn test_record_creation_inherits_configured_retention_horizon() {
    let ctx = setup();
    let retention_seconds = 7 * 86_400u64; // 7 days

    ctx.client.set_retention_period(
        &ctx.admin,
        &RecordType::Examination,
        &retention_seconds,
    );

    let now = 1_000_000u64;
    ctx.env.ledger().set_timestamp(now);

    let exam_rec_id = add_record(&ctx, RecordType::Examination);
    let diag_rec_id = add_record(&ctx, RecordType::Diagnosis);

    let exam_record = ctx.client.get_record(&ctx.patient, &exam_rec_id);
    assert_eq!(exam_record.retain_until, Some(now + retention_seconds));

    // Diagnosis has no retention policy, so retain_until is None (indefinite)
    let diag_record = ctx.client.get_record(&ctx.patient, &diag_rec_id);
    assert_eq!(diag_record.retain_until, None);
}

#[test]
fn test_active_record_accessible_before_expiry() {
    let ctx = setup();
    let retention_seconds = 1000u64;

    ctx.client.set_retention_period(
        &ctx.admin,
        &RecordType::Examination,
        &retention_seconds,
    );

    let start_time = 10_000u64;
    ctx.env.ledger().set_timestamp(start_time);
    let rec_id = add_record(&ctx, RecordType::Examination);

    // Time progresses but is still within retention period
    ctx.env.ledger().set_timestamp(start_time + 500);

    // Patient and provider can read
    let rec_patient = ctx.client.get_record(&ctx.patient, &rec_id);
    assert_eq!(rec_patient.id, rec_id);

    let rec_provider = ctx.client.get_record(&ctx.provider, &rec_id);
    assert_eq!(rec_provider.id, rec_id);

    // Listed in patient records
    let patient_records = ctx.client.get_patient_records(&ctx.patient);
    assert!(patient_records.contains(&rec_id));
}

#[test]
fn test_record_becomes_inaccessible_once_expired() {
    let ctx = setup();
    let retention_seconds = 3600u64; // 1 hour

    ctx.client.set_retention_period(
        &ctx.admin,
        &RecordType::Prescription,
        &retention_seconds,
    );

    let start_time = 1_000_000u64;
    ctx.env.ledger().set_timestamp(start_time);
    let rec_id = add_record(&ctx, RecordType::Prescription);

    // Advance time past retain_until (e.g. 1 hour + 1 second)
    ctx.env.ledger().set_timestamp(start_time + retention_seconds + 1);

    // Reading record returns RecordExpired for patient
    let res_patient = ctx.client.try_get_record(&ctx.patient, &rec_id);
    assert!(res_patient.is_err());
    assert_eq!(res_patient.unwrap_err(), Ok(ContractError::RecordExpired));

    // Reading record returns RecordExpired for provider
    let res_provider = ctx.client.try_get_record(&ctx.provider, &rec_id);
    assert!(res_provider.is_err());
    assert_eq!(res_provider.unwrap_err(), Ok(ContractError::RecordExpired));

    // Reading record returns RecordExpired even for admin
    let res_admin = ctx.client.try_get_record(&ctx.admin, &rec_id);
    assert!(res_admin.is_err());
    assert_eq!(res_admin.unwrap_err(), Ok(ContractError::RecordExpired));

    // Automatically excluded from get_patient_records query
    let patient_records = ctx.client.get_patient_records(&ctx.patient);
    assert!(!patient_records.contains(&rec_id));

    // Automatically excluded from get_records_by_ids query
    let mut query_ids = Vec::new(&ctx.env);
    query_ids.push_back(rec_id);
    let records_by_ids = ctx.client.get_records_by_ids(&query_ids);
    assert_eq!(records_by_ids.len(), 0);
}

#[test]
fn test_purge_expired_record_lifecycle_and_cleanup() {
    let ctx = setup();
    let retention_seconds = 600u64;

    ctx.client.set_retention_period(
        &ctx.admin,
        &RecordType::LabResult,
        &retention_seconds,
    );

    let start_time = 50_000u64;
    ctx.env.ledger().set_timestamp(start_time);
    let rec_id = add_record(&ctx, RecordType::LabResult);

    // 1. Purge before expiry must fail (returns false) and preserve data
    let purged_early = ctx.client.purge_expired_record(&rec_id);
    assert!(!purged_early, "Record must not be purged before retention period ends");
    assert!(ctx.client.get_patient_records(&ctx.patient).contains(&rec_id));

    // 2. Advance time past expiry
    ctx.env.ledger().set_timestamp(start_time + retention_seconds + 10);

    // 3. Purge after expiry must succeed (returns true)
    let purged = ctx.client.purge_expired_record(&rec_id);
    assert!(purged, "Expired record must be successfully purged");

    // 4. Calling purge again returns false (already purged)
    let purged_again = ctx.client.purge_expired_record(&rec_id);
    assert!(!purged_again, "Purging already-purged record returns false");

    // 5. Record is now completely gone from storage (RecordNotFound, not RecordExpired)
    let res_after_purge = ctx.client.try_get_record(&ctx.patient, &rec_id);
    assert_eq!(
        res_after_purge.unwrap_err(),
        Ok(ContractError::RecordNotFound)
    );

    // Patient records list is updated
    assert!(!ctx.client.get_patient_records(&ctx.patient).contains(&rec_id));
}

#[test]
fn test_purge_expired_records_batch() {
    let ctx = setup();
    let retention_seconds = 500u64;

    ctx.client.set_retention_period(
        &ctx.admin,
        &RecordType::Examination,
        &retention_seconds,
    );

    let start_time = 100_000u64;
    ctx.env.ledger().set_timestamp(start_time);

    let rec1 = add_record(&ctx, RecordType::Examination);
    let rec2 = add_record(&ctx, RecordType::Examination);

    // Create an unexpiring record
    let rec_permanent = add_record(&ctx, RecordType::Diagnosis);

    // Advance time past expiration for Examination records
    ctx.env.ledger().set_timestamp(start_time + retention_seconds + 50);

    let mut batch_ids = Vec::new(&ctx.env);
    batch_ids.push_back(rec1);
    batch_ids.push_back(rec2);
    batch_ids.push_back(rec_permanent);

    let purged_count = ctx.client.purge_expired_records_batch(&batch_ids);
    assert_eq!(purged_count, 2);

    // Permanent record is still intact and accessible
    let permanent = ctx.client.get_record(&ctx.patient, &rec_permanent);
    assert_eq!(permanent.id, rec_permanent);

    let patient_records = ctx.client.get_patient_records(&ctx.patient);
    assert_eq!(patient_records.len(), 1);
    assert_eq!(patient_records.get(0).unwrap(), rec_permanent);
}
