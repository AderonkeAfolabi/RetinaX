#![cfg(test)]

//! Verifiable credential revocation registry (#450).

use identity::{IdentityContract, IdentityContractClient};
use soroban_sdk::{
    testutils::{Address as _, MockAuth, MockAuthInvoke},
    Address, Env, IntoVal,
};

/// No authorization is mocked: each test grants exactly what it needs.
fn setup() -> (Env, IdentityContractClient<'static>) {
    let env = Env::default();
    let client = IdentityContractClient::new(&env, &env.register(IdentityContract, ()));
    (env, client)
}

/// Calls `revoke(issuer, index)` in a transaction signed by `signer` only.
fn revoke_as(
    env: &Env,
    client: &IdentityContractClient,
    signer: &Address,
    issuer: &Address,
    index: u64,
) -> bool {
    client
        .mock_auths(&[MockAuth {
            address: signer,
            invoke: &MockAuthInvoke {
                contract: &client.address,
                fn_name: "revoke",
                args: (issuer, index).into_val(env),
                sub_invokes: &[],
            },
        }])
        .try_revoke(issuer, &index)
        .is_ok()
}

#[test]
fn issuer_revokes_only_the_given_credential() {
    let (env, client) = setup();
    let issuer = Address::generate(&env);
    assert!(!client.is_revoked(&issuer, &5));

    assert!(revoke_as(&env, &client, &issuer, &issuer, 5));

    assert!(client.is_revoked(&issuer, &5));
    assert!(!client.is_revoked(&issuer, &4));
    assert!(!client.is_revoked(&issuer, &6));
}

#[test]
fn only_the_issuer_can_revoke() {
    let (env, client) = setup();
    let issuer = Address::generate(&env);
    let attacker = Address::generate(&env);

    assert!(client.try_revoke(&issuer, &5).is_err());
    assert!(!revoke_as(&env, &client, &attacker, &issuer, 5));

    assert!(!client.is_revoked(&issuer, &5));
}

#[test]
fn an_issuer_cannot_affect_another_issuers_credentials() {
    let (env, client) = setup();
    let issuer = Address::generate(&env);
    let other = Address::generate(&env);

    assert!(revoke_as(&env, &client, &other, &other, 5));

    assert!(client.is_revoked(&other, &5));
    assert!(!client.is_revoked(&issuer, &5));
}

#[test]
fn indexes_do_not_collide_across_bitmap_words() {
    let (env, client) = setup();
    let issuer = Address::generate(&env);
    let revoked = [0, 127, 128, 256, u64::MAX];

    for index in revoked {
        assert!(revoke_as(&env, &client, &issuer, &issuer, index));
    }

    for index in revoked {
        assert!(client.is_revoked(&issuer, &index), "{index}");
    }
    for index in [1, 126, 129, 255, 257, 384, u64::MAX - 1] {
        assert!(!client.is_revoked(&issuer, &index), "{index}");
    }
}
