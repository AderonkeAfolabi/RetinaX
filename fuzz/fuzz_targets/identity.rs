#![no_main]

use arbitrary::Arbitrary;
use identity::{IdentityContract, IdentityContractClient};
use libfuzzer_sys::fuzz_target;
use soroban_sdk::{testutils::Address as _, testutils::Ledger, Address, Env, String};

#[derive(Arbitrary, Debug)]
pub enum FuzzAction {
    CreateDid { did_suffix: u16 },
    UpdateDid { did_suffix: u16 },
    DeactivateDid { did_suffix: u16 },
    AddGuardian { guardian_idx: u8 },
    RemoveGuardian { guardian_idx: u8 },
    SetThreshold { threshold: u32 },
    PrepareAddGuardian { guardian_idx: u8 },
    CommitAddGuardian { guardian_idx: u8 },
    RollbackAddGuardian { guardian_idx: u8 },
    PrepareRemoveGuardian { guardian_idx: u8 },
    CommitRemoveGuardian { guardian_idx: u8 },
    RollbackRemoveGuardian { guardian_idx: u8 },
    PrepareSetThreshold { threshold: u32 },
    CommitSetThreshold { threshold: u32 },
    RollbackSetThreshold { threshold: u32 },
    InitiateRecovery { guardian_idx: u8, candidate_idx: u8 },
    ApproveRecovery { guardian_idx: u8 },
    ExecuteRecovery { caller_idx: u8 },
    CancelRecovery { caller_idx: u8 },
    AdvanceTime { delta: u32 },
}

fuzz_target!(|actions: std::vec::Vec<FuzzAction>| {
    let env = Env::default();
    env.mock_all_auths();

    let owner = Address::generate(&env);
    let contract_id = env.register(IdentityContract, ());
    let client = IdentityContractClient::new(&env, &contract_id);

    if client.try_initialize(&owner).is_err() {
        return;
    }

    let mut pool: std::vec::Vec<Address> = std::vec::Vec::new();
    for _ in 0..8 {
        pool.push(Address::generate(&env));
    }

    for action in actions {
        match action {
            FuzzAction::CreateDid { did_suffix } => {
                let did_str = format!("did:retinax:user_{}", did_suffix);
                let did = String::from_str(&env, &did_str);
                let _ = client.try_create_did(&owner, &did);
            }
            FuzzAction::UpdateDid { did_suffix } => {
                let did_str = format!("did:retinax:user_{}", did_suffix);
                let did = String::from_str(&env, &did_str);
                let _ = client.try_update_did(&owner, &did);
            }
            FuzzAction::DeactivateDid { did_suffix } => {
                let did_str = format!("did:retinax:user_{}", did_suffix);
                let did = String::from_str(&env, &did_str);
                let _ = client.try_deactivate_did(&owner, &did);
            }
            FuzzAction::AddGuardian { guardian_idx } => {
                let g = &pool[(guardian_idx as usize) % pool.len()];
                let _ = client.try_add_guardian(&owner, g);
            }
            FuzzAction::RemoveGuardian { guardian_idx } => {
                let g = &pool[(guardian_idx as usize) % pool.len()];
                let _ = client.try_remove_guardian(&owner, g);
            }
            FuzzAction::SetThreshold { threshold } => {
                let _ = client.try_set_recovery_threshold(&owner, &threshold);
            }
            FuzzAction::PrepareAddGuardian { guardian_idx } => {
                let g = &pool[(guardian_idx as usize) % pool.len()];
                let _ = client.try_prepare_add_guardian(&owner, g);
            }
            FuzzAction::CommitAddGuardian { guardian_idx } => {
                let g = &pool[(guardian_idx as usize) % pool.len()];
                let _ = client.try_commit_add_guardian(&owner, g);
            }
            FuzzAction::RollbackAddGuardian { guardian_idx } => {
                let g = &pool[(guardian_idx as usize) % pool.len()];
                let _ = client.try_rollback_add_guardian(&owner, g);
            }
            FuzzAction::PrepareRemoveGuardian { guardian_idx } => {
                let g = &pool[(guardian_idx as usize) % pool.len()];
                let _ = client.try_prepare_remove_guardian(&owner, g);
            }
            FuzzAction::CommitRemoveGuardian { guardian_idx } => {
                let g = &pool[(guardian_idx as usize) % pool.len()];
                let _ = client.try_commit_remove_guardian(&owner, g);
            }
            FuzzAction::RollbackRemoveGuardian { guardian_idx } => {
                let g = &pool[(guardian_idx as usize) % pool.len()];
                let _ = client.try_rollback_remove_guardian(&owner, g);
            }
            FuzzAction::PrepareSetThreshold { threshold } => {
                let _ = client.try_prepare_set_recovery_threshold(&owner, &threshold);
            }
            FuzzAction::CommitSetThreshold { threshold } => {
                let _ = client.try_commit_set_recovery_threshold(&owner, &threshold);
            }
            FuzzAction::RollbackSetThreshold { threshold } => {
                let _ = client.try_rollback_set_recovery_threshold(&owner, &threshold);
            }
            FuzzAction::InitiateRecovery {
                guardian_idx,
                candidate_idx,
            } => {
                let g = &pool[(guardian_idx as usize) % pool.len()];
                let c = &pool[(candidate_idx as usize) % pool.len()];
                let _ = client.try_initiate_recovery(g, &owner, c);
            }
            FuzzAction::ApproveRecovery { guardian_idx } => {
                let g = &pool[(guardian_idx as usize) % pool.len()];
                let _ = client.try_approve_recovery(g, &owner);
            }
            FuzzAction::ExecuteRecovery { caller_idx } => {
                let caller = &pool[(caller_idx as usize) % pool.len()];
                let _ = client.try_execute_recovery(caller, &owner);
            }
            FuzzAction::CancelRecovery { caller_idx } => {
                let caller = &pool[(caller_idx as usize) % pool.len()];
                let _ = client.try_cancel_recovery(caller);
            }
            FuzzAction::AdvanceTime { delta } => {
                let ts = env.ledger().timestamp().saturating_add(delta as u64);
                env.ledger().set_timestamp(ts);
            }
        }

        // Post-action invariant checks
        let guardians = client.get_guardians(&owner);
        let threshold = client.get_recovery_threshold(&owner);

        // Invariant 1: Threshold must not exceed guardian count when guardians exist
        if !guardians.is_empty() && threshold > 0 {
            assert!(
                threshold <= guardians.len(),
                "INVARIANT VIOLATION: threshold {} exceeds guardian count {}",
                threshold,
                guardians.len()
            );
        }

        // Invariant 2: Guardian list must not contain duplicates
        for i in 0..guardians.len() {
            for j in (i + 1)..guardians.len() {
                assert_ne!(
                    guardians.get(i).unwrap(),
                    guardians.get(j).unwrap(),
                    "INVARIANT VIOLATION: duplicate guardian detected"
                );
            }
        }
    }
});
