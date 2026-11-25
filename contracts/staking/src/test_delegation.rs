//! Delegated staking tests (issue #461).

#![allow(clippy::unwrap_used, clippy::expect_used)]

use soroban_sdk::{
    testutils::{Address as _, Ledger as _},
    token::StellarAssetClient,
    Address, Env,
};

use crate::{ContractError, StakingContract, StakingContractClient};

// ── Test helpers ──────────────────────────────────────────────────────────────

fn setup(
    reward_rate: i128,
    lock_period: u64,
) -> (Env, StakingContractClient<'static>, Address, Address) {
    let env = Env::default();
    env.mock_all_auths();

    let stake_token = env.register_stellar_asset_contract_v2(Address::generate(&env));
    let reward_token = env.register_stellar_asset_contract_v2(Address::generate(&env));
    let stake_token_id = stake_token.address();
    let reward_token_id = reward_token.address();

    let contract_id = env.register(StakingContract, ());
    let client = StakingContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    client.initialize(
        &admin,
        &stake_token_id,
        &reward_token_id,
        &reward_rate,
        &lock_period,
    );

    StellarAssetClient::new(&env, &reward_token_id).mint(&contract_id, &1_000_000_000i128);

    (env, client, admin, stake_token_id)
}

fn mint_and_stake(
    env: &Env,
    client: &StakingContractClient,
    stake_token: &Address,
    staker: &Address,
    amount: i128,
) {
    StellarAssetClient::new(env, stake_token).mint(staker, &amount);
    client.stake(staker, &amount);
}

fn mint_only(env: &Env, stake_token: &Address, who: &Address, amount: i128) {
    StellarAssetClient::new(env, stake_token).mint(who, &amount);
}

// ── Provider registration ─────────────────────────────────────────────────────

#[test]
fn register_provider_rejects_commission_above_100_percent() {
    let (env, client, _, _) = setup(0, 0);
    let provider = Address::generate(&env);

    assert_eq!(
        client.try_register_provider(&provider, &10_001u32),
        Err(Ok(ContractError::InvalidInput))
    );
}

#[test]
fn delegate_requires_registered_provider() {
    let (env, client, _, stake_token) = setup(0, 0);
    let provider = Address::generate(&env);
    let delegator = Address::generate(&env);
    mint_only(&env, &stake_token, &delegator, 500);

    assert_eq!(
        client.try_delegate(&delegator, &provider, &500),
        Err(Ok(ContractError::NotProvider))
    );
}

#[test]
fn delegate_rejects_self_delegation() {
    let (env, client, _, stake_token) = setup(0, 0);
    let provider = Address::generate(&env);
    client.register_provider(&provider, &1_000u32);
    mint_only(&env, &stake_token, &provider, 500);

    assert_eq!(
        client.try_delegate(&provider, &provider, &500),
        Err(Ok(ContractError::InvalidInput))
    );
}

// ── Delegation records ────────────────────────────────────────────────────────

#[test]
fn delegate_credits_delegator_stake_and_records_delegation() {
    let (env, client, _, stake_token) = setup(0, 0);
    let provider = Address::generate(&env);
    let delegator = Address::generate(&env);
    client.register_provider(&provider, &1_000u32);
    mint_only(&env, &stake_token, &delegator, 500);

    client.delegate(&delegator, &provider, &500);

    assert_eq!(client.get_staked(&delegator), 500);
    assert_eq!(client.get_total_staked(), 500);
    assert_eq!(client.get_delegated_total(&provider), 500);
    let record = client.get_delegation(&delegator).unwrap();
    assert_eq!(record.provider, provider);
    assert_eq!(record.amount, 500);
}

#[test]
fn delegate_top_up_to_same_provider_accumulates() {
    let (env, client, _, stake_token) = setup(0, 0);
    let provider = Address::generate(&env);
    let delegator = Address::generate(&env);
    client.register_provider(&provider, &1_000u32);
    mint_only(&env, &stake_token, &delegator, 900);

    client.delegate(&delegator, &provider, &400);
    client.delegate(&delegator, &provider, &300);

    assert_eq!(client.get_delegation(&delegator).unwrap().amount, 700);
    assert_eq!(client.get_delegated_total(&provider), 700);
}

#[test]
fn delegate_to_a_second_provider_while_delegated_is_rejected() {
    let (env, client, _, stake_token) = setup(0, 0);
    let first = Address::generate(&env);
    let second = Address::generate(&env);
    let delegator = Address::generate(&env);
    client.register_provider(&first, &1_000u32);
    client.register_provider(&second, &1_000u32);
    mint_only(&env, &stake_token, &delegator, 1_000);
    client.delegate(&delegator, &first, &500);

    assert_eq!(
        client.try_delegate(&delegator, &second, &500),
        Err(Ok(ContractError::DelegationExists))
    );
    assert_eq!(client.get_delegated_total(&second), 0);
}

#[test]
fn undelegate_reduces_delegation_but_keeps_staked_balance() {
    let (env, client, _, stake_token) = setup(0, 0);
    let provider = Address::generate(&env);
    let delegator = Address::generate(&env);
    client.register_provider(&provider, &1_000u32);
    mint_only(&env, &stake_token, &delegator, 500);
    client.delegate(&delegator, &provider, &500);

    client.undelegate(&delegator, &200);

    assert_eq!(client.get_delegation(&delegator).unwrap().amount, 300);
    assert_eq!(client.get_delegated_total(&provider), 300);
    assert_eq!(client.get_staked(&delegator), 500);
}

#[test]
fn undelegate_more_than_delegated_is_rejected() {
    let (env, client, _, stake_token) = setup(0, 0);
    let provider = Address::generate(&env);
    let delegator = Address::generate(&env);
    client.register_provider(&provider, &1_000u32);
    mint_only(&env, &stake_token, &delegator, 500);
    client.delegate(&delegator, &provider, &500);

    assert_eq!(
        client.try_undelegate(&delegator, &501),
        Err(Ok(ContractError::InsufficientBalance))
    );
}

#[test]
fn undelegate_without_delegation_is_rejected() {
    let (env, client, _, _) = setup(0, 0);
    let delegator = Address::generate(&env);

    assert_eq!(
        client.try_undelegate(&delegator, &1),
        Err(Ok(ContractError::DelegationNotFound))
    );
}

#[test]
fn delegated_stake_cannot_be_unstaked_beyond_own_balance() {
    let (env, client, _, stake_token) = setup(0, 0);
    let provider = Address::generate(&env);
    let delegator = Address::generate(&env);
    client.register_provider(&provider, &1_000u32);
    mint_and_stake(&env, &client, &stake_token, &delegator, 200);
    mint_only(&env, &stake_token, &delegator, 500);
    client.delegate(&delegator, &provider, &500);

    // 700 staked, 500 of it delegated: only 200 is free to unstake.
    assert_eq!(
        client.try_request_unstake(&delegator, &201),
        Err(Ok(ContractError::InsufficientBalance))
    );
    assert!(client.try_request_unstake(&delegator, &200).is_ok());
}

// ── Slashing propagation ──────────────────────────────────────────────────────

#[test]
fn slash_is_shared_pro_rata_with_delegators() {
    let (env, client, admin, stake_token) = setup(0, 0);
    let provider = Address::generate(&env);
    let delegator = Address::generate(&env);
    client.register_provider(&provider, &1_000u32);
    mint_and_stake(&env, &client, &stake_token, &provider, 1_000);
    mint_only(&env, &stake_token, &delegator, 1_000);
    client.delegate(&delegator, &provider, &1_000);

    // Provider holds 1_000 own + 1_000 delegated: a 1_000 slash splits 50/50.
    assert_eq!(client.slash(&admin, &provider, &1_000), 1_000);
    assert_eq!(client.get_staked(&provider), 500);
    assert_eq!(client.get_total_staked(), 1_000);

    // Any delegator action syncs the lazy slash into their record.
    assert_eq!(client.claim_rewards(&delegator), 0);
    assert_eq!(client.get_delegation(&delegator).unwrap().amount, 500);
    assert_eq!(client.get_staked(&delegator), 500);
    assert_eq!(client.get_delegated_total(&provider), 500);
}

#[test]
fn slash_larger_than_total_stake_wipes_delegation() {
    let (env, client, admin, stake_token) = setup(0, 0);
    let provider = Address::generate(&env);
    let delegator = Address::generate(&env);
    client.register_provider(&provider, &1_000u32);
    mint_and_stake(&env, &client, &stake_token, &provider, 1_000);
    mint_only(&env, &stake_token, &delegator, 1_000);
    client.delegate(&delegator, &provider, &1_000);

    // Capped at the combined 2_000.
    assert_eq!(client.slash(&admin, &provider, &5_000), 2_000);
    assert_eq!(client.get_staked(&provider), 0);
    assert_eq!(client.get_delegated_total(&provider), 0);

    client.claim_rewards(&delegator);
    assert_eq!(client.get_delegation(&delegator).unwrap().amount, 0);
    assert_eq!(client.get_staked(&delegator), 0);
}

#[test]
fn slash_of_provider_without_delegation_is_unchanged() {
    let (env, client, admin, stake_token) = setup(0, 0);
    let validator = Address::generate(&env);
    mint_and_stake(&env, &client, &stake_token, &validator, 1_000);

    assert_eq!(client.slash(&admin, &validator, &400), 400);
    assert_eq!(client.get_staked(&validator), 600);
    assert_eq!(client.get_total_staked(), 600);
}

// ── Reward split ──────────────────────────────────────────────────────────────

#[test]
fn commission_is_taken_from_delegator_rewards_and_credited_to_provider() {
    let (env, client, _, stake_token) = setup(10, 0);
    let provider = Address::generate(&env);
    let delegator = Address::generate(&env);
    client.register_provider(&provider, &1_000u32); // 10 %
    mint_only(&env, &stake_token, &delegator, 1_000);
    client.delegate(&delegator, &provider, &1_000);

    env.ledger().set_timestamp(100);
    let net = client.claim_rewards(&delegator);
    let commission = client.claim_rewards(&provider);

    assert!(commission > 0);
    // Gross = net + commission and commission is 10 % of gross.
    assert_eq!(commission, (net + commission) / 10);
}
