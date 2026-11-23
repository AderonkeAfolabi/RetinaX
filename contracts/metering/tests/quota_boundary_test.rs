#![allow(clippy::unwrap_used, clippy::expect_used)]

//! Quota boundary tests (#431): usage exactly at, just below and just above a
//! quota, including burst allowance and `u64` limits.

use metering::{
    billing::BillingModel, quota::TenantQuota, GasCosts, MeteringContract, MeteringContractClient,
    MeteringError, OperationType, TenantLevel,
};
use soroban_sdk::{testutils::Address as _, Address, Env};

fn setup(env: &Env) -> (MeteringContractClient<'_>, Address, Address) {
    env.mock_all_auths();
    let client = MeteringContractClient::new(env, &env.register(MeteringContract, ()));
    let admin = Address::generate(env);
    let tenant = Address::generate(env);
    client.initialize(&admin);
    client.register_tenant(&admin, &tenant, &TenantLevel::Organization, &tenant);
    (client, admin, tenant)
}

/// Every operation type costs `units` gas.
fn set_cost(client: &MeteringContractClient, admin: &Address, units: u64) {
    client.set_gas_costs(
        admin,
        &GasCosts {
            read_cost: units,
            write_cost: units,
            compute_cost: units,
            storage_cost: units,
        },
    );
}

/// Every bucket limited to `bucket`, total to `total`.
fn quota(bucket: u64, total: u64, burst: u64) -> TenantQuota {
    TenantQuota {
        read_limit: bucket,
        write_limit: bucket,
        compute_limit: bucket,
        storage_limit: bucket,
        total_limit: total,
        burst_allowance: burst,
        enabled: true,
    }
}

fn read(
    client: &MeteringContractClient,
    admin: &Address,
    tenant: &Address,
) -> Result<(), MeteringError> {
    match client.try_record_gas(admin, tenant, &OperationType::Read) {
        Ok(_) => Ok(()),
        Err(Ok(e)) => Err(e),
        Err(Err(e)) => panic!("unexpected invoke error {e:?}"),
    }
}

// ── Per-bucket limit ──────────────────────────────────────────────────────────

#[test]
fn usage_exactly_at_the_bucket_limit_is_allowed() {
    let env = Env::default();
    let (client, admin, tenant) = setup(&env);
    set_cost(&client, &admin, 1);
    client.set_quota(&admin, &tenant, &quota(10, 1_000, 0));

    for _ in 0..10 {
        read(&client, &admin, &tenant).unwrap();
    }
    assert_eq!(client.get_usage(&tenant).read_used, 10);
}

#[test]
fn usage_one_unit_below_the_limit_leaves_room_for_one_more() {
    let env = Env::default();
    let (client, admin, tenant) = setup(&env);
    set_cost(&client, &admin, 1);
    client.set_quota(&admin, &tenant, &quota(10, 1_000, 0));

    for _ in 0..9 {
        read(&client, &admin, &tenant).unwrap();
    }
    assert_eq!(client.get_usage(&tenant).read_used, 9);
    assert_eq!(read(&client, &admin, &tenant), Ok(()));
}

#[test]
fn usage_one_unit_above_the_limit_is_rejected_without_recording() {
    let env = Env::default();
    let (client, admin, tenant) = setup(&env);
    set_cost(&client, &admin, 1);
    client.set_quota(&admin, &tenant, &quota(10, 1_000, 0));
    for _ in 0..10 {
        read(&client, &admin, &tenant).unwrap();
    }

    assert_eq!(
        read(&client, &admin, &tenant),
        Err(MeteringError::QuotaExceeded)
    );
    let usage = client.get_usage(&tenant);
    assert_eq!(usage.read_used, 10);
    assert_eq!(usage.burst_used, 0);
}

#[test]
fn an_operation_that_would_overshoot_is_rejected_even_with_headroom_left() {
    let env = Env::default();
    let (client, admin, tenant) = setup(&env);
    set_cost(&client, &admin, 4);
    client.set_quota(&admin, &tenant, &quota(10, 1_000, 0));

    read(&client, &admin, &tenant).unwrap();
    read(&client, &admin, &tenant).unwrap();
    // 8 used, 2 left, but the next read costs 4.
    assert_eq!(
        read(&client, &admin, &tenant),
        Err(MeteringError::QuotaExceeded)
    );
    assert_eq!(client.get_usage(&tenant).read_used, 8);
}

#[test]
fn buckets_are_limited_independently() {
    let env = Env::default();
    let (client, admin, tenant) = setup(&env);
    set_cost(&client, &admin, 5);
    client.set_quota(&admin, &tenant, &quota(5, 1_000, 0));

    client.record_gas(&admin, &tenant, &OperationType::Read);
    assert_eq!(
        read(&client, &admin, &tenant),
        Err(MeteringError::QuotaExceeded)
    );
    // A full read bucket does not block writes.
    client.record_gas(&admin, &tenant, &OperationType::Write);
    assert_eq!(
        client.try_record_gas(&admin, &tenant, &OperationType::Write),
        Err(Ok(MeteringError::QuotaExceeded))
    );
}

// ── Total limit ───────────────────────────────────────────────────────────────

#[test]
fn total_limit_is_enforced_across_buckets_at_the_boundary() {
    let env = Env::default();
    let (client, admin, tenant) = setup(&env);
    set_cost(&client, &admin, 5);
    client.set_quota(&admin, &tenant, &quota(100, 15, 0));

    client.record_gas(&admin, &tenant, &OperationType::Read);
    client.record_gas(&admin, &tenant, &OperationType::Write);
    // 10 + 5 == 15: exactly at the total cap.
    client.record_gas(&admin, &tenant, &OperationType::Compute);
    assert_eq!(client.get_usage(&tenant).total(), 15);

    assert_eq!(
        client.try_record_gas(&admin, &tenant, &OperationType::Storage),
        Err(Ok(MeteringError::QuotaExceeded))
    );
    assert_eq!(client.get_usage(&tenant).total(), 15);
}

// ── Burst allowance ───────────────────────────────────────────────────────────

#[test]
fn burst_covers_an_overflow_exactly_equal_to_the_remaining_burst() {
    let env = Env::default();
    let (client, admin, tenant) = setup(&env);
    set_cost(&client, &admin, 5);
    client.set_quota(&admin, &tenant, &quota(10, 1_000, 5));

    read(&client, &admin, &tenant).unwrap();
    read(&client, &admin, &tenant).unwrap();
    // Bucket full; the next 5 units come entirely from the 5-unit burst.
    read(&client, &admin, &tenant).unwrap();
    let usage = client.get_usage(&tenant);
    assert_eq!(usage.read_used, 15);
    assert_eq!(usage.burst_used, 5);

    assert_eq!(
        read(&client, &admin, &tenant),
        Err(MeteringError::QuotaExceeded)
    );
    assert_eq!(client.get_usage(&tenant).burst_used, 5);
}

#[test]
fn burst_smaller_than_the_operation_cost_does_not_help() {
    let env = Env::default();
    let (client, admin, tenant) = setup(&env);
    set_cost(&client, &admin, 5);
    client.set_quota(&admin, &tenant, &quota(10, 1_000, 4));

    read(&client, &admin, &tenant).unwrap();
    read(&client, &admin, &tenant).unwrap();
    assert_eq!(
        read(&client, &admin, &tenant),
        Err(MeteringError::QuotaExceeded)
    );
    assert_eq!(client.get_usage(&tenant).burst_used, 0);
}

#[test]
fn only_the_overflow_beyond_the_bucket_is_drawn_from_burst() {
    let env = Env::default();
    let (client, admin, tenant) = setup(&env);
    set_cost(&client, &admin, 4);
    client.set_quota(&admin, &tenant, &quota(10, 1_000, 4));

    read(&client, &admin, &tenant).unwrap();
    read(&client, &admin, &tenant).unwrap();
    // 8 used: the next 4 units are 2 of headroom plus 2 of burst.
    read(&client, &admin, &tenant).unwrap();
    let usage = client.get_usage(&tenant);
    assert_eq!(usage.read_used, 12);
    assert_eq!(usage.burst_used, 2);
    // 2 burst left is less than the 4-unit cost.
    assert_eq!(
        read(&client, &admin, &tenant),
        Err(MeteringError::QuotaExceeded)
    );
}

// ── Zero and disabled quotas ──────────────────────────────────────────────────

#[test]
fn a_zero_limit_blocks_every_paid_operation() {
    let env = Env::default();
    let (client, admin, tenant) = setup(&env);
    set_cost(&client, &admin, 1);
    client.set_quota(&admin, &tenant, &quota(0, 0, 0));

    assert_eq!(
        read(&client, &admin, &tenant),
        Err(MeteringError::QuotaExceeded)
    );
}

#[test]
fn a_zero_cost_operation_fits_a_zero_limit() {
    let env = Env::default();
    let (client, admin, tenant) = setup(&env);
    set_cost(&client, &admin, 0);
    client.set_quota(&admin, &tenant, &quota(0, 0, 0));

    assert_eq!(read(&client, &admin, &tenant), Ok(()));
    assert_eq!(client.get_usage(&tenant).total(), 0);
}

#[test]
fn a_disabled_or_missing_quota_is_unlimited() {
    let env = Env::default();
    let (client, admin, tenant) = setup(&env);
    set_cost(&client, &admin, 1_000);

    read(&client, &admin, &tenant).unwrap();

    let mut disabled = quota(0, 0, 0);
    disabled.enabled = false;
    client.set_quota(&admin, &tenant, &disabled);
    read(&client, &admin, &tenant).unwrap();
    assert_eq!(client.get_usage(&tenant).read_used, 2_000);

    client.set_quota(&admin, &tenant, &quota(0, 0, 0));
    assert_eq!(
        read(&client, &admin, &tenant),
        Err(MeteringError::QuotaExceeded)
    );
    client.remove_quota(&admin, &tenant);
    assert_eq!(read(&client, &admin, &tenant), Ok(()));
}

// ── u64 limits ────────────────────────────────────────────────────────────────

#[test]
fn a_u64_max_operation_fits_a_u64_max_limit() {
    let env = Env::default();
    let (client, admin, tenant) = setup(&env);
    set_cost(&client, &admin, u64::MAX);
    client.set_quota(&admin, &tenant, &quota(u64::MAX, u64::MAX, 0));

    read(&client, &admin, &tenant).unwrap();
    assert_eq!(client.get_usage(&tenant).read_used, u64::MAX);
}

#[test]
fn usage_saturates_at_u64_max_instead_of_overflowing() {
    let env = Env::default();
    let (client, admin, tenant) = setup(&env);
    set_cost(&client, &admin, u64::MAX);
    client.set_quota(&admin, &tenant, &quota(u64::MAX, u64::MAX, 0));

    read(&client, &admin, &tenant).unwrap();
    // u64::MAX + u64::MAX saturates to u64::MAX, which is still within the limit.
    read(&client, &admin, &tenant).unwrap();
    client.record_gas(&admin, &tenant, &OperationType::Write);

    let usage = client.get_usage(&tenant);
    assert_eq!(usage.read_used, u64::MAX);
    assert_eq!(usage.write_used, u64::MAX);
    assert_eq!(usage.total(), u64::MAX);
}

#[test]
fn a_u64_max_operation_is_rejected_one_below_u64_max() {
    let env = Env::default();
    let (client, admin, tenant) = setup(&env);
    set_cost(&client, &admin, u64::MAX);
    client.set_quota(&admin, &tenant, &quota(u64::MAX - 1, u64::MAX, 0));

    assert_eq!(
        read(&client, &admin, &tenant),
        Err(MeteringError::QuotaExceeded)
    );
    assert_eq!(client.get_usage(&tenant).read_used, 0);
}

#[test]
fn a_u64_max_burst_absorbs_a_u64_max_overflow() {
    let env = Env::default();
    let (client, admin, tenant) = setup(&env);
    set_cost(&client, &admin, u64::MAX);
    client.set_quota(&admin, &tenant, &quota(0, 0, u64::MAX));

    read(&client, &admin, &tenant).unwrap();
    let usage = client.get_usage(&tenant);
    assert_eq!(usage.read_used, u64::MAX);
    assert_eq!(usage.burst_used, u64::MAX);
    assert_eq!(
        read(&client, &admin, &tenant),
        Err(MeteringError::QuotaExceeded)
    );
}

// ── Prepaid balance boundary ──────────────────────────────────────────────────

#[test]
fn prepaid_balance_exactly_equal_to_the_cost_is_enough() {
    let env = Env::default();
    let (client, admin, tenant) = setup(&env);
    set_cost(&client, &admin, 7);
    client.set_billing_model(&admin, &tenant, &BillingModel::Prepaid);
    client.mint_gas_tokens(&admin, &tenant, &7);

    read(&client, &admin, &tenant).unwrap();
    assert_eq!(client.gas_token_balance(&tenant), 0);
    assert_eq!(
        read(&client, &admin, &tenant),
        Err(MeteringError::InsufficientPrepaidBalance)
    );
}

#[test]
fn prepaid_balance_one_below_the_cost_is_rejected() {
    let env = Env::default();
    let (client, admin, tenant) = setup(&env);
    set_cost(&client, &admin, 7);
    client.set_billing_model(&admin, &tenant, &BillingModel::Prepaid);
    client.mint_gas_tokens(&admin, &tenant, &6);

    assert_eq!(
        read(&client, &admin, &tenant),
        Err(MeteringError::InsufficientPrepaidBalance)
    );
    assert_eq!(client.gas_token_balance(&tenant), 6);
    assert_eq!(client.get_usage(&tenant).read_used, 0);
}
