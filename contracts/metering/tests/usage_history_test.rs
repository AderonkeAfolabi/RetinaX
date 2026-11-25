#![allow(clippy::unwrap_used, clippy::expect_used)]

//! Usage history tests (#532): what is recorded, the cap and its eviction order, pagination, and
//! the cost of recording.

use metering::{
    history::{UsageHistoryPage, DEFAULT_HISTORY_CAP, DEFAULT_LIMIT, MAX_HISTORY_CAP, MAX_LIMIT},
    GasCosts, MeteringContract, MeteringContractClient, MeteringError, OperationType, TenantLevel,
};
use soroban_sdk::{
    testutils::{Address as _, Ledger as _},
    Address, Env,
};

const START: u64 = 1_700_000_000;

fn setup(env: &Env) -> (MeteringContractClient<'_>, Address, Address) {
    env.mock_all_auths();
    env.ledger().set_timestamp(START);
    let client = MeteringContractClient::new(env, &env.register(MeteringContract, ()));
    let admin = Address::generate(env);
    let tenant = Address::generate(env);
    client.initialize(&admin);
    client.register_tenant(&admin, &tenant, &TenantLevel::Organization, &tenant);
    (client, admin, tenant)
}

/// Records `count` reads.
fn record(client: &MeteringContractClient, admin: &Address, tenant: &Address, count: u32) {
    for _ in 0..count {
        client.record_gas(admin, tenant, &OperationType::Read);
    }
}

fn ids(page: &UsageHistoryPage) -> Vec<u64> {
    page.entries.iter().map(|entry| entry.id).collect()
}

/// IDs of everything stored for `tenant`, newest first.
fn all_ids(client: &MeteringContractClient, tenant: &Address) -> Vec<u64> {
    ids(&client.get_usage_history(tenant, &Some(MAX_LIMIT), &None))
}

/// Organization → clinic → provider → patient, in that order.
fn hierarchy(
    env: &Env,
    client: &MeteringContractClient,
    admin: &Address,
    org: &Address,
) -> [Address; 4] {
    let [clinic, provider, patient] = [(); 3].map(|_| Address::generate(env));
    client.register_tenant(admin, &clinic, &TenantLevel::Clinic, org);
    client.register_tenant(admin, &provider, &TenantLevel::Provider, &clinic);
    client.register_tenant(admin, &patient, &TenantLevel::Patient, &provider);
    [org.clone(), clinic, provider, patient]
}

// ── Recording ─────────────────────────────────────────────────────────────────

#[test]
fn every_operation_is_recorded_with_the_units_it_was_charged() {
    let env = Env::default();
    let (client, admin, tenant) = setup(&env);
    let costs = GasCosts::default_costs();
    let operations = [
        OperationType::Read,
        OperationType::Write,
        OperationType::Compute,
        OperationType::Storage,
    ];

    for (i, op) in operations.iter().enumerate() {
        env.ledger().set_timestamp(START + i as u64);
        client.record_gas(&admin, &tenant, op);
    }

    let page = client.get_usage_history(&tenant, &None, &None);
    assert_eq!(page.entries.len(), 4);
    for (i, op) in operations.iter().enumerate() {
        let entry = page.entries.get(3 - i as u32).unwrap();
        assert_eq!(entry.id, i as u64 + 1);
        assert_eq!(entry.op_type, *op);
        assert_eq!(entry.units, costs.cost_for(op));
        assert_eq!(entry.timestamp, START + i as u64);
    }
}

#[test]
fn operations_are_recorded_for_every_ancestor() {
    let env = Env::default();
    let (client, admin, org) = setup(&env);
    let tenants = hierarchy(&env, &client, &admin, &org);

    client.record_gas(&admin, &tenants[3], &OperationType::Write);

    let expected = client.get_usage_history(&tenants[3], &None, &None);
    assert_eq!(expected.entries.len(), 1);
    for tenant in &tenants {
        assert_eq!(client.get_usage_history(tenant, &None, &None), expected);
    }
}

#[test]
fn a_new_billing_cycle_resets_usage_but_keeps_the_history() {
    let env = Env::default();
    let (client, admin, tenant) = setup(&env);
    record(&client, &admin, &tenant, 2);

    client.open_billing_cycle(&admin);
    assert_eq!(client.get_usage(&tenant).total(), 0);
    assert_eq!(all_ids(&client, &tenant), [2, 1]);

    record(&client, &admin, &tenant, 1);
    assert_eq!(client.get_usage(&tenant).total(), 1);
    assert_eq!(all_ids(&client, &tenant), [3, 2, 1]);
}

// ── Cap and eviction ──────────────────────────────────────────────────────────

#[test]
fn history_never_exceeds_the_cap_and_evicts_the_oldest_first() {
    let env = Env::default();
    let (client, admin, tenant) = setup(&env);
    client.set_history_cap(&admin, &3);

    for (recorded, expected) in [
        (1, vec![1]),
        (2, vec![2, 1]),
        (3, vec![3, 2, 1]),
        (4, vec![4, 3, 2]),
        (5, vec![5, 4, 3]),
    ] {
        record(&client, &admin, &tenant, 1);
        assert_eq!(all_ids(&client, &tenant), expected, "after {recorded}");
    }
}

#[test]
fn history_is_capped_by_default() {
    let env = Env::default();
    let (client, admin, tenant) = setup(&env);

    record(&client, &admin, &tenant, DEFAULT_HISTORY_CAP + 1);

    let stored = all_ids(&client, &tenant);
    assert_eq!(stored.len() as u32, DEFAULT_HISTORY_CAP);
    assert_eq!(stored.last(), Some(&2));
}

#[test]
fn lowering_the_cap_applies_before_and_after_the_next_write() {
    let env = Env::default();
    let (client, admin, tenant) = setup(&env);
    record(&client, &admin, &tenant, 5);

    client.set_history_cap(&admin, &2);
    assert_eq!(all_ids(&client, &tenant), [5, 4]);

    record(&client, &admin, &tenant, 1);
    assert_eq!(all_ids(&client, &tenant), [6, 5]);

    // Evicted entries do not come back when the cap is raised again.
    client.set_history_cap(&admin, &5);
    assert_eq!(all_ids(&client, &tenant), [6, 5]);
}

#[test]
fn only_the_admin_sets_the_cap_and_only_within_bounds() {
    let env = Env::default();
    let (client, admin, tenant) = setup(&env);

    assert_eq!(
        client.try_set_history_cap(&tenant, &3),
        Err(Ok(MeteringError::Unauthorized))
    );
    for cap in [0, MAX_HISTORY_CAP + 1] {
        assert_eq!(
            client.try_set_history_cap(&admin, &cap),
            Err(Ok(MeteringError::InvalidInput)),
            "{cap}"
        );
    }
    client.set_history_cap(&admin, &1);
    client.set_history_cap(&admin, &MAX_HISTORY_CAP);
}

// ── Pagination ────────────────────────────────────────────────────────────────

#[test]
fn pages_run_newest_first_until_there_is_no_cursor() {
    let env = Env::default();
    let (client, admin, tenant) = setup(&env);
    record(&client, &admin, &tenant, 5);

    let page = client.get_usage_history(&tenant, &Some(2), &None);
    assert_eq!((ids(&page), page.next_cursor), (vec![5, 4], Some(4)));
    let page = client.get_usage_history(&tenant, &Some(2), &page.next_cursor);
    assert_eq!((ids(&page), page.next_cursor), (vec![3, 2], Some(2)));
    let page = client.get_usage_history(&tenant, &Some(2), &page.next_cursor);
    assert_eq!((ids(&page), page.next_cursor), (vec![1], None));
}

#[test]
fn a_page_that_ends_on_the_oldest_entry_has_no_cursor() {
    let env = Env::default();
    let (client, admin, tenant) = setup(&env);
    record(&client, &admin, &tenant, 4);

    let page = client.get_usage_history(&tenant, &Some(4), &None);
    assert_eq!((ids(&page), page.next_cursor), (vec![4, 3, 2, 1], None));
}

#[test]
fn pages_are_stable_while_operations_are_recorded_between_calls() {
    let env = Env::default();
    let (client, admin, tenant) = setup(&env);
    record(&client, &admin, &tenant, 6);

    let first = client.get_usage_history(&tenant, &Some(2), &None);
    let second = client.get_usage_history(&tenant, &Some(2), &first.next_cursor);
    assert_eq!(ids(&second), [4, 3]);

    record(&client, &admin, &tenant, 3);
    env.ledger().set_timestamp(START + 60);

    assert_eq!(
        client.get_usage_history(&tenant, &Some(2), &first.next_cursor),
        second
    );
}

#[test]
fn cursors_survive_eviction() {
    let env = Env::default();
    let (client, admin, tenant) = setup(&env);
    client.set_history_cap(&admin, &4);
    record(&client, &admin, &tenant, 4);
    let cursor = client
        .get_usage_history(&tenant, &Some(2), &None)
        .next_cursor;
    assert_eq!(cursor, Some(3));

    // 1 is evicted: what is left below the cursor is still returned, nothing is repeated.
    record(&client, &admin, &tenant, 1);
    let page = client.get_usage_history(&tenant, &Some(2), &cursor);
    assert_eq!((ids(&page), page.next_cursor), (vec![2], None));

    // Everything below the cursor is evicted.
    record(&client, &admin, &tenant, 4);
    let page = client.get_usage_history(&tenant, &Some(2), &cursor);
    assert_eq!((ids(&page), page.next_cursor), (vec![], None));
}

#[test]
fn limit_and_before_are_validated_like_the_audit_log_endpoint() {
    let env = Env::default();
    let (client, admin, tenant) = setup(&env);
    client.set_history_cap(&admin, &MAX_HISTORY_CAP);
    record(&client, &admin, &tenant, DEFAULT_LIMIT + 1);

    let page = client.get_usage_history(&tenant, &None, &None);
    assert_eq!(page.entries.len(), DEFAULT_LIMIT);
    client.get_usage_history(&tenant, &Some(MAX_LIMIT), &Some(1));

    for (limit, before) in [
        (Some(0), None),
        (Some(MAX_LIMIT + 1), None),
        (None, Some(0)),
    ] {
        assert_eq!(
            client.try_get_usage_history(&tenant, &limit, &before),
            Err(Ok(MeteringError::InvalidInput)),
            "{limit:?} {before:?}"
        );
    }
}

#[test]
fn unknown_tenant_gets_an_empty_page() {
    let env = Env::default();
    let (client, _, _) = setup(&env);

    let page = client.get_usage_history(&Address::generate(&env), &None, &None);
    assert_eq!((ids(&page), page.next_cursor), (vec![], None));
}

// ── Cost ──────────────────────────────────────────────────────────────────────

#[test]
fn recording_adds_no_ledger_write() {
    let env = Env::default();
    let (client, admin, org) = setup(&env);
    let tenants = hierarchy(&env, &client, &admin, &org);

    for op in [OperationType::Read, OperationType::Write] {
        client.record_gas(&admin, &tenants[3], &op);
        // The usage entries of the tenant and its three ancestors, and the caller's auth nonce.
        assert_eq!(env.cost_estimate().resources().write_entries, 5);
    }
}
