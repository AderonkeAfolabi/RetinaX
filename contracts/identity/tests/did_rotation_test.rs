#![cfg(test)]

use identity::recovery::RecoveryError;
use identity::{IdentityContract, IdentityContractClient};
use soroban_sdk::{
    testutils::{Address as _, Ledger as _},
    Address, BytesN, Env, String,
};

const COOLDOWN_SECONDS: u64 = 172_800;

fn setup() -> (Env, IdentityContractClient<'static>, Address) {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(IdentityContract, ());
    let client = IdentityContractClient::new(&env, &contract_id);
    let owner = Address::generate(&env);
    client.initialize(&owner);
    (env, client, owner)
}

#[test]
fn test_did_key_update_twice_and_resolution() {
    let (env, client, owner) = setup();
    let did = String::from_str(&env, "did:retinax:patient-rotation-1");

    env.ledger().set_timestamp(100);
    client.create_did(&owner, &did);

    let doc0 = client.resolve_did(&did).expect("DID should resolve");
    assert_eq!(doc0.controller, owner);
    assert!(!doc0.deactivated);
    assert_eq!(doc0.credential_ids.len(), 0);
    assert_eq!(doc0.updated_at, 100);

    // First key/credential update + DID rotation.
    let cred_1 = BytesN::from_array(&env, &[0x11; 32]);
    client.bind_credential(&owner, &cred_1);
    env.ledger().set_timestamp(200);
    client.update_did(&owner, &did);

    let doc1 = client
        .resolve_did(&did)
        .expect("DID should resolve after 1st update");
    assert_eq!(doc1.credential_ids.len(), 1);
    assert_eq!(doc1.updated_at, 200);

    // Second key/credential update + DID rotation.
    let cred_2 = BytesN::from_array(&env, &[0x22; 32]);
    client.bind_credential(&owner, &cred_2);
    env.ledger().set_timestamp(300);
    client.update_did(&owner, &did);

    let doc2 = client
        .resolve_did(&did)
        .expect("DID should resolve after 2nd update");
    assert_eq!(doc2.credential_ids.len(), 2);
    assert_eq!(doc2.updated_at, 300);
    assert!(!doc2.deactivated);
}

#[test]
fn test_controller_rotation_twice_via_recovery_and_did_resolution() {
    let (env, client, owner_1) = setup();
    let g1 = Address::generate(&env);
    let g2 = Address::generate(&env);
    let g3 = Address::generate(&env);
    client.add_guardian(&owner_1, &g1);
    client.add_guardian(&owner_1, &g2);
    client.add_guardian(&owner_1, &g3);
    client.set_recovery_threshold(&owner_1, &2);

    let cred = BytesN::from_array(&env, &[0xAA; 32]);
    client.bind_credential(&owner_1, &cred);

    // Rotation 1: owner_1 -> owner_2.
    let owner_2 = Address::generate(&env);
    env.ledger().set_timestamp(1_000);
    client.initiate_recovery(&g1, &owner_1, &owner_2);
    client.approve_recovery(&g2, &owner_1);
    env.ledger().set_timestamp(1_000 + COOLDOWN_SECONDS);
    assert_eq!(client.execute_recovery(&g1, &owner_1), owner_2);

    let did_2 = String::from_str(&env, "did:retinax:rotated-owner-2");
    client.create_did(&owner_2, &did_2);
    let resolved_2 = client.resolve_did(&did_2).expect("owner_2 DID resolves");
    assert_eq!(resolved_2.controller, owner_2);
    assert_eq!(resolved_2.credential_ids.len(), 1);

    // Rotation 2: owner_2 -> owner_3.
    let owner_3 = Address::generate(&env);
    let t2 = 2_000 + COOLDOWN_SECONDS;
    env.ledger().set_timestamp(t2);
    client.initiate_recovery(&g1, &owner_2, &owner_3);
    client.approve_recovery(&g3, &owner_2);
    env.ledger().set_timestamp(t2 + COOLDOWN_SECONDS);
    assert_eq!(client.execute_recovery(&g1, &owner_2), owner_3);

    assert!(!client.is_owner_active(&owner_1));
    assert!(!client.is_owner_active(&owner_2));
    assert!(client.is_owner_active(&owner_3));

    let did_3 = String::from_str(&env, "did:retinax:rotated-owner-3");
    client.create_did(&owner_3, &did_3);
    let resolved_3 = client.resolve_did(&did_3).expect("owner_3 DID resolves");
    assert_eq!(resolved_3.controller, owner_3);
    assert_eq!(resolved_3.credential_ids.len(), 1);
}

#[test]
fn test_did_revocation_and_unauthorized_rotation_rejected() {
    let (env, client, owner) = setup();
    let attacker = Address::generate(&env);
    let did = String::from_str(&env, "did:retinax:revoke-target");

    client.create_did(&owner, &did);

    // Unauthorised caller cannot update or deactivate the DID.
    assert_eq!(
        client.try_update_did(&attacker, &did),
        Err(Ok(RecoveryError::Unauthorized))
    );
    assert_eq!(
        client.try_deactivate_did(&attacker, &did),
        Err(Ok(RecoveryError::Unauthorized))
    );

    // Legitimate owner revokes (deactivates) the DID.
    client.deactivate_did(&owner, &did);
    let revoked = client
        .resolve_did(&did)
        .expect("deactivated DID still resolves");
    assert!(revoked.deactivated);

    // Further updates or repeated deactivation fail with OwnerDeactivated.
    assert_eq!(
        client.try_update_did(&owner, &did),
        Err(Ok(RecoveryError::OwnerDeactivated))
    );
    assert_eq!(
        client.try_deactivate_did(&owner, &did),
        Err(Ok(RecoveryError::OwnerDeactivated))
    );
}
