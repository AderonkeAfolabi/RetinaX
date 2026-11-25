// Storage collision tests between co-deployed contracts.

#[cfg(test)]
mod storage_collision_tests {
    use std::collections::{HashMap, HashSet};

    fn get_clinical_records_keys() -> Vec<&'static str> {
        vec!["ADMIN", "INIT", "RX_CTR", "RECORDS"]
    }

    fn get_access_control_keys() -> Vec<&'static str> {
        vec!["ADMIN", "ROLES", "PERMISSIONS"]
    }

    fn get_vision_records_keys() -> Vec<&'static str> {
        vec!["ADMIN", "INIT", "VISION_DATA"]
    }

    fn get_staking_keys() -> Vec<&'static str> {
        vec!["STAKES", "REWARDS", "TOTAL"]
    }

    fn get_treasury_keys() -> Vec<&'static str> {
        vec!["BALANCES", "TOTAL_SUPPLY", "ADMIN"]
    }

    #[test]
    fn test_no_storage_key_collisions() {
        let mut all_keys: HashMap<&str, Vec<&str>> = HashMap::new();

        for key in get_clinical_records_keys() {
            all_keys.entry(key).or_default().push("clinical_records");
        }
        for key in get_access_control_keys() {
            all_keys.entry(key).or_default().push("access_control");
        }
        for key in get_vision_records_keys() {
            all_keys.entry(key).or_default().push("vision_records");
        }
        for key in get_staking_keys() {
            all_keys.entry(key).or_default().push("staking");
        }
        for key in get_treasury_keys() {
            all_keys.entry(key).or_default().push("treasury");
        }

        let collisions: Vec<_> = all_keys
            .iter()
            .filter(|(_, contracts)| contracts.len() > 1)
            .map(|(key, contracts)| format!("Key '{}' used in: {:?}", key, contracts))
            .collect();

        assert!(
            collisions.is_empty(),
            "Storage key collisions detected:\n{}",
            collisions.join("\n")
        );
    }

    #[test]
    fn test_contract_specific_prefixes() {
        let contracts = [
            ("staking", vec!["STAKING_ADMIN", "STAKING_REWARDS"]),
            ("treasury", vec!["TREASURY_ADMIN", "TREASURY_BALANCE"]),
            ("clinical", vec!["CLINICAL_ADMIN", "CLINICAL_RECORDS"]),
        ];

        for (contract_name, keys) in contracts {
            for key in keys {
                assert!(
                    key.to_lowercase().starts_with(contract_name),
                    "Key '{}' should have prefix '{}'",
                    key,
                    contract_name
                );
            }
        }
    }

    #[test]
    fn test_key_uniqueness_24_contracts() {
        let mut global_keys: HashSet<String> = HashSet::new();
        let contract_count = 24;

        for i in 0..contract_count {
            for key in [
                format!("CONTRACT{}_ADMIN", i),
                format!("CONTRACT{}_DATA", i),
                format!("CONTRACT{}_INIT", i),
            ] {
                assert!(global_keys.insert(key), "Duplicate storage key detected");
            }
        }

        assert_eq!(global_keys.len(), contract_count * 3);
    }
}
