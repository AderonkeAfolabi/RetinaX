//! Issue #423 — state channel signature validation coverage.
//!
//! Updates to a channel must be signed by both parties. The tests in this
//! module exercise the signature-bearing entry points and the paths that
//! protect state transitions: missing signer auth, a wrong party attempting a
//! close, replayed state, and strictly increasing nonces.

use super::*;
use crate::settlement::BatchRecordInput;
use soroban_sdk::{testutils::Address as _, Address, BytesN, Env, Vec};

/// Mock of the `vision_records` contract used by `settle`.
#[soroban_sdk::contract]
pub struct SignatureTestVisionRecords;

#[soroban_sdk::contractimpl]
impl SignatureTestVisionRecords {
    pub fn add_records(
        _env: Env,
        _provider: Address,
        _records: Vec<BatchRecordInput>,
    ) -> Result<Vec<u64>, common::CommonError> {
        Ok(Vec::new(&_env))
    }
}

/// Register a fresh contract and open a 1_000-capacity channel between two
/// parties. Returns `(client, patient, provider, channel_id)`.
fn setup_channel<'a>(env: &Env) -> (StateChannelContractClient<'a>, Address, Address, u64) {
    env.mock_all_auths();

    let admin = Address::generate(env);
    let vision_records = env.register(SignatureTestVisionRecords, ());
    let patient = Address::generate(env);
    let provider = Address::generate(env);

    let contract_id = env.register(StateChannelContract, ());
    let client = StateChannelContractClient::new(env, &contract_id);
    client.initialize(&admin, &vision_records);

    let channel_id = client.open_channel(&patient, &provider, &1_000);
    (client, patient, provider, channel_id)
}

/// A party that supplies no signature/auth must not be able to close a
/// unilaterally-disputed channel. `require_auth` is the on-chain signature
/// check for the closer address.
#[test]
fn test_missing_signature_cannot_close_channel() {
    let env = Env::default();
    let (client, patient, _provider, channel_id) = setup_channel(&env);

    // Drop the mocked authorization so the call arrives unsigned.
    env.set_auths(&[]);
    let result = client.try_unilateral_close(&channel_id, &patient);
    assert!(
        result.is_err(),
        "close without the closer's signature/auth must fail"
    );
}

/// Only a channel participant may close it. A signature from an unrelated
/// party is not a valid party signature.
#[test]
fn test_wrong_party_signature_cannot_close_channel() {
    let env = Env::default();
    let (client, _patient, _provider, channel_id) = setup_channel(&env);
    let outsider = Address::generate(&env);

    // Auth is mocked for the outsider (a syntactically valid signature), but
    // they are not one of the two parties that signed the channel open.
    assert_eq!(
        client.try_unilateral_close(&channel_id, &outsider),
        Err(Ok(CommonError::AccessDenied))
    );
}

/// A previously accepted state must not be accepted again. The fraud-proof
/// path enforces `nonce > channel.nonce` and charges a penalty on replay.
#[test]
fn test_replayed_state_signature_is_rejected_and_penalized() {
    let env = Env::default();
    let (client, patient, _provider, channel_id) = setup_channel(&env);
    client.unilateral_close(&channel_id, &patient);

    let sig = BytesN::from_array(&env, &[7u8; 64]);
    client.submit_fraud_proof(&channel_id, &5, &600, &sig);

    // Replay the exact same signed state.
    assert_eq!(
        client.try_submit_fraud_proof(&channel_id, &5, &600, &sig),
        Err(Ok(CommonError::InvalidNonce))
    );
    assert_eq!(client.get_fraud_penalty(&channel_id), 100); // 10% of 1_000
}

/// Nonces for channel updates must be strictly monotonic: equal, stale and
/// replayed values are all rejected, while a higher value is accepted.
#[test]
fn test_state_update_nonce_monotonicity() {
    let env = Env::default();
    let (client, patient, _provider, channel_id) = setup_channel(&env);
    client.unilateral_close(&channel_id, &patient);

    let sig = BytesN::from_array(&env, &[9u8; 64]);

    // The channel starts at nonce 0, so nonce 0 is stale.
    assert_eq!(
        client.try_submit_fraud_proof(&channel_id, &0, &100, &sig),
        Err(Ok(CommonError::InvalidNonce))
    );

    client.submit_fraud_proof(&channel_id, &1, &100, &sig);

    // Equal and older nonces are rejected as replays.
    assert_eq!(
        client.try_submit_fraud_proof(&channel_id, &1, &150, &sig),
        Err(Ok(CommonError::InvalidNonce))
    );
    assert_eq!(
        client.try_submit_fraud_proof(&channel_id, &0, &150, &sig),
        Err(Ok(CommonError::InvalidNonce))
    );

    // A newer nonce is accepted.
    client.submit_fraud_proof(&channel_id, &2, &200, &sig);
}

/// The signature parameters of `cooperative_close` are currently not
/// verified on-chain (the verification calls are TODOs in `channel.rs`).
/// This test keeps that path covered and makes the gap explicit: unsigned
/// zeroed signatures still settle the channel today. Once both-party
/// signature verification is implemented, this assertion must be flipped to
/// `InvalidSignature` (see `CommonError::InvalidSignature`).
#[test]
fn test_cooperative_close_signature_parameters_are_not_verified_yet() {
    let env = Env::default();
    let (client, _patient, _provider, channel_id) = setup_channel(&env);

    let missing = BytesN::from_array(&env, &[0u8; 64]);
    assert!(client
        .try_cooperative_close(&channel_id, &400, &1, &missing, &missing)
        .is_ok());

    // A stale nonce is likewise accepted on this path today, unlike the
    // fraud-proof path covered above where monotonicity is enforced. Use a
    // separate channel because a settled channel cannot be closed twice.
    let patient_2 = Address::generate(&env);
    let provider_2 = Address::generate(&env);
    let channel_2 = client.open_channel(&patient_2, &provider_2, &1_000);
    assert!(client
        .try_cooperative_close(&channel_2, &100, &9, &missing, &missing)
        .is_ok());

    let patient_3 = Address::generate(&env);
    let provider_3 = Address::generate(&env);
    let channel_3 = client.open_channel(&patient_3, &provider_3, &1_000);
    assert!(client
        .try_cooperative_close(&channel_3, &100, &1, &missing, &missing)
        .is_ok());
}
