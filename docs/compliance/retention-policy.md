# Vision Records Retention Policy & Automatic Expiry

## 1. Overview & Regulatory Objectives

Medical and optical health data retention is governed by stringent international regulations:
- **HIPAA Privacy & Security Rule (45 CFR § 164.316(b)(2)(i))**: Mandates policies, procedures, and retention of records for required statutory periods (typically 6 years from creation date, or longer based on applicable state law).
- **GDPR Article 5(1)(e) (Storage Limitation Principle)**: Requires personal data to be kept in a form permitting identification of data subjects for no longer than is necessary for the purposes for which the personal data are processed.
- **GDPR Article 17 (Right to Erasure / "Right to be Forgotten")**: Requires mechanisms to irreversibly purge or restrict expired clinical data from active query indices.

RetinaX implements a granular, data-class-specific retention system in `contracts/vision_records`. Each clinical data class (`RecordType`) can be assigned a retention horizon, after which:
1. Records immediately become inaccessible to all callers (`RecordExpired`).
2. Records are automatically excluded from patient lists and batch queries.
3. Storage entries are safely reclaimable via decentralized TTL purge functions (`purge_expired_record`).

---

## 2. Architecture & Data Model

### Data Class Retention Mapping

Retention durations are tracked per `RecordType`:
- `Examination`: Routine eye exams, visual acuity, IOP, fundus photos
- `Prescription`: Eyeglass and contact lens prescriptions
- `Diagnosis`: Clinical diagnostic assessments
- `Treatment`: Therapeutic interventions
- `Surgery`: Operative notes and surgical logs
- `LabResult`: Diagnostic laboratory findings

```text
+-----------------------+     +------------------------+
| Admin Configuration   | --> | Persistent Storage     |
| (set_retention_period)|     | (RET_POL, RecordType)  |
+-----------------------+     +------------------------+
                                         |
                                         v
+-----------------------+     +------------------------+
| Record Addition       | --> | VisionRecord           |
| (add_record)          |     | .retain_until =        |
+-----------------------+     |   now + policy_seconds |
                              +------------------------+
```

### Storage Representation

Each `VisionRecord` stores:
```rust
pub struct VisionRecord {
    pub id: u64,
    pub patient: Address,
    pub provider: Address,
    pub record_type: RecordType,
    pub data_hash: String,
    pub key_version: Option<String>,
    pub created_at: u64,
    pub updated_at: u64,
    pub retain_until: Option<u64>,
}
```

- When `retain_until == None`: Indefinite retention (default when no policy is configured).
- When `retain_until == Some(timestamp)`: Hard expiration boundary.

---

## 3. Enforcement: Automatic Expiry

Enforcement occurs at query time based on the immutable Soroban ledger timestamp:

1. **Direct Lookup (`get_record`)**:
   - If `ledger.timestamp() > retain_until`, access is blocked immediately with `ContractError::RecordExpired`.
   - A denied audit log entry is written for compliance tracking.
2. **Patient Listings (`get_patient_records`, `get_patient_records_paginated`)**:
   - Expired records are excluded from index enumerations.
3. **Batch Fetching (`get_records_by_ids`)**:
   - Expired records are silently omitted.

---

## 4. Storage Purging & Issue #325 Coordination

To coordinate with the decentralized storage management patterns established in **Issue #325** (`purge_expired_consent`, `purge_expired_role_assignment`):

- **Reclamation Endpoint (`purge_expired_record`)**:
  - Validates that the record has genuinely reached expiration (`ledger.timestamp() > retain_until`).
  - Premature purge attempts return `false` without modifying storage.
  - Upon valid expiration:
    1. Removes the `(RECORD, record_id)` entry from persistent storage.
    2. Evicts the `record_id` from the patient index `(PAT_REC, patient)`.
    3. Evicts associated examination `(EXAM, record_id)` or prescription `(PRESC, record_id)` data.
    4. Records an audit entry (`AccessAction::Delete`) for compliance assurance.
    5. Emits a `RecordPurgedEvent` to the ledger event stream.
- **Batch Endpoint (`purge_expired_records_batch`)**:
  - Allows relayer daemons or scheduled maintenance cron tasks to clean batches of expired records in a single transaction.

---

## 5. Compliance Sign-Off

| Requirement | Standard | Implementation Status | Sign-Off Verified |
|-------------|----------|-----------------------|-------------------|
| Data Retention Management | HIPAA 45 CFR § 164.316(b)(2)(i) | Configurable data class retention durations | **YES** |
| Storage Limitation | GDPR Art. 5(1)(e) | Immediate read lockdown upon expiration (`RecordExpired`) | **YES** |
| Secure Erasure / Purge | GDPR Art. 17 | On-chain TTL storage reclamation via `purge_expired_record` | **YES** |
| Audit Trail on Expiry & Purge | HIPAA 45 CFR § 164.312(b) | Immutable audit log entries for read denials and purge events | **YES** |
