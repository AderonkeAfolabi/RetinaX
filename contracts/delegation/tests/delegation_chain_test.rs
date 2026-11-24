#![allow(clippy::unwrap_used, clippy::expect_used)]

//! Delegation cycles and chains (#430).
//!
//! A delegation is a task one account submits and another executes. These
//! tests cover A→B→A cycles (two accounts delegating to each other, and an
//! account executing its own task), long chains where each task's result is
//! the next task's input, and a chain broken in the middle by a failed or
//! hijacked step. The contract has no explicit revoke entrypoint, so a broken
//! link is modelled by a failed proof, which ends that task permanently.

use soroban_sdk::{testutils::Address as _, Address, Bytes, BytesN, Env};
use teye_delegation::task_queue::TaskStatus;
use teye_delegation::{DelegationContract, DelegationContractClient};

const INITIAL_REPUTATION: u32 = 100;
const SLASH_PENALTY: u32 = 10;

fn setup(env: &Env) -> DelegationContractClient<'_> {
    env.mock_all_auths();
    let client = DelegationContractClient::new(env, &env.register(DelegationContract, ()));
    client.initialize(&Address::generate(env));
    client
}

fn hash(env: &Env, seed: u32) -> BytesN<32> {
    let mut raw = [0u8; 32];
    raw[..4].copy_from_slice(&seed.to_be_bytes());
    BytesN::from_array(env, &raw)
}

/// The proof `verify_execution_proof` accepts: sha256(input || result).
fn proof_for(env: &Env, input: &BytesN<32>, result: &BytesN<32>) -> BytesN<32> {
    let mut payload = [0u8; 64];
    payload[..32].copy_from_slice(&input.to_array());
    payload[32..].copy_from_slice(&result.to_array());
    BytesN::from_array(
        env,
        &env.crypto()
            .sha256(&Bytes::from_slice(env, &payload))
            .to_array(),
    )
}

/// `creator` delegates a task over `input` to `executor`, who completes it
/// with `result`. Returns the task id.
fn delegate_and_complete(
    env: &Env,
    client: &DelegationContractClient,
    creator: &Address,
    executor: &Address,
    input: &BytesN<32>,
    result: &BytesN<32>,
) -> u64 {
    let id = client.submit_task(creator, input, &1, &0);
    client.assign_task(executor, &id);
    client.submit_result(executor, &id, result, &proof_for(env, input, result));
    id
}

fn registered(env: &Env, client: &DelegationContractClient, count: u32) -> std::vec::Vec<Address> {
    (0..count)
        .map(|_| {
            let a = Address::generate(env);
            client.register_executor(&a);
            a
        })
        .collect()
}

// ── Cycles ────────────────────────────────────────────────────────────────────

#[test]
fn two_accounts_can_delegate_to_each_other() {
    let env = Env::default();
    let client = setup(&env);
    let accounts = registered(&env, &client, 2);
    let (a, b) = (&accounts[0], &accounts[1]);

    // A → B, then B → A.
    let ab = delegate_and_complete(&env, &client, a, b, &hash(&env, 1), &hash(&env, 2));
    let ba = delegate_and_complete(&env, &client, b, a, &hash(&env, 2), &hash(&env, 3));

    let ab_task = client.get_task(&ab).unwrap();
    let ba_task = client.get_task(&ba).unwrap();
    assert_eq!(
        (ab_task.creator.clone(), ab_task.executor.clone()),
        (a.clone(), Some(b.clone()))
    );
    assert_eq!(
        (ba_task.creator.clone(), ba_task.executor.clone()),
        (b.clone(), Some(a.clone()))
    );
    assert_eq!(ab_task.status, TaskStatus::Completed);
    assert_eq!(ba_task.status, TaskStatus::Completed);

    for account in [a, b] {
        let info = client.get_executor_info(account).unwrap();
        assert_eq!(info.tasks_completed, 1);
        assert_eq!(info.reputation, INITIAL_REPUTATION + 1);
    }
}

#[test]
fn a_cycle_repeated_many_times_keeps_each_task_independent() {
    let env = Env::default();
    let client = setup(&env);
    let accounts = registered(&env, &client, 2);
    let (a, b) = (&accounts[0], &accounts[1]);

    let mut ids = std::vec::Vec::new();
    for round in 0u32..10 {
        let (creator, executor) = if round % 2 == 0 { (a, b) } else { (b, a) };
        ids.push(delegate_and_complete(
            &env,
            &client,
            creator,
            executor,
            &hash(&env, round),
            &hash(&env, round + 1),
        ));
    }

    assert_eq!(ids, (1u64..=10).collect::<std::vec::Vec<_>>());
    for id in &ids {
        assert_eq!(client.get_task(id).unwrap().status, TaskStatus::Completed);
    }
    assert_eq!(client.get_executor_info(a).unwrap().tasks_completed, 5);
    assert_eq!(client.get_executor_info(b).unwrap().tasks_completed, 5);
}

#[test]
fn an_account_can_execute_its_own_task() {
    let env = Env::default();
    let client = setup(&env);
    let a = registered(&env, &client, 1).remove(0);

    // A → A: the contract does not forbid self-delegation.
    let id = delegate_and_complete(&env, &client, &a, &a, &hash(&env, 7), &hash(&env, 8));

    let task = client.get_task(&id).unwrap();
    assert_eq!(task.creator, a);
    assert_eq!(task.executor, Some(a.clone()));
    assert_eq!(task.status, TaskStatus::Completed);
}

#[test]
fn a_cycle_cannot_reassign_a_task_already_taken() {
    let env = Env::default();
    let client = setup(&env);
    let accounts = registered(&env, &client, 2);
    let (a, b) = (&accounts[0], &accounts[1]);

    let id = client.submit_task(a, &hash(&env, 1), &1, &0);
    client.assign_task(b, &id);

    // A tries to take back its own delegated task.
    assert!(client.try_assign_task(a, &id).is_err());
    let input = hash(&env, 1);
    let result = hash(&env, 2);
    assert!(client
        .try_submit_result(a, &id, &result, &proof_for(&env, &input, &result))
        .is_err());
    assert_eq!(client.get_task(&id).unwrap().executor, Some(b.clone()));
}

// ── Chains ────────────────────────────────────────────────────────────────────

#[test]
fn a_long_chain_feeds_each_result_into_the_next_task() {
    let env = Env::default();
    let client = setup(&env);
    let links = 25u32;
    let accounts = registered(&env, &client, links + 1);

    let mut input = hash(&env, 0);
    for i in 0..links as usize {
        let result = hash(&env, i as u32 + 1);
        let id = delegate_and_complete(
            &env,
            &client,
            &accounts[i],
            &accounts[i + 1],
            &input,
            &result,
        );
        let task = client.get_task(&id).unwrap();
        assert_eq!(task.input_data, input);
        assert_eq!(task.result, Some(result.clone()));
        input = result;
    }

    assert_eq!(input, hash(&env, links));
    for executor in &accounts[1..] {
        assert_eq!(
            client.get_executor_info(executor).unwrap().tasks_completed,
            1
        );
    }
    // The head of the chain only delegated.
    assert_eq!(
        client
            .get_executor_info(&accounts[0])
            .unwrap()
            .tasks_completed,
        0
    );
}

#[test]
fn a_failed_link_in_the_middle_breaks_only_that_link() {
    let env = Env::default();
    let client = setup(&env);
    let accounts = registered(&env, &client, 6);

    let first = delegate_and_complete(
        &env,
        &client,
        &accounts[0],
        &accounts[1],
        &hash(&env, 0),
        &hash(&env, 1),
    );
    let second = delegate_and_complete(
        &env,
        &client,
        &accounts[1],
        &accounts[2],
        &hash(&env, 1),
        &hash(&env, 2),
    );

    // Middle link: the executor submits a proof that does not match.
    let middle = client.submit_task(&accounts[2], &hash(&env, 2), &1, &0);
    client.assign_task(&accounts[3], &middle);
    client.submit_result(&accounts[3], &middle, &hash(&env, 3), &hash(&env, 999));

    let broken = client.get_task(&middle).unwrap();
    assert_eq!(broken.status, TaskStatus::Failed);
    assert_eq!(broken.result, None);
    assert_eq!(
        client.get_executor_info(&accounts[3]).unwrap().reputation,
        INITIAL_REPUTATION - SLASH_PENALTY
    );

    // Earlier links are untouched.
    for id in [first, second] {
        assert_eq!(client.get_task(&id).unwrap().status, TaskStatus::Completed);
    }

    // The failed link is final: it cannot be retried or handed to someone else.
    assert!(client.try_assign_task(&accounts[4], &middle).is_err());
    let proof = proof_for(&env, &hash(&env, 2), &hash(&env, 3));
    assert!(client
        .try_submit_result(&accounts[3], &middle, &hash(&env, 3), &proof)
        .is_err());

    // The chain resumes from the last good result with a fresh task.
    let resumed = delegate_and_complete(
        &env,
        &client,
        &accounts[2],
        &accounts[4],
        &hash(&env, 2),
        &hash(&env, 3),
    );
    let tail = delegate_and_complete(
        &env,
        &client,
        &accounts[4],
        &accounts[5],
        &hash(&env, 3),
        &hash(&env, 4),
    );
    assert_eq!(
        client.get_task(&resumed).unwrap().status,
        TaskStatus::Completed
    );
    assert_eq!(client.get_task(&tail).unwrap().result, Some(hash(&env, 4)));
}

#[test]
fn a_pending_link_does_not_block_the_rest_of_the_chain() {
    let env = Env::default();
    let client = setup(&env);
    let accounts = registered(&env, &client, 3);

    // The middle task is never picked up.
    let stalled = client.submit_task(&accounts[0], &hash(&env, 0), &1, &0);
    let next = delegate_and_complete(
        &env,
        &client,
        &accounts[1],
        &accounts[2],
        &hash(&env, 1),
        &hash(&env, 2),
    );

    assert_eq!(
        client.get_task(&stalled).unwrap().status,
        TaskStatus::Pending
    );
    assert_eq!(client.get_task(&stalled).unwrap().executor, None);
    assert_eq!(
        client.get_task(&next).unwrap().status,
        TaskStatus::Completed
    );
}

#[test]
fn unregistered_accounts_cannot_join_a_chain() {
    let env = Env::default();
    let client = setup(&env);
    let creator = registered(&env, &client, 1).remove(0);
    let outsider = Address::generate(&env);

    let id = client.submit_task(&creator, &hash(&env, 0), &1, &0);
    assert!(client.try_assign_task(&outsider, &id).is_err());
    assert_eq!(client.get_task(&id).unwrap().status, TaskStatus::Pending);
}

#[test]
fn repeated_failures_along_a_chain_floor_reputation_at_zero() {
    let env = Env::default();
    let client = setup(&env);
    let accounts = registered(&env, &client, 2);
    let (creator, executor) = (&accounts[0], &accounts[1]);

    let failures = INITIAL_REPUTATION / SLASH_PENALTY + 2;
    for i in 0..failures {
        let id = client.submit_task(creator, &hash(&env, i), &1, &0);
        client.assign_task(executor, &id);
        client.submit_result(executor, &id, &hash(&env, i + 1), &hash(&env, 0));
        assert_eq!(client.get_task(&id).unwrap().status, TaskStatus::Failed);
    }

    let info = client.get_executor_info(executor).unwrap();
    assert_eq!(info.reputation, 0);
    assert_eq!(info.tasks_completed, 0);
}

#[test]
fn a_missing_task_cannot_be_assigned_or_completed() {
    let env = Env::default();
    let client = setup(&env);
    let executor = registered(&env, &client, 1).remove(0);

    assert!(client.try_assign_task(&executor, &42).is_err());
    let proof = proof_for(&env, &hash(&env, 0), &hash(&env, 1));
    assert!(client
        .try_submit_result(&executor, &42, &hash(&env, 1), &proof)
        .is_err());
    assert!(client.get_task(&42).is_none());
}
