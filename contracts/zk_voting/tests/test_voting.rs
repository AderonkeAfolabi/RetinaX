#![cfg(test)]
#![allow(clippy::unwrap_used, clippy::expect_used)]

use soroban_sdk::{testutils::Address as _, Address, BytesN, Env, Vec};
use zk_verifier::verifier::{G1Point, G2Point};
use zk_verifier::Proof;
use zk_voting::ballot::VoteError;
use zk_voting::merkle::{make_leaf, MerkleTree};
use zk_voting::{ZkVoting, ZkVotingClient};

// ── Helpers ───────────────────────────────────────────────────────────────────

/// Build a valid Groth16 proof (matches Bn254Verifier mock rules:
/// a[0]==1, c[0]==1, public_inputs[0][0]==1).
fn valid_proof(env: &Env) -> (Proof, Vec<BytesN<32>>) {
    // a is a G1Point: x=[1,0..], y=[0..]
    let mut ax = [0u8; 32];
    ax[0] = 1;
    let ay = [0u8; 32];

    // b is a G2Point: x=(x0,x1), y=(y0,y1) — all zeros is fine for the mock
    let bx0 = [0u8; 32];
    let bx1 = [0u8; 32];
    let by0 = [0u8; 32];
    let by1 = [0u8; 32];

    // c is a G1Point: x=[1,0..], y=[0..]
    let mut cx = [0u8; 32];
    cx[0] = 1;
    let cy = [0u8; 32];

    let proof = Proof {
        a: G1Point {
            x: BytesN::from_array(env, &ax),
            y: BytesN::from_array(env, &ay),
        },
        b: G2Point {
            x: (BytesN::from_array(env, &bx0), BytesN::from_array(env, &bx1)),
            y: (BytesN::from_array(env, &by0), BytesN::from_array(env, &by1)),
        },
        c: G1Point {
            x: BytesN::from_array(env, &cx),
            y: BytesN::from_array(env, &cy),
        },
    };

    let mut pi = [0u8; 32];
    pi[0] = 1;

    let mut inputs: Vec<BytesN<32>> = Vec::new(env);
    inputs.push_back(BytesN::from_array(env, &pi));
    (proof, inputs)
}

/// Build an invalid proof (a.x[0]==0 fails the mock verifier).
fn invalid_proof(env: &Env) -> (Proof, Vec<BytesN<32>>) {
    // All-zero G1/G2 points — mock verifier rejects because a.x[0] != 1
    let z32 = [0u8; 32];

    let proof = Proof {
        a: G1Point {
            x: BytesN::from_array(env, &z32),
            y: BytesN::from_array(env, &z32),
        },
        b: G2Point {
            x: (BytesN::from_array(env, &z32), BytesN::from_array(env, &z32)),
            y: (BytesN::from_array(env, &z32), BytesN::from_array(env, &z32)),
        },
        c: G1Point {
            x: BytesN::from_array(env, &z32),
            y: BytesN::from_array(env, &z32),
        },
    };

    let mut inputs: Vec<BytesN<32>> = Vec::new(env);
    inputs.push_back(BytesN::from_array(env, &z32));
    (proof, inputs)
}

/// Make a 32-byte nullifier from a seed byte.
fn nullifier(env: &Env, seed: u8) -> BytesN<32> {
    let mut raw = [0u8; 32];
    raw[0] = seed;
    BytesN::from_array(env, &raw)
}

/// Deploy and initialise a fresh ballot with 3 options + a Merkle root.
fn setup() -> (Env, Address, ZkVotingClient<'static>, BytesN<32>) {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(ZkVoting, ());
    let client = ZkVotingClient::new(&env, &contract_id);
    let admin = Address::generate(&env);

    client.initialize(&admin, &3u32);

    // Build a 4-leaf Merkle tree and set the root.
    let mut leaves: Vec<BytesN<32>> = Vec::new(&env);
    for i in 0u8..4 {
        leaves.push_back(make_leaf(&env, i));
    }
    let tree = MerkleTree::new(&env, leaves);
    let root = tree.root();

    client.set_merkle_root(&admin, &root);

    // Setup verification key
    let g1 = zk_verifier::vk::G1Point {
        x: BytesN::from_array(&env, &[0u8; 32]),
        y: BytesN::from_array(&env, &[0u8; 32]),
    };
    let g2 = zk_verifier::vk::G2Point {
        x: (
            BytesN::from_array(&env, &[0u8; 32]),
            BytesN::from_array(&env, &[0u8; 32]),
        ),
        y: (
            BytesN::from_array(&env, &[0u8; 32]),
            BytesN::from_array(&env, &[0u8; 32]),
        ),
    };
    let mut ic = Vec::new(&env);
    ic.push_back(g1.clone());
    let vk = zk_verifier::vk::VerificationKey {
        alpha_g1: g1.clone(),
        beta_g2: g2.clone(),
        gamma_g2: g2.clone(),
        delta_g2: g2.clone(),
        ic,
    };
    client.set_verification_key(&admin, &vk);

    (env, admin, client, root)
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[test]
fn test_valid_vote_increments_tally() {
    let (env, _admin, client, _root) = setup();
    let (proof, inputs) = valid_proof(&env);

    client.cast_vote(&nullifier(&env, 1), &0u32, &proof, &inputs);

    let results = client.get_results();
    assert_eq!(results.tallies.get(0).unwrap(), 1u64);
    assert_eq!(results.tallies.get(1).unwrap(), 0u64);
    assert_eq!(results.tallies.get(2).unwrap(), 0u64);
}

#[test]
fn test_double_vote_rejected() {
    let (env, _admin, client, _root) = setup();
    let (proof1, inputs1) = valid_proof(&env);
    let (proof2, inputs2) = valid_proof(&env);
    let n = nullifier(&env, 2);

    // First vote succeeds.
    client.cast_vote(&n, &0u32, &proof1, &inputs1);

    // Second vote with same nullifier must fail.
    let result = client.try_cast_vote(&n, &1u32, &proof2, &inputs2);
    assert!(result.is_err());

    // Tally unchanged after the rejected vote.
    let results = client.get_results();
    assert_eq!(results.tallies.get(0).unwrap(), 1u64);
    assert_eq!(results.tallies.get(1).unwrap(), 0u64);
}

#[test]
fn test_invalid_proof_rejected() {
    let (env, _admin, client, _root) = setup();
    let (proof, inputs) = invalid_proof(&env);

    let result = client.try_cast_vote(&nullifier(&env, 3), &0u32, &proof, &inputs);
    assert!(result.is_err());

    let results = client.get_results();
    assert_eq!(results.tallies.get(0).unwrap(), 0u64);
}

#[test]
fn test_invalid_option_rejected() {
    let (env, _admin, client, _root) = setup();
    let (proof, inputs) = valid_proof(&env);

    // Option 3 is out of range (valid: 0, 1, 2).
    let result = client.try_cast_vote(&nullifier(&env, 4), &3u32, &proof, &inputs);
    assert!(result.is_err());
}

#[test]
fn test_ballot_results_are_public() {
    let (env, _admin, client, _root) = setup();

    for (seed, option) in [(1u8, 0u32), (2, 0), (3, 1), (4, 2)] {
        let (proof, inputs) = valid_proof(&env);
        client.cast_vote(&nullifier(&env, seed), &option, &proof, &inputs);
    }

    let results = client.get_results();
    let total: u64 = results.tallies.iter().sum();
    assert_eq!(total, 4u64);
    assert_eq!(results.tallies.get(0).unwrap(), 2u64);
    assert_eq!(results.tallies.get(1).unwrap(), 1u64);
    assert_eq!(results.tallies.get(2).unwrap(), 1u64);
}

#[test]
fn test_voting_closed_rejects_votes() {
    let (env, admin, client, _root) = setup();

    client.close_ballot(&admin);

    let (proof, inputs) = valid_proof(&env);
    let result = client.try_cast_vote(&nullifier(&env, 5), &0u32, &proof, &inputs);
    assert!(result.is_err());

    let results = client.get_results();
    assert!(results.closed);
}

#[test]
fn test_nullifier_tracking() {
    let (env, _admin, client, _root) = setup();
    let n = nullifier(&env, 6);

    assert!(!client.is_nullifier_used(&n));

    let (proof, inputs) = valid_proof(&env);
    client.cast_vote(&n, &2u32, &proof, &inputs);

    assert!(client.is_nullifier_used(&n));
}

#[test]
fn test_merkle_proof_verification() {
    let env = Env::default();

    let mut leaves: Vec<BytesN<32>> = Vec::new(&env);
    for i in 0u8..4 {
        leaves.push_back(make_leaf(&env, i));
    }

    let tree = MerkleTree::new(&env, leaves);
    let root = tree.root();

    for idx in 0u32..4 {
        let leaf = tree.leaf(idx);
        let proof = tree.proof(&env, idx);
        assert!(
            MerkleTree::verify_proof(&env, &root, &leaf, idx, &proof),
            "proof failed for leaf {idx}"
        );
    }
}

#[test]
fn test_tampered_merkle_leaf_fails() {
    let env = Env::default();

    let mut leaves: Vec<BytesN<32>> = Vec::new(&env);
    for i in 0u8..4 {
        leaves.push_back(make_leaf(&env, i));
    }

    let tree = MerkleTree::new(&env, leaves);
    let root = tree.root();
    let bogus = make_leaf(&env, 0xFF);
    let proof = tree.proof(&env, 0);

    assert!(!MerkleTree::verify_proof(&env, &root, &bogus, 0, &proof));
}

// ── Double-vote prevention (issue #428) ───────────────────────────────────────

/// Verification key that the mock verifier accepts, as used by `setup`.
fn mock_vk(env: &Env) -> zk_verifier::vk::VerificationKey {
    let zero = zk_verifier::vk::G1Point {
        x: BytesN::from_array(env, &[0u8; 32]),
        y: BytesN::from_array(env, &[0u8; 32]),
    };
    let zero_g2 = zk_verifier::vk::G2Point {
        x: (
            BytesN::from_array(env, &[0u8; 32]),
            BytesN::from_array(env, &[0u8; 32]),
        ),
        y: (
            BytesN::from_array(env, &[0u8; 32]),
            BytesN::from_array(env, &[0u8; 32]),
        ),
    };
    let mut ic = Vec::new(env);
    ic.push_back(zero.clone());
    zk_verifier::vk::VerificationKey {
        alpha_g1: zero.clone(),
        beta_g2: zero_g2.clone(),
        gamma_g2: zero_g2.clone(),
        delta_g2: zero_g2.clone(),
        ic,
    }
}

/// Deploy a second, independent ballot in the same environment.
fn register_ballot<'a>(env: &'a Env, root: &BytesN<32>) -> ZkVotingClient<'a> {
    let contract_id = env.register(ZkVoting, ());
    let client = ZkVotingClient::new(env, &contract_id);
    let admin = Address::generate(env);
    client.initialize(&admin, &3u32);
    client.set_merkle_root(&admin, root);
    client.set_verification_key(&admin, &mock_vk(env));
    client
}

#[test]
fn test_replayed_proof_with_spent_nullifier_is_rejected() {
    let (env, _admin, client, _root) = setup();
    let n = nullifier(&env, 10);
    let (proof, inputs) = valid_proof(&env);
    client.cast_vote(&n, &0u32, &proof, &inputs);

    // Replaying the same nullifier with a valid proof must fail.
    let (replay, replay_inputs) = valid_proof(&env);
    assert_eq!(
        client.try_cast_vote(&n, &0u32, &replay, &replay_inputs),
        Err(Ok(VoteError::NullifierAlreadyUsed))
    );
    assert_eq!(client.get_results().tallies.get(0).unwrap(), 1u64);
}

#[test]
fn test_same_nullifier_is_rejected_for_a_different_option() {
    let (env, _admin, client, _root) = setup();
    let n = nullifier(&env, 11);
    let (proof, inputs) = valid_proof(&env);
    client.cast_vote(&n, &0u32, &proof, &inputs);

    let (second, second_inputs) = valid_proof(&env);
    assert_eq!(
        client.try_cast_vote(&n, &2u32, &second, &second_inputs),
        Err(Ok(VoteError::NullifierAlreadyUsed))
    );

    let results = client.get_results();
    assert_eq!(results.tallies.get(0).unwrap(), 1u64);
    assert_eq!(results.tallies.get(2).unwrap(), 0u64);
}

#[test]
fn test_rejected_proof_does_not_spend_the_nullifier() {
    let (env, _admin, client, _root) = setup();
    let n = nullifier(&env, 12);
    let (bad, bad_inputs) = invalid_proof(&env);
    assert_eq!(
        client.try_cast_vote(&n, &0u32, &bad, &bad_inputs),
        Err(Ok(VoteError::InvalidProof))
    );
    assert!(!client.is_nullifier_used(&n));

    // The same nullifier can still be used once a valid proof is supplied.
    let (proof, inputs) = valid_proof(&env);
    client.cast_vote(&n, &0u32, &proof, &inputs);
    assert!(client.is_nullifier_used(&n));
    assert_eq!(client.get_results().tallies.get(0).unwrap(), 1u64);
}

#[test]
fn test_distinct_nullifiers_are_each_counted_once() {
    let (env, _admin, client, _root) = setup();
    let first = nullifier(&env, 13);
    let second = nullifier(&env, 14);
    let (p1, i1) = valid_proof(&env);
    let (p2, i2) = valid_proof(&env);
    let (p3, i3) = valid_proof(&env);
    client.cast_vote(&first, &1u32, &p1, &i1);
    client.cast_vote(&second, &1u32, &p2, &i2);

    // Replaying the first nullifier does not add a third vote.
    assert_eq!(
        client.try_cast_vote(&first, &1u32, &p3, &i3),
        Err(Ok(VoteError::NullifierAlreadyUsed))
    );
    assert_eq!(client.get_results().tallies.get(1).unwrap(), 2u64);
}

#[test]
fn test_nullifier_spent_on_one_ballot_is_free_on_another() {
    let (env, _admin, client, root) = setup();
    let other = register_ballot(&env, &root);
    let n = nullifier(&env, 15);

    let (p1, i1) = valid_proof(&env);
    client.cast_vote(&n, &0u32, &p1, &i1);
    assert!(client.is_nullifier_used(&n));

    // Each ballot keeps its own nullifier set, so the same tag is accepted once
    // on the second ballot. Nullifiers are scoped per ballot, not globally.
    assert!(!other.is_nullifier_used(&n));
    let (p2, i2) = valid_proof(&env);
    other.cast_vote(&n, &0u32, &p2, &i2);
    assert_eq!(other.get_results().tallies.get(0).unwrap(), 1u64);

    // ...and is still rejected on the ballot that spent it.
    let (p3, i3) = valid_proof(&env);
    assert_eq!(
        client.try_cast_vote(&n, &0u32, &p3, &i3),
        Err(Ok(VoteError::NullifierAlreadyUsed))
    );
}

// ── Merkle inclusion proof edge cases (issue #530) ────────────────────────────

/// A tree over the `count` distinct leaves `make_leaf(0)..=make_leaf(count - 1)`.
fn merkle_tree(env: &Env, count: u8) -> MerkleTree {
    let mut leaves: Vec<BytesN<32>> = Vec::new(env);
    for i in 0..count {
        leaves.push_back(make_leaf(env, i));
    }
    MerkleTree::new(env, leaves)
}

#[test]
fn test_single_leaf_tree_proves_only_its_leaf() {
    let env = Env::default();
    let tree = merkle_tree(&env, 1);
    let root = tree.root();
    let leaf = make_leaf(&env, 0);
    let other = make_leaf(&env, 1);
    let proof = tree.proof(&env, 0);

    // The only leaf is the root itself, so its proof has no siblings.
    assert_eq!(root, leaf);
    assert!(proof.is_empty());
    assert!(MerkleTree::verify_proof(&env, &root, &leaf, 0, &proof));
    assert!(!MerkleTree::verify_proof(&env, &root, &other, 0, &proof));
}

#[test]
fn test_odd_leaf_count_tree_proves_every_leaf() {
    let env = Env::default();
    for count in [3u8, 5, 7] {
        let tree = merkle_tree(&env, count);
        let root = tree.root();
        for i in 0..count {
            let idx = u32::from(i);
            let leaf = make_leaf(&env, i);
            let proof = tree.proof(&env, idx);
            assert!(
                MerkleTree::verify_proof(&env, &root, &leaf, idx, &proof),
                "proof failed for leaf {i} of {count}"
            );
        }

        // The padding slot after the last leaf must not admit a new leaf.
        let idx = u32::from(count);
        let outsider = make_leaf(&env, count);
        let proof = tree.proof(&env, idx);
        assert!(
            !MerkleTree::verify_proof(&env, &root, &outsider, idx, &proof),
            "padding admitted a new leaf in a tree of {count}"
        );
    }
}

#[test]
fn test_last_leaf_proof_is_valid() {
    let env = Env::default();
    // Full trees, and odd trees where the last leaf is paired with its padding.
    for count in [2u8, 3, 4, 5, 8] {
        let tree = merkle_tree(&env, count);
        let last = count - 1;
        let idx = u32::from(last);
        let leaf = make_leaf(&env, last);
        let proof = tree.proof(&env, idx);
        assert!(
            MerkleTree::verify_proof(&env, &tree.root(), &leaf, idx, &proof),
            "last-leaf proof failed for {count} leaves"
        );
    }
}

#[test]
fn test_swapped_sibling_order_is_rejected() {
    let env = Env::default();
    let tree = merkle_tree(&env, 4);
    let root = tree.root();

    for i in 0u8..4 {
        let idx = u32::from(i);
        let leaf = make_leaf(&env, i);
        let proof = tree.proof(&env, idx);

        // Siblings listed root-first instead of leaf-first.
        let mut swapped: Vec<BytesN<32>> = Vec::new(&env);
        swapped.push_back(proof.get(1).unwrap());
        swapped.push_back(proof.get(0).unwrap());
        assert!(
            !MerkleTree::verify_proof(&env, &root, &leaf, idx, &swapped),
            "reordered siblings accepted for leaf {i}"
        );

        // Sibling hashed on the wrong side of the leaf.
        assert!(
            !MerkleTree::verify_proof(&env, &root, &leaf, idx ^ 1, &proof),
            "wrong-side sibling accepted for leaf {i}"
        );
    }
}

#[test]
fn test_proof_for_leaf_not_in_tree_is_rejected() {
    let env = Env::default();
    let root = merkle_tree(&env, 4).root();

    // A genuine proof, but from a larger tree that does contain the leaf.
    let larger = merkle_tree(&env, 8);
    let larger_root = larger.root();
    let outsider = make_leaf(&env, 4);
    let proof = larger.proof(&env, 4);
    assert!(
        MerkleTree::verify_proof(&env, &larger_root, &outsider, 4, &proof),
        "proof must be valid in its own tree"
    );
    assert!(
        !MerkleTree::verify_proof(&env, &root, &outsider, 4, &proof),
        "leaf outside the tree accepted"
    );
}
