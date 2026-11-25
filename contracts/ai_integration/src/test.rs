#![allow(clippy::unwrap_used)]

extern crate std;

use soroban_sdk::{testutils::Address as _, Address, Env, String};

use crate::{
    AiIntegrationContract, AiIntegrationContractClient, AiIntegrationError, ProviderStatus,
    RequestStatus, VerificationState,
};

fn setup() -> (Env, AiIntegrationContractClient<'static>, Address, Address) {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(AiIntegrationContract, ());
    let client = AiIntegrationContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let operator = Address::generate(&env);

    client.initialize(&admin, &7_000);

    (env, client, admin, operator)
}

#[test]
fn test_provider_registration_and_status_management() {
    let (env, client, admin, operator) = setup();

    client.register_provider(
        &admin,
        &1,
        &operator,
        &String::from_str(&env, "Provider A"),
        &String::from_str(&env, "retina-v1"),
        &String::from_str(&env, "sha256:endpoint"),
    );

    let provider = client.get_provider(&1);
    assert_eq!(provider.provider_id, 1);
    assert_eq!(provider.status, ProviderStatus::Active);

    client.set_provider_status(&admin, &1, &ProviderStatus::Paused);
    let paused = client.get_provider(&1);
    assert_eq!(paused.status, ProviderStatus::Paused);
}

#[test]
fn test_request_result_and_verification_flow() {
    let (env, client, admin, operator) = setup();
    let requester = Address::generate(&env);
    let patient = Address::generate(&env);

    client.register_provider(
        &admin,
        &42,
        &operator,
        &String::from_str(&env, "Provider B"),
        &String::from_str(&env, "fundus-v3"),
        &String::from_str(&env, "sha256:provider-b"),
    );

    let request_id = client.submit_analysis_request(
        &requester,
        &42,
        &patient,
        &9001,
        &String::from_str(&env, "sha256:image-1"),
        &String::from_str(&env, "retina_screening"),
    );

    let status = client.store_analysis_result(
        &operator,
        &request_id,
        &String::from_str(&env, "sha256:result-1"),
        &8_800,
        &3_500,
    );
    assert_eq!(status, RequestStatus::Completed);

    client.verify_analysis_result(
        &admin,
        &request_id,
        &true,
        &String::from_str(&env, "sha256:qa-1"),
    );

    let result = client.get_analysis_result(&request_id);
    assert_eq!(result.verification_state, VerificationState::Verified);
    assert!(result.verified_by.is_some());

    let request = client.get_analysis_request(&request_id);
    assert_eq!(request.status, RequestStatus::Completed);
}

#[test]
fn test_anomaly_detection_flags_request() {
    let (env, client, admin, operator) = setup();
    let requester = Address::generate(&env);
    let patient = Address::generate(&env);

    client.register_provider(
        &admin,
        &7,
        &operator,
        &String::from_str(&env, "Provider C"),
        &String::from_str(&env, "oct-v2"),
        &String::from_str(&env, "sha256:provider-c"),
    );

    let request_id = client.submit_analysis_request(
        &requester,
        &7,
        &patient,
        &100,
        &String::from_str(&env, "sha256:image-2"),
        &String::from_str(&env, "macula_scan"),
    );

    let status = client.store_analysis_result(
        &operator,
        &request_id,
        &String::from_str(&env, "sha256:result-2"),
        &6_200,
        &9_300,
    );
    assert_eq!(status, RequestStatus::Flagged);

    let flagged = client.get_flagged_requests();
    assert_eq!(flagged.len(), 1);
    assert_eq!(flagged.get(0).unwrap(), request_id);

    client.verify_analysis_result(
        &admin,
        &request_id,
        &false,
        &String::from_str(&env, "sha256:qa-reject"),
    );

    let result = client.get_analysis_result(&request_id);
    assert_eq!(result.verification_state, VerificationState::Rejected);

    let request = client.get_analysis_request(&request_id);
    assert_eq!(request.status, RequestStatus::Rejected);
}

#[test]
fn test_register_provider_requires_admin() {
    let (env, client, _admin, operator) = setup();
    let unauthorized = Address::generate(&env);

    let result = client.try_register_provider(
        &unauthorized,
        &3,
        &operator,
        &String::from_str(&env, "Bad Actor"),
        &String::from_str(&env, "model-x"),
        &String::from_str(&env, "sha256:bad"),
    );

    assert!(result.is_err());
}

// ── #330: Provider scoring and verification tests ────────────────────────────

#[test]
fn test_provider_registration_requires_admin() {
    let (env, client, _admin, operator) = setup();
    let non_admin = Address::generate(&env);

    let result = client.try_register_provider(
        &non_admin,
        &10,
        &operator,
        &String::from_str(&env, "Rogue Provider"),
        &String::from_str(&env, "model-x"),
        &String::from_str(&env, "sha256:rogue"),
    );

    assert!(result.is_err(), "non-admin must not register a provider");
}

#[test]
fn test_provider_duplicate_registration_rejected() {
    let (env, client, admin, operator) = setup();

    client.register_provider(
        &admin,
        &1,
        &operator,
        &String::from_str(&env, "Provider A"),
        &String::from_str(&env, "retina-v1"),
        &String::from_str(&env, "sha256:a"),
    );

    let result = client.try_register_provider(
        &admin,
        &1,
        &operator,
        &String::from_str(&env, "Provider A Duplicate"),
        &String::from_str(&env, "retina-v1"),
        &String::from_str(&env, "sha256:a-dup"),
    );

    assert!(result.is_err(), "duplicate provider_id must be rejected");
}

#[test]
fn test_provider_zero_id_rejected() {
    let (env, client, admin, operator) = setup();

    let result = client.try_register_provider(
        &admin,
        &0,
        &operator,
        &String::from_str(&env, "Bad Provider"),
        &String::from_str(&env, "model"),
        &String::from_str(&env, "sha256:bad"),
    );

    assert!(result.is_err(), "provider_id 0 must be rejected as invalid");
}

#[test]
fn test_inactive_provider_cannot_store_result() {
    let (env, client, admin, operator) = setup();
    let requester = Address::generate(&env);
    let patient = Address::generate(&env);

    client.register_provider(
        &admin,
        &5,
        &operator,
        &String::from_str(&env, "Provider E"),
        &String::from_str(&env, "model-e"),
        &String::from_str(&env, "sha256:e"),
    );

    // Pause the provider
    client.set_provider_status(&admin, &5, &ProviderStatus::Paused);

    // Submit a request while provider was still active (before pause)
    // — we simulate by re-activating, submitting, then pausing again
    client.set_provider_status(&admin, &5, &ProviderStatus::Active);
    let request_id = client.submit_analysis_request(
        &requester,
        &5,
        &patient,
        &1,
        &String::from_str(&env, "sha256:img"),
        &String::from_str(&env, "task"),
    );
    client.set_provider_status(&admin, &5, &ProviderStatus::Paused);

    // Storing result with paused provider must fail
    let result = client.try_store_analysis_result(
        &operator,
        &request_id,
        &String::from_str(&env, "sha256:out"),
        &8_000,
        &1_000,
    );

    assert!(result.is_err(), "paused provider must not store a result");
}

#[test]
fn test_wrong_operator_cannot_store_result() {
    let (env, client, admin, operator) = setup();
    let requester = Address::generate(&env);
    let patient = Address::generate(&env);
    let impostor = Address::generate(&env);

    client.register_provider(
        &admin,
        &6,
        &operator,
        &String::from_str(&env, "Provider F"),
        &String::from_str(&env, "model-f"),
        &String::from_str(&env, "sha256:f"),
    );

    let request_id = client.submit_analysis_request(
        &requester,
        &6,
        &patient,
        &2,
        &String::from_str(&env, "sha256:img2"),
        &String::from_str(&env, "task2"),
    );

    // Impostor (not the registered operator) tries to store result
    let result = client.try_store_analysis_result(
        &impostor,
        &request_id,
        &String::from_str(&env, "sha256:out2"),
        &7_500,
        &500,
    );

    assert!(result.is_err(), "non-operator must not store a result");
}

#[test]
fn test_provider_score_calculation_via_confidence() {
    // The contract stores confidence_bps on the result. This test verifies
    // that high-confidence results are marked Completed and low-flagged ones
    // are marked Flagged, confirming the scoring threshold logic.
    let (env, client, admin, operator) = setup();
    let requester = Address::generate(&env);
    let patient = Address::generate(&env);

    client.register_provider(
        &admin,
        &20,
        &operator,
        &String::from_str(&env, "Scorer"),
        &String::from_str(&env, "score-v1"),
        &String::from_str(&env, "sha256:scorer"),
    );

    // High confidence, low anomaly → Completed
    let r1 = client.submit_analysis_request(
        &requester,
        &20,
        &patient,
        &10,
        &String::from_str(&env, "sha256:hi-conf"),
        &String::from_str(&env, "screening"),
    );
    let status1 = client.store_analysis_result(
        &operator,
        &r1,
        &String::from_str(&env, "sha256:res-hi"),
        &9_500, // confidence 95 %
        &2_000, // anomaly 20 % — below 70 % threshold
    );
    assert_eq!(status1, RequestStatus::Completed);

    let result1 = client.get_analysis_result(&r1);
    assert_eq!(result1.confidence_bps, 9_500);
    assert_eq!(result1.anomaly_score_bps, 2_000);
    assert_eq!(result1.verification_state, VerificationState::Unverified);

    // Low confidence, high anomaly → Flagged
    let r2 = client.submit_analysis_request(
        &requester,
        &20,
        &patient,
        &11,
        &String::from_str(&env, "sha256:lo-conf"),
        &String::from_str(&env, "macula"),
    );
    let status2 = client.store_analysis_result(
        &operator,
        &r2,
        &String::from_str(&env, "sha256:res-lo"),
        &4_000, // confidence 40 %
        &8_500, // anomaly 85 % — above 70 % threshold
    );
    assert_eq!(status2, RequestStatus::Flagged);

    let flagged = client.get_flagged_requests();
    assert!(
        flagged.contains(&r2),
        "high-anomaly request must appear in flagged list"
    );
}

#[test]
fn test_anomaly_threshold_boundary() {
    // Verifies exact boundary: anomaly == threshold → Flagged;
    // anomaly == threshold - 1 → Completed.
    let (env, client, admin, operator) = setup();
    let requester = Address::generate(&env);
    let patient = Address::generate(&env);

    // Default threshold is 7_000 bps (70 %) set in setup()
    client.register_provider(
        &admin,
        &30,
        &operator,
        &String::from_str(&env, "Boundary Tester"),
        &String::from_str(&env, "bnd-v1"),
        &String::from_str(&env, "sha256:bnd"),
    );

    // Exactly at threshold → Flagged
    let r_at = client.submit_analysis_request(
        &requester,
        &30,
        &patient,
        &20,
        &String::from_str(&env, "sha256:at"),
        &String::from_str(&env, "task"),
    );
    let status_at = client.store_analysis_result(
        &operator,
        &r_at,
        &String::from_str(&env, "sha256:out-at"),
        &5_000,
        &7_000, // anomaly == threshold
    );
    assert_eq!(status_at, RequestStatus::Flagged);

    // One below threshold → Completed
    let r_below = client.submit_analysis_request(
        &requester,
        &30,
        &patient,
        &21,
        &String::from_str(&env, "sha256:below"),
        &String::from_str(&env, "task"),
    );
    let status_below = client.store_analysis_result(
        &operator,
        &r_below,
        &String::from_str(&env, "sha256:out-below"),
        &5_000,
        &6_999, // one below threshold
    );
    assert_eq!(status_below, RequestStatus::Completed);
}

#[test]
fn test_verified_result_cannot_be_overwritten() {
    // Once a result is stored, calling store_analysis_result again on the
    // same request_id must return ResultAlreadyExists.
    let (env, client, admin, operator) = setup();
    let requester = Address::generate(&env);
    let patient = Address::generate(&env);

    client.register_provider(
        &admin,
        &40,
        &operator,
        &String::from_str(&env, "Idempotent"),
        &String::from_str(&env, "idem-v1"),
        &String::from_str(&env, "sha256:idem"),
    );

    let r = client.submit_analysis_request(
        &requester,
        &40,
        &patient,
        &30,
        &String::from_str(&env, "sha256:once"),
        &String::from_str(&env, "task"),
    );
    client.store_analysis_result(
        &operator,
        &r,
        &String::from_str(&env, "sha256:first"),
        &8_000,
        &1_000,
    );

    // Second attempt must fail
    let result = client.try_store_analysis_result(
        &operator,
        &r,
        &String::from_str(&env, "sha256:second"),
        &8_000,
        &1_000,
    );
    assert!(result.is_err(), "duplicate result store must be rejected");
}

#[test]
fn test_non_admin_cannot_verify_result() {
    let (env, client, admin, operator) = setup();
    let requester = Address::generate(&env);
    let patient = Address::generate(&env);
    let non_admin = Address::generate(&env);

    client.register_provider(
        &admin,
        &50,
        &operator,
        &String::from_str(&env, "AuthChecker"),
        &String::from_str(&env, "auth-v1"),
        &String::from_str(&env, "sha256:auth"),
    );

    let r = client.submit_analysis_request(
        &requester,
        &50,
        &patient,
        &40,
        &String::from_str(&env, "sha256:img-auth"),
        &String::from_str(&env, "task"),
    );
    client.store_analysis_result(
        &operator,
        &r,
        &String::from_str(&env, "sha256:out-auth"),
        &8_000,
        &1_000,
    );

    let result = client.try_verify_analysis_result(
        &non_admin,
        &r,
        &true,
        &String::from_str(&env, "sha256:qa-nonauth"),
    );
    assert!(result.is_err(), "non-admin must not verify a result");
}

#[test]
fn test_set_anomaly_threshold_requires_admin() {
    let (env, client, _admin, _operator) = setup();
    let non_admin = Address::generate(&env);

    let result = client.try_set_anomaly_threshold(&non_admin, &5_000);
    assert!(
        result.is_err(),
        "non-admin must not change anomaly threshold"
    );
}

#[test]
fn test_anomaly_threshold_above_max_bps_rejected() {
    let (env, client, admin, _operator) = setup();

    // 10_001 exceeds MAX_BPS (10_000)
    let result = client.try_set_anomaly_threshold(&admin, &10_001);
    assert!(result.is_err(), "threshold above MAX_BPS must be rejected");
}

// ── Model version pinning (#465) ───────────────────────────────────────────────

fn register_retina_provider(
    env: &Env,
    client: &AiIntegrationContractClient,
    admin: &Address,
    operator: &Address,
) {
    client.register_provider(
        admin,
        &1,
        operator,
        &String::from_str(env, "Provider M"),
        &String::from_str(env, "retina-v1"),
        &String::from_str(env, "sha256:endpoint"),
    );
}

#[test]
fn pinned_request_is_accepted_and_records_the_version_on_its_result() {
    let (env, client, admin, operator) = setup();
    register_retina_provider(&env, &client, &admin, &operator);
    let requester = Address::generate(&env);
    let patient = Address::generate(&env);

    assert_eq!(client.get_provider(&1).model_version, 1);
    let request_id = client.submit_pinned_analysis_request(
        &requester,
        &1,
        &patient,
        &7,
        &String::from_str(&env, "sha256:input"),
        &String::from_str(&env, "fundus"),
        &1,
    );

    let request = client.get_analysis_request(&request_id);
    assert_eq!(request.model_version, 1);
    assert_eq!(request.pinned_model_version, Some(1));

    client.store_analysis_result(
        &operator,
        &request_id,
        &String::from_str(&env, "sha256:output"),
        &9_000,
        &100,
    );
    assert_eq!(client.get_analysis_result(&request_id).model_version, 1);
}

#[test]
fn pinned_request_for_another_version_is_refused_until_the_provider_moves_to_it() {
    let (env, client, admin, operator) = setup();
    register_retina_provider(&env, &client, &admin, &operator);
    let requester = Address::generate(&env);
    let patient = Address::generate(&env);
    let input = String::from_str(&env, "sha256:input");
    let task = String::from_str(&env, "fundus");

    let stale =
        client.try_submit_pinned_analysis_request(&requester, &1, &patient, &7, &input, &task, &2);
    assert_eq!(stale, Err(Ok(AiIntegrationError::ModelVersionMismatch)));

    client.set_model_version(&admin, &1, &2);
    assert_eq!(client.get_provider(&1).model_version, 2);
    let request_id =
        client.submit_pinned_analysis_request(&requester, &1, &patient, &7, &input, &task, &2);
    assert_eq!(client.get_analysis_request(&request_id).model_version, 2);

    // The version a request was made against stays on it after the provider moves on.
    client.set_model_version(&admin, &1, &3);
    assert_eq!(client.get_analysis_request(&request_id).model_version, 2);
}

#[test]
fn unpinned_requests_still_work_and_record_the_current_version() {
    let (env, client, admin, operator) = setup();
    register_retina_provider(&env, &client, &admin, &operator);
    client.set_model_version(&admin, &1, &5);
    let requester = Address::generate(&env);
    let patient = Address::generate(&env);

    let request_id = client.submit_analysis_request(
        &requester,
        &1,
        &patient,
        &7,
        &String::from_str(&env, "sha256:input"),
        &String::from_str(&env, "fundus"),
    );

    let request = client.get_analysis_request(&request_id);
    assert_eq!(request.model_version, 5);
    assert_eq!(request.pinned_model_version, None);
}

#[test]
fn model_version_changes_require_the_admin_and_a_positive_version() {
    let (env, client, admin, operator) = setup();
    register_retina_provider(&env, &client, &admin, &operator);

    let zero = client.try_set_model_version(&admin, &1, &0);
    assert_eq!(zero, Err(Ok(AiIntegrationError::InvalidInput)));
    let missing = client.try_set_model_version(&admin, &99, &2);
    assert_eq!(missing, Err(Ok(AiIntegrationError::ProviderNotFound)));
    let stranger = Address::generate(&env);
    let not_admin = client.try_set_model_version(&stranger, &1, &2);
    assert_eq!(not_admin, Err(Ok(AiIntegrationError::Unauthorized)));
    assert_eq!(client.get_provider(&1).model_version, 1);
}
