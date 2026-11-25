//! Tiered pricing plans (issue #462).
//!
//! A plan bundles the quota a tenant gets and the base charge for a billing cycle. The admin manages the registry
//! with `set_plan`, and assigns a registered tenant to a plan with `assign_plan`. Assigning a different plan is an
//! upgrade or a downgrade: the tenant's quota becomes the new plan's quota immediately, and the usage already recorded
//! in the current cycle is kept.
//!
//! The base charge (`price_per_cycle`) is added to the postpaid invoice of each cycle the tenant is on the plan, even
//! when the tenant used no gas. Prepaid tenants draw on their balance for gas as before, and the base charge does not
//! change that.

use soroban_sdk::{contracttype, symbol_short, Address, Env, Symbol};

use crate::quota::{TenantQuota, TTL_EXTEND_TO, TTL_THRESHOLD};

const PLAN_KEY: Symbol = symbol_short!("PLAN");
const TENANT_PLAN_KEY: Symbol = symbol_short!("TPLAN");

/// A pricing tier: the per-cycle quota and base charge for every tenant on it.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Plan {
    /// Quota applied to a tenant when it is assigned to this plan. All limits are per billing cycle.
    pub quota: TenantQuota,
    /// Base charge per billing cycle, in the same units as gas costs.
    pub price_per_cycle: u64,
}

fn plan_key(plan_id: u32) -> (Symbol, u32) {
    (PLAN_KEY, plan_id)
}

fn tenant_plan_key(tenant: &Address) -> (Symbol, Address) {
    (TENANT_PLAN_KEY, tenant.clone())
}

pub fn set_plan(env: &Env, plan_id: u32, plan: &Plan) {
    let key = plan_key(plan_id);
    env.storage().persistent().set(&key, plan);
    env.storage()
        .persistent()
        .extend_ttl(&key, TTL_THRESHOLD, TTL_EXTEND_TO);
}

pub fn get_plan(env: &Env, plan_id: u32) -> Option<Plan> {
    env.storage().persistent().get(&plan_key(plan_id))
}

pub fn assign(env: &Env, tenant: &Address, plan_id: u32) {
    let key = tenant_plan_key(tenant);
    env.storage().persistent().set(&key, &plan_id);
    env.storage()
        .persistent()
        .extend_ttl(&key, TTL_THRESHOLD, TTL_EXTEND_TO);
}

pub fn tenant_plan(env: &Env, tenant: &Address) -> Option<u32> {
    env.storage().persistent().get(&tenant_plan_key(tenant))
}

/// Base charge the tenant owes for a cycle: the price of its plan, or 0 when it has none.
pub fn tenant_plan_price(env: &Env, tenant: &Address) -> u64 {
    tenant_plan(env, tenant)
        .and_then(|plan_id| get_plan(env, plan_id))
        .map_or(0, |plan| plan.price_per_cycle)
}
