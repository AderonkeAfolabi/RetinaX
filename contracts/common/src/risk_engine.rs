//! Risk scoring primitives for progressive authorization.
//!
//! The engine combines operation intent, data sensitivity, runtime context,
//! and behavioral anomalies to produce a bounded risk score (0..100).
//!
//! # Scoring algorithm
//!
//! ## 1. Base score
//!
//! `base = action_weight + sensitivity_weight`
//!
//! | Action | Weight | Sensitivity | Weight |
//! |--------|--------|-------------|--------|
//! | Read | 5 | Public | 5 |
//! | Write | 20 | Internal | 15 |
//! | Share | 25 | Sensitive | 30 |
//! | Delete | 35 | Restricted | 45 |
//! | AdminChange | 40 | | |
//! | EmergencyOverride | 45 | | |
//!
//! ## 2. Context adjustments
//!
//! Every adjustment is additive:
//!
//! | Context flag | Adjustment |
//! |--------------|------------|
//! | `off_hours` | +8 |
//! | `unusual_location` | +10 |
//! | `unusual_frequency` | +12 |
//! | `recent_auth_failures` | +3 per failure, capped at 6 failures (+18) |
//! | `emergency_signal` | +12 |
//!
//! The result is clamped to 100: the lowest possible score is 10 (Read +
//! Public) and the highest is 100.
//!
//! ## 3. Behavioral adjustment
//!
//! [`evaluate_risk`] tracks state per `(actor, operation)` inside a one-hour
//! window. The first 8 evaluations add nothing. From the 9th evaluation on,
//! each call adds `2 * (count - 8)` and records one anomaly hit. More than 2
//! recent auth failures also add 6 and one anomaly hit on every call. When
//! the window expires, count and anomaly hits reset. All arithmetic is
//! saturating and `final_score = min(base + adjustment, 100)`.
//!
//! ## 4. Thresholds and access denial
//!
//! [`crate::progressive_auth::default_policy`] maps scores to auth levels:
//! 0-24 = Level1, 25-49 = Level2, 50-74 = Level3, 75-100 = Level4. Level3
//! requires a matching executable multisig proposal; Level4 additionally
//! requires a verified ZK proof. Missing session, mismatched auth start,
//! unmet delay, or missing step-up evidence denies access.

use soroban_sdk::{contracttype, symbol_short, Address, Env, Symbol};

const RISK_BEHAVIOR: Symbol = symbol_short!("RSK_BEH");
const BEHAVIOR_WINDOW_SECONDS: u64 = 3_600;
const BEHAVIOR_ANOMALY_THRESHOLD: u32 = 8;

/// Operation type influences baseline risk.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum ActionType {
    Read = 1,
    Write = 2,
    Share = 3,
    Delete = 4,
    AdminChange = 5,
    EmergencyOverride = 6,
}

/// Sensitivity classification for data touched by the action.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum DataSensitivity {
    Public = 1,
    Internal = 2,
    Sensitive = 3,
    Restricted = 4,
}

/// Runtime context dimensions that can increase risk.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RiskContext {
    pub off_hours: bool,
    pub unusual_location: bool,
    pub unusual_frequency: bool,
    pub recent_auth_failures: u32,
    pub emergency_signal: bool,
}

/// Input envelope for risk scoring.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OperationRiskInput {
    pub actor: Address,
    pub operation: Symbol,
    pub action: ActionType,
    pub sensitivity: DataSensitivity,
    pub context: RiskContext,
}

/// Persisted behavior tracker per actor + operation window.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BehaviorState {
    pub window_start: u64,
    pub count: u32,
    pub anomaly_hits: u32,
}

/// Score decomposition for auditability.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RiskAssessment {
    pub base_score: u32,
    pub behavioral_adjustment: u32,
    pub final_score: u32,
}

pub type RiskScoringFn = fn(&OperationRiskInput) -> u32;

fn behavior_key(actor: &Address, operation: Symbol) -> (Symbol, Address, Symbol) {
    (RISK_BEHAVIOR, actor.clone(), operation)
}

fn base_weight_for_action(action: &ActionType) -> u32 {
    match action {
        ActionType::Read => 5,
        ActionType::Write => 20,
        ActionType::Share => 25,
        ActionType::Delete => 35,
        ActionType::AdminChange => 40,
        ActionType::EmergencyOverride => 45,
    }
}

fn base_weight_for_sensitivity(sensitivity: &DataSensitivity) -> u32 {
    match sensitivity {
        DataSensitivity::Public => 5,
        DataSensitivity::Internal => 15,
        DataSensitivity::Sensitive => 30,
        DataSensitivity::Restricted => 45,
    }
}

/// Compute the base score plus context adjustments, clamped to 0..100.
pub fn default_risk_score(input: &OperationRiskInput) -> u32 {
    let mut score =
        base_weight_for_action(&input.action) + base_weight_for_sensitivity(&input.sensitivity);
    if input.context.off_hours {
        score += 8;
    }
    if input.context.unusual_location {
        score += 10;
    }
    if input.context.unusual_frequency {
        score += 12;
    }
    score += input.context.recent_auth_failures.min(6) * 3;
    if input.context.emergency_signal {
        score += 12;
    }
    score.min(100)
}

/// Score risk and update behavioral state to dynamically adjust anomaly pressure.
///
/// Uses `scorer` when supplied, otherwise [`default_risk_score`]. The returned
/// [`RiskAssessment`] exposes the base score, behavioral adjustment, and the
/// clamped final score so access decisions and audits can use each component.
pub fn evaluate_risk(
    env: &Env,
    input: &OperationRiskInput,
    scorer: Option<RiskScoringFn>,
) -> RiskAssessment {
    let score_fn = scorer.unwrap_or(default_risk_score);
    let base_score = score_fn(input).min(100);

    let now = env.ledger().timestamp();
    let key = behavior_key(&input.actor, input.operation.clone());
    let mut state: BehaviorState = env
        .storage()
        .persistent()
        .get(&key)
        .unwrap_or(BehaviorState {
            window_start: now,
            count: 0,
            anomaly_hits: 0,
        });

    if now.saturating_sub(state.window_start) > BEHAVIOR_WINDOW_SECONDS {
        state.window_start = now;
        state.count = 0;
        state.anomaly_hits = 0;
    }

    state.count = state.count.saturating_add(1);

    let mut behavioral_adjustment: u32 = 0;
    if state.count > BEHAVIOR_ANOMALY_THRESHOLD {
        state.anomaly_hits = state.anomaly_hits.saturating_add(1);
        behavioral_adjustment = behavioral_adjustment
            .saturating_add((state.count - BEHAVIOR_ANOMALY_THRESHOLD).saturating_mul(2));
    }

    if input.context.recent_auth_failures > 2 {
        state.anomaly_hits = state.anomaly_hits.saturating_add(1);
        behavioral_adjustment = behavioral_adjustment.saturating_add(6);
    }

    env.storage().persistent().set(&key, &state);

    RiskAssessment {
        base_score,
        behavioral_adjustment,
        final_score: base_score.saturating_add(behavioral_adjustment).min(100),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::progressive_auth::{
        default_policy, enforce_for_risk, level_for_score, AuthLevel, ProgressiveAuthError,
        ProgressiveAuthPolicy,
    };
    use crate::{multisig, session};
    use soroban_sdk::{
        contract, contractimpl, symbol_short, testutils::Address as _, testutils::Ledger as _,
        BytesN, Env, Vec,
    };

    #[contract]
    struct TestContract;

    #[contractimpl]
    impl TestContract {
        pub fn noop(_env: Env) {}
    }

    fn quiet_context() -> RiskContext {
        RiskContext {
            off_hours: false,
            unusual_location: false,
            unusual_frequency: false,
            recent_auth_failures: 0,
            emergency_signal: false,
        }
    }

    fn input_for(
        env: &Env,
        operation: Symbol,
        action: ActionType,
        sensitivity: DataSensitivity,
        context: RiskContext,
    ) -> OperationRiskInput {
        OperationRiskInput {
            actor: Address::generate(env),
            operation,
            action,
            sensitivity,
            context,
        }
    }

    fn behavior_state(
        env: &Env,
        contract_id: &Address,
        input: &OperationRiskInput,
    ) -> BehaviorState {
        let key = behavior_key(&input.actor, input.operation.clone());
        env.as_contract(contract_id, || {
            env.storage()
                .persistent()
                .get(&key)
                .expect("behavior state should be stored")
        })
    }

    fn constant_42(_input: &OperationRiskInput) -> u32 {
        42
    }

    fn excessive_score(_input: &OperationRiskInput) -> u32 {
        u32::MAX
    }

    #[test]
    fn base_score_is_action_plus_sensitivity_weight() {
        let env = Env::default();
        let actions = [
            (ActionType::Read, 5),
            (ActionType::Write, 20),
            (ActionType::Share, 25),
            (ActionType::Delete, 35),
            (ActionType::AdminChange, 40),
            (ActionType::EmergencyOverride, 45),
        ];
        let sensitivities = [
            (DataSensitivity::Public, 5),
            (DataSensitivity::Internal, 15),
            (DataSensitivity::Sensitive, 30),
            (DataSensitivity::Restricted, 45),
        ];

        for (action, action_weight) in actions {
            for (sensitivity, sensitivity_weight) in sensitivities.iter().cloned() {
                let input = input_for(
                    &env,
                    symbol_short!("OP"),
                    action.clone(),
                    sensitivity.clone(),
                    quiet_context(),
                );
                assert_eq!(
                    default_risk_score(&input),
                    action_weight + sensitivity_weight
                );
            }
        }
    }

    #[test]
    fn context_modifiers_are_additive_and_failure_bonus_is_capped() {
        let env = Env::default();
        let quiet = input_for(
            &env,
            symbol_short!("OP"),
            ActionType::Read,
            DataSensitivity::Public,
            quiet_context(),
        );
        assert_eq!(default_risk_score(&quiet), 10);

        let cases = [
            (true, false, false, 0, false, 18),
            (false, true, false, 0, false, 20),
            (false, false, true, 0, false, 22),
            (false, false, false, 1, false, 13),
            (false, false, false, 6, false, 28),
            (false, false, false, 7, false, 28),
            (false, false, false, 12, false, 28),
            (false, false, false, 0, true, 22),
        ];
        for (off_hours, location, frequency, failures, emergency, expected) in cases {
            let input = input_for(
                &env,
                symbol_short!("OP"),
                ActionType::Read,
                DataSensitivity::Public,
                RiskContext {
                    off_hours,
                    unusual_location: location,
                    unusual_frequency: frequency,
                    recent_auth_failures: failures,
                    emergency_signal: emergency,
                },
            );
            assert_eq!(default_risk_score(&input), expected);
        }
    }

    #[test]
    fn combined_inputs_are_clamped_to_100() {
        let env = Env::default();
        let input = input_for(
            &env,
            symbol_short!("OP"),
            ActionType::EmergencyOverride,
            DataSensitivity::Restricted,
            RiskContext {
                off_hours: true,
                unusual_location: true,
                unusual_frequency: true,
                recent_auth_failures: 100,
                emergency_signal: true,
            },
        );
        assert_eq!(default_risk_score(&input), 100);

        let contract_id = env.register(TestContract, ());
        let assessment = env.as_contract(&contract_id, || evaluate_risk(&env, &input, None));
        assert_eq!(assessment.base_score, 100);
        assert_eq!(assessment.final_score, 100);
    }

    #[test]
    fn custom_scorer_is_used_and_its_output_is_clamped() {
        let env = Env::default();
        let contract_id = env.register(TestContract, ());
        let input = input_for(
            &env,
            symbol_short!("OP"),
            ActionType::Read,
            DataSensitivity::Public,
            quiet_context(),
        );

        let custom = env.as_contract(&contract_id, || {
            evaluate_risk(&env, &input, Some(constant_42))
        });
        assert_eq!(custom.base_score, 42);
        assert_eq!(custom.behavioral_adjustment, 0);
        assert_eq!(custom.final_score, 42);

        let excessive = env.as_contract(&contract_id, || {
            evaluate_risk(&env, &input, Some(excessive_score))
        });
        assert_eq!(excessive.base_score, 100);
        assert_eq!(excessive.final_score, 100);
    }

    #[test]
    fn behavioral_adjustment_starts_after_eight_evaluations() {
        let env = Env::default();
        let contract_id = env.register(TestContract, ());
        let input = input_for(
            &env,
            symbol_short!("OP"),
            ActionType::Write,
            DataSensitivity::Sensitive,
            quiet_context(),
        );

        for index in 1..=8 {
            let assessment = env.as_contract(&contract_id, || evaluate_risk(&env, &input, None));
            assert_eq!(assessment.behavioral_adjustment, 0, "evaluation {index}");
            assert_eq!(assessment.final_score, assessment.base_score);
        }
        let state = behavior_state(&env, &contract_id, &input);
        assert_eq!(state.count, 8);
        assert_eq!(state.anomaly_hits, 0);

        let ninth = env.as_contract(&contract_id, || evaluate_risk(&env, &input, None));
        assert_eq!(ninth.behavioral_adjustment, 2);
        let state = behavior_state(&env, &contract_id, &input);
        assert_eq!(state.count, 9);
        assert_eq!(state.anomaly_hits, 1);

        let tenth = env.as_contract(&contract_id, || evaluate_risk(&env, &input, None));
        assert_eq!(tenth.behavioral_adjustment, 4);
        assert_eq!(tenth.final_score, tenth.base_score + 4);
    }

    #[test]
    fn auth_failure_anomaly_applies_only_above_two_failures() {
        let env = Env::default();
        let contract_id = env.register(TestContract, ());

        for (operation, failures, expected_adjustment) in [
            (symbol_short!("F2"), 2, 0),
            (symbol_short!("F3"), 3, 6),
            (symbol_short!("F4"), 4, 6),
        ] {
            let input = input_for(
                &env,
                operation,
                ActionType::Write,
                DataSensitivity::Internal,
                RiskContext {
                    recent_auth_failures: failures,
                    ..quiet_context()
                },
            );
            let assessment = env.as_contract(&contract_id, || evaluate_risk(&env, &input, None));
            assert_eq!(assessment.behavioral_adjustment, expected_adjustment);
            assert_eq!(
                assessment.final_score,
                assessment.base_score + expected_adjustment
            );

            let state = behavior_state(&env, &contract_id, &input);
            assert_eq!(state.anomaly_hits, if failures > 2 { 1 } else { 0 });
        }
    }

    #[test]
    fn behavior_window_expires_only_after_one_hour() {
        let env = Env::default();
        let contract_id = env.register(TestContract, ());
        env.ledger().set_timestamp(10_000);
        let input = input_for(
            &env,
            symbol_short!("OP"),
            ActionType::Write,
            DataSensitivity::Sensitive,
            quiet_context(),
        );

        for _ in 0..9 {
            env.as_contract(&contract_id, || evaluate_risk(&env, &input, None));
        }
        let state = behavior_state(&env, &contract_id, &input);
        assert_eq!(state.window_start, 10_000);
        assert_eq!(state.count, 9);
        assert_eq!(state.anomaly_hits, 1);

        env.ledger().set_timestamp(13_600);
        let within_window = env.as_contract(&contract_id, || evaluate_risk(&env, &input, None));
        assert_eq!(within_window.behavioral_adjustment, 4);
        let state = behavior_state(&env, &contract_id, &input);
        assert_eq!(state.count, 10);
        assert_eq!(state.anomaly_hits, 2);
        assert_eq!(state.window_start, 10_000);

        env.ledger().set_timestamp(13_601);
        let after_window = env.as_contract(&contract_id, || evaluate_risk(&env, &input, None));
        assert_eq!(after_window.behavioral_adjustment, 0);
        assert_eq!(after_window.final_score, after_window.base_score);
        let state = behavior_state(&env, &contract_id, &input);
        assert_eq!(state.window_start, 13_601);
        assert_eq!(state.count, 1);
        assert_eq!(state.anomaly_hits, 0);
    }

    #[test]
    fn threshold_boundaries_map_to_expected_auth_levels() {
        let policy = default_policy();
        for (score, expected) in [
            (0, AuthLevel::Level1),
            (24, AuthLevel::Level1),
            (25, AuthLevel::Level2),
            (49, AuthLevel::Level2),
            (50, AuthLevel::Level3),
            (74, AuthLevel::Level3),
            (75, AuthLevel::Level4),
            (100, AuthLevel::Level4),
        ] {
            assert_eq!(level_for_score(score, &policy), expected, "score {score}");
        }

        let custom = ProgressiveAuthPolicy {
            level2_min_score: 10,
            level3_min_score: 40,
            level4_min_score: 70,
            level2_delay_seconds: 0,
            level3_delay_seconds: 0,
            level4_delay_seconds: 0,
        };
        assert_eq!(level_for_score(9, &custom), AuthLevel::Level1);
        assert_eq!(level_for_score(10, &custom), AuthLevel::Level2);
        assert_eq!(level_for_score(40, &custom), AuthLevel::Level3);
        assert_eq!(level_for_score(70, &custom), AuthLevel::Level4);
    }

    #[test]
    fn high_risk_access_is_denied_until_step_up_evidence_is_present() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(TestContract, ());
        let caller = Address::generate(&env);
        let second_signer = Address::generate(&env);
        let policy = default_policy();

        let low = input_for(
            &env,
            symbol_short!("READ"),
            ActionType::Read,
            DataSensitivity::Public,
            quiet_context(),
        );
        let high = input_for(
            &env,
            symbol_short!("SET_ENC"),
            ActionType::EmergencyOverride,
            DataSensitivity::Restricted,
            RiskContext {
                off_hours: true,
                unusual_location: true,
                unusual_frequency: true,
                recent_auth_failures: 6,
                emergency_signal: true,
            },
        );
        assert_eq!(default_risk_score(&low), 10);
        assert_eq!(default_risk_score(&high), 100);

        let auth = env.as_contract(&contract_id, || {
            session::start_or_refresh_session(&env, &caller, AuthLevel::Level4, 3_600, 900)
        });

        let low_risk = env.as_contract(&contract_id, || {
            enforce_for_risk(
                &env,
                &caller,
                default_risk_score(&low),
                auth.issued_at,
                None,
                symbol_short!("READ"),
                BytesN::from_array(&env, &[1; 32]),
                false,
                &policy,
            )
        });
        assert_eq!(low_risk, Ok(AuthLevel::Level1));

        let mut signers = Vec::new(&env);
        signers.push_back(caller.clone());
        signers.push_back(second_signer.clone());
        env.as_contract(&contract_id, || {
            multisig::configure(&env, signers, 2).expect("valid multisig config");
        });

        let missing_multisig = env.as_contract(&contract_id, || {
            enforce_for_risk(
                &env,
                &caller,
                default_risk_score(&high),
                auth.issued_at,
                None,
                symbol_short!("SET_ENC"),
                BytesN::from_array(&env, &[2; 32]),
                false,
                &policy,
            )
        });
        assert_eq!(
            missing_multisig,
            Err(ProgressiveAuthError::MissingMultisigApproval)
        );

        let proposal = env.as_contract(&contract_id, || {
            multisig::propose(
                &env,
                &caller,
                symbol_short!("SET_ENC"),
                BytesN::from_array(&env, &[2; 32]),
            )
            .expect("proposal should be created")
        });
        env.as_contract(&contract_id, || {
            multisig::approve(&env, &second_signer, proposal).expect("second approval");
        });

        let missing_zk = env.as_contract(&contract_id, || {
            enforce_for_risk(
                &env,
                &caller,
                default_risk_score(&high),
                auth.issued_at,
                Some(proposal),
                symbol_short!("SET_ENC"),
                BytesN::from_array(&env, &[2; 32]),
                false,
                &policy,
            )
        });
        assert_eq!(missing_zk, Err(ProgressiveAuthError::MissingZkProof));

        let allowed = env.as_contract(&contract_id, || {
            enforce_for_risk(
                &env,
                &caller,
                default_risk_score(&high),
                auth.issued_at,
                Some(proposal),
                symbol_short!("SET_ENC"),
                BytesN::from_array(&env, &[2; 32]),
                true,
                &policy,
            )
        });
        assert_eq!(allowed, Ok(AuthLevel::Level4));
    }

    #[test]
    fn medium_risk_access_requires_multisig_but_not_zk_proof() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(TestContract, ());
        let caller = Address::generate(&env);
        let second_signer = Address::generate(&env);
        let policy = default_policy();

        let mut signers = Vec::new(&env);
        signers.push_back(caller.clone());
        signers.push_back(second_signer.clone());
        env.as_contract(&contract_id, || {
            multisig::configure(&env, signers, 2).expect("valid multisig config");
        });

        let auth = env.as_contract(&contract_id, || {
            session::start_or_refresh_session(&env, &caller, AuthLevel::Level3, 3_600, 900)
        });
        let score = 60;
        assert_eq!(level_for_score(score, &policy), AuthLevel::Level3);

        let denied = env.as_contract(&contract_id, || {
            enforce_for_risk(
                &env,
                &caller,
                score,
                auth.issued_at,
                None,
                symbol_short!("SET_RATE"),
                BytesN::from_array(&env, &[3; 32]),
                false,
                &policy,
            )
        });
        assert_eq!(denied, Err(ProgressiveAuthError::MissingMultisigApproval));

        let proposal = env.as_contract(&contract_id, || {
            multisig::propose(
                &env,
                &caller,
                symbol_short!("SET_RATE"),
                BytesN::from_array(&env, &[3; 32]),
            )
            .expect("proposal should be created")
        });
        env.as_contract(&contract_id, || {
            multisig::approve(&env, &second_signer, proposal).expect("second approval");
        });

        let allowed = env.as_contract(&contract_id, || {
            enforce_for_risk(
                &env,
                &caller,
                score,
                auth.issued_at,
                Some(proposal),
                symbol_short!("SET_RATE"),
                BytesN::from_array(&env, &[3; 32]),
                false,
                &policy,
            )
        });
        assert_eq!(allowed, Ok(AuthLevel::Level3));
    }
}
