use soroban_sdk::{testutils::Address as _, Address, BytesN, Env};
use teye_delegation::{task_queue::TaskStatus, DelegationContract, DelegationContractClient};
use soroban_sdk::{
    testutils::{Address as _, Ledger as _},
    Env,
};
use retinax_delegation::{DelegationContract, DelegationContractClient};

#[test]
fn assignment_preserves_task_deadline() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(DelegationContract, ());
    let client = DelegationContractClient::new(&env, &contract_id);
    let owner = Address::generate(&env);
    let executor = Address::generate(&env);

    client.initialize(&owner);
    client.register_executor(&executor);
    let task_id = client.submit_task(&owner, &BytesN::from_array(&env, &[1; 32]), &1, &2_000);

    client.assign_task(&executor, &task_id);

    let task = client.get_task(&task_id).expect("assigned task");
    assert_eq!(task.status, TaskStatus::Assigned);
    assert_eq!(task.executor, Some(executor));
    assert_eq!(task.deadline, 2_000);
}
