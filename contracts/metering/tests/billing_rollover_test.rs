#![allow(clippy::unwrap_used, clippy::expect_used)]

//! Billing period rollover tests (#432). Billing cycles are opened and closed
//! by the admin, not by ledger time: usage counters reset when a new cycle
//! opens, however much time has passed, and never on their own.

use metering::{
    billing::CycleStatus, quota::TenantQuota, GasCosts, MeteringContract, MeteringContractClient,
    MeteringError, OperationType, TenantLevel,
};
use soroban_sdk::{
    testutils::{Address as _, Ledger as _},
    Address, Env,
};

const START: u64 = 1_700_000_000;
const PERIOD: u64 = 30 * 24 * 60 * 60;

fn setup(env: &Env) -> (MeteringContractClient<'_>, Address, Address) {
    env.mock_all_auths();
    env.ledger().with_mut(|l| l.timestamp = START);
    let client = MeteringContractClient::new(env, &env.register(MeteringContract, ()));
    let admin = Address::generate(env);
    let tenant = Address::generate(env);
    client.initialize(&admin);
    client.register_tenant(&admin, &tenant, &TenantLevel::Organization, &tenant);
    client.set_gas_costs(
        &admin,
        &GasCosts {
            read_cost: 1,
            write_cost: 5,
            compute_cost: 10,
            storage_cost: 3,
        },
    );
    (client, admin, tenant)
}

fn advance(env: &Env, seconds: u64) {
    env.ledger().with_mut(|l| l.timestamp += seconds);
}

fn now(env: &Env) -> u64 {
    env.ledger().timestamp()
}

fn quota(bucket: u64, burst: u64) -> TenantQuota {
    TenantQuota {
        read_limit: bucket,
        write_limit: bucket,
        compute_limit: bucket,
        storage_limit: bucket,
        total_limit: bucket * 4,
        burst_allowance: burst,
        enabled: true,
    }
}

#[test]
fn usage_does_not_reset_when_time_passes_without_a_new_cycle() {
    let env = Env::default();
    let (client, admin, tenant) = setup(&env);
    client.open_billing_cycle(&admin);
    client.record_gas(&admin, &tenant, &OperationType::Write);

    advance(&env, PERIOD * 3);

    assert_eq!(client.get_usage(&tenant).write_used, 5);
    assert_eq!(client.current_cycle_id(), 1);
}

#[test]
fn opening_the_next_cycle_resets_every_usage_counter() {
    let env = Env::default();
    let (client, admin, tenant) = setup(&env);
    client.set_quota(&admin, &tenant, &quota(10, 20));
    client.open_billing_cycle(&admin);
    for op in [
        OperationType::Read,
        OperationType::Write,
        OperationType::Compute,
        OperationType::Storage,
    ] {
        client.record_gas(&admin, &tenant, &op);
    }
    // Overflow the compute bucket into burst.
    client.record_gas(&admin, &tenant, &OperationType::Compute);
    let before = client.get_usage(&tenant);
    assert_eq!(before.total(), 29);
    assert_eq!(before.burst_used, 10);

    client.close_billing_cycle(&admin);
    advance(&env, PERIOD);
    client.open_billing_cycle(&admin);

    let after = client.get_usage(&tenant);
    assert_eq!(after.read_used, 0);
    assert_eq!(after.write_used, 0);
    assert_eq!(after.compute_used, 0);
    assert_eq!(after.storage_used, 0);
    assert_eq!(after.burst_used, 0);
}

#[test]
fn a_tenant_blocked_at_quota_is_allowed_again_after_rollover() {
    let env = Env::default();
    let (client, admin, tenant) = setup(&env);
    client.set_quota(&admin, &tenant, &quota(2, 0));
    client.open_billing_cycle(&admin);
    client.record_gas(&admin, &tenant, &OperationType::Read);
    client.record_gas(&admin, &tenant, &OperationType::Read);
    assert_eq!(
        client.try_record_gas(&admin, &tenant, &OperationType::Read),
        Err(Ok(MeteringError::QuotaExceeded))
    );

    // Time alone does not lift the block.
    advance(&env, PERIOD);
    assert_eq!(
        client.try_record_gas(&admin, &tenant, &OperationType::Read),
        Err(Ok(MeteringError::QuotaExceeded))
    );

    client.close_billing_cycle(&admin);
    client.open_billing_cycle(&admin);
    client.record_gas(&admin, &tenant, &OperationType::Read);
    assert_eq!(client.get_usage(&tenant).read_used, 1);
}

#[test]
fn cycle_boundaries_record_the_ledger_timestamps() {
    let env = Env::default();
    let (client, admin, _tenant) = setup(&env);

    let first = client.open_billing_cycle(&admin);
    advance(&env, PERIOD);
    let report = client.close_billing_cycle(&admin);
    advance(&env, 1);
    let second = client.open_billing_cycle(&admin);

    assert_eq!((first, second), (1, 2));
    let closed = client.get_billing_cycle(&first).unwrap();
    assert_eq!(closed.started_at, START);
    assert_eq!(closed.ended_at, START + PERIOD);
    assert_eq!(closed.status, CycleStatus::Closed);
    assert_eq!(report.closed_at, START + PERIOD);

    let open = client.get_billing_cycle(&second).unwrap();
    assert_eq!(open.started_at, START + PERIOD + 1);
    assert_eq!(open.ended_at, 0);
    assert_eq!(open.status, CycleStatus::Open);
}

#[test]
fn multiple_missed_periods_stay_in_one_cycle_and_ids_do_not_catch_up() {
    let env = Env::default();
    let (client, admin, tenant) = setup(&env);
    client.open_billing_cycle(&admin);
    client.record_gas(&admin, &tenant, &OperationType::Read);

    // Three billing periods pass without the admin rolling the cycle over.
    for _ in 0..3 {
        advance(&env, PERIOD);
        client.record_gas(&admin, &tenant, &OperationType::Write);
    }

    let report = client.close_billing_cycle(&admin);
    assert_eq!(report.cycle_id, 1);
    let record = report.records.get(0).unwrap();
    assert_eq!(record.read_units, 1);
    assert_eq!(record.write_units, 15);
    assert_eq!(record.total_cost, 16);
    assert_eq!(client.get_invoice(&tenant, &1).unwrap().amount_due, 16);
    assert_eq!(
        client.get_billing_cycle(&1).unwrap().ended_at,
        START + PERIOD * 3
    );

    let next = client.open_billing_cycle(&admin);
    assert_eq!(next, 2);
    assert_eq!(client.get_usage(&tenant).total(), 0);
    assert!(client.get_billing_cycle(&3).is_none());
}

#[test]
fn many_consecutive_rollovers_bill_each_cycle_separately() {
    let env = Env::default();
    let (client, admin, tenant) = setup(&env);

    for cycle in 1u64..=6 {
        assert_eq!(client.open_billing_cycle(&admin), cycle);
        for _ in 0..cycle {
            client.record_gas(&admin, &tenant, &OperationType::Storage);
        }
        advance(&env, PERIOD);
        let report = client.close_billing_cycle(&admin);
        assert_eq!(report.records.get(0).unwrap().storage_units, cycle * 3);
    }

    for cycle in 1u64..=6 {
        let invoice = client.get_invoice(&tenant, &cycle).unwrap();
        assert_eq!(invoice.amount_due, cycle * 3, "cycle {cycle}");
        assert!(!invoice.settled);
    }
}

#[test]
fn a_cycle_with_no_usage_issues_no_invoice() {
    let env = Env::default();
    let (client, admin, tenant) = setup(&env);
    client.open_billing_cycle(&admin);
    client.record_gas(&admin, &tenant, &OperationType::Read);
    client.close_billing_cycle(&admin);

    advance(&env, PERIOD);
    client.open_billing_cycle(&admin);
    advance(&env, PERIOD);
    let report = client.close_billing_cycle(&admin);

    assert_eq!(report.records.get(0).unwrap().total_cost, 0);
    assert!(client.get_invoice(&tenant, &2).is_none());
    assert_eq!(client.get_invoice(&tenant, &1).unwrap().amount_due, 1);
}

#[test]
fn rollover_resets_parent_usage_rolled_up_from_children() {
    let env = Env::default();
    let (client, admin, org) = setup(&env);
    let clinic = Address::generate(&env);
    client.register_tenant(&admin, &clinic, &TenantLevel::Clinic, &org);
    client.open_billing_cycle(&admin);
    client.record_gas(&admin, &clinic, &OperationType::Compute);
    assert_eq!(client.get_usage(&org).compute_used, 10);

    client.close_billing_cycle(&admin);
    advance(&env, PERIOD);
    client.open_billing_cycle(&admin);

    assert_eq!(client.get_usage(&clinic).total(), 0);
    assert_eq!(client.get_usage(&org).total(), 0);
}

#[test]
fn rollover_requires_the_previous_cycle_to_be_closed() {
    let env = Env::default();
    let (client, admin, tenant) = setup(&env);

    assert_eq!(
        client.try_close_billing_cycle(&admin),
        Err(Ok(MeteringError::NoCycleOpen))
    );
    client.open_billing_cycle(&admin);
    client.record_gas(&admin, &tenant, &OperationType::Read);

    advance(&env, PERIOD * 2);
    assert_eq!(
        client.try_open_billing_cycle(&admin),
        Err(Ok(MeteringError::CycleAlreadyOpen))
    );
    // The failed rollover leaves usage untouched.
    assert_eq!(client.get_usage(&tenant).read_used, 1);

    client.close_billing_cycle(&admin);
    assert_eq!(
        client.try_close_billing_cycle(&admin),
        Err(Ok(MeteringError::NoCycleOpen))
    );
}

#[test]
fn usage_recorded_between_cycles_is_cleared_by_the_next_open() {
    let env = Env::default();
    let (client, admin, tenant) = setup(&env);
    client.open_billing_cycle(&admin);
    client.close_billing_cycle(&admin);

    // No cycle is open, but gas is still metered.
    advance(&env, PERIOD);
    client.record_gas(&admin, &tenant, &OperationType::Write);
    assert_eq!(client.get_usage(&tenant).write_used, 5);

    client.open_billing_cycle(&admin);
    assert_eq!(client.get_usage(&tenant).write_used, 0);
    assert_eq!(now(&env), START + PERIOD);
}
