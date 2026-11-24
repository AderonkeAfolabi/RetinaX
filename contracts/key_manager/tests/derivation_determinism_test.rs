#![cfg(test)]
#![allow(clippy::unwrap_used, clippy::expect_used)]

use identity::{IdentityContract, IdentityContractClient};
use key_manager::{KeyLevel, KeyManagerContract, KeyManagerContractClient, KeyPolicy, KeyType};
use proptest::prelude::*;
use soroban_sdk::{testutils::Address as _, Address, BytesN, Env, Vec};

fn setup() -> (Env, KeyManagerContractClient<'static>, Address) {
    let env = Env::default();
    env.mock_all_auths();

    let identity_id = env.register(IdentityContract, ());
    let identity = IdentityContractClient::new(&env, &identity_id);
    let admin = Address::generate(&env);
    identity.initialize(&admin);

    let key_manager_id = env.register(KeyManagerContract, ());
    let client = KeyManagerContractClient::new(&env, &key_manager_id);
    client.initialize(&admin, &identity_id);

    (env, client, admin)
}

fn default_policy(env: &Env) -> KeyPolicy {
    KeyPolicy {
        max_uses: 0,
        not_before: 0,
        not_after: 0,
        allowed_ops: Vec::new(env),
    }
}

#[test]
fn test_child_key_derivation_determinism_across_contexts_and_indexes() {
    let (env, client, admin) = setup();
    let policy = default_policy(&env);
    let seed = BytesN::from_array(&env, &[0x42u8; 32]);

    let master_id = client.create_master_key(&admin, &KeyType::Encryption, &policy, &0, &seed);

    // Same parent, index, and hardened context produce identical derived key material.
    let child_a = client.derive_key(
        &admin,
        &master_id,
        &KeyLevel::Contract,
        &0,
        &false,
        &KeyType::Encryption,
        &policy,
        &0,
    );
    let child_a_again = client.derive_key(
        &admin,
        &master_id,
        &KeyLevel::Contract,
        &0,
        &false,
        &KeyType::Encryption,
        &policy,
        &0,
    );
    let v_a = client.get_key_version(&child_a, &1).unwrap();
    let v_a_again = client.get_key_version(&child_a_again, &1).unwrap();
    assert_eq!(v_a.key_bytes, v_a_again.key_bytes);

    // Different index produces distinct key material.
    let child_idx1 = client.derive_key(
        &admin,
        &master_id,
        &KeyLevel::Contract,
        &1,
        &false,
        &KeyType::Encryption,
        &policy,
        &0,
    );
    let v_idx1 = client.get_key_version(&child_idx1, &1).unwrap();
    assert_ne!(v_a.key_bytes, v_idx1.key_bytes);

    // Different hardened context flag produces distinct key material.
    let child_hard = client.derive_key(
        &admin,
        &master_id,
        &KeyLevel::Contract,
        &0,
        &true,
        &KeyType::Encryption,
        &policy,
        &0,
    );
    let v_hard = client.get_key_version(&child_hard, &1).unwrap();
    assert_ne!(v_a.key_bytes, v_hard.key_bytes);

    // Deeper hierarchy context (Contract -> Operation) also derives deterministically & distinctly.
    let op_0 = client.derive_key(
        &admin,
        &child_a,
        &KeyLevel::Operation,
        &0,
        &true,
        &KeyType::Encryption,
        &policy,
        &0,
    );
    let op_1 = client.derive_key(
        &admin,
        &child_a,
        &KeyLevel::Operation,
        &1,
        &true,
        &KeyType::Encryption,
        &policy,
        &0,
    );
    let v_op_0 = client.get_key_version(&op_0, &1).unwrap();
    let v_op_1 = client.get_key_version(&op_1, &1).unwrap();
    assert_ne!(v_op_0.key_bytes, v_op_1.key_bytes);
    assert_ne!(v_op_0.key_bytes, v_a.key_bytes);
}

#[test]
fn test_record_key_derivation_determinism_across_versions_and_records() {
    let (env, client, admin) = setup();
    let policy = default_policy(&env);
    let key_bytes = BytesN::from_array(&env, &[0x13u8; 32]);

    let master_id = client.create_master_key(&admin, &KeyType::Encryption, &policy, &0, &key_bytes);
    let child_id = client.derive_key(
        &admin,
        &master_id,
        &KeyLevel::Contract,
        &7,
        &true,
        &KeyType::Encryption,
        &policy,
        &0,
    );

    let d1 = client.derive_record_key(&child_id, &101);
    let d1_repeat = client.derive_record_key(&child_id, &101);
    let d2 = client.derive_record_key(&child_id, &102);

    assert_eq!(d1.key, d1_repeat.key);
    assert_eq!(d1.version, d1_repeat.version);
    assert_ne!(d1.key, d2.key);

    // After rotating the key, deriving with version 1 still matches the original v1 key,
    // while the new active version produces distinct key material.
    let new_version = client.rotate_key(&admin, &child_id);
    assert_eq!(new_version, 2);

    let d1_v1 = client.derive_record_key_with_version(&child_id, &101, &1);
    let d1_v2 = client.derive_record_key(&child_id, &101);
    assert_eq!(d1_v1.key, d1.key);
    assert_ne!(d1_v2.key, d1.key);
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(16))]

    #[test]
    fn prop_child_and_record_derivation_determinism(
        seed_arr in any::<[u8; 32]>(),
        index_a in any::<u32>(),
        index_b in any::<u32>(),
        hardened in any::<bool>(),
        record_a in any::<u64>(),
        record_b in any::<u64>(),
    ) {
        let (env, client, admin) = setup();
        let policy = default_policy(&env);
        let seed = BytesN::from_array(&env, &seed_arr);
        let master_id = client.create_master_key(&admin, &KeyType::Encryption, &policy, &0, &seed);

        let c1 = client.derive_key(
            &admin,
            &master_id,
            &KeyLevel::Contract,
            &index_a,
            &hardened,
            &KeyType::Encryption,
            &policy,
            &0,
        );
        let c1_again = client.derive_key(
            &admin,
            &master_id,
            &KeyLevel::Contract,
            &index_a,
            &hardened,
            &KeyType::Encryption,
            &policy,
            &0,
        );
        let k1 = client.get_key_version(&c1, &1).unwrap().key_bytes;
        let k1_again = client.get_key_version(&c1_again, &1).unwrap().key_bytes;
        prop_assert_eq!(&k1, &k1_again);

        // Flipping hardened context must change derived key material.
        let c_flipped = client.derive_key(
            &admin,
            &master_id,
            &KeyLevel::Contract,
            &index_a,
            &(!hardened),
            &KeyType::Encryption,
            &policy,
            &0,
        );
        let k_flipped = client.get_key_version(&c_flipped, &1).unwrap().key_bytes;
        prop_assert_ne!(&k1, &k_flipped);

        // Different index must change derived key material.
        if index_a != index_b {
            let c_other = client.derive_key(
                &admin,
                &master_id,
                &KeyLevel::Contract,
                &index_b,
                &hardened,
                &KeyType::Encryption,
                &policy,
                &0,
            );
            let k_other = client.get_key_version(&c_other, &1).unwrap().key_bytes;
            prop_assert_ne!(&k1, &k_other);
        }

        // Record key derivation determinism and distinctness.
        let rk1 = client.derive_record_key(&c1, &record_a).key;
        let rk2 = client.derive_record_key(&c1, &record_a).key;
        prop_assert_eq!(&rk1, &rk2);
        if record_a != record_b {
            let rk_other = client.derive_record_key(&c1, &record_b).key;
            prop_assert_ne!(&rk1, &rk_other);
        }
    }
}
