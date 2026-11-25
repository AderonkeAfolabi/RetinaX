#![cfg(test)]
#![allow(clippy::unwrap_used, clippy::expect_used)]

//! W3C-style DID document resolution from on-chain keys and services (#449).

extern crate std;

use identity::did::{
    DidKeyType, MAX_DID_KEYS, MAX_DID_SERVICES, PURPOSE_ASSERTION_METHOD, PURPOSE_AUTHENTICATION,
    PURPOSE_CAPABILITY_DELEGATION, PURPOSE_CAPABILITY_INVOCATION, PURPOSE_KEY_AGREEMENT,
};
use identity::{recovery::RecoveryError, IdentityContract, IdentityContractClient};
use soroban_sdk::{
    testutils::{Address as _, Events as _, Ledger},
    Address, Bytes, BytesN, Env, String,
};

const DID: &str = "did:retinax:patient-42";

fn setup(env: &Env) -> (IdentityContractClient<'_>, Address, String) {
    env.mock_all_auths();
    env.ledger().set_timestamp(1_700_000_000);
    let id = env.register(IdentityContract, ());
    let client = IdentityContractClient::new(env, &id);
    let owner = Address::generate(env);
    client.initialize(&owner);
    let did = String::from_str(env, DID);
    client.create_did(&owner, &did);
    (client, owner, did)
}

fn s(env: &Env, v: &str) -> String {
    String::from_str(env, v)
}

fn key(env: &Env, fill: u8, len: usize) -> Bytes {
    let mut b = Bytes::new(env);
    for _ in 0..len {
        b.push_back(fill);
    }
    b
}

fn secp_key(env: &Env) -> Bytes {
    let mut b = Bytes::new(env);
    b.push_back(0x02);
    for _ in 0..32 {
        b.push_back(0x11);
    }
    b
}

// ── Resolution ──────────────────────────────────────────────────────────────

#[test]
fn bare_did_resolves_to_minimal_document() {
    let env = Env::default();
    let (client, owner, did) = setup(&env);

    let res = client.resolve_did_document(&did);
    let doc = res.did_document;
    assert_eq!(doc.id, did);
    assert_eq!(doc.controller, did);
    assert_eq!(doc.context.len(), 1);
    assert_eq!(
        doc.context.get(0).unwrap(),
        s(&env, "https://www.w3.org/ns/did/v1")
    );
    assert!(doc.verification_method.is_empty());
    assert!(doc.authentication.is_empty());
    assert!(doc.service.is_empty());

    assert_eq!(res.metadata.created, 1_700_000_000);
    assert_eq!(res.metadata.updated, 1_700_000_000);
    assert!(!res.metadata.deactivated);
    assert_eq!(res.metadata.controller_account, owner);
}

#[test]
fn keys_and_services_map_to_w3c_fields() {
    let env = Env::default();
    let (client, owner, did) = setup(&env);

    env.ledger().set_timestamp(1_700_000_500);
    client.add_did_key(
        &owner,
        &did,
        &s(&env, "key-1"),
        &DidKeyType::Ed25519,
        &key(&env, 0xAA, 32),
        &(PURPOSE_AUTHENTICATION | PURPOSE_ASSERTION_METHOD | PURPOSE_CAPABILITY_INVOCATION),
    );
    client.add_did_key(
        &owner,
        &did,
        &s(&env, "enc-1"),
        &DidKeyType::X25519,
        &key(&env, 0xBB, 32),
        &PURPOSE_KEY_AGREEMENT,
    );
    client.add_did_key(
        &owner,
        &did,
        &s(&env, "delegate"),
        &DidKeyType::Secp256k1,
        &secp_key(&env),
        &PURPOSE_CAPABILITY_DELEGATION,
    );
    client.add_did_service(
        &owner,
        &did,
        &s(&env, "records"),
        &s(&env, "RetinaXRecords"),
        &s(&env, "https://records.retinax.example/patients/42"),
    );

    let res = client.resolve_did_document(&did);
    let doc = res.did_document;

    // One suite context per key type, after the base DID context.
    assert_eq!(doc.context.len(), 4);
    assert_eq!(
        doc.context.get(1).unwrap(),
        s(&env, "https://w3id.org/security/suites/ed25519-2020/v1")
    );
    assert_eq!(
        doc.context.get(2).unwrap(),
        s(&env, "https://w3id.org/security/suites/x25519-2020/v1")
    );
    assert_eq!(
        doc.context.get(3).unwrap(),
        s(&env, "https://w3id.org/security/suites/secp256k1-2019/v1")
    );

    assert_eq!(doc.verification_method.len(), 3);
    let vm = doc.verification_method.get(0).unwrap();
    assert_eq!(vm.id, s(&env, "did:retinax:patient-42#key-1"));
    assert_eq!(vm.method_type, s(&env, "Ed25519VerificationKey2020"));
    assert_eq!(vm.controller, did);
    assert_eq!(vm.public_key, key(&env, 0xAA, 32));
    assert_eq!(
        doc.verification_method.get(1).unwrap().method_type,
        s(&env, "X25519KeyAgreementKey2020")
    );
    assert_eq!(
        doc.verification_method.get(2).unwrap().method_type,
        s(&env, "EcdsaSecp256k1VerificationKey2019")
    );

    let key1 = s(&env, "did:retinax:patient-42#key-1");
    let enc1 = s(&env, "did:retinax:patient-42#enc-1");
    let delegate = s(&env, "did:retinax:patient-42#delegate");
    assert_eq!(doc.authentication.len(), 1);
    assert_eq!(doc.authentication.get(0).unwrap(), key1);
    assert_eq!(doc.assertion_method.len(), 1);
    assert_eq!(doc.assertion_method.get(0).unwrap(), key1);
    assert_eq!(doc.capability_invocation.len(), 1);
    assert_eq!(doc.capability_invocation.get(0).unwrap(), key1);
    assert_eq!(doc.key_agreement.len(), 1);
    assert_eq!(doc.key_agreement.get(0).unwrap(), enc1);
    assert_eq!(doc.capability_delegation.len(), 1);
    assert_eq!(doc.capability_delegation.get(0).unwrap(), delegate);

    assert_eq!(doc.service.len(), 1);
    let svc = doc.service.get(0).unwrap();
    assert_eq!(svc.id, s(&env, "did:retinax:patient-42#records"));
    assert_eq!(svc.service_type, s(&env, "RetinaXRecords"));
    assert_eq!(
        svc.service_endpoint,
        s(&env, "https://records.retinax.example/patients/42")
    );

    // Writes bump `updated`, never `created`.
    assert_eq!(res.metadata.created, 1_700_000_000);
    assert_eq!(res.metadata.updated, 1_700_000_500);
}

#[test]
fn suite_context_is_listed_once_per_type() {
    let env = Env::default();
    let (client, owner, did) = setup(&env);
    for (i, frag) in ["key-1", "key-2", "key-3"].iter().enumerate() {
        client.add_did_key(
            &owner,
            &did,
            &s(&env, frag),
            &DidKeyType::Ed25519,
            &key(&env, i as u8, 32),
            &PURPOSE_AUTHENTICATION,
        );
    }
    let doc = client.resolve_did_document(&did).did_document;
    assert_eq!(doc.context.len(), 2);
    assert_eq!(doc.verification_method.len(), 3);
    assert_eq!(doc.authentication.len(), 3);
}

#[test]
fn bound_credentials_appear_in_metadata() {
    let env = Env::default();
    let (client, owner, did) = setup(&env);
    let cred = BytesN::from_array(&env, &[0xCC; 32]);
    client.bind_credential(&owner, &cred);
    client.update_did(&owner, &did);

    let meta = client.resolve_did_document(&did).metadata;
    assert_eq!(meta.credential_ids.len(), 1);
    assert_eq!(meta.credential_ids.get(0).unwrap(), cred);
}

#[test]
fn removed_keys_and_services_disappear_from_document() {
    let env = Env::default();
    let (client, owner, did) = setup(&env);
    client.add_did_key(
        &owner,
        &did,
        &s(&env, "key-1"),
        &DidKeyType::Ed25519,
        &key(&env, 1, 32),
        &PURPOSE_AUTHENTICATION,
    );
    client.add_did_key(
        &owner,
        &did,
        &s(&env, "key-2"),
        &DidKeyType::Ed25519,
        &key(&env, 2, 32),
        &PURPOSE_AUTHENTICATION,
    );
    client.add_did_service(
        &owner,
        &did,
        &s(&env, "hub"),
        &s(&env, "LinkedDomains"),
        &s(&env, "https://retinax.example"),
    );

    client.remove_did_key(&owner, &did, &s(&env, "key-1"));
    client.remove_did_service(&owner, &did, &s(&env, "hub"));

    let doc = client.resolve_did_document(&did).did_document;
    assert_eq!(doc.verification_method.len(), 1);
    assert_eq!(
        doc.verification_method.get(0).unwrap().id,
        s(&env, "did:retinax:patient-42#key-2")
    );
    assert_eq!(doc.authentication.len(), 1);
    assert!(doc.service.is_empty());
    assert_eq!(client.get_did_keys(&did).len(), 1);
    assert!(client.get_did_services(&did).is_empty());

    assert_eq!(
        client.try_remove_did_key(&owner, &did, &s(&env, "key-1")),
        Err(Ok(RecoveryError::RecordNotFound))
    );
    assert_eq!(
        client.try_remove_did_service(&owner, &did, &s(&env, "hub")),
        Err(Ok(RecoveryError::RecordNotFound))
    );
}

#[test]
fn deactivated_did_resolves_without_keys_or_services() {
    let env = Env::default();
    let (client, owner, did) = setup(&env);
    client.add_did_key(
        &owner,
        &did,
        &s(&env, "key-1"),
        &DidKeyType::Ed25519,
        &key(&env, 1, 32),
        &PURPOSE_AUTHENTICATION,
    );
    client.add_did_service(
        &owner,
        &did,
        &s(&env, "hub"),
        &s(&env, "LinkedDomains"),
        &s(&env, "https://retinax.example"),
    );
    client.deactivate_did(&owner, &did);

    let res = client.resolve_did_document(&did);
    assert!(res.metadata.deactivated);
    assert_eq!(res.did_document.id, did);
    assert!(res.did_document.verification_method.is_empty());
    assert!(res.did_document.authentication.is_empty());
    assert!(res.did_document.service.is_empty());

    // A deactivated DID can no longer be modified.
    assert_eq!(
        client.try_add_did_key(
            &owner,
            &did,
            &s(&env, "key-2"),
            &DidKeyType::Ed25519,
            &key(&env, 2, 32),
            &PURPOSE_AUTHENTICATION,
        ),
        Err(Ok(RecoveryError::OwnerDeactivated))
    );
}

#[test]
fn resolution_errors() {
    let env = Env::default();
    let (client, _owner, _did) = setup(&env);
    assert_eq!(
        client.try_resolve_did_document(&s(&env, "did:example:123")),
        Err(Ok(RecoveryError::InvalidInput))
    );
    assert_eq!(
        client.try_resolve_did_document(&s(&env, "did:retinax:")),
        Err(Ok(RecoveryError::InvalidInput))
    );
    assert_eq!(
        client.try_resolve_did_document(&s(&env, "did:retinax:unknown")),
        Err(Ok(RecoveryError::RecordNotFound))
    );
}

#[test]
fn overlong_did_is_rejected() {
    let env = Env::default();
    let (client, owner, _did) = setup(&env);
    let long = std::format!("did:retinax:{}", "a".repeat(120));
    assert!(long.len() > 128);
    let long = String::from_str(&env, &long);
    assert_eq!(
        client.try_resolve_did_document(&long),
        Err(Ok(RecoveryError::InvalidInput))
    );
    assert_eq!(
        client.try_create_did(&owner, &long),
        Err(Ok(RecoveryError::InvalidInput))
    );
}

// ── Write validation ────────────────────────────────────────────────────────

#[test]
fn key_validation() {
    let env = Env::default();
    let (client, owner, did) = setup(&env);
    let add = |frag: &str, kt: DidKeyType, pk: Bytes, purposes: u32| {
        client.try_add_did_key(&owner, &did, &s(&env, frag), &kt, &pk, &purposes)
    };
    let invalid = Err(Ok(RecoveryError::InvalidInput));

    // Wrong key lengths.
    assert_eq!(
        add(
            "k",
            DidKeyType::Ed25519,
            key(&env, 1, 31),
            PURPOSE_AUTHENTICATION
        ),
        invalid
    );
    assert_eq!(
        add(
            "k",
            DidKeyType::X25519,
            key(&env, 1, 33),
            PURPOSE_KEY_AGREEMENT
        ),
        invalid
    );
    assert_eq!(
        add(
            "k",
            DidKeyType::Secp256k1,
            key(&env, 2, 32),
            PURPOSE_AUTHENTICATION
        ),
        invalid
    );
    // secp256k1 keys must be compressed (0x02/0x03 prefix).
    assert_eq!(
        add(
            "k",
            DidKeyType::Secp256k1,
            key(&env, 4, 33),
            PURPOSE_AUTHENTICATION
        ),
        invalid
    );
    // Purposes: none, unknown bits, signing key for key agreement, X25519 for signing.
    assert_eq!(add("k", DidKeyType::Ed25519, key(&env, 1, 32), 0), invalid);
    assert_eq!(
        add("k", DidKeyType::Ed25519, key(&env, 1, 32), 1 << 5),
        invalid
    );
    assert_eq!(
        add(
            "k",
            DidKeyType::Ed25519,
            key(&env, 1, 32),
            PURPOSE_KEY_AGREEMENT
        ),
        invalid
    );
    assert_eq!(
        add(
            "k",
            DidKeyType::X25519,
            key(&env, 1, 32),
            PURPOSE_KEY_AGREEMENT | PURPOSE_AUTHENTICATION
        ),
        invalid
    );
    // Fragments: empty, illegal chars, too long.
    assert_eq!(
        add(
            "",
            DidKeyType::Ed25519,
            key(&env, 1, 32),
            PURPOSE_AUTHENTICATION
        ),
        invalid
    );
    assert_eq!(
        add(
            "key#1",
            DidKeyType::Ed25519,
            key(&env, 1, 32),
            PURPOSE_AUTHENTICATION
        ),
        invalid
    );
    assert_eq!(
        add(
            "key 1",
            DidKeyType::Ed25519,
            key(&env, 1, 32),
            PURPOSE_AUTHENTICATION
        ),
        invalid
    );
    assert_eq!(
        add(
            &"k".repeat(33),
            DidKeyType::Ed25519,
            key(&env, 1, 32),
            PURPOSE_AUTHENTICATION
        ),
        invalid
    );

    // Nothing was stored.
    assert!(client.get_did_keys(&did).is_empty());
    // Valid fragment characters are accepted.
    assert_eq!(
        add(
            "key_1.v2-a",
            DidKeyType::Ed25519,
            key(&env, 1, 32),
            PURPOSE_AUTHENTICATION
        ),
        Ok(Ok(()))
    );
}

#[test]
fn service_validation() {
    let env = Env::default();
    let (client, owner, did) = setup(&env);
    let add = |frag: &str, ty: &str, ep: &str| {
        client.try_add_did_service(&owner, &did, &s(&env, frag), &s(&env, ty), &s(&env, ep))
    };
    let invalid = Err(Ok(RecoveryError::InvalidInput));

    assert_eq!(add("svc", "", "https://a.example"), invalid);
    assert_eq!(add("svc", "Linked Domains", "https://a.example"), invalid);
    assert_eq!(add("svc", &"T".repeat(65), "https://a.example"), invalid);
    assert_eq!(add("svc", "LinkedDomains", ""), invalid);
    assert_eq!(add("svc", "LinkedDomains", "no-scheme"), invalid);
    assert_eq!(add("svc", "LinkedDomains", ":missing-scheme"), invalid);
    assert_eq!(
        add("svc", "LinkedDomains", "https://a.example/with space"),
        invalid
    );
    let long = std::format!("https://{}", "a".repeat(250));
    assert_eq!(add("svc", "LinkedDomains", &long), invalid);
    assert_eq!(
        add("bad/frag", "LinkedDomains", "https://a.example"),
        invalid
    );
    assert!(client.get_did_services(&did).is_empty());

    assert_eq!(add("svc", "LinkedDomains", "https://a.example"), Ok(Ok(())));
    assert_eq!(
        add("msg", "DIDCommMessaging", "did:retinax:relay#inbox"),
        Ok(Ok(()))
    );
}

#[test]
fn fragments_are_unique_across_keys_and_services() {
    let env = Env::default();
    let (client, owner, did) = setup(&env);
    client.add_did_key(
        &owner,
        &did,
        &s(&env, "main"),
        &DidKeyType::Ed25519,
        &key(&env, 1, 32),
        &PURPOSE_AUTHENTICATION,
    );
    assert_eq!(
        client.try_add_did_key(
            &owner,
            &did,
            &s(&env, "main"),
            &DidKeyType::Ed25519,
            &key(&env, 2, 32),
            &PURPOSE_AUTHENTICATION,
        ),
        Err(Ok(RecoveryError::AlreadyExists))
    );
    assert_eq!(
        client.try_add_did_service(
            &owner,
            &did,
            &s(&env, "main"),
            &s(&env, "LinkedDomains"),
            &s(&env, "https://a.example"),
        ),
        Err(Ok(RecoveryError::AlreadyExists))
    );
}

#[test]
fn keys_and_services_are_bounded() {
    let env = Env::default();
    let (client, owner, did) = setup(&env);
    for i in 0..MAX_DID_KEYS {
        client.add_did_key(
            &owner,
            &did,
            &s(&env, &std::format!("key-{}", i)),
            &DidKeyType::Ed25519,
            &key(&env, i as u8, 32),
            &PURPOSE_AUTHENTICATION,
        );
    }
    assert_eq!(
        client.try_add_did_key(
            &owner,
            &did,
            &s(&env, "overflow"),
            &DidKeyType::Ed25519,
            &key(&env, 0xFF, 32),
            &PURPOSE_AUTHENTICATION,
        ),
        Err(Ok(RecoveryError::LimitExceeded))
    );

    for i in 0..MAX_DID_SERVICES {
        client.add_did_service(
            &owner,
            &did,
            &s(&env, &std::format!("svc-{}", i)),
            &s(&env, "LinkedDomains"),
            &s(&env, "https://a.example"),
        );
    }
    assert_eq!(
        client.try_add_did_service(
            &owner,
            &did,
            &s(&env, "overflow"),
            &s(&env, "LinkedDomains"),
            &s(&env, "https://a.example"),
        ),
        Err(Ok(RecoveryError::LimitExceeded))
    );

    let doc = client.resolve_did_document(&did).did_document;
    assert_eq!(doc.verification_method.len(), MAX_DID_KEYS);
    assert_eq!(doc.service.len(), MAX_DID_SERVICES);
}

// ── Authorization ───────────────────────────────────────────────────────────

#[test]
fn only_controller_can_modify() {
    let env = Env::default();
    let (client, _owner, did) = setup(&env);
    let stranger = Address::generate(&env);
    assert_eq!(
        client.try_add_did_key(
            &stranger,
            &did,
            &s(&env, "key-1"),
            &DidKeyType::Ed25519,
            &key(&env, 1, 32),
            &PURPOSE_AUTHENTICATION,
        ),
        Err(Ok(RecoveryError::Unauthorized))
    );
    assert_eq!(
        client.try_add_did_service(
            &stranger,
            &did,
            &s(&env, "svc"),
            &s(&env, "LinkedDomains"),
            &s(&env, "https://a.example"),
        ),
        Err(Ok(RecoveryError::Unauthorized))
    );
    assert_eq!(
        client.try_add_did_key(
            &_owner,
            &s(&env, "did:retinax:nobody"),
            &s(&env, "key-1"),
            &DidKeyType::Ed25519,
            &key(&env, 1, 32),
            &PURPOSE_AUTHENTICATION,
        ),
        Err(Ok(RecoveryError::RecordNotFound))
    );
}

#[test]
fn modifications_require_controller_signature_and_emit_events() {
    let env = Env::default();
    let (client, owner, did) = setup(&env);
    client.add_did_key(
        &owner,
        &did,
        &s(&env, "key-1"),
        &DidKeyType::Ed25519,
        &key(&env, 1, 32),
        &PURPOSE_AUTHENTICATION,
    );
    assert!(env.auths().iter().any(|(addr, _)| *addr == owner));
    assert_eq!(
        env.events()
            .all()
            .filter_by_contract(&client.address)
            .events()
            .len(),
        1
    );
}
