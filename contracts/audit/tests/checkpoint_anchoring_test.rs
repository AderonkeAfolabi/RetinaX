#![cfg(test)]
#![allow(clippy::unwrap_used)]

use audit::contract::{AuditContract, AuditContractClient};
use soroban_sdk::{symbol_short, testutils::Address as _, Address, BytesN, Env, Vec};

fn setup() -> (Env, AuditContractClient<'static>, Address) {
    let env = Env::default();
    env.mock_all_auths();
    let id = env.register(AuditContract, ());
    let client = AuditContractClient::new(&env, &id);
    let admin = Address::generate(&env);
    client.initialize(&admin);
    (env, client, admin)
}

#[test]
fn test_checkpoint_empty_segment() {
    let (env, client, _admin) = setup();
    let segment = symbol_short!("EMPTY");
    client.create_segment(&segment);

    let root = client.checkpoint(&segment);
    let zero_root = BytesN::from_array(&env, &[0u8; 32]);
    assert_eq!(root, zero_root);

    let latest = client.get_latest_checkpoint(&segment);
    assert_eq!(latest.entry_count, 0);
    assert_eq!(latest.root, zero_root);
}

#[test]
fn test_checkpoint_anchoring_and_inclusion_verification() {
    let (env, client, _admin) = setup();
    let segment = symbol_short!("LOG_1");
    client.create_segment(&segment);

    let actor1 = Address::generate(&env);
    let actor2 = Address::generate(&env);
    let actor3 = Address::generate(&env);

    let seq1 = client.append_entry(
        &segment,
        &actor1,
        &symbol_short!("LOGIN"),
        &symbol_short!("USER"),
        &symbol_short!("OK"),
    );
    let seq2 = client.append_entry(
        &segment,
        &actor2,
        &symbol_short!("ACCESS"),
        &symbol_short!("RECORD"),
        &symbol_short!("OK"),
    );
    let seq3 = client.append_entry(
        &segment,
        &actor3,
        &symbol_short!("EXPORT"),
        &symbol_short!("REPORT"),
        &symbol_short!("DENY"),
    );

    assert_eq!(seq1, 1);
    assert_eq!(seq2, 2);
    assert_eq!(seq3, 3);

    // Anchor first checkpoint
    let root = client.checkpoint(&segment);
    let latest = client.get_latest_checkpoint(&segment);
    assert_eq!(latest.entry_count, 3);
    assert_eq!(latest.root, root);

    // Verify inclusion for each entry
    let entries = client.get_entries(&segment);
    assert_eq!(entries.len(), 3);

    for (idx, entry) in entries.iter().enumerate() {
        let seq = entry.sequence;
        let leaf_hash = client.compute_entry_leaf_hash(&entry);
        let proof = client.get_inclusion_proof(&segment, &seq);

        let valid = client.verify_inclusion(&leaf_hash, &(idx as u64), &3u64, &proof, &root);
        assert!(
            valid,
            "Inclusion proof verification failed for entry index {}",
            idx
        );
    }
}

#[test]
fn test_tampered_leaf_or_proof_rejected() {
    let (env, client, _admin) = setup();
    let segment = symbol_short!("TAMPER");
    client.create_segment(&segment);

    let actor = Address::generate(&env);
    client.append_entry(
        &segment,
        &actor,
        &symbol_short!("ACT1"),
        &symbol_short!("TGT1"),
        &symbol_short!("RES1"),
    );
    client.append_entry(
        &segment,
        &actor,
        &symbol_short!("ACT2"),
        &symbol_short!("TGT2"),
        &symbol_short!("RES2"),
    );

    let root = client.checkpoint(&segment);
    let entries = client.get_entries(&segment);
    let entry0 = entries.get(0).unwrap();

    let leaf_hash = client.compute_entry_leaf_hash(&entry0);
    let proof = client.get_inclusion_proof(&segment, &1u64);

    // Valid check passes
    assert!(client.verify_inclusion(&leaf_hash, &0u64, &2u64, &proof, &root));

    // Tampered leaf hash fails
    let fake_leaf = BytesN::from_array(&env, &[0xffu8; 32]);
    assert!(!client.verify_inclusion(&fake_leaf, &0u64, &2u64, &proof, &root));

    // Tampered root fails
    let fake_root = BytesN::from_array(&env, &[0xeeu8; 32]);
    assert!(!client.verify_inclusion(&leaf_hash, &0u64, &2u64, &proof, &fake_root));

    // Tampered siblings fail
    let mut bad_proof: Vec<BytesN<32>> = Vec::new(&env);
    bad_proof.push_back(BytesN::from_array(&env, &[0xddu8; 32]));
    assert!(!client.verify_inclusion(&leaf_hash, &0u64, &2u64, &bad_proof, &root));
}

#[test]
fn test_multiple_checkpoints_history() {
    let (env, client, _admin) = setup();
    let segment = symbol_short!("HIST");
    client.create_segment(&segment);

    let actor = Address::generate(&env);
    client.append_entry(
        &segment,
        &actor,
        &symbol_short!("E1"),
        &symbol_short!("T1"),
        &symbol_short!("OK"),
    );
    let root1 = client.checkpoint(&segment);

    client.append_entry(
        &segment,
        &actor,
        &symbol_short!("E2"),
        &symbol_short!("T2"),
        &symbol_short!("OK"),
    );
    let root2 = client.checkpoint(&segment);
    assert_ne!(root1, root2);

    let history = client.get_checkpoints(&segment);
    assert_eq!(history.len(), 2);
    assert_eq!(history.get(0).unwrap().root, root1);
    assert_eq!(history.get(1).unwrap().root, root2);
}

#[test]
fn test_checkpoint_nonexistent_segment() {
    let (_env, client, _admin) = setup();
    let segment = symbol_short!("NO_SEG");
    let err = client.try_checkpoint(&segment);
    assert!(err.is_err());
}
