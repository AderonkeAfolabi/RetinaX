use super::{DelegationContract, DelegationContractClient};
use soroban_sdk::{testutils::Address as _, Address, Bytes, BytesN, Env};

fn setup() -> (Env, DelegationContractClient<'static>) {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(DelegationContract, ());
    let client = DelegationContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    client.initialize(&admin);

    (env, client)
}

fn zero_hash(env: &Env) -> BytesN<32> {
    BytesN::from_array(env, &[0; 32])
}

fn proof_for(env: &Env, input: &BytesN<32>, result: &BytesN<32>) -> BytesN<32> {
    let mut payload = [0u8; 64];
    payload[..32].copy_from_slice(&input.to_array());
    payload[32..].copy_from_slice(&result.to_array());
    let proof = env.crypto().sha256(&Bytes::from_slice(env, &payload));
    BytesN::from_array(env, &proof.to_array())
}

#[test]
fn test_submit_result_on_completed_task_fails() {
    let (env, client) = setup();
    let creator = Address::generate(&env);
    let executor = Address::generate(&env);
    let input = zero_hash(&env);
    let result = zero_hash(&env);
    let proof = proof_for(&env, &input, &result);

    let task_id = client.submit_task(&creator, &input, &0, &0);
    client.register_executor(&executor);
    client.assign_task(&executor, &task_id);

    // Initial submission succeeds
    client.submit_result(&executor, &task_id, &result, &proof);

    // Second submission should fail because task is no longer in Assigned state (it's Completed)
    let result2 = client.try_submit_result(&executor, &task_id, &result, &proof);
    assert!(result2.is_err());
}

#[test]
fn test_executor_reputation_consistency() {
    let (env, client) = setup();
    let creator = Address::generate(&env);
    let executor = Address::generate(&env);
    let input = zero_hash(&env);
    let result = zero_hash(&env);
    let proof = proof_for(&env, &input, &result);

    let task_id = client.submit_task(&creator, &input, &0, &0);
    client.register_executor(&executor);
    client.assign_task(&executor, &task_id);

    client.submit_result(&executor, &task_id, &result, &proof);

    let info = client.get_executor_info(&executor).unwrap();
    assert_eq!(info.reputation, 101);
    assert_eq!(info.tasks_completed, 1);
}

#[test]
fn test_forged_result_rejected_and_emits_event() {
    extern crate std;
    let (env, client) = setup();
    let creator = Address::generate(&env);
    let executor = Address::generate(&env);
    let input = zero_hash(&env);
    let forged_result = BytesN::from_array(&env, &[42u8; 32]);
    let fake_proof = BytesN::from_array(&env, &[99u8; 32]); // Invalid proof

    let task_id = client.submit_task(&creator, &input, &1, &1000);
    client.register_executor(&executor);
    client.assign_task(&executor, &task_id);

    // Initial reputation is 100
    let info_before = client.get_executor_info(&executor).unwrap();
    assert_eq!(info_before.reputation, 100);

    // Submit forged result -> Verification fails
    client.submit_result(&executor, &task_id, &forged_result, &fake_proof);

    // Task must transition to Failed, not Completed
    let task = client.get_task(&task_id).unwrap();
    assert_eq!(task.status, crate::task_queue::TaskStatus::Failed);

    // Executor must be penalized (slashed by 10)
    let info_after = client.get_executor_info(&executor).unwrap();
    assert_eq!(info_after.reputation, 90);

    // Verify rejection event TSK_REJ was published
    let events_str = std::format!("{:?}", env.events().all());
    assert!(events_str.contains("TSK_REJ"));
}

#[test]
fn test_mismatched_result_rejected() {
    let (env, client) = setup();
    let creator = Address::generate(&env);
    let executor = Address::generate(&env);

    let input_a = BytesN::from_array(&env, &[1u8; 32]);
    let input_b = BytesN::from_array(&env, &[2u8; 32]);
    let result_b = BytesN::from_array(&env, &[3u8; 32]);
    // Proof valid for input_b and result_b, but NOT for input_a
    let proof_b = proof_for(&env, &input_b, &result_b);

    let task_id = client.submit_task(&creator, &input_a, &1, &1000);
    client.register_executor(&executor);
    client.assign_task(&executor, &task_id);

    // Submitting output from task B for task A must fail verification
    client.submit_result(&executor, &task_id, &result_b, &proof_b);

    let task = client.get_task(&task_id).unwrap();
    assert_eq!(task.status, crate::task_queue::TaskStatus::Failed);

    let info = client.get_executor_info(&executor).unwrap();
    assert_eq!(info.reputation, 90);
}

#[test]
fn test_direct_state_update_to_completed_rejected() {
    extern crate std;
    let (env, client) = setup();
    let creator = Address::generate(&env);
    let input = zero_hash(&env);
    let fake_result = zero_hash(&env);
    let invalid_proof = BytesN::from_array(&env, &[77u8; 32]);

    let task_id = client.submit_task(&creator, &input, &1, &1000);

    // Attempting a direct call to update_task setting status to Completed without valid proof must panic
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let task = crate::task_queue::Task {
            id: task_id,
            creator: creator.clone(),
            executor: None,
            input_data: input.clone(),
            result: Some(fake_result),
            proof: Some(invalid_proof),
            status: crate::task_queue::TaskStatus::Completed,
            priority: 1,
            deadline: 1000,
        };
        crate::task_queue::update_task(&env, task);
    }));

    assert!(result.is_err(), "Direct update to Completed without passing verification must be rejected");
}
