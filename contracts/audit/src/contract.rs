#![allow(deprecated)]
#![allow(clippy::too_many_arguments)]

use soroban_sdk::{
    contract, contracterror, contractimpl, contracttype, symbol_short, Address, Bytes, BytesN, Env,
    Error, IntoVal, Symbol, Val, Vec,
};

/// Typed error enum for the AuditContract.
///
/// Using `#[contracterror]` ensures the SDK encodes these as u32 contract
/// errors so callers can match on them with `try_*` client methods.
#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum AuditContractError {
    /// `initialize` was called on an already-initialized contract.
    AlreadyInitialized = 1,
    /// The requested segment does not exist.
    SegmentNotFound = 2,
    /// The segment already exists.
    SegmentAlreadyExists = 3,
    /// Identity verification failed.
    IdentityCheckFailed = 4,
    /// Vault balance check failed.
    VaultBalanceInsufficient = 5,
    /// Compliance check failed.
    ComplianceCheckFailed = 6,
    /// No checkpoints exist for the segment.
    NoCheckpoints = 7,
    /// Entry not found at the requested sequence.
    EntryNotFound = 8,
}

#[contract]
pub struct AuditContract;

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AuditLogEntry {
    pub sequence: u64,
    pub timestamp: u64,
    pub actor: Address,
    pub action: Symbol,
    pub target: Symbol,
    pub result: Symbol,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CheckpointRecord {
    pub sequence: u64,
    pub timestamp: u64,
    pub root: BytesN<32>,
    pub entry_count: u64,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SegmentInfo {
    pub entries: Vec<AuditLogEntry>,
    pub next_sequence: u64,
}

const ADMIN: Symbol = symbol_short!("ADMIN");
const SEGMENTS: Symbol = symbol_short!("SEGMENTS");
const CHECKPOINTS: Symbol = symbol_short!("CHK_HIST");
const CHECKPOINT_LATEST: Symbol = symbol_short!("CHK_LAST");
const CHECKPOINT_EVENT: Symbol = symbol_short!("CHECKPNT");

#[contractimpl]
impl AuditContract {
    /// Initialize the contract with an admin address.
    ///
    /// # Errors
    /// Returns [`AuditContractError::AlreadyInitialized`] if called more than once.
    pub fn initialize(env: Env, admin: Address) -> Result<(), AuditContractError> {
        if env.storage().instance().has(&ADMIN) {
            return Err(AuditContractError::AlreadyInitialized);
        }
        env.storage().instance().set(&ADMIN, &admin);
        Ok(())
    }

    pub fn create_segment(env: Env, segment_id: Symbol) -> Result<(), Error> {
        let admin: Address = env.storage().instance().get(&ADMIN).unwrap();
        admin.require_auth();

        if env
            .storage()
            .persistent()
            .has(&(SEGMENTS, segment_id.clone()))
        {
            return Err(Error::from_contract_error(
                AuditContractError::SegmentAlreadyExists as u32,
            ));
        }

        let segment_info = SegmentInfo {
            entries: Vec::new(&env),
            next_sequence: 1,
        };

        env.storage()
            .persistent()
            .set(&(SEGMENTS, segment_id), &segment_info);
        Ok(())
    }

    pub fn append_entry(
        env: Env,
        segment_id: Symbol,
        actor: Address,
        action: Symbol,
        target: Symbol,
        result: Symbol,
    ) -> Result<u64, Error> {
        let mut segment_info: SegmentInfo = env
            .storage()
            .persistent()
            .get(&(SEGMENTS, segment_id.clone()))
            .ok_or(Error::from_contract_error(
                AuditContractError::SegmentNotFound as u32,
            ))?;

        let sequence = segment_info.next_sequence;
        let entry = AuditLogEntry {
            sequence,
            timestamp: env.ledger().timestamp(),
            actor,
            action,
            target,
            result,
        };

        segment_info.entries.push_back(entry);
        segment_info.next_sequence += 1;

        env.storage()
            .persistent()
            .set(&(SEGMENTS, segment_id), &segment_info);
        Ok(sequence)
    }

    pub fn get_entries(env: Env, segment_id: Symbol) -> Result<Vec<AuditLogEntry>, Error> {
        let segment_info: SegmentInfo = env
            .storage()
            .persistent()
            .get(&(SEGMENTS, segment_id))
            .ok_or(Error::from_contract_error(
                AuditContractError::SegmentNotFound as u32,
            ))?;

        Ok(segment_info.entries)
    }

    /// Retrieve audit entries within a sequence range (from <= seq <= to).
    /// Returns up to `limit` entries to bound resource consumption.
    pub fn get_entries_range(
        env: Env,
        segment_id: Symbol,
        from_seq: u64,
        to_seq: u64,
        limit: u32,
    ) -> Result<Vec<AuditLogEntry>, Error> {
        let segment_info: SegmentInfo = env
            .storage()
            .persistent()
            .get(&(SEGMENTS, segment_id))
            .ok_or(Error::from_contract_error(
                AuditContractError::SegmentNotFound as u32,
            ))?;

        let mut result = Vec::new(&env);
        let capped_limit = limit.saturating_add(1).saturating_div(1).min(1000);
        let mut count = 0u32;

        for entry in segment_info.entries.iter() {
            if entry.sequence >= from_seq && entry.sequence <= to_seq {
                result.push_back(entry);
                count += 1;
                if count >= capped_limit {
                    break;
                }
            }
        }

        Ok(result)
    }

    /// Retrieve audit entries within a timestamp range (from_ts <= timestamp <= to_ts).
    /// Returns up to `limit` entries to bound resource consumption.
    pub fn get_entries_by_time(
        env: Env,
        segment_id: Symbol,
        from_ts: u64,
        to_ts: u64,
        limit: u32,
    ) -> Result<Vec<AuditLogEntry>, Error> {
        let segment_info: SegmentInfo = env
            .storage()
            .persistent()
            .get(&(SEGMENTS, segment_id))
            .ok_or(Error::from_contract_error(
                AuditContractError::SegmentNotFound as u32,
            ))?;

        let mut result = Vec::new(&env);
        let capped_limit = limit.saturating_add(1).saturating_div(1).min(1000);
        let mut count = 0u32;

        for entry in segment_info.entries.iter() {
            if entry.timestamp >= from_ts && entry.timestamp <= to_ts {
                result.push_back(entry);
                count += 1;
                if count >= capped_limit {
                    break;
                }
            }
        }

        Ok(result)
    }

    pub fn get_entry_count(env: Env, segment_id: Symbol) -> Result<u64, Error> {
        let segment_info: SegmentInfo = env
            .storage()
            .persistent()
            .get(&(SEGMENTS, segment_id))
            .ok_or(Error::from_contract_error(
                AuditContractError::SegmentNotFound as u32,
            ))?;

        Ok(segment_info.entries.len() as u64)
    }

    pub fn verify_identity(
        env: Env,
        identity_contract: Address,
        actor: Address,
        method: Symbol,
    ) -> Result<bool, Error> {
        let mut args: Vec<Val> = Vec::new(&env);
        args.push_back(actor.into_val(&env));
        let result: bool = env.invoke_contract(&identity_contract, &method, args);
        Ok(result)
    }

    pub fn check_vault_balance(
        env: Env,
        vault_contract: Address,
        account: Address,
        method: Symbol,
    ) -> Result<i128, Error> {
        let mut args: Vec<Val> = Vec::new(&env);
        args.push_back(account.into_val(&env));
        let balance: i128 = env.invoke_contract(&vault_contract, &method, args);
        Ok(balance)
    }

    pub fn check_compliance(
        env: Env,
        compliance_contract: Address,
        action: Symbol,
        method: Symbol,
    ) -> Result<bool, Error> {
        let mut args: Vec<Val> = Vec::new(&env);
        args.push_back(action.into_val(&env));
        let compliant: bool = env.invoke_contract(&compliance_contract, &method, args);
        Ok(compliant)
    }

    pub fn append_entry_with_checks(
        env: Env,
        segment_id: Symbol,
        actor: Address,
        action: Symbol,
        target: Symbol,
        result: Symbol,
        identity_contract: Address,
        identity_method: Symbol,
        vault_contract: Address,
        vault_method: Symbol,
        compliance_contract: Address,
        compliance_action: Symbol,
        compliance_method: Symbol,
    ) -> Result<u64, Error> {
        let identity_ok = Self::verify_identity(
            env.clone(),
            identity_contract,
            actor.clone(),
            identity_method,
        )?;
        if !identity_ok {
            return Err(Error::from_contract_error(
                AuditContractError::IdentityCheckFailed as u32,
            ));
        }

        let balance =
            Self::check_vault_balance(env.clone(), vault_contract, actor.clone(), vault_method)?;
        if balance < 0 {
            return Err(Error::from_contract_error(
                AuditContractError::VaultBalanceInsufficient as u32,
            ));
        }

        let compliant = Self::check_compliance(
            env.clone(),
            compliance_contract,
            compliance_action,
            compliance_method,
        )?;
        if !compliant {
            return Err(Error::from_contract_error(
                AuditContractError::ComplianceCheckFailed as u32,
            ));
        }

        Self::append_entry(env, segment_id, actor, action, target, result)
    }

    /// Hash an internal node: SHA256(0x01 || left || right)
    fn hash_internal_node(env: &Env, left: &BytesN<32>, right: &BytesN<32>) -> BytesN<32> {
        let mut payload = Bytes::new(env);
        payload.push_back(0x01);
        payload.append(&left.to_bytes());
        payload.append(&right.to_bytes());
        env.crypto().sha256(&payload).into()
    }

    /// Iterative bottom-up reduction to compute RFC 6962 Merkle root.
    fn compute_merkle_root(env: &Env, mut level: Vec<BytesN<32>>) -> BytesN<32> {
        if level.is_empty() {
            return BytesN::from_array(env, &[0u8; 32]);
        }
        while level.len() > 1 {
            let mut next = Vec::new(env);
            let mut i = 0u32;
            let len = level.len();
            while i < len {
                if i + 1 < len {
                    let left = level.get(i).unwrap();
                    let right = level.get(i + 1).unwrap();
                    next.push_back(Self::hash_internal_node(env, &left, &right));
                } else {
                    next.push_back(level.get(i).unwrap());
                }
                i += 2;
            }
            level = next;
        }
        level.get(0).unwrap()
    }

    /// Compute RFC 6962 leaf hash for an audit log entry: SHA256(0x00 || fields).
    pub fn compute_entry_leaf_hash(env: Env, entry: AuditLogEntry) -> BytesN<32> {
        let mut payload = Bytes::new(&env);
        payload.push_back(0x00);
        for b in entry.sequence.to_be_bytes() {
            payload.push_back(b);
        }
        for b in entry.timestamp.to_be_bytes() {
            payload.push_back(b);
        }
        for b in entry.action.to_val().get_payload().to_be_bytes() {
            payload.push_back(b);
        }
        for b in entry.target.to_val().get_payload().to_be_bytes() {
            payload.push_back(b);
        }
        for b in entry.result.to_val().get_payload().to_be_bytes() {
            payload.push_back(b);
        }
        env.crypto().sha256(&payload).into()
    }

    /// Periodically anchor the log Merkle root to allow cheap verification.
    ///
    /// Computes Merkle root over all segment entries and records a CheckpointRecord.
    pub fn checkpoint(env: Env, segment_id: Symbol) -> Result<BytesN<32>, Error> {
        let segment_info: SegmentInfo = env
            .storage()
            .persistent()
            .get(&(SEGMENTS, segment_id.clone()))
            .ok_or(Error::from_contract_error(
                AuditContractError::SegmentNotFound as u32,
            ))?;

        let mut leaves: Vec<BytesN<32>> = Vec::new(&env);
        for entry in segment_info.entries.iter() {
            leaves.push_back(Self::compute_entry_leaf_hash(env.clone(), entry));
        }

        let root = Self::compute_merkle_root(&env, leaves);
        let count = segment_info.entries.len() as u64;
        let record = CheckpointRecord {
            sequence: count,
            timestamp: env.ledger().timestamp(),
            root: root.clone(),
            entry_count: count,
        };

        let mut history: Vec<CheckpointRecord> = env
            .storage()
            .persistent()
            .get(&(CHECKPOINTS, segment_id.clone()))
            .unwrap_or_else(|| Vec::new(&env));
        history.push_back(record.clone());

        env.storage()
            .persistent()
            .set(&(CHECKPOINTS, segment_id.clone()), &history);
        env.storage()
            .persistent()
            .set(&(CHECKPOINT_LATEST, segment_id.clone()), &record);

        env.events().publish(
            (CHECKPOINT_EVENT, segment_id),
            (record.sequence, record.root.clone()),
        );

        Ok(root)
    }

    /// Retrieve the most recent checkpoint for a given segment.
    pub fn get_latest_checkpoint(env: Env, segment_id: Symbol) -> Result<CheckpointRecord, Error> {
        env.storage()
            .persistent()
            .get(&(CHECKPOINT_LATEST, segment_id))
            .ok_or(Error::from_contract_error(
                AuditContractError::NoCheckpoints as u32,
            ))
    }

    /// Retrieve all checkpoints for a given segment.
    pub fn get_checkpoints(env: Env, segment_id: Symbol) -> Result<Vec<CheckpointRecord>, Error> {
        let history: Vec<CheckpointRecord> = env
            .storage()
            .persistent()
            .get(&(CHECKPOINTS, segment_id))
            .unwrap_or_else(|| Vec::new(&env));
        Ok(history)
    }

    /// Generate Merkle inclusion proof siblings for a given entry sequence.
    pub fn get_inclusion_proof(
        env: Env,
        segment_id: Symbol,
        sequence: u64,
    ) -> Result<Vec<BytesN<32>>, Error> {
        let segment_info: SegmentInfo = env
            .storage()
            .persistent()
            .get(&(SEGMENTS, segment_id))
            .ok_or(Error::from_contract_error(
                AuditContractError::SegmentNotFound as u32,
            ))?;

        if sequence == 0 || sequence > segment_info.entries.len() as u64 {
            return Err(Error::from_contract_error(
                AuditContractError::EntryNotFound as u32,
            ));
        }

        let mut target_idx = (sequence - 1) as u32;
        let mut level: Vec<BytesN<32>> = Vec::new(&env);
        for entry in segment_info.entries.iter() {
            level.push_back(Self::compute_entry_leaf_hash(env.clone(), entry));
        }

        let mut siblings: Vec<BytesN<32>> = Vec::new(&env);
        while level.len() > 1 {
            let len = level.len();
            if target_idx % 2 == 1 {
                siblings.push_back(level.get(target_idx - 1).unwrap());
            } else if target_idx < len - 1 {
                siblings.push_back(level.get(target_idx + 1).unwrap());
            }

            let mut next = Vec::new(&env);
            let mut i = 0u32;
            while i < len {
                if i + 1 < len {
                    let left = level.get(i).unwrap();
                    let right = level.get(i + 1).unwrap();
                    next.push_back(Self::hash_internal_node(&env, &left, &right));
                } else {
                    next.push_back(level.get(i).unwrap());
                }
                i += 2;
            }
            level = next;
            target_idx /= 2;
        }

        Ok(siblings)
    }

    /// Verify an inclusion proof against an anchored root.
    ///
    /// Complexity: O(log n).
    pub fn verify_inclusion(
        env: Env,
        leaf_hash: BytesN<32>,
        leaf_index: u64,
        tree_size: u64,
        siblings: Vec<BytesN<32>>,
        root: BytesN<32>,
    ) -> bool {
        let mut computed = leaf_hash;
        let mut index = leaf_index;
        let mut size = tree_size;
        let mut sib_idx = 0u32;

        while size > 1 {
            if index % 2 == 1 {
                if sib_idx >= siblings.len() {
                    return false;
                }
                let sibling = siblings.get(sib_idx).unwrap();
                sib_idx += 1;
                computed = Self::hash_internal_node(&env, &sibling, &computed);
            } else if index < size - 1 {
                if sib_idx >= siblings.len() {
                    return false;
                }
                let sibling = siblings.get(sib_idx).unwrap();
                sib_idx += 1;
                computed = Self::hash_internal_node(&env, &computed, &sibling);
            }
            index /= 2;
            size = size.saturating_add(1) / 2;
        }

        if sib_idx != siblings.len() {
            return false;
        }

        computed == root
    }
}
