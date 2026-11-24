# Vault Contract Guide

The Vault contract (`contracts/vault/src/lib.rs`) manages encrypted clinical and identity secrets using Shamir's Secret Sharing (SSS), multi-guardian thresholds, key rotation, emergency reconstruction, and a deadman switch with a configurable grace period.

---

## 1. Vault Configuration & Grace Period

A patient or organization configures a vault using `configure_vault`:

```rust
pub fn configure_vault(
    env: Env,
    owner: Address,
    threshold: u32,
    shard_count: u32,
    emergency_threshold: u32,
    inactivity_timeout_secs: u64,
    data_ref_hash: String,
    secret_seed: BytesN<32>,
    grace_period_secs: Option<u64>,
) -> Result<VaultSnapshot, VaultError>
```

### Parameters
- `owner`: Address owning the vault. Must authorize the call.
- `threshold`: Minimum number of guardian shares required to reconstruct the key during normal operation.
- `shard_count`: Total number of shares split across registered guardians (`threshold <= shard_count`).
- `emergency_threshold`: Minimum number of guardian approvals required for emergency reconstruction (`threshold <= emergency_threshold <= shard_count`).
- `inactivity_timeout_secs`: Base inactivity duration before the deadman switch begins triggering.
- `data_ref_hash`: Reference pointer (e.g. IPFS URI or sha256 hash) to the off-chain encrypted payload.
- `secret_seed`: 32-byte secret seed to split across guardians.
- `grace_period_secs`: Optional buffer duration (in seconds) added after `deadman_release_at`. Defaults to `0` when omitted (`None`).

---

## 2. Deadman Switch & Grace Period Lifecycle

The deadman switch protects keys from being locked forever if the vault owner becomes incapacitated, while preventing premature release if the owner is only temporarily offline:

```
+------------------------------------+
|  Normal Activity                   | <------- touch_activity() resets timer
|  now < deadman_release_at          |
+------------------------------------+
                  |
                  | Time passes > inactivity_timeout_secs
                  v
+------------------------------------+
|  Grace Period Active               | <------- touch_activity() still resets timer!
|  deadman_release_at <= now         |
|  now < deadman_release_at + grace  | -------> trigger_deadman_release() rejected (DeadmanNotReady)
+------------------------------------+
                  |
                  | Time passes > (deadman_release_at + grace_period_secs)
                  v
+------------------------------------+
|  Deadman Release Allowed           |
|  trigger_deadman_release() -> true |
+------------------------------------+
```

1. **Deadman Release Condition**:
   - `effective_release_at = deadman_release_at + policy.grace_period_secs`.
   - Release is permitted only when `now >= effective_release_at`.
   - Any attempt to trigger release before `effective_release_at` returns `VaultError::DeadmanNotReady`.
2. **Owner Activity During Grace Period**:
   - If the owner calls `touch_activity` during the grace period, the inactivity timer is fully reset:
     - `last_activity_at = now`
     - `deadman_release_at = now + policy.inactivity_timeout_secs`
   - This gives the owner time to check in and preserve custody.
3. **Backward Compatibility & Migration**:
   - Vault records configured under prior versions default to `grace_period_secs = 0` automatically via `get_vault_record` and `get_grace_period`, requiring no manual storage migrations.
   - Contracts can also explicitly migrate stored legacy records forward using `migrate_vault(env, owner)`.

---

## 3. Emergency Reconstruction Review

The contract provides emergency approval and key reconstruction workflows:
- `submit_emergency_approval(env, guardian, owner)`
- `emergency_reconstruct(env, owner) -> Result<BytesN<32>, VaultError>`

### Do Emergency Paths Need the Grace Period?
**No.** Emergency reconstruction intentionally does **not** enforce the deadman inactivity timeout or grace period:
1. **Multi-Party Consensus**: Emergency reconstruction requires `emergency_threshold` independent approvals from verified guardians, providing cryptographic protection against unilateral actions.
2. **Clinical Urgency**: In urgent medical situations where an owner is hospitalized or incapacitated, requiring guardians to wait for an inactivity timeout and grace period would endanger patient care.
3. **Separation of Concerns**: The deadman switch is an automated inactivity fail-safe, whereas emergency reconstruction is an active, multi-guardian override.
