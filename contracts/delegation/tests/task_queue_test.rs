//! Direct unit tests for the `task_queue` helpers (issue #529).
//!
//! The helpers use contract storage, so each test calls them inside
//! `Env::as_contract` instead of going through the contract's entry points.

use retinax_delegation::task_queue::{
    create_task, get_task, next_task_id, update_task, Task, TaskStatus,
};
use retinax_delegation::DelegationContract;
use soroban_sdk::{testutils::Address as _, Address, Bytes, BytesN, Env};

fn setup() -> (Env, Address) {
    let env = Env::default();
    let contract_id = env.register(DelegationContract, ());
    (env, contract_id)
}

fn hash(env: &Env, value: u8) -> BytesN<32> {
    BytesN::from_array(env, &[value; 32])
}

#[test]
fn next_task_id_is_strictly_increasing_and_not_reused_after_create() {
    let (env, contract_id) = setup();
    let creator = Address::generate(&env);

    // Separate calls, so the counter must be read back from storage each time.
    let first = env.as_contract(&contract_id, || next_task_id(&env));
    let second = env.as_contract(&contract_id, || next_task_id(&env));
    let created = env.as_contract(&contract_id, || {
        create_task(&env, creator, hash(&env, 1), 0, 0)
    });
    let after_create = env.as_contract(&contract_id, || next_task_id(&env));

    assert_eq!([first, second, created, after_create], [1, 2, 3, 4]);
}

#[test]
fn create_task_is_read_back_with_identical_fields() {
    let (env, contract_id) = setup();
    let creator = Address::generate(&env);

    let id = env.as_contract(&contract_id, || {
        create_task(&env, creator.clone(), hash(&env, 7), u32::MAX, u64::MAX)
    });

    assert_eq!(
        env.as_contract(&contract_id, || get_task(&env, id)),
        Some(Task {
            id,
            creator,
            executor: None,
            input_data: hash(&env, 7),
            result: None,
            proof: None,
            status: TaskStatus::Pending,
            priority: u32::MAX,
            deadline: u64::MAX,
        })
    );
}

#[test]
fn get_task_returns_none_for_unknown_ids() {
    let (env, contract_id) = setup();
    let creator = Address::generate(&env);

    env.as_contract(&contract_id, || {
        assert_eq!(get_task(&env, 1), None);

        let id = create_task(&env, creator, hash(&env, 1), 0, 0);
        for unknown in [0, id + 1, u64::MAX] {
            assert_eq!(get_task(&env, unknown), None);
        }
    });
}

#[test]
fn update_task_persists_every_field_and_leaves_other_tasks_alone() {
    let (env, contract_id) = setup();
    let creator = Address::generate(&env);

    // A Completed task is only accepted with proof == sha256(input || result).
    let proof = env
        .crypto()
        .sha256(&Bytes::from_slice(&env, &[[4u8; 32], [5u8; 32]].concat()))
        .to_bytes();
    let mut updated = Task {
        id: 2,
        creator: Address::generate(&env),
        executor: Some(Address::generate(&env)),
        input_data: hash(&env, 4),
        result: Some(hash(&env, 5)),
        proof: Some(proof),
        status: TaskStatus::Assigned,
        priority: 7,
        deadline: 8,
    };

    env.as_contract(&contract_id, || {
        for value in 1..=3u8 {
            create_task(&env, creator.clone(), hash(&env, value), 1, 1);
        }
        let others = (get_task(&env, 1), get_task(&env, 3));

        for status in [
            TaskStatus::Assigned,
            TaskStatus::Completed,
            TaskStatus::Failed,
            TaskStatus::Slushed,
        ] {
            updated.status = status;
            update_task(&env, updated.clone());
            assert_eq!(get_task(&env, 2), Some(updated.clone()));
        }

        assert_eq!((get_task(&env, 1), get_task(&env, 3)), others);
    });
}
