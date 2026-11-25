# Storage Key Conventions

This document catalogs all storage keys used across RetinaX contracts to prevent collisions and data corruption.

## Storage Key Registry

### Clinical Records Contract
Located in: `contracts/clinical_records/src/`

| Key | Symbol | Purpose | Defined In |
|-----|--------|---------|------------|
| `ADMIN` | `symbol_short!("ADMIN")` | Admin address | `admin.rs` |
| `INIT` | `symbol_short!("INIT")` | Initialization flag | `admin.rs` |
| `V_REC` | `symbol_short!("V_REC")` | Vision records contract reference | `admin.rs` |
| `EXAM` | `symbol_short!("EXAM")` | Eye examination records | `examination.rs` |
| `RX` | `symbol_short!("RX")` | Prescription key | `prescription.rs` |
| `RX_CTR` | `symbol_short!("RX_CTR")` | Prescription counter | `prescription.rs` |
| `SESS_GRT` | `symbol_short!("SESS_GRT")` | Session key → principal grant | `session.rs` |
| `SESS` | `symbol_short!("SESS")` | Session level and expiry | `common/src/session.rs` |

### Access Control Contract
Located in: `contracts/access_control/src/permissions.rs`

| Key | Symbol | Purpose |
|-----|--------|---------|
| `ROLE_ASN` | `symbol_short!("ROLE_ASN")` | Role assignments |
| `DELEG` | `symbol_short!("DELEG")` | Delegation mapping |
| `SC_DEL` | `symbol_short!("SC_DEL")` | Scoped delegation |
| `ACL_GRP` | `symbol_short!("ACL_GRP")` | ACL group definitions |
| `USR_GRPS` | `symbol_short!("USR_GRPS")` | User group memberships |
| `ACC_POL` | `symbol_short!("ACC_POL")` | Access policies |
| `USR_CRED` | `symbol_short!("USR_CRED")` | User credentials |
| `REC_SENS` | `symbol_short!("REC_SENS")` | Record sensitivity levels |

### Vision Records Contract
Located in: `contracts/vision_records/src/lib.rs`

| Key | Symbol | Purpose |
|-----|--------|---------|
| *(To be documented)* | - | No conflicting keys found in initial audit |

### State Channel Contract
Located in: `contracts/state_channel/src/lib.rs`

| Key | Symbol | Purpose |
|-----|--------|---------|
| `V_REC` | `symbol_short!("V_REC")` | Vision records contract reference |
| `ADMIN` | `symbol_short!("ADMIN")` | Admin address |

### Orchestrator Contract
Located in: `contracts/orchestrator/src/lib.rs`

| Key | Symbol | Purpose |
|-----|--------|---------|
| `ADMIN` | `symbol_short!("ADMIN")` | Admin address |
| `INIT` | `symbol_short!("INIT")` | Initialization flag |

### Vault Contract
Located in: `contracts/vault/src/`

| Key | Symbol | Purpose | Defined In |
|-----|--------|---------|------------|
| `ADMIN` | `symbol_short!("ADMIN")` | Admin address | `lib.rs` |
| `IDENTITY` | `symbol_short!("IDENTITY")` | Identity contract reference | `lib.rs` |
| `INIT` | `symbol_short!("INIT")` | Initialization flag | `lib.rs` |
| `VAULT` | `symbol_short!("VAULT")` | Vault record per owner | `lib.rs` |
| `SHARE` | `symbol_short!("SHARE")` | Guardian share per owner/epoch | `lib.rs` |
| `EM_APR` | `symbol_short!("EM_APR")` | Emergency approvals | `lib.rs` |
| `AST_REG` | `symbol_short!("AST_REG")` | Supported asset registry | `accounting.rs` |
| `BAL` | `symbol_short!("BAL")` | Per-owner, per-asset balance | `accounting.rs` |
| `V_ASSETS` | `symbol_short!("V_ASSETS")` | Assets held by a vault | `accounting.rs` |
| `AST_TOT` | `symbol_short!("AST_TOT")` | Per-asset total of all vault balances | `accounting.rs` |
| `ACC_SEQ` | `symbol_short!("ACC_SEQ")` | Audit trail length per vault | `accounting.rs` |
| `ACC_LOG` | `symbol_short!("ACC_LOG")` | Audit trail entries | `accounting.rs` |

### Identity Contract (DID)
Located in: `contracts/identity/src/`

| Key | Symbol | Purpose | Defined In |
|-----|--------|---------|------------|
| `DID_DOC` | `Symbol::new("DID_DOC")` | DID record by DID and by controller | `lib.rs` |
| `DID_KEYS` | `Symbol::new("DID_KEYS")` | DID verification keys | `did.rs` |
| `DID_SVCS` | `Symbol::new("DID_SVCS")` | DID service endpoints | `did.rs` |
| `DID_CRT` | `Symbol::new("DID_CRT")` | DID creation timestamp | `did.rs` |

## Collision Analysis

### Potential Collisions Identified

1. **`ADMIN` key collision**:
   - Used in: `clinical_records`, `state_channel`, `orchestrator`
   - Risk: LOW - Each contract has its own storage space
   - Status: ✅ Safe (contracts are isolated)

2. **`INIT` key collision**:
   - Used in: `clinical_records`, `orchestrator`
   - Risk: LOW - Each contract has its own storage space
   - Status: ✅ Safe (contracts are isolated)

3. **`V_REC` key collision**:
   - Used in: `clinical_records`, `state_channel`
   - Risk: LOW - Each contract has its own storage space
   - Status: ✅ Safe (contracts are isolated)

## Key Naming Conventions

1. **Administrative keys**: `ADMIN`, `INIT`, `OWNER`
2. **Contract references**: `V_REC`, `ACC_CTL`
3. **Data counters**: `*_CTR` suffix (e.g., `RX_CTR`)
4. **Record types**: Abbreviated names (e.g., `RX` for prescriptions, `EXAM` for examinations)
5. **Permission keys**: Descriptive abbreviations (e.g., `ROLE_ASN`, `DELEG`)

## Best Practices

1. **Use descriptive abbreviations** (5-8 characters max for `symbol_short!`)
2. **Document all keys** in this file when adding new storage
3. **Check for conflicts** before adding new keys using `scripts/check_storage_keys.sh`
4. **Prefix contract-specific keys** with contract identifier when necessary
5. **Avoid generic names** that might be used across multiple contracts

## Validation Script

Run the storage key uniqueness check:

```bash
bash scripts/check_storage_keys.sh
```

This script scans all contracts for `symbol_short!()` declarations and reports any within-contract duplications.

## CI Integration

The storage key check is integrated into CI pipeline to automatically detect collisions on every PR.

See: `.github/workflows/storage-keys-check.yml`

---

**Last Updated**: 2026-10-17
**Audited By**: greyforreal
**Status**: ✅ No critical collisions detected
