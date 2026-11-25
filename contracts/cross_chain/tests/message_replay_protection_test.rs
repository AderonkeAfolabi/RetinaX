#![allow(clippy::unwrap_used, clippy::expect_used)]

//! # Cross-Chain Message Replay Protection Tests (Issue #439)
//!
//! Acceptance criteria:
//! - A relayed message must be accepted once only.
//! - Submit identical message twice -> second attempt rejected with `AlreadyProcessed`.
//! - Test across different source chains (Ethereum, Polygon, Arbitrum, Optimism, Base, Solana).
//! - Cross-chain message ID collision / replay between different source chains is rejected.
//! - Multiple relayers attempting to replay the same message are rejected.

use cross_chain::{
    CrossChainContract, CrossChainContractClient, CrossChainError, CrossChainMessage,
};
use soroban_sdk::{
    contract, contractimpl, symbol_short, testutils::Address as _, Address, Bytes, Env, String,
};

#[contract]
struct MockVisionRecords;

#[contractimpl]
impl MockVisionRecords {
    pub fn grant_cross_chain_access(
        env: Env,
        bridge_caller: Address,
        patient: Address,
        payload: Bytes,
    ) {
        bridge_caller.require_auth();
        env.storage()
            .instance()
            .set(&symbol_short!("LAST_PAT"), &patient);
        env.storage()
            .instance()
            .set(&symbol_short!("LAST_PAY"), &payload);
    }
}

fn s(env: &Env, val: &str) -> String {
    String::from_str(env, val)
}

struct TestFixture {
    env: Env,
    client: CrossChainContractClient<'static>,
    admin: Address,
    relayer_1: Address,
    relayer_2: Address,
    vision_contract: Address,
}

fn setup_fixture() -> TestFixture {
    let env = Env::default();
    env.mock_all_auths();
    #[allow(deprecated)]
    env.budget().reset_unlimited();

    let contract_id = env.register(CrossChainContract, ());
    let client = CrossChainContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let relayer_1 = Address::generate(&env);
    let relayer_2 = Address::generate(&env);
    let vision_contract = env.register(MockVisionRecords, ());

    client.initialize(&admin);
    client.add_relayer(&admin, &relayer_1);
    client.add_relayer(&admin, &relayer_2);

    TestFixture {
        env,
        client,
        admin,
        relayer_1,
        relayer_2,
        vision_contract,
    }
}

/// Helper to map an identity and return the local address
fn map_patient(
    fixture: &TestFixture,
    source_chain: &str,
    source_address: &str,
) -> Address {
    let local_patient = Address::generate(&fixture.env);
    fixture.client.map_identity(
        &fixture.admin,
        &s(&fixture.env, source_chain),
        &s(&fixture.env, source_address),
        &local_patient,
    );
    local_patient
}

#[test]
fn test_identical_message_submitted_twice_rejected() {
    let fix = setup_fixture();
    map_patient(&fix, "ethereum", "0x1111111111111111111111111111111111111111");

    let message_id = Bytes::from_slice(&fix.env, b"unique_eth_msg_001");
    let message = CrossChainMessage {
        source_chain: s(&fix.env, "ethereum"),
        source_address: s(&fix.env, "0x1111111111111111111111111111111111111111"),
        target_action: symbol_short!("GRANT"),
        payload: Bytes::from_slice(&fix.env, b"grant_read_records"),
    };

    // First submission must succeed
    let first = fix.client.try_process_message(
        &fix.relayer_1,
        &message_id,
        &message,
        &fix.vision_contract,
    );
    assert!(first.is_ok(), "First message submission must succeed");

    // Second identical submission must fail with AlreadyProcessed
    let second = fix.client.try_process_message(
        &fix.relayer_1,
        &message_id,
        &message,
        &fix.vision_contract,
    );
    assert_eq!(
        second,
        Err(Ok(CrossChainError::AlreadyProcessed)),
        "Replaying identical message must be rejected"
    );
}

#[test]
fn test_replay_protection_across_multiple_source_chains() {
    let fix = setup_fixture();

    let chains = [
        ("ethereum", "0xeth_sender"),
        ("polygon", "0xpoly_sender"),
        ("arbitrum", "0xarb_sender"),
        ("optimism", "0xopt_sender"),
        ("base", "0xbase_sender"),
        ("solana", "SolanaPubKey1111111111111111111111111111111"),
    ];

    for (chain_name, sender_addr) in chains.iter() {
        map_patient(&fix, chain_name, sender_addr);

        let mut msg_id_bytes = [0u8; 32];
        let prefix = b"replay_guard_";
        msg_id_bytes[..prefix.len()].copy_from_slice(prefix);
        let name_bytes = chain_name.as_bytes();
        let copy_len = name_bytes.len().min(32 - prefix.len());
        msg_id_bytes[prefix.len()..prefix.len() + copy_len].copy_from_slice(&name_bytes[..copy_len]);

        let message_id = Bytes::from_slice(&fix.env, &msg_id_bytes);
        let message = CrossChainMessage {
            source_chain: s(&fix.env, chain_name),
            source_address: s(&fix.env, sender_addr),
            target_action: symbol_short!("GRANT"),
            payload: Bytes::from_slice(&fix.env, b"payload_data"),
        };

        // First attempt succeeds
        let res1 = fix.client.try_process_message(
            &fix.relayer_1,
            &message_id,
            &message,
            &fix.vision_contract,
        );
        assert!(res1.is_ok(), "First attempt for chain {} must succeed", chain_name);

        // Replay attempt on same chain fails
        let res2 = fix.client.try_process_message(
            &fix.relayer_1,
            &message_id,
            &message,
            &fix.vision_contract,
        );
        assert_eq!(
            res2,
            Err(Ok(CrossChainError::AlreadyProcessed)),
            "Replay on chain {} must be rejected with AlreadyProcessed",
            chain_name
        );
    }
}

#[test]
fn test_cross_chain_message_id_hijack_attempt_fails() {
    let fix = setup_fixture();

    // Map identities on both chains
    map_patient(&fix, "ethereum", "0xalice_eth");
    map_patient(&fix, "polygon", "0xbob_polygon");

    let message_id = Bytes::from_slice(&fix.env, b"shared_message_id_999");

    let eth_message = CrossChainMessage {
        source_chain: s(&fix.env, "ethereum"),
        source_address: s(&fix.env, "0xalice_eth"),
        target_action: symbol_short!("GRANT"),
        payload: Bytes::from_slice(&fix.env, b"payload_eth"),
    };

    // Process on Ethereum
    let eth_res = fix.client.try_process_message(
        &fix.relayer_1,
        &message_id,
        &eth_message,
        &fix.vision_contract,
    );
    assert!(eth_res.is_ok());

    // Attacker tries to reuse the same message_id from Polygon
    let polygon_message = CrossChainMessage {
        source_chain: s(&fix.env, "polygon"),
        source_address: s(&fix.env, "0xbob_polygon"),
        target_action: symbol_short!("GRANT"),
        payload: Bytes::from_slice(&fix.env, b"payload_polygon"),
    };

    let poly_res = fix.client.try_process_message(
        &fix.relayer_1,
        &message_id,
        &polygon_message,
        &fix.vision_contract,
    );
    assert_eq!(
        poly_res,
        Err(Ok(CrossChainError::AlreadyProcessed)),
        "Reusing message_id across different source chains must be rejected"
    );
}

#[test]
fn test_different_relayers_cannot_replay_same_message() {
    let fix = setup_fixture();
    map_patient(&fix, "arbitrum", "0xrelayer_test_sender");

    let message_id = Bytes::from_slice(&fix.env, b"relayer_compete_msg_001");
    let message = CrossChainMessage {
        source_chain: s(&fix.env, "arbitrum"),
        source_address: s(&fix.env, "0xrelayer_test_sender"),
        target_action: symbol_short!("GRANT"),
        payload: Bytes::from_slice(&fix.env, b"grant_access_data"),
    };

    // Relayer 1 processes message successfully
    let res1 = fix.client.try_process_message(
        &fix.relayer_1,
        &message_id,
        &message,
        &fix.vision_contract,
    );
    assert!(res1.is_ok());

    // Relayer 2 attempts to process the exact same message
    let res2 = fix.client.try_process_message(
        &fix.relayer_2,
        &message_id,
        &message,
        &fix.vision_contract,
    );
    assert_eq!(
        res2,
        Err(Ok(CrossChainError::AlreadyProcessed)),
        "A second relayer must not be able to replay an already-processed message"
    );
}

#[test]
fn test_replay_with_mutated_payload_rejected() {
    let fix = setup_fixture();
    map_patient(&fix, "base", "0xbase_sender_payload_test");

    let message_id = Bytes::from_slice(&fix.env, b"payload_mutation_msg_001");
    let orig_message = CrossChainMessage {
        source_chain: s(&fix.env, "base"),
        source_address: s(&fix.env, "0xbase_sender_payload_test"),
        target_action: symbol_short!("GRANT"),
        payload: Bytes::from_slice(&fix.env, b"legitimate_payload"),
    };

    // Process original
    let res1 = fix.client.try_process_message(
        &fix.relayer_1,
        &message_id,
        &orig_message,
        &fix.vision_contract,
    );
    assert!(res1.is_ok());

    // Attacker alters payload but reuses message_id
    let tampered_message = CrossChainMessage {
        source_chain: s(&fix.env, "base"),
        source_address: s(&fix.env, "0xbase_sender_payload_test"),
        target_action: symbol_short!("GRANT"),
        payload: Bytes::from_slice(&fix.env, b"tampered_malicious_payload"),
    };

    let res2 = fix.client.try_process_message(
        &fix.relayer_1,
        &message_id,
        &tampered_message,
        &fix.vision_contract,
    );
    assert_eq!(
        res2,
        Err(Ok(CrossChainError::AlreadyProcessed)),
        "Message ID cannot be reused even if payload is changed"
    );
}
