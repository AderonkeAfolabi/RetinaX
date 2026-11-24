#![cfg(test)]
#![allow(clippy::unwrap_used, clippy::expect_used)]

use soroban_sdk::{
    testutils::{Address as _, Ledger as _, LedgerInfo},
    token::{Client as TokenClient, StellarAssetClient},
    Address, Env,
};
use staking::{AppealStatus, ContractError, StakingContract, StakingContractClient};

const COOLDOWN_SECONDS: u64 = 86_400; // 24 hours

fn set_ledger_time(env: &Env, timestamp: u64) {
    env.ledger().set(LedgerInfo {
        timestamp,
        protocol_version: 25,
        sequence_number: 100,
        network_id: Default::default(),
        base_reserve: 10,
        min_temp_entry_ttl: 16,
        min_persistent_entry_ttl: 16,
        max_entry_ttl: 6_312_000,
    });
}

fn setup(lock_period: u64) -> (Env, StakingContractClient<'static>, Address, Address) {
    let env = Env::default();
    env.mock_all_auths();

    let stake_token = env
        .register_stellar_asset_contract_v2(Address::generate(&env))
        .address();
    let reward_token = env
        .register_stellar_asset_contract_v2(Address::generate(&env))
        .address();

    let contract_id = env.register(StakingContract, ());
    let client = StakingContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);

    client.initialize(&admin, &stake_token, &reward_token, &0i128, &lock_period);

    (env, client, admin, stake_token)
}

#[test]
fn test_unstake_blocked_before_cooldown_and_succeeds_after() {
    let (env, client, _admin, stake_token) = setup(COOLDOWN_SECONDS);
    let staker = Address::generate(&env);
    let token_admin = StellarAssetClient::new(&env, &stake_token);
    let token = TokenClient::new(&env, &stake_token);

    set_ledger_time(&env, 10_000);
    token_admin.mint(&staker, &1_000);
    client.stake(&staker, &1_000);

    let req_id = client.request_unstake(&staker, &400);
    assert_eq!(client.get_staked(&staker), 600);

    let req = client.get_unstake_request(&req_id);
    assert_eq!(req.amount, 400);
    assert_eq!(req.unlock_at, 10_000 + COOLDOWN_SECONDS);
    assert!(!req.withdrawn);

    // 1 second before cooldown expires -> TimelockNotExpired.
    set_ledger_time(&env, 10_000 + COOLDOWN_SECONDS - 1);
    assert_eq!(
        client.try_withdraw(&staker, &req_id),
        Err(Ok(ContractError::TimelockNotExpired))
    );
    assert_eq!(token.balance(&staker), 0);

    // Exactly at unlock_at -> withdraw succeeds and transfers tokens.
    set_ledger_time(&env, 10_000 + COOLDOWN_SECONDS);
    client.withdraw(&staker, &req_id);
    assert_eq!(token.balance(&staker), 400);
    assert!(client.get_unstake_request(&req_id).withdrawn);

    // Second withdraw on the same request fails with AlreadyWithdrawn.
    assert_eq!(
        client.try_withdraw(&staker, &req_id),
        Err(Ok(ContractError::AlreadyWithdrawn))
    );
}

#[test]
fn test_unstake_cooldown_with_pending_slash_and_appeal_resolution() {
    let (env, client, admin, stake_token) = setup(COOLDOWN_SECONDS);
    let validator = Address::generate(&env);
    let token_admin = StellarAssetClient::new(&env, &stake_token);
    let token = TokenClient::new(&env, &stake_token);

    set_ledger_time(&env, 20_000);
    token_admin.mint(&validator, &1_000);
    client.stake(&validator, &1_000);

    // Admin slashes 400 -> active stake drops to 600.
    client.slash(&admin, &validator, &400);
    assert_eq!(client.get_staked(&validator), 600);

    // Validator files an appeal for 300 of the slashed amount.
    let appeal_id = client.file_appeal(&validator, &1, &300);
    assert_eq!(client.get_appeal(&appeal_id).status, AppealStatus::Pending);

    // While the appeal is pending, validator cannot unstake more than the remaining 600.
    assert_eq!(
        client.try_request_unstake(&validator, &700),
        Err(Ok(ContractError::InsufficientBalance))
    );

    // Unstaking 200 of remaining active stake enters cooldown.
    let req_1 = client.request_unstake(&validator, &200);
    assert_eq!(client.get_staked(&validator), 400);

    // Before cooldown expires, withdraw fails even while appeal is pending.
    set_ledger_time(&env, 20_000 + COOLDOWN_SECONDS - 10);
    assert_eq!(
        client.try_withdraw(&validator, &req_1),
        Err(Ok(ContractError::TimelockNotExpired))
    );

    // Admin resolves the appeal approving 300 refund -> active stake increases to 700.
    client.resolve_appeal(&admin, &appeal_id, &300);
    assert_eq!(client.get_appeal(&appeal_id).status, AppealStatus::Approved);
    assert_eq!(client.get_staked(&validator), 700);

    // Validator can now request unstake of the restored 300 as a second cooldown request.
    let req_2 = client.request_unstake(&validator, &300);
    assert_eq!(client.get_staked(&validator), 400);

    // First request unlocks at 20_000 + COOLDOWN_SECONDS, while second is still locked.
    set_ledger_time(&env, 20_000 + COOLDOWN_SECONDS);
    client.withdraw(&validator, &req_1);
    assert_eq!(token.balance(&validator), 200);
    assert_eq!(
        client.try_withdraw(&validator, &req_2),
        Err(Ok(ContractError::TimelockNotExpired))
    );

    // Once second request's cooldown elapses, withdraw succeeds.
    set_ledger_time(&env, 20_000 + 2 * COOLDOWN_SECONDS);
    client.withdraw(&validator, &req_2);
    assert_eq!(token.balance(&validator), 500);
}
