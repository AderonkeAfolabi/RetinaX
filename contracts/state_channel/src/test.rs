use super::*;
use crate::settlement::BatchRecordInput;
use soroban_sdk::{
    testutils::{Address as _, Ledger},
    Address, BytesN, Env, Vec,
};

#[soroban_sdk::contract]
pub struct MockVisionRecords;

#[soroban_sdk::contractimpl]
impl MockVisionRecords {
    pub fn add_records(
        _env: Env,
        _provider: Address,
        _records: Vec<BatchRecordInput>,
    ) -> Result<Vec<u64>, common::CommonError> {
        Ok(Vec::new(&_env))
    }
}

#[test]
fn test_initialize() {
    let env = Env::default();
    let admin = Address::generate(&env);
    let vision_records = Address::generate(&env);

    let contract_id = env.register(StateChannelContract, ());
    let client = StateChannelContractClient::new(&env, &contract_id);

    client.initialize(&admin, &vision_records);

    // Check double initialization fails
    let res = client.try_initialize(&admin, &vision_records);
    assert!(res.is_err());
}

#[test]
fn test_cooperative_lifecycle() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let vision_records_id = env.register(MockVisionRecords, ());
    let patient = Address::generate(&env);
    let provider = Address::generate(&env);

    let contract_id = env.register(StateChannelContract, ());
    let client = StateChannelContractClient::new(&env, &contract_id);

    client.initialize(&admin, &vision_records_id);

    // 1. Open
    let capacity = 1000;
    let channel_id = client.open_channel(&patient, &provider, &capacity);
    assert_eq!(channel_id, 1);

    // 2. Cooperative Close
    let balance = 400;
    let nonce = 5;
    let sig = BytesN::from_array(&env, &[0u8; 64]);

    client.cooperative_close(&channel_id, &balance, &nonce, &sig, &sig);

    // 3. Settle
    client.settle(&channel_id);
}

#[test]
fn test_dispute_lifecycle() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let vision_records_id = env.register(MockVisionRecords, ());
    let patient = Address::generate(&env);
    let provider = Address::generate(&env);

    let contract_id = env.register(StateChannelContract, ());
    let client = StateChannelContractClient::new(&env, &contract_id);

    client.initialize(&admin, &vision_records_id);

    let channel_id = client.open_channel(&patient, &provider, &1000);

    // 1. Unilateral Close by patient
    client.unilateral_close(&channel_id, &patient);

    // 2. Submit Fraud Proof (e.g. provider showing a later state)
    let sig = BytesN::from_array(&env, &[0u8; 64]);
    client.submit_fraud_proof(&channel_id, &10, &600, &sig);

    // 3. Try settle too early
    let res = client.try_settle(&channel_id);
    assert!(res.is_err());

    // 4. Advance time
    env.ledger().set_timestamp(env.ledger().timestamp() + 86401);

    // 5. Settle
    client.settle(&channel_id);
}

#[test]
fn test_rebalance() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let vision_records_id = env.register(MockVisionRecords, ());
    let patient = Address::generate(&env);
    let provider = Address::generate(&env);

    let contract_id = env.register(StateChannelContract, ());
    let client = StateChannelContractClient::new(&env, &contract_id);

    client.initialize(&admin, &vision_records_id);

    let channel_id = client.open_channel(&patient, &provider, &1000);

    let sig = BytesN::from_array(&env, &[0u8; 64]);
    client.rebalance(&channel_id, &2000, &sig, &sig);
}

#[test]
fn test_multi_hop() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let vision_records_id = env.register(MockVisionRecords, ());
    let patient = Address::generate(&env);
    let provider = Address::generate(&env);
    let intermediary = Address::generate(&env);

    let contract_id = env.register(StateChannelContract, ());
    let client = StateChannelContractClient::new(&env, &contract_id);

    client.initialize(&admin, &vision_records_id);

    let channel_id = client.open_multi_hop(&patient, &provider, &intermediary, &1000);
    assert_eq!(channel_id, 1);
}

#[test]
fn test_fraudulent_dispute_penalty_and_valid_evidence() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let vision_records = env.register(MockVisionRecords, ());
    let patient = Address::generate(&env);
    let provider = Address::generate(&env);

    let contract_id = env.register(StateChannelContract, ());
    let client = StateChannelContractClient::new(&env, &contract_id);
    client.initialize(&admin, &vision_records);

    let channel_id = client.open_channel(&patient, &provider, &1000);
    client.unilateral_close(&channel_id, &patient);
    let sig = BytesN::from_array(&env, &[0u8; 64]);

    assert_eq!(
        client.try_submit_fraud_proof(&channel_id, &0, &100, &sig),
        Err(Ok(common::CommonError::InvalidNonce))
    );
    client.submit_fraud_proof(&channel_id, &1, &600, &sig);
    env.ledger().set_timestamp(env.ledger().timestamp() + 86401);
    client.settle(&channel_id);
}

#[test]
fn test_settlement_rejected_before_timeout_and_succeeds_after_without_fraud_proof() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let vision_records = env.register(MockVisionRecords, ());
    let patient = Address::generate(&env);
    let provider = Address::generate(&env);

    let contract_id = env.register(StateChannelContract, ());
    let client = StateChannelContractClient::new(&env, &contract_id);
    client.initialize(&admin, &vision_records);

    // 1. Open channel with 3600s dispute timeout
    let dispute_window = 3_600;
    let channel_id = client.open_channel_with_timeout(&patient, &provider, &1000, &dispute_window);

    // 2. Unilateral close initiates forced settlement
    client.unilateral_close(&channel_id, &patient);

    // 3. Settlement before timeout expires MUST be rejected with ChallengePeriodActive
    env.ledger().set_timestamp(env.ledger().timestamp() + 1_800);
    assert_eq!(
        client.try_settle(&channel_id),
        Err(Ok(common::CommonError::ChallengePeriodActive))
    );

    // 4. Settlement after timeout expires (with no fraud proof) MUST succeed
    env.ledger().set_timestamp(env.ledger().timestamp() + 1_801);
    client.settle(&channel_id);

    // Settled channel cannot be settled again
    assert_eq!(
        client.try_settle(&channel_id),
        Err(Ok(common::CommonError::InvalidChannelState))
    );
}

#[test]
fn test_fraud_proof_inside_window_overrides_stale_state() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let vision_records = env.register(MockVisionRecords, ());
    let patient = Address::generate(&env);
    let provider = Address::generate(&env);

    let contract_id = env.register(StateChannelContract, ());
    let client = StateChannelContractClient::new(&env, &contract_id);
    client.initialize(&admin, &vision_records);

    let channel_id = client.open_channel_with_timeout(&patient, &provider, &1000, &3600);
    client.unilateral_close(&channel_id, &patient);

    // Inside dispute window: submit fraud proof with later state (nonce = 5, balance = 600)
    let sig = BytesN::from_array(&env, &[0u8; 64]);
    client.submit_fraud_proof(&channel_id, &5, &600, &sig);

    // Advance time past dispute window
    env.ledger().set_timestamp(env.ledger().timestamp() + 3601);

    // Finalize forced settlement with the updated state
    client.settle(&channel_id);
}

#[test]
fn test_fraud_proof_after_window_rejected() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let vision_records = env.register(MockVisionRecords, ());
    let patient = Address::generate(&env);
    let provider = Address::generate(&env);

    let contract_id = env.register(StateChannelContract, ());
    let client = StateChannelContractClient::new(&env, &contract_id);
    client.initialize(&admin, &vision_records);

    let channel_id = client.open_channel_with_timeout(&patient, &provider, &1000, &3600);
    client.unilateral_close(&channel_id, &patient);

    // Advance time past the dispute window
    env.ledger().set_timestamp(env.ledger().timestamp() + 3601);

    // Fraud proof submitted after window MUST be rejected
    let sig = BytesN::from_array(&env, &[0u8; 64]);
    assert_eq!(
        client.try_submit_fraud_proof(&channel_id, &2, &500, &sig),
        Err(Ok(common::CommonError::InvalidChannelState))
    );

    // Settle proceeds normally
    client.settle(&channel_id);
}

#[test]
fn test_funds_never_locked_forever_when_counterparty_goes_silent() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let vision_records = env.register(MockVisionRecords, ());
    let patient = Address::generate(&env);
    let unresponsive_provider = Address::generate(&env);

    let contract_id = env.register(StateChannelContract, ());
    let client = StateChannelContractClient::new(&env, &contract_id);
    client.initialize(&admin, &vision_records);

    // 1. Patient opens channel with unresponsive counterparty
    let channel_id = client.open_channel(&patient, &unresponsive_provider, &5000);

    // 2. Provider stalls indefinitely. Patient unilaterally closes to force settlement.
    client.unilateral_close(&channel_id, &patient);

    // 3. Counterparty never responds or submits proofs. Dispute window passes.
    let dispute_window = client.get_dispute_window();
    env.ledger().set_timestamp(env.ledger().timestamp() + dispute_window + 1);

    // 4. Patient invokes settle: channel closes without counterparty participation!
    client.settle(&channel_id);
}

#[test]
fn test_admin_configure_dispute_window() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let non_admin = Address::generate(&env);
    let vision_records = env.register(MockVisionRecords, ());

    let contract_id = env.register(StateChannelContract, ());
    let client = StateChannelContractClient::new(&env, &contract_id);
    client.initialize(&admin, &vision_records);

    assert_eq!(client.get_dispute_window(), 86_400);

    // Non-admin cannot set dispute window
    assert_eq!(
        client.try_set_dispute_window(&non_admin, &7_200),
        Err(Ok(common::CommonError::AccessDenied))
    );

    // Admin sets dispute window to 7200s
    client.set_dispute_window(&admin, &7_200);
    assert_eq!(client.get_dispute_window(), 7_200);
}
