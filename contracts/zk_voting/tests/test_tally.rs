#![cfg(test)]
#![allow(clippy::unwrap_used, clippy::expect_used)]

//! Tally correctness (#429): the tally must equal the number of valid,
//! unique votes, whatever mix of invalid votes is submitted alongside them.

extern crate std;

use proptest::prelude::*;
use soroban_sdk::{testutils::Address as _, Address, BytesN, Env, Vec};
use std::collections::BTreeSet;
use zk_verifier::verifier::{G1Point, G2Point};
use zk_verifier::Proof;
use zk_voting::ballot::VoteError;
use zk_voting::merkle::{make_leaf, MerkleTree};
use zk_voting::{ZkVoting, ZkVotingClient};

const OPTIONS: u32 = 3;

// ── Helpers ───────────────────────────────────────────────────────────────────

fn point(env: &Env, first: u8) -> G1Point {
    let mut x = [0u8; 32];
    x[0] = first;
    G1Point {
        x: BytesN::from_array(env, &x),
        y: BytesN::from_array(env, &[0u8; 32]),
    }
}

/// A proof the mock Bn254 verifier accepts (`a.x[0] == 1`, `c.x[0] == 1`,
/// `public_inputs[0][0] == 1`) or rejects (all zero).
fn proof(env: &Env, valid: bool) -> (Proof, Vec<BytesN<32>>) {
    let flag = u8::from(valid);
    let zero = BytesN::from_array(env, &[0u8; 32]);
    let proof = Proof {
        a: point(env, flag),
        b: G2Point {
            x: (zero.clone(), zero.clone()),
            y: (zero.clone(), zero),
        },
        c: point(env, flag),
    };
    let mut input = [0u8; 32];
    input[0] = flag;
    let mut inputs = Vec::new(env);
    inputs.push_back(BytesN::from_array(env, &input));
    (proof, inputs)
}

fn nullifier(env: &Env, id: u16) -> BytesN<32> {
    let mut raw = [0u8; 32];
    raw[..2].copy_from_slice(&id.to_be_bytes());
    BytesN::from_array(env, &raw)
}

/// A 3-option ballot with a Merkle root and verification key set.
fn setup(env: &Env) -> (Address, ZkVotingClient<'_>) {
    env.mock_all_auths();
    let client = ZkVotingClient::new(env, &env.register(ZkVoting, ()));
    let admin = Address::generate(env);
    client.initialize(&admin, &OPTIONS);

    let mut leaves = Vec::new(env);
    for i in 0u8..4 {
        leaves.push_back(make_leaf(env, i));
    }
    client.set_merkle_root(&admin, &MerkleTree::new(env, leaves).root());

    let g1 = point(env, 0);
    let zero = BytesN::from_array(env, &[0u8; 32]);
    let g2 = zk_verifier::vk::G2Point {
        x: (zero.clone(), zero.clone()),
        y: (zero.clone(), zero),
    };
    let mut ic = Vec::new(env);
    ic.push_back(g1.clone());
    client.set_verification_key(
        &admin,
        &zk_verifier::vk::VerificationKey {
            alpha_g1: g1,
            beta_g2: g2.clone(),
            gamma_g2: g2.clone(),
            delta_g2: g2,
            ic,
        },
    );
    (admin, client)
}

/// One submitted vote: nullifier id, chosen option, and whether its proof verifies.
#[derive(Clone, Debug)]
struct Vote {
    nullifier: u16,
    option: u32,
    valid_proof: bool,
}

/// Expected outcome under the contract's rules, applied in order: the option
/// must exist, the nullifier must be unspent, then the proof must verify. A
/// nullifier is only spent by a vote that is counted.
fn expected_outcome(vote: &Vote, spent: &BTreeSet<u16>) -> Result<(), VoteError> {
    if vote.option >= OPTIONS {
        Err(VoteError::InvalidOption)
    } else if spent.contains(&vote.nullifier) {
        Err(VoteError::NullifierAlreadyUsed)
    } else if !vote.valid_proof {
        Err(VoteError::InvalidProof)
    } else {
        Ok(())
    }
}

/// Casts every vote, checking each result against the model, and returns the
/// expected per-option tallies plus the spent nullifiers.
fn cast_all(
    env: &Env,
    client: &ZkVotingClient,
    votes: &[Vote],
) -> ([u64; OPTIONS as usize], BTreeSet<u16>) {
    let mut tallies = [0u64; OPTIONS as usize];
    let mut spent = BTreeSet::new();
    for vote in votes {
        let expected = expected_outcome(vote, &spent);
        let (p, inputs) = proof(env, vote.valid_proof);
        let actual =
            client.try_cast_vote(&nullifier(env, vote.nullifier), &vote.option, &p, &inputs);
        match expected {
            Ok(()) => {
                assert_eq!(actual, Ok(Ok(())), "vote {vote:?} should be counted");
                tallies[vote.option as usize] += 1;
                spent.insert(vote.nullifier);
            }
            Err(error) => assert_eq!(actual, Err(Ok(error)), "vote {vote:?}"),
        }
    }
    (tallies, spent)
}

fn assert_tallies(client: &ZkVotingClient, expected: &[u64; OPTIONS as usize]) {
    let results = client.get_results();
    assert_eq!(results.option_count, OPTIONS);
    let actual: std::vec::Vec<u64> = results.tallies.iter().collect();
    assert_eq!(actual, expected.to_vec());
}

// ── Deterministic mixed batches ───────────────────────────────────────────────

#[test]
fn tally_counts_only_valid_unique_votes_in_a_mixed_batch() {
    let env = Env::default();
    let (_admin, client) = setup(&env);

    let votes = [
        Vote {
            nullifier: 1,
            option: 0,
            valid_proof: true,
        },
        Vote {
            nullifier: 2,
            option: 1,
            valid_proof: true,
        },
        Vote {
            nullifier: 1,
            option: 2,
            valid_proof: true,
        }, // reused nullifier
        Vote {
            nullifier: 3,
            option: 2,
            valid_proof: false,
        }, // bad proof
        Vote {
            nullifier: 3,
            option: 2,
            valid_proof: true,
        }, // retry after bad proof counts
        Vote {
            nullifier: 4,
            option: 3,
            valid_proof: true,
        }, // option out of range
        Vote {
            nullifier: 4,
            option: 0,
            valid_proof: true,
        },
        Vote {
            nullifier: 5,
            option: u32::MAX,
            valid_proof: false,
        },
        Vote {
            nullifier: 2,
            option: 1,
            valid_proof: false,
        }, // reused and bad proof
    ];
    let (expected, spent) = cast_all(&env, &client, &votes);

    assert_eq!(expected, [2, 1, 1]);
    assert_tallies(&client, &expected);
    assert_eq!(spent.len(), 4);
}

#[test]
fn tally_sum_equals_unique_valid_votes_for_a_large_batch() {
    let env = Env::default();
    let (_admin, client) = setup(&env);

    // 300 votes: every 3rd has a bad proof, every 5th reuses an earlier
    // nullifier, every 7th picks a missing option.
    let votes: std::vec::Vec<Vote> = (0u16..300)
        .map(|i| Vote {
            nullifier: if i % 5 == 4 { i / 2 } else { i },
            option: if i % 7 == 6 {
                OPTIONS + u32::from(i % 2)
            } else {
                u32::from(i) % OPTIONS
            },
            valid_proof: i % 3 != 2,
        })
        .collect();
    let (expected, spent) = cast_all(&env, &client, &votes);

    assert_tallies(&client, &expected);
    let total: u64 = client.get_results().tallies.iter().sum();
    assert_eq!(total, spent.len() as u64);
    assert!(total > 0 && total < votes.len() as u64);
}

#[test]
fn spent_nullifiers_match_counted_votes_only() {
    let env = Env::default();
    let (_admin, client) = setup(&env);

    let votes = [
        Vote {
            nullifier: 10,
            option: 0,
            valid_proof: true,
        },
        Vote {
            nullifier: 11,
            option: 0,
            valid_proof: false,
        },
        Vote {
            nullifier: 12,
            option: 9,
            valid_proof: true,
        },
    ];
    cast_all(&env, &client, &votes);

    assert!(client.is_nullifier_used(&nullifier(&env, 10)));
    assert!(!client.is_nullifier_used(&nullifier(&env, 11)));
    assert!(!client.is_nullifier_used(&nullifier(&env, 12)));
}

#[test]
fn votes_after_close_do_not_change_the_tally() {
    let env = Env::default();
    let (admin, client) = setup(&env);
    let (expected, _) = cast_all(
        &env,
        &client,
        &[
            Vote {
                nullifier: 1,
                option: 1,
                valid_proof: true,
            },
            Vote {
                nullifier: 2,
                option: 1,
                valid_proof: true,
            },
        ],
    );

    client.close_ballot(&admin);
    let (p, inputs) = proof(&env, true);
    assert!(client
        .try_cast_vote(&nullifier(&env, 3), &0, &p, &inputs)
        .is_err());

    assert_tallies(&client, &expected);
    assert!(client.get_results().closed);
    assert!(!client.is_nullifier_used(&nullifier(&env, 3)));
}

#[test]
fn votes_without_a_merkle_root_are_never_counted() {
    let env = Env::default();
    env.mock_all_auths();
    let client = ZkVotingClient::new(&env, &env.register(ZkVoting, ()));
    client.initialize(&Address::generate(&env), &OPTIONS);

    let (p, inputs) = proof(&env, true);
    assert_eq!(
        client.try_cast_vote(&nullifier(&env, 1), &0, &p, &inputs),
        Err(Ok(VoteError::MerkleRootNotSet))
    );
    assert_tallies(&client, &[0, 0, 0]);
}

// ── Property test ─────────────────────────────────────────────────────────────

fn vote_strategy() -> impl Strategy<Value = Vote> {
    // A small nullifier space forces frequent reuse; options 0..5 include
    // two that do not exist.
    (0u16..12, 0u32..5, any::<bool>()).prop_map(|(nullifier, option, valid_proof)| Vote {
        nullifier,
        option,
        valid_proof,
    })
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(48))]

    #[test]
    fn tally_always_equals_valid_unique_votes(votes in prop::collection::vec(vote_strategy(), 0..40)) {
        let env = Env::default();
        let (_admin, client) = setup(&env);
        let (expected, spent) = cast_all(&env, &client, &votes);

        let results = client.get_results();
        let actual: std::vec::Vec<u64> = results.tallies.iter().collect();
        prop_assert_eq!(actual, expected.to_vec());
        prop_assert_eq!(results.tallies.iter().sum::<u64>(), spent.len() as u64);
        for id in 0u16..12 {
            prop_assert_eq!(client.is_nullifier_used(&nullifier(&env, id)), spent.contains(&id));
        }
    }
}
