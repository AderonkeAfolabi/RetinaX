#![no_main]

use arbitrary::Arbitrary;
use libfuzzer_sys::fuzz_target;
use soroban_sdk::{testutils::Address as _, Address, BytesN, Env, Vec};
use zk_verifier::{AccessRequest, Proof, G1Point, G2Point, ZkVerifierContract, ZkVerifierContractClient};

#[derive(Arbitrary, Debug)]
pub enum FuzzAction {
    Initialize { nonce: u8 },
    SetRateLimit { max_requests: u32, window: u32 },
    VerifyProof {
        g1_x: [u8; 32],
        g1_y: [u8; 32],
        g2_x0: [u8; 32],
        g2_x1: [u8; 32],
        g2_y0: [u8; 32],
        g2_y1: [u8; 32],
        c_x: [u8; 32],
        c_y: [u8; 32],
        public_input_count: u8,
        expires_at: u64,
        nonce: u64,
    },
}

fuzz_target!(|actions: Vec<FuzzAction>| {
    let env = Env::default();
    let admin = Address::generate(&env);
    let user = Address::generate(&env);

    let contract_id = env.register(ZkVerifierContract, ());
    let client = ZkVerifierContractClient::new(&env, &contract_id);

    for action in actions {
        match action {
            FuzzAction::Initialize { .. } => {
                let _ = client.try_initialize(&admin);
            }
            FuzzAction::SetRateLimit {
                max_requests,
                window,
            } => {
                let clamped_max = max_requests.saturating_add(1).saturating_div(1).min(1000);
                let clamped_window = window.saturating_add(1).saturating_div(1).min(86400);
                let _ = client.try_set_rate_limit(&admin, &clamped_max, &clamped_window);
            }
            FuzzAction::VerifyProof {
                g1_x,
                g1_y,
                g2_x0,
                g2_x1,
                g2_y0,
                g2_y1,
                c_x,
                c_y,
                public_input_count,
                expires_at,
                nonce,
            } => {
                let a = G1Point {
                    x: BytesN::from_array(&env, &g1_x),
                    y: BytesN::from_array(&env, &g1_y),
                };

                let b = G2Point {
                    x: (
                        BytesN::from_array(&env, &g2_x0),
                        BytesN::from_array(&env, &g2_x1),
                    ),
                    y: (
                        BytesN::from_array(&env, &g2_y0),
                        BytesN::from_array(&env, &g2_y1),
                    ),
                };

                let c = G1Point {
                    x: BytesN::from_array(&env, &c_x),
                    y: BytesN::from_array(&env, &c_y),
                };

                let proof = Proof { a, b, c };

                let count = (public_input_count as usize).min(16);
                let mut public_inputs = Vec::new(&env);
                for i in 0..count {
                    let mut bytes = [0u8; 32];
                    bytes[0] = (i as u8).wrapping_add(1);
                    public_inputs.push_back(BytesN::from_array(&env, &bytes));
                }

                let request = AccessRequest {
                    user: user.clone(),
                    resource_id: BytesN::from_array(&env, &[0u8; 32]),
                    proof,
                    public_inputs,
                    expires_at,
                    nonce,
                };

                let _ = client.try_verify_access(&admin, &request);
            }
        }
    }
});
