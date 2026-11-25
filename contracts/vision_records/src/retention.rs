//! Data class retention policy with automatic expiry and storage purging.
//!
//! Provides compliance-ready lifecycle management for medical vision records.
//! Each data class (`RecordType`) can be assigned a retention duration in seconds.
//! When a record exceeds its retention horizon (`retain_until`), it immediately
//! becomes inaccessible (`RecordExpired`), satisfying GDPR Art. 5(1)(e) (storage limitation)
//! and HIPAA 45 CFR § 164.316(b)(2)(i).
//!
//! Storage reclamation coordinates with Issue #325 TTL cleanup patterns via
//! `purge_expired_record`.

use crate::archive;
use crate::audit::{self, AccessAction, AccessResult};
use crate::errors::ContractError;
use crate::events;
use crate::types::{RecordType, VisionRecord};
use soroban_sdk::{symbol_short, Address, Env, String, Symbol, Vec};

pub const RETENTION_POLICY_KEY: Symbol = symbol_short!("RET_POL");

const TTL_THRESHOLD: u32 = 5184000;
const TTL_EXTEND_TO: u32 = 10368000;

#[inline]
pub fn policy_key(record_type: &RecordType) -> (Symbol, RecordType) {
    (RETENTION_POLICY_KEY, record_type.clone())
}

/// Sets the retention duration (in seconds) for a given record data class.
/// Only administrators can configure data class retention policies.
/// Setting `retention_seconds = 0` removes any retention period, marking the data class
/// for indefinite retention.
pub fn set_retention_period(
    env: &Env,
    admin: &Address,
    record_type: &RecordType,
    retention_seconds: u64,
) -> Result<(), ContractError> {
    admin.require_auth();

    if !archive::is_admin(env, admin) {
        return Err(ContractError::Unauthorized);
    }

    let key = policy_key(record_type);
    if retention_seconds == 0 {
        env.storage().persistent().remove(&key);
    } else {
        env.storage().persistent().set(&key, &retention_seconds);
        env.storage()
            .persistent()
            .extend_ttl(&key, TTL_THRESHOLD, TTL_EXTEND_TO);
    }

    events::publish_retention_period_set(
        env,
        admin.clone(),
        record_type.clone(),
        retention_seconds,
    );

    Ok(())
}

/// Returns the configured retention duration in seconds for a given data class.
/// Returns `0` if no specific retention policy has been set (indefinite retention).
pub fn get_retention_period(env: &Env, record_type: &RecordType) -> u64 {
    let key = policy_key(record_type);
    env.storage().persistent().get::<_, u64>(&key).unwrap_or(0)
}

/// Checks whether a record has exceeded its data class retention period.
/// Returns `false` if the record does not exist or has indefinite retention.
pub fn is_record_expired(env: &Env, record_id: u64) -> bool {
    let key = (symbol_short!("RECORD"), record_id);
    if let Some(record) = env.storage().persistent().get::<_, VisionRecord>(&key) {
        if let Some(expiry) = record.retain_until {
            return env.ledger().timestamp() > expiry;
        }
    }
    false
}

/// Purges an expired record and all its associated child entries from persistent storage.
///
/// Coordinates with Issue #325 TTL cleanup patterns:
/// - Reclaims persistent storage entries for the record and its examinations/prescriptions.
/// - Cleans the record ID from the patient index (`PAT_REC`).
/// - Emits compliance audit and purge events.
///
/// Returns `true` if an expired record was purged. Returns `false` if the record does not exist
/// or its retention period has not yet elapsed.
pub fn purge_expired_record(env: &Env, record_id: u64) -> bool {
    let key = (symbol_short!("RECORD"), record_id);
    let record: VisionRecord = match env.storage().persistent().get(&key) {
        Some(r) => r,
        None => return false,
    };

    let expiry = match record.retain_until {
        Some(exp) => exp,
        None => return false,
    };

    if env.ledger().timestamp() <= expiry {
        return false;
    }

    // 1. Remove the main record entry from storage
    env.storage().persistent().remove(&key);

    // 2. Remove record ID from patient index
    let patient_key = (symbol_short!("PAT_REC"), record.patient.clone());
    if let Some(patient_records) = env.storage().persistent().get::<_, Vec<u64>>(&patient_key) {
        let mut updated = Vec::new(env);
        for id in patient_records.iter() {
            if id != record_id {
                updated.push_back(id);
            }
        }
        env.storage().persistent().set(&patient_key, &updated);
    }

    // 3. Remove associated exam, prescription, and archive markers if present
    env.storage()
        .persistent()
        .remove(&(symbol_short!("EXAM"), record_id));
    env.storage()
        .persistent()
        .remove(&(symbol_short!("PRESC"), record_id));
    env.storage()
        .persistent()
        .remove(&(symbol_short!("ARCHIVE"), record_id));

    // 4. Record compliance audit entry
    let audit_entry = audit::create_audit_entry(
        env,
        record.patient.clone(),
        record.patient.clone(),
        Some(record_id),
        AccessAction::Delete,
        AccessResult::Success,
        Some(String::from_str(env, "Record purged per retention policy")),
    );
    audit::add_audit_entry(env, &audit_entry);
    events::publish_audit_log_entry(env, &audit_entry);

    // 5. Emit purge event
    events::publish_record_purged(env, record_id);

    true
}
