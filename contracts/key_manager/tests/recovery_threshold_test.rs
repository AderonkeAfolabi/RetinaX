#![allow(clippy::unwrap_used, clippy::expect_used)]

use identity::{IdentityContract, IdentityContractClient};
use key_manager::{
    ContractError, KeyManagerContract, KeyManagerContractClient, KeyPolicy, KeyType,
};
use soroban_sdk::{testutils::Address as _, testutils::Ledger, Address, BytesN, Env, Vec};

fn setup() -> (
    Env,
    KeyManagerContractClient<'static>,
    IdentityContractClient<'static>,
    Address,
    Address,
    Address,
    Address,
) {
    let env = Env::default();
    env.mock_all_auths();

    let identity_id = env.register(IdentityContract, ());
    let identity = IdentityContractClient::new(&env, &identity_id);

    let admin = Address::generate(&env);
    identity.initialize(&admin);

    let key_manager_id = env.register(KeyManagerContract, ());
    let key_manager = KeyManagerContractClient::new(&env, &key_manager_id);
    key_manager.initialize(&admin, &identity_id);

    let guardian1 = Address::generate(&env);
    let guardian2 = Address::generate(&env);
    let guardian3 = Address::generate(&env);

    identity.add_guardian(&admin, &guardian1);
    identity.add_guardian(&admin, &guardian2);
    identity.add_guardian(&admin, &guardian3);
    identity.set_recovery_threshold(&admin, &2);

    (
        env,
        key_manager,
        identity,
        admin,
        guardian1,
        guardian2,
        guardian3,
    )
}

fn create_owner_key(
    env: &Env,
    client: &KeyManagerContractClient<'static>,
    admin: &Address,
) -> BytesN<32> {
    let policy = KeyPolicy {
        max_uses: 0,
        not_before: 0,
        not_after: 0,
        allowed_ops: Vec::new(env),
    };
    let key_bytes = BytesN::from_array(env, &[15u8; 32]);
    client.create_master_key(admin, &KeyType::Encryption, &policy, &0u64, &key_bytes)
}

#[test]
fn test_recovery_full_threshold_success() {
    let (env, client, _identity, admin, g1, g2, _g3) = setup();
    let key_id = create_owner_key(&env, &client, &admin);
    let new_key = BytesN::from_array(&env, &[99u8; 32]);

    client.initiate_recovery(&g1, &key_id, &new_key);
    let req = client
        .get_recovery_request(&key_id)
        .expect("request exists");
    assert_eq!(req.approvals.len(), 1);
    assert_eq!(req.new_key, new_key);

    client.approve_recovery(&g2, &key_id);
    let req2 = client
        .get_recovery_request(&key_id)
        .expect("request exists");
    assert_eq!(req2.approvals.len(), 2);

    let now = env.ledger().timestamp();
    env.ledger().set_timestamp(now + 86_401);

    let next_ver = client.execute_recovery(&admin, &key_id);
    assert_eq!(next_ver, 2);

    let ver_rec = client
        .get_key_version(&key_id, &next_ver)
        .expect("version exists");
    assert_eq!(ver_rec.key_bytes, new_key);

    assert!(client.get_recovery_request(&key_id).is_none());
}

#[test]
fn test_recovery_insufficient_approvals_rejected() {
    let (env, client, _identity, admin, g1, _g2, _g3) = setup();
    let key_id = create_owner_key(&env, &client, &admin);
    let new_key = BytesN::from_array(&env, &[101u8; 32]);

    client.initiate_recovery(&g1, &key_id, &new_key);

    let now = env.ledger().timestamp();
    env.ledger().set_timestamp(now + 86_401);

    let res = client.try_execute_recovery(&admin, &key_id);
    assert_eq!(res, Err(Ok(ContractError::InsufficientApprovals)));
}

#[test]
fn test_recovery_cooldown_not_expired_rejected() {
    let (env, client, _identity, admin, g1, g2, _g3) = setup();
    let key_id = create_owner_key(&env, &client, &admin);
    let new_key = BytesN::from_array(&env, &[102u8; 32]);

    client.initiate_recovery(&g1, &key_id, &new_key);
    client.approve_recovery(&g2, &key_id);

    let res = client.try_execute_recovery(&admin, &key_id);
    assert_eq!(res, Err(Ok(ContractError::CooldownNotExpired)));
}

#[test]
fn test_recovery_non_guardian_initiate_rejected() {
    let (env, client, _identity, admin, _g1, _g2, _g3) = setup();
    let key_id = create_owner_key(&env, &client, &admin);
    let non_guardian = Address::generate(&env);
    let new_key = BytesN::from_array(&env, &[103u8; 32]);

    let res = client.try_initiate_recovery(&non_guardian, &key_id, &new_key);
    assert_eq!(res, Err(Ok(ContractError::NotAGuardian)));
}

#[test]
fn test_recovery_already_approved_rejected() {
    let (env, client, _identity, admin, g1, _g2, _g3) = setup();
    let key_id = create_owner_key(&env, &client, &admin);
    let new_key = BytesN::from_array(&env, &[104u8; 32]);

    client.initiate_recovery(&g1, &key_id, &new_key);
    let res = client.try_approve_recovery(&g1, &key_id);
    assert_eq!(res, Err(Ok(ContractError::AlreadyApproved)));
}

#[test]
fn test_recovery_cancellation_by_owner() {
    let (env, client, _identity, admin, g1, _g2, _g3) = setup();
    let key_id = create_owner_key(&env, &client, &admin);
    let new_key = BytesN::from_array(&env, &[105u8; 32]);

    client.initiate_recovery(&g1, &key_id, &new_key);
    assert!(client.get_recovery_request(&key_id).is_some());

    client.cancel_recovery(&admin, &key_id);
    assert!(client.get_recovery_request(&key_id).is_none());

    let res = client.try_execute_recovery(&admin, &key_id);
    assert_eq!(res, Err(Ok(ContractError::RecoveryNotActive)));
}

#[test]
fn test_recovery_cancellation_unauthorized() {
    let (env, client, _identity, admin, g1, _g2, _g3) = setup();
    let key_id = create_owner_key(&env, &client, &admin);
    let stranger = Address::generate(&env);
    let new_key = BytesN::from_array(&env, &[107u8; 32]);

    client.initiate_recovery(&g1, &key_id, &new_key);
    let res = client.try_cancel_recovery(&stranger, &key_id);
    assert_eq!(res, Err(Ok(ContractError::Unauthorized)));
}
