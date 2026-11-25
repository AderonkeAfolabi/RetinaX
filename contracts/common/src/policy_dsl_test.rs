//! Evaluation tests for the policy DSL (`policy_dsl`) and policy engine
//! (`policy_engine`).
//!
//! Coverage:
//! - simple, single-condition policies (every `AttrOperator`, `Allow`/`Deny`)
//! - compound policies (`And`, `Or`, `Not`, nesting, `IfThenElse`, `Unless`)
//! - credential conditions (credential claims surfaced as context attributes,
//!   and delegated credentials via `DelegationCheck`)
//! - time-based conditions (validity windows, business hours, overnight
//!   shifts, day-of-week masks) evaluated through the stored-policy engine
//! - performance of large and deep policy trees, measured with the host
//!   budget so regressions show up as instruction-count changes.

use crate::conflict_resolver::ResolutionStrategy;
use crate::policy_dsl::*;
use crate::policy_engine::{self, evaluate_rule};
use soroban_sdk::testutils::Address as _;
use soroban_sdk::{contract, Address, Env, String, Vec};

// ── Fixtures ────────────────────────────────────────────────────────────────

/// 2023-11-14 (Tuesday) 10:00:00 UTC.
const TUESDAY_10AM: u64 = 1_699_956_000;
const HOUR: u64 = 3_600;
const DAY: u64 = 86_400;

const SUNDAY: u32 = 1 << 0;
const MONDAY: u32 = 1 << 1;
const TUESDAY: u32 = 1 << 2;
const WEDNESDAY: u32 = 1 << 3;
const THURSDAY: u32 = 1 << 4;
const FRIDAY: u32 = 1 << 5;
const SATURDAY: u32 = 1 << 6;
const WEEKDAYS: u32 = MONDAY | TUESDAY | WEDNESDAY | THURSDAY | FRIDAY;

#[contract]
struct MockContract;

/// Storage-backed engine calls need a contract context.
fn with_contract<F: FnOnce() -> R, R>(env: &Env, f: F) -> R {
    let contract_id = env.register(MockContract, ());
    env.as_contract(&contract_id, f)
}

fn s(env: &Env, v: &str) -> String {
    String::from_str(env, v)
}

fn strings(env: &Env, vals: &[&str]) -> Vec<String> {
    let mut out = Vec::new(env);
    for v in vals {
        out.push_back(s(env, v));
    }
    out
}

/// Builds a context from `(key, value)` attribute pairs.
fn ctx_with(env: &Env, timestamp: u64, attrs: &[(&str, &str)]) -> EvalContext {
    let mut keys = Vec::new(env);
    let mut vals = Vec::new(env);
    for (k, v) in attrs {
        keys.push_back(s(env, k));
        vals.push_back(s(env, v));
    }
    EvalContext {
        subject: Address::generate(env),
        resource_id: s(env, "record_1"),
        action: s(env, "read"),
        timestamp,
        attr_keys: keys,
        attr_vals: vals,
    }
}

fn doctor_ctx(env: &Env) -> EvalContext {
    ctx_with(
        env,
        TUESDAY_10AM,
        &[
            ("role", "doctor"),
            ("department", "cardiology"),
            ("clearance", "3"),
        ],
    )
}

fn attr(env: &Env, key: &str, operator: AttrOperator, values: &[&str]) -> PolicyRule {
    PolicyRule::Attribute(AttributeCondition {
        key: s(env, key),
        operator,
        values: strings(env, values),
    })
}

fn temporal(valid_from: u64, valid_until: u64, start: u32, end: u32, days: u32) -> PolicyRule {
    PolicyRule::Temporal(TemporalConstraint {
        valid_from,
        valid_until,
        allowed_hour_start: start,
        allowed_hour_end: end,
        allowed_days_mask: days,
    })
}

fn rules(env: &Env, items: &[PolicyRule]) -> Vec<PolicyRule> {
    let mut out = Vec::new(env);
    for r in items {
        out.push_back(r.clone());
    }
    out
}

fn and(env: &Env, items: &[PolicyRule]) -> PolicyRule {
    PolicyRule::And(rules(env, items))
}

fn or(env: &Env, items: &[PolicyRule]) -> PolicyRule {
    PolicyRule::Or(rules(env, items))
}

fn not(env: &Env, inner: PolicyRule) -> PolicyRule {
    PolicyRule::Not(rules(env, &[inner]))
}

fn policy(
    env: &Env,
    name: &str,
    rule: PolicyRule,
    effect: PolicyEffect,
    priority: u32,
) -> PolicyDefinition {
    PolicyDefinition {
        id: PolicyId {
            name: s(env, name),
            version: 1,
        },
        description: s(env, "policy_dsl_test"),
        rule,
        effect,
        priority,
        enabled: true,
    }
}

fn eval(env: &Env, rule: &PolicyRule, ctx: &EvalContext) -> bool {
    evaluate_rule(env, rule, ctx)
}

// ── Simple policies (single condition) ──────────────────────────────────────

#[test]
fn simple_allow_and_deny_are_constant() {
    let env = Env::default();
    let ctx = doctor_ctx(&env);
    assert!(eval(&env, &PolicyRule::Allow, &ctx));
    assert!(!eval(&env, &PolicyRule::Deny, &ctx));
}

#[test]
fn simple_eq_and_not_eq() {
    let env = Env::default();
    let ctx = doctor_ctx(&env);
    assert!(eval(
        &env,
        &attr(&env, "role", AttrOperator::Eq, &["doctor"]),
        &ctx
    ));
    assert!(!eval(
        &env,
        &attr(&env, "role", AttrOperator::Eq, &["nurse"]),
        &ctx
    ));
    assert!(eval(
        &env,
        &attr(&env, "role", AttrOperator::NotEq, &["nurse"]),
        &ctx
    ));
    assert!(!eval(
        &env,
        &attr(&env, "role", AttrOperator::NotEq, &["doctor"]),
        &ctx
    ));
}

#[test]
fn simple_in_and_not_in() {
    let env = Env::default();
    let ctx = doctor_ctx(&env);
    let depts = ["neurology", "cardiology"];
    assert!(eval(
        &env,
        &attr(&env, "department", AttrOperator::In, &depts),
        &ctx
    ));
    assert!(!eval(
        &env,
        &attr(&env, "department", AttrOperator::NotIn, &depts),
        &ctx
    ));
    assert!(!eval(
        &env,
        &attr(&env, "department", AttrOperator::In, &["oncology"]),
        &ctx
    ));
    assert!(eval(
        &env,
        &attr(&env, "department", AttrOperator::NotIn, &["oncology"]),
        &ctx
    ));
}

#[test]
fn simple_gte_and_lte_on_single_digit_levels() {
    let env = Env::default();
    let ctx = doctor_ctx(&env); // clearance = "3"
    assert!(eval(
        &env,
        &attr(&env, "clearance", AttrOperator::Gte, &["3"]),
        &ctx
    ));
    assert!(eval(
        &env,
        &attr(&env, "clearance", AttrOperator::Gte, &["2"]),
        &ctx
    ));
    assert!(!eval(
        &env,
        &attr(&env, "clearance", AttrOperator::Gte, &["4"]),
        &ctx
    ));
    assert!(eval(
        &env,
        &attr(&env, "clearance", AttrOperator::Lte, &["3"]),
        &ctx
    ));
    assert!(!eval(
        &env,
        &attr(&env, "clearance", AttrOperator::Lte, &["2"]),
        &ctx
    ));
}

#[test]
fn simple_missing_attribute_never_matches() {
    let env = Env::default();
    let ctx = doctor_ctx(&env);
    // Even negative operators fail closed when the attribute is absent.
    for op in [
        AttrOperator::Eq,
        AttrOperator::NotEq,
        AttrOperator::In,
        AttrOperator::NotIn,
        AttrOperator::Gte,
        AttrOperator::Lte,
    ] {
        assert!(
            !eval(&env, &attr(&env, "specialty", op, &["retina"]), &ctx),
            "operator {:?} matched a missing attribute",
            op
        );
    }
}

#[test]
fn simple_empty_value_lists() {
    let env = Env::default();
    let ctx = doctor_ctx(&env);
    assert!(!eval(
        &env,
        &attr(&env, "role", AttrOperator::Eq, &[]),
        &ctx
    ));
    assert!(eval(
        &env,
        &attr(&env, "role", AttrOperator::NotEq, &[]),
        &ctx
    ));
    assert!(!eval(
        &env,
        &attr(&env, "role", AttrOperator::In, &[]),
        &ctx
    ));
    assert!(eval(
        &env,
        &attr(&env, "role", AttrOperator::NotIn, &[]),
        &ctx
    ));
    assert!(!eval(
        &env,
        &attr(&env, "clearance", AttrOperator::Gte, &[]),
        &ctx
    ));
}

#[test]
fn simple_policy_through_engine_permits_and_denies() {
    let env = Env::default();
    with_contract(&env, || {
        let rule = attr(&env, "role", AttrOperator::Eq, &["doctor"]);
        policy_engine::store_policy(
            &env,
            &policy(&env, "doctors", rule, PolicyEffect::Permit, 1),
        );

        let doctor = doctor_ctx(&env);
        assert_eq!(
            policy_engine::evaluate(&env, &doctor).effect,
            PolicyEffect::Permit
        );

        let nurse = ctx_with(&env, TUESDAY_10AM, &[("role", "nurse")]);
        assert_eq!(
            policy_engine::evaluate(&env, &nurse).effect,
            PolicyEffect::Deny
        );
    });
}

// ── Compound policies (AND, OR, NOT) ────────────────────────────────────────

#[test]
fn compound_and_requires_every_child() {
    let env = Env::default();
    let ctx = doctor_ctx(&env);
    let role = attr(&env, "role", AttrOperator::Eq, &["doctor"]);
    let dept = attr(&env, "department", AttrOperator::Eq, &["cardiology"]);
    let wrong_dept = attr(&env, "department", AttrOperator::Eq, &["oncology"]);

    assert!(eval(&env, &and(&env, &[role.clone(), dept]), &ctx));
    assert!(!eval(&env, &and(&env, &[role, wrong_dept]), &ctx));
}

#[test]
fn compound_or_requires_any_child() {
    let env = Env::default();
    let ctx = doctor_ctx(&env);
    let nurse = attr(&env, "role", AttrOperator::Eq, &["nurse"]);
    let doctor = attr(&env, "role", AttrOperator::Eq, &["doctor"]);
    let admin = attr(&env, "role", AttrOperator::Eq, &["admin"]);

    assert!(eval(&env, &or(&env, &[nurse.clone(), doctor]), &ctx));
    assert!(!eval(&env, &or(&env, &[nurse, admin]), &ctx));
}

#[test]
fn compound_not_inverts_child() {
    let env = Env::default();
    let ctx = doctor_ctx(&env);
    let doctor = attr(&env, "role", AttrOperator::Eq, &["doctor"]);
    assert!(!eval(&env, &not(&env, doctor.clone()), &ctx));
    assert!(eval(&env, &not(&env, not(&env, doctor)), &ctx));
}

#[test]
fn compound_empty_operands_have_identity_semantics() {
    let env = Env::default();
    let ctx = doctor_ctx(&env);
    // Empty AND is vacuously true, empty OR is false, malformed NOT fails closed.
    assert!(eval(&env, &PolicyRule::And(Vec::new(&env)), &ctx));
    assert!(!eval(&env, &PolicyRule::Or(Vec::new(&env)), &ctx));
    assert!(!eval(&env, &PolicyRule::Not(Vec::new(&env)), &ctx));
}

#[test]
fn compound_not_only_uses_first_child() {
    let env = Env::default();
    let ctx = doctor_ctx(&env);
    let rule = PolicyRule::Not(rules(&env, &[PolicyRule::Deny, PolicyRule::Allow]));
    assert!(eval(&env, &rule, &ctx));
}

#[test]
fn compound_de_morgan_equivalence() {
    let env = Env::default();
    let a = attr(&env, "role", AttrOperator::Eq, &["doctor"]);
    let b = attr(&env, "department", AttrOperator::Eq, &["cardiology"]);

    let contexts = [
        ctx_with(&env, 0, &[("role", "doctor"), ("department", "cardiology")]),
        ctx_with(&env, 0, &[("role", "doctor"), ("department", "oncology")]),
        ctx_with(&env, 0, &[("role", "nurse"), ("department", "cardiology")]),
        ctx_with(&env, 0, &[("role", "nurse"), ("department", "oncology")]),
    ];

    for ctx in contexts.iter() {
        // NOT(a AND b) == NOT a OR NOT b
        let lhs = not(&env, and(&env, &[a.clone(), b.clone()]));
        let rhs = or(&env, &[not(&env, a.clone()), not(&env, b.clone())]);
        assert_eq!(eval(&env, &lhs, ctx), eval(&env, &rhs, ctx));

        // NOT(a OR b) == NOT a AND NOT b
        let lhs = not(&env, or(&env, &[a.clone(), b.clone()]));
        let rhs = and(&env, &[not(&env, a.clone()), not(&env, b.clone())]);
        assert_eq!(eval(&env, &lhs, ctx), eval(&env, &rhs, ctx));
    }
}

#[test]
fn compound_nested_role_or_department_and_not_suspended() {
    let env = Env::default();
    // (role == doctor OR department IN [emergency]) AND NOT(status == suspended)
    let rule = and(
        &env,
        &[
            or(
                &env,
                &[
                    attr(&env, "role", AttrOperator::Eq, &["doctor"]),
                    attr(&env, "department", AttrOperator::In, &["emergency"]),
                ],
            ),
            not(&env, attr(&env, "status", AttrOperator::Eq, &["suspended"])),
        ],
    );

    let active_doctor = ctx_with(&env, 0, &[("role", "doctor"), ("status", "active")]);
    let er_nurse = ctx_with(&env, 0, &[("role", "nurse"), ("department", "emergency")]);
    let suspended_doctor = ctx_with(&env, 0, &[("role", "doctor"), ("status", "suspended")]);
    let ward_nurse = ctx_with(&env, 0, &[("role", "nurse"), ("department", "ward")]);

    assert!(eval(&env, &rule, &active_doctor));
    assert!(eval(&env, &rule, &er_nurse));
    assert!(!eval(&env, &rule, &suspended_doctor));
    assert!(!eval(&env, &rule, &ward_nurse));
}

#[test]
fn compound_if_then_else_and_unless_combine_with_boolean_ops() {
    let env = Env::default();
    // IF role == doctor THEN dept == cardiology ELSE role == admin,
    // UNLESS status == suspended.
    let ite = PolicyRule::IfThenElse(rules(
        &env,
        &[
            attr(&env, "role", AttrOperator::Eq, &["doctor"]),
            attr(&env, "department", AttrOperator::Eq, &["cardiology"]),
            attr(&env, "role", AttrOperator::Eq, &["admin"]),
        ],
    ));
    let rule = PolicyRule::Unless(rules(
        &env,
        &[ite, attr(&env, "status", AttrOperator::Eq, &["suspended"])],
    ));

    let cardio_doctor = ctx_with(&env, 0, &[("role", "doctor"), ("department", "cardiology")]);
    let onc_doctor = ctx_with(&env, 0, &[("role", "doctor"), ("department", "oncology")]);
    let admin = ctx_with(&env, 0, &[("role", "admin")]);
    let suspended_admin = ctx_with(&env, 0, &[("role", "admin"), ("status", "suspended")]);

    assert!(eval(&env, &rule, &cardio_doctor));
    assert!(!eval(&env, &rule, &onc_doctor));
    assert!(eval(&env, &rule, &admin));
    assert!(!eval(&env, &rule, &suspended_admin));

    // Malformed arity fails closed.
    let short_ite = PolicyRule::IfThenElse(rules(&env, &[PolicyRule::Allow, PolicyRule::Allow]));
    let short_unless = PolicyRule::Unless(rules(&env, &[PolicyRule::Allow]));
    assert!(!eval(&env, &short_ite, &admin));
    assert!(!eval(&env, &short_unless, &admin));
}

#[test]
fn compound_policies_resolve_under_each_strategy() {
    let env = Env::default();
    with_contract(&env, || {
        // Permit doctors (priority 2), explicitly deny the oncology dept (priority 1).
        let permit_doctors = policy(
            &env,
            "permit_doctors",
            attr(&env, "role", AttrOperator::Eq, &["doctor"]),
            PolicyEffect::Permit,
            2,
        );
        let deny_oncology = policy(
            &env,
            "deny_oncology",
            attr(&env, "department", AttrOperator::Eq, &["oncology"]),
            PolicyEffect::Deny,
            1,
        );
        policy_engine::store_policy(&env, &permit_doctors);
        policy_engine::store_policy(&env, &deny_oncology);

        let onc_doctor = ctx_with(&env, 0, &[("role", "doctor"), ("department", "oncology")]);
        let cardio_doctor = ctx_with(&env, 0, &[("role", "doctor"), ("department", "cardiology")]);

        // Default strategy: DenyOverride.
        let r = policy_engine::evaluate(&env, &onc_doctor);
        assert_eq!(r.effect, PolicyEffect::Deny);
        assert_eq!(
            r.winning_policy.get(0).unwrap().name,
            s(&env, "deny_oncology")
        );
        assert_eq!(
            policy_engine::evaluate(&env, &cardio_doctor).effect,
            PolicyEffect::Permit
        );

        policy_engine::set_resolution_strategy(&env, ResolutionStrategy::PermitOverride);
        assert_eq!(
            policy_engine::evaluate(&env, &onc_doctor).effect,
            PolicyEffect::Permit
        );

        // FirstApplicable picks the lowest priority number (deny_oncology).
        policy_engine::set_resolution_strategy(&env, ResolutionStrategy::FirstApplicable);
        assert_eq!(
            policy_engine::evaluate(&env, &onc_doctor).effect,
            PolicyEffect::Deny
        );
        assert_eq!(
            policy_engine::evaluate(&env, &cardio_doctor).effect,
            PolicyEffect::Permit
        );
    });
}

#[test]
fn compound_disabled_policies_are_skipped() {
    let env = Env::default();
    with_contract(&env, || {
        let mut deny_all = policy(&env, "deny_all", PolicyRule::Allow, PolicyEffect::Deny, 0);
        deny_all.enabled = false;
        policy_engine::store_policy(&env, &deny_all);
        policy_engine::store_policy(
            &env,
            &policy(
                &env,
                "allow_all",
                PolicyRule::Allow,
                PolicyEffect::Permit,
                1,
            ),
        );

        let ctx = doctor_ctx(&env);
        assert_eq!(
            policy_engine::evaluate(&env, &ctx).effect,
            PolicyEffect::Permit
        );
        assert_eq!(policy_engine::simulate(&env, &ctx).evaluated_count, 1);
    });
}

// ── Credential conditions ───────────────────────────────────────────────────
//
// The DSL has no dedicated credential node. Credential claims (type, status,
// issuer, assurance level) are surfaced to the engine as subject attributes,
// and delegated credentials are expressed with `DelegationCheck`.

fn credential_rule(env: &Env) -> PolicyRule {
    and(
        env,
        &[
            attr(
                env,
                "cred_type",
                AttrOperator::In,
                &["medical_license", "optometry_license"],
            ),
            attr(env, "cred_status", AttrOperator::Eq, &["active"]),
            attr(
                env,
                "cred_issuer",
                AttrOperator::In,
                &["state_board", "national_board"],
            ),
            attr(env, "cred_level", AttrOperator::Gte, &["2"]),
        ],
    )
}

#[test]
fn credential_valid_license_is_permitted() {
    let env = Env::default();
    let ctx = ctx_with(
        &env,
        TUESDAY_10AM,
        &[
            ("cred_type", "medical_license"),
            ("cred_status", "active"),
            ("cred_issuer", "state_board"),
            ("cred_level", "3"),
        ],
    );
    assert!(eval(&env, &credential_rule(&env), &ctx));
}

#[test]
fn credential_revoked_untrusted_or_low_level_is_denied() {
    let env = Env::default();
    let base = [
        ("cred_type", "medical_license"),
        ("cred_status", "active"),
        ("cred_issuer", "state_board"),
        ("cred_level", "3"),
    ];
    let overrides = [
        ("cred_status", "revoked"),
        ("cred_status", "suspended"),
        ("cred_issuer", "unknown_issuer"),
        ("cred_type", "student_id"),
        ("cred_level", "1"),
    ];

    for (key, bad_val) in overrides {
        let mut attrs = alloc::vec::Vec::new();
        for (k, v) in base {
            attrs.push((k, if k == key { bad_val } else { v }));
        }
        let ctx = ctx_with(&env, TUESDAY_10AM, &attrs);
        assert!(
            !eval(&env, &credential_rule(&env), &ctx),
            "credential with {}={} should be denied",
            key,
            bad_val
        );
    }
}

#[test]
fn credential_missing_claims_are_denied() {
    let env = Env::default();
    let ctx = ctx_with(
        &env,
        TUESDAY_10AM,
        &[("cred_type", "medical_license"), ("cred_status", "active")],
    );
    assert!(!eval(&env, &credential_rule(&env), &ctx));
}

fn delegation_chain(env: &Env, expires_at: u64) -> DelegationChain {
    let hospital = Address::generate(env);
    let doctor = Address::generate(env);
    let resident = Address::generate(env);
    let mut links = Vec::new(env);
    links.push_back(DelegationLink {
        delegator: hospital,
        delegatee: doctor.clone(),
        scoped_permissions: strings(env, &["read", "write"]),
        expires_at: 0,
    });
    links.push_back(DelegationLink {
        delegator: doctor,
        delegatee: resident,
        scoped_permissions: strings(env, &["read"]),
        expires_at,
    });
    DelegationChain { links }
}

#[test]
fn credential_delegated_access_requires_live_chain() {
    let env = Env::default();
    let ctx = ctx_with(&env, TUESDAY_10AM, &[("cred_status", "active")]);

    // Delegated credential AND the holder's own credential is active.
    let live = and(
        &env,
        &[
            PolicyRule::DelegationCheck(delegation_chain(&env, TUESDAY_10AM + HOUR)),
            attr(&env, "cred_status", AttrOperator::Eq, &["active"]),
        ],
    );
    assert!(eval(&env, &live, &ctx));

    // The resident's delegated link expired one second before evaluation.
    let expired = PolicyRule::DelegationCheck(delegation_chain(&env, TUESDAY_10AM - 1));
    assert!(!eval(&env, &expired, &ctx));

    // An empty chain never grants anything.
    let empty = PolicyRule::DelegationCheck(DelegationChain {
        links: Vec::new(&env),
    });
    assert!(!eval(&env, &empty, &ctx));
}

#[test]
fn credential_policy_or_delegation_through_engine() {
    let env = Env::default();
    with_contract(&env, || {
        // Permit if the subject holds a valid license OR has a live delegation.
        let rule = or(
            &env,
            &[
                credential_rule(&env),
                PolicyRule::DelegationCheck(delegation_chain(&env, TUESDAY_10AM + DAY)),
            ],
        );
        policy_engine::store_policy(
            &env,
            &policy(&env, "licensed_or_delegated", rule, PolicyEffect::Permit, 1),
        );

        // No license claims, but the delegation is live.
        let ctx = ctx_with(&env, TUESDAY_10AM, &[("role", "resident")]);
        assert_eq!(
            policy_engine::evaluate(&env, &ctx).effect,
            PolicyEffect::Permit
        );

        // A day later the delegation has lapsed and there is still no license.
        let later = ctx_with(&env, TUESDAY_10AM + 2 * DAY, &[("role", "resident")]);
        assert_eq!(
            policy_engine::evaluate(&env, &later).effect,
            PolicyEffect::Deny
        );
    });
}

// ── Time-based conditions ───────────────────────────────────────────────────

#[test]
fn time_validity_window_boundaries_are_inclusive() {
    let env = Env::default();
    let from = TUESDAY_10AM;
    let until = TUESDAY_10AM + 7 * DAY;
    let rule = temporal(from, until, 0, 23, 0);

    let at = |t: u64| ctx_with(&env, t, &[]);
    assert!(!eval(&env, &rule, &at(from - 1)));
    assert!(eval(&env, &rule, &at(from)));
    assert!(eval(&env, &rule, &at(until)));
    assert!(!eval(&env, &rule, &at(until + 1)));
}

#[test]
fn time_open_ended_bounds() {
    let env = Env::default();
    let at = |t: u64| ctx_with(&env, t, &[]);

    let not_before = temporal(TUESDAY_10AM, 0, 0, 23, 0);
    assert!(!eval(&env, &not_before, &at(TUESDAY_10AM - 1)));
    assert!(eval(&env, &not_before, &at(TUESDAY_10AM + 365 * DAY)));

    let not_after = temporal(0, TUESDAY_10AM, 0, 23, 0);
    assert!(eval(&env, &not_after, &at(0)));
    assert!(!eval(&env, &not_after, &at(TUESDAY_10AM + 1)));
}

#[test]
fn time_business_hours_on_weekdays() {
    let env = Env::default();
    let rule = temporal(0, 0, 9, 17, WEEKDAYS);
    let at = |t: u64| ctx_with(&env, t, &[]);
    let tuesday_midnight = TUESDAY_10AM - 10 * HOUR;

    assert!(eval(&env, &rule, &at(tuesday_midnight + 9 * HOUR)));
    assert!(eval(
        &env,
        &rule,
        &at(tuesday_midnight + 17 * HOUR + 59 * 60)
    ));
    assert!(!eval(
        &env,
        &rule,
        &at(tuesday_midnight + 8 * HOUR + 59 * 60)
    ));
    assert!(!eval(&env, &rule, &at(tuesday_midnight + 18 * HOUR)));

    // Saturday and Sunday at 10 AM are outside the weekday mask.
    assert!(!eval(&env, &rule, &at(TUESDAY_10AM + 4 * DAY)));
    assert!(!eval(&env, &rule, &at(TUESDAY_10AM + 5 * DAY)));
    // Monday at 10 AM is allowed.
    assert!(eval(&env, &rule, &at(TUESDAY_10AM + 6 * DAY)));
}

#[test]
fn time_day_mask_maps_every_weekday() {
    let env = Env::default();
    // TUESDAY_10AM + n days walks Tue, Wed, Thu, Fri, Sat, Sun, Mon.
    let order = [
        TUESDAY, WEDNESDAY, THURSDAY, FRIDAY, SATURDAY, SUNDAY, MONDAY,
    ];
    for (offset, day_bit) in order.iter().enumerate() {
        let ctx = ctx_with(&env, TUESDAY_10AM + offset as u64 * DAY, &[]);
        assert!(eval(&env, &temporal(0, 0, 0, 23, *day_bit), &ctx));
        assert!(!eval(&env, &temporal(0, 0, 0, 23, 0x7F & !day_bit), &ctx));
    }
}

#[test]
fn time_overnight_shift_wraps_midnight() {
    let env = Env::default();
    let rule = temporal(0, 0, 22, 6, 0);
    let midnight = TUESDAY_10AM - 10 * HOUR;
    for hour in [22u64, 23, 0, 3, 6] {
        let t = if hour >= 22 {
            midnight + hour * HOUR
        } else {
            midnight + DAY + hour * HOUR
        };
        assert!(eval(&env, &rule, &ctx_with(&env, t, &[])), "hour {}", hour);
    }
    for hour in [7u64, 12, 21] {
        assert!(
            !eval(&env, &rule, &ctx_with(&env, midnight + hour * HOUR, &[])),
            "hour {}",
            hour
        );
    }
}

#[test]
fn time_combined_with_role_through_engine_and_simulation() {
    let env = Env::default();
    with_contract(&env, || {
        // Doctors may read during weekday business hours; on-call staff any time.
        let rule = or(
            &env,
            &[
                and(
                    &env,
                    &[
                        attr(&env, "role", AttrOperator::Eq, &["doctor"]),
                        temporal(0, 0, 9, 17, WEEKDAYS),
                    ],
                ),
                attr(&env, "on_call", AttrOperator::Eq, &["true"]),
            ],
        );
        policy_engine::store_policy(&env, &policy(&env, "hours", rule, PolicyEffect::Permit, 1));

        let weekday = ctx_with(&env, TUESDAY_10AM, &[("role", "doctor")]);
        let saturday = ctx_with(&env, TUESDAY_10AM + 4 * DAY, &[("role", "doctor")]);
        let saturday_on_call = ctx_with(
            &env,
            TUESDAY_10AM + 4 * DAY,
            &[("role", "doctor"), ("on_call", "true")],
        );

        assert_eq!(
            policy_engine::simulate(&env, &weekday).verdict,
            SimulationVerdict::Permitted
        );
        assert_eq!(
            policy_engine::simulate(&env, &saturday).verdict,
            SimulationVerdict::Denied
        );
        assert_eq!(
            policy_engine::evaluate(&env, &saturday_on_call).effect,
            PolicyEffect::Permit
        );
    });
}

#[test]
fn time_cached_result_is_reused_until_policies_change() {
    let env = Env::default();
    with_contract(&env, || {
        policy_engine::store_policy(
            &env,
            &policy(
                &env,
                "weekdays",
                temporal(0, 0, 0, 23, WEEKDAYS),
                PolicyEffect::Permit,
                1,
            ),
        );
        let hint = s(&env, "subject:read");

        let weekday = ctx_with(&env, TUESDAY_10AM, &[]);
        let weekend = ctx_with(&env, TUESDAY_10AM + 4 * DAY, &[]);
        assert_eq!(
            policy_engine::evaluate_cached(&env, &weekday, &hint).effect,
            PolicyEffect::Permit
        );
        // Callers own the cache hint: the cached result is served for the
        // same hint even though the new timestamp would evaluate differently.
        assert_eq!(
            policy_engine::evaluate_cached(&env, &weekend, &hint).effect,
            PolicyEffect::Permit
        );

        // Any policy write bumps the cache generation and forces re-evaluation.
        policy_engine::store_policy(
            &env,
            &policy(&env, "noop", PolicyRule::Deny, PolicyEffect::Deny, 5),
        );
        assert_eq!(
            policy_engine::evaluate_cached(&env, &weekend, &hint).effect,
            PolicyEffect::Deny
        );
    });
}

// ── Performance with complex policy trees ───────────────────────────────────

/// Builds a full binary tree of alternating AND/OR nodes whose leaves are
/// attribute and temporal conditions that all evaluate to `true` for
/// `doctor_ctx`.
fn balanced_tree(env: &Env, depth: u32, leaf_counter: &mut u32) -> PolicyRule {
    if depth == 0 {
        *leaf_counter += 1;
        return match *leaf_counter % 3 {
            0 => attr(env, "role", AttrOperator::Eq, &["doctor"]),
            1 => attr(
                env,
                "department",
                AttrOperator::In,
                &["neurology", "cardiology"],
            ),
            _ => temporal(0, 0, 9, 17, WEEKDAYS),
        };
    }
    let left = balanced_tree(env, depth - 1, leaf_counter);
    let right = balanced_tree(env, depth - 1, leaf_counter);
    if depth % 2 == 0 {
        and(env, &[left, right])
    } else {
        or(env, &[left, right])
    }
}

fn measure<F: FnOnce() -> bool>(env: &Env, f: F) -> (bool, u64) {
    env.cost_estimate().budget().reset_unlimited();
    let before = env.cost_estimate().budget().cpu_instruction_cost();
    let result = f();
    let after = env.cost_estimate().budget().cpu_instruction_cost();
    (result, after - before)
}

/// Default Soroban per-transaction CPU limit.
const TX_CPU_LIMIT: u64 = 100_000_000;

#[test]
fn perf_balanced_tree_fits_transaction_budget() {
    let env = Env::default();
    let ctx = doctor_ctx(&env);
    let mut leaves = 0;
    // 2^8 = 256 leaves, 511 nodes.
    let tree = balanced_tree(&env, 8, &mut leaves);
    assert_eq!(leaves, 256);

    let (result, cost) = measure(&env, || eval(&env, &tree, &ctx));
    assert!(result);
    assert!(
        cost < TX_CPU_LIMIT / 2,
        "256-leaf policy tree used {} CPU instructions",
        cost
    );
}

#[test]
fn perf_wide_and_scales_linearly() {
    let env = Env::default();
    let ctx = doctor_ctx(&env);
    let leaf = attr(
        &env,
        "department",
        AttrOperator::In,
        &["neurology", "cardiology"],
    );

    let wide = |n: usize| {
        let mut children = Vec::new(&env);
        for _ in 0..n {
            children.push_back(leaf.clone());
        }
        PolicyRule::And(children)
    };
    let small = wide(50);
    let large = wide(200);

    let (r_small, c_small) = measure(&env, || eval(&env, &small, &ctx));
    let (r_large, c_large) = measure(&env, || eval(&env, &large, &ctx));
    assert!(r_small && r_large);
    // 4x the children should cost roughly 4x, never super-linear.
    assert!(
        c_large < c_small * 6,
        "200 children cost {} vs 50 children {}",
        c_large,
        c_small
    );
    assert!(c_large < TX_CPU_LIMIT / 2);
}

#[test]
fn perf_short_circuit_skips_expensive_subtrees() {
    let env = Env::default();
    let ctx = doctor_ctx(&env);
    let mut leaves = 0;
    let expensive = balanced_tree(&env, 7, &mut leaves);

    // OR: a true first child must skip the expensive subtree.
    let or_fast = or(&env, &[PolicyRule::Allow, expensive.clone()]);
    let or_slow = or(&env, &[PolicyRule::Deny, expensive.clone()]);
    // AND: a false first child must skip the expensive subtree.
    let and_fast = and(&env, &[PolicyRule::Deny, expensive.clone()]);
    let and_slow = and(&env, &[PolicyRule::Allow, expensive]);

    let (r1, c_or_fast) = measure(&env, || eval(&env, &or_fast, &ctx));
    let (r2, c_or_slow) = measure(&env, || eval(&env, &or_slow, &ctx));
    let (r3, c_and_fast) = measure(&env, || eval(&env, &and_fast, &ctx));
    let (r4, c_and_slow) = measure(&env, || eval(&env, &and_slow, &ctx));

    assert!(r1 && r2 && !r3 && r4);
    assert!(c_or_fast * 10 < c_or_slow, "{} vs {}", c_or_fast, c_or_slow);
    assert!(
        c_and_fast * 10 < c_and_slow,
        "{} vs {}",
        c_and_fast,
        c_and_slow
    );
}

#[test]
fn perf_deeply_nested_not_chain() {
    let env = Env::default();
    let ctx = doctor_ctx(&env);
    // 30 nested NOTs around a true leaf: even depth => true.
    let mut rule = attr(&env, "role", AttrOperator::Eq, &["doctor"]);
    for _ in 0..30 {
        rule = not(&env, rule);
    }
    let (result, cost) = measure(&env, || eval(&env, &rule, &ctx));
    assert!(result);
    assert!(cost < TX_CPU_LIMIT / 10, "nested NOT chain used {}", cost);
}

#[test]
fn perf_many_stored_policies_evaluate_within_budget() {
    let env = Env::default();
    with_contract(&env, || {
        let ctx = doctor_ctx(&env);
        let names = [
            "p00", "p01", "p02", "p03", "p04", "p05", "p06", "p07", "p08", "p09", "p10", "p11",
            "p12", "p13", "p14", "p15", "p16", "p17", "p18", "p19",
        ];
        for (i, name) in names.iter().enumerate() {
            let mut leaves = 0;
            let tree = balanced_tree(&env, 4, &mut leaves);
            policy_engine::store_policy(
                &env,
                &policy(&env, name, tree, PolicyEffect::Permit, i as u32),
            );
        }
        assert_eq!(policy_engine::list_policies(&env).len(), names.len() as u32);

        env.cost_estimate().budget().reset_unlimited();
        let before = env.cost_estimate().budget().cpu_instruction_cost();
        let sim = policy_engine::simulate(&env, &ctx);
        let cost = env.cost_estimate().budget().cpu_instruction_cost() - before;

        assert_eq!(sim.verdict, SimulationVerdict::Permitted);
        assert_eq!(sim.evaluated_count, names.len() as u32);
        assert!(cost < TX_CPU_LIMIT, "20 stored policies used {}", cost);
    });
}
