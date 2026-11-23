#![no_main]

use arbitrary::Arbitrary;
use identity::{IdentityContract, IdentityContractClient};
use key_manager::{
    KeyLevel, KeyManagerContract, KeyManagerContractClient, KeyPolicy, KeyStatus, KeyType,
};
use libfuzzer_sys::fuzz_target;
use soroban_sdk::{
    symbol_short, testutils::Address as _, testutils::Ledger, Address, BytesN, Env,
    Vec as SorobanVec,
};

#[derive(Arbitrary, Debug)]
pub enum FuzzAction {
    CreateMasterKey {
        key_type_idx: u8,
        rotation_interval: u32,
        seed: u8,
    },
    DeriveChildKey {
        parent_idx: u8,
        level_idx: u8,
        index: u32,
        hardened: bool,
        key_type_idx: u8,
        rotation_interval: u32,
    },
    UseKey {
        key_idx: u8,
        op_idx: u8,
    },
    DeriveRecordKey {
        key_idx: u8,
        record_id: u64,
    },
    RotateKey {
        key_idx: u8,
    },
    RevokeKey {
        key_idx: u8,
    },
    InitiateRecovery {
        key_idx: u8,
        guardian_idx: u8,
        new_key_seed: u8,
    },
    ApproveRecovery {
        key_idx: u8,
        guardian_idx: u8,
    },
    ExecuteRecovery {
        key_idx: u8,
    },
    CancelRecovery {
        key_idx: u8,
    },
    AdvanceTime {
        delta: u32,
    },
}

fn map_key_type(idx: u8) -> KeyType {
    match idx % 4 {
        0 => KeyType::Signing,
        1 => KeyType::Encryption,
        2 => KeyType::Authentication,
        _ => KeyType::Delegation,
    }
}

fn map_key_level(idx: u8) -> KeyLevel {
    match idx % 3 {
        0 => KeyLevel::Contract,
        1 => KeyLevel::Operation,
        _ => KeyLevel::Session,
    }
}

fuzz_target!(|actions: Vec<FuzzAction>| {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);

    let identity_id = env.register(IdentityContract, ());
    let identity = IdentityContractClient::new(&env, &identity_id);
    if identity.try_initialize(&admin).is_err() {
        return;
    }

    let mut guardians: Vec<Address> = Vec::new();
    for _ in 0..4 {
        let g = Address::generate(&env);
        let _ = identity.try_add_guardian(&admin, &g);
        guardians.push(g);
    }
    let _ = identity.try_set_recovery_threshold(&admin, &2);

    let key_manager_id = env.register(KeyManagerContract, ());
    let client = KeyManagerContractClient::new(&env, &key_manager_id);
    if client.try_initialize(&admin, &identity_id).is_err() {
        return;
    }

    let mut keys: Vec<BytesN<32>> = Vec::new();

    // Create an initial master key
    let empty_ops = SorobanVec::new(&env);
    let policy = KeyPolicy {
        max_uses: 1000,
        not_before: 0,
        not_after: 0,
        allowed_ops: empty_ops.clone(),
    };
    let initial_material = BytesN::from_array(&env, &[1u8; 32]);
    if let Ok(Ok(id)) = client.try_create_master_key(
        &admin,
        &KeyType::Encryption,
        &policy,
        &3600u64,
        &initial_material,
    ) {
        keys.push(id);
    }

    for action in actions {
        match action {
            FuzzAction::CreateMasterKey {
                key_type_idx,
                rotation_interval,
                seed,
            } => {
                let kt = map_key_type(key_type_idx);
                let pol = KeyPolicy {
                    max_uses: 500,
                    not_before: 0,
                    not_after: 0,
                    allowed_ops: empty_ops.clone(),
                };
                let material = BytesN::from_array(&env, &[seed; 32]);
                if let Ok(Ok(id)) = client.try_create_master_key(
                    &admin,
                    &kt,
                    &pol,
                    &(rotation_interval as u64),
                    &material,
                ) {
                    keys.push(id);
                }
            }
            FuzzAction::DeriveChildKey {
                parent_idx,
                level_idx,
                index,
                hardened,
                key_type_idx,
                rotation_interval,
            } => {
                if !keys.is_empty() {
                    let parent_id = &keys[(parent_idx as usize) % keys.len()];
                    let child_lvl = map_key_level(level_idx);
                    let kt = map_key_type(key_type_idx);
                    let pol = KeyPolicy {
                        max_uses: 200,
                        not_before: 0,
                        not_after: 0,
                        allowed_ops: empty_ops.clone(),
                    };
                    if let Ok(Ok(child_id)) = client.try_derive_key(
                        &admin,
                        parent_id,
                        &child_lvl,
                        &index,
                        &hardened,
                        &kt,
                        &pol,
                        &(rotation_interval as u64),
                    ) {
                        keys.push(child_id);
                    }
                }
            }
            FuzzAction::UseKey { key_idx, op_idx } => {
                if !keys.is_empty() {
                    let key_id = &keys[(key_idx as usize) % keys.len()];
                    let op = match op_idx % 3 {
                        0 => symbol_short!("SIGN"),
                        1 => symbol_short!("ENC"),
                        _ => symbol_short!("AUTH"),
                    };
                    let _ = client.try_use_key(&admin, key_id, &op);
                }
            }
            FuzzAction::DeriveRecordKey { key_idx, record_id } => {
                if !keys.is_empty() {
                    let key_id = &keys[(key_idx as usize) % keys.len()];
                    if let Ok(Ok(derived1)) = client.try_derive_record_key(key_id, &record_id) {
                        // Invariant: record derivation is completely deterministic
                        if let Ok(Ok(derived2)) = client.try_derive_record_key(key_id, &record_id) {
                            assert_eq!(
                                derived1.key, derived2.key,
                                "INVARIANT VIOLATION: non-deterministic record key derivation"
                            );
                        }
                    }
                }
            }
            FuzzAction::RotateKey { key_idx } => {
                if !keys.is_empty() {
                    let key_id = &keys[(key_idx as usize) % keys.len()];
                    let _ = client.try_rotate_key(&admin, key_id);
                }
            }
            FuzzAction::RevokeKey { key_idx } => {
                if !keys.is_empty() {
                    let key_id = &keys[(key_idx as usize) % keys.len()];
                    let _ = client.try_revoke_key(&admin, key_id);
                }
            }
            FuzzAction::InitiateRecovery {
                key_idx,
                guardian_idx,
                new_key_seed,
            } => {
                if !keys.is_empty() && !guardians.is_empty() {
                    let key_id = &keys[(key_idx as usize) % keys.len()];
                    let guardian = &guardians[(guardian_idx as usize) % guardians.len()];
                    let new_key = BytesN::from_array(&env, &[new_key_seed; 32]);
                    let _ = client.try_initiate_recovery(guardian, key_id, &new_key);
                }
            }
            FuzzAction::ApproveRecovery {
                key_idx,
                guardian_idx,
            } => {
                if !keys.is_empty() && !guardians.is_empty() {
                    let key_id = &keys[(key_idx as usize) % keys.len()];
                    let guardian = &guardians[(guardian_idx as usize) % guardians.len()];
                    let _ = client.try_approve_recovery(guardian, key_id);
                }
            }
            FuzzAction::ExecuteRecovery { key_idx } => {
                if !keys.is_empty() {
                    let key_id = &keys[(key_idx as usize) % keys.len()];
                    let _ = client.try_execute_recovery(&admin, key_id);
                }
            }
            FuzzAction::CancelRecovery { key_idx } => {
                if !keys.is_empty() {
                    let key_id = &keys[(key_idx as usize) % keys.len()];
                    let _ = client.try_cancel_recovery(&admin, key_id);
                }
            }
            FuzzAction::AdvanceTime { delta } => {
                let ts = env.ledger().timestamp().saturating_add(delta as u64);
                env.ledger().set_timestamp(ts);
            }
        }

        // Post-action invariant checks across all known keys
        for key_id in &keys {
            if let Some(record) = client.get_key_record(key_id) {
                // Invariant 1: Revoked keys must remain revoked
                if record.status == KeyStatus::Revoked {
                    let use_res = client.try_use_key(&admin, key_id, &symbol_short!("ENC"));
                    assert!(
                        use_res.is_err(),
                        "INVARIANT VIOLATION: revoked key can be used"
                    );
                }

                // Invariant 2: Current version must exist in version store
                assert!(
                    client
                        .get_key_version(key_id, &record.current_version)
                        .is_some(),
                    "INVARIANT VIOLATION: current key version not found in storage"
                );
            }
        }
    }
});
