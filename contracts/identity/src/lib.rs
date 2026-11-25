#![no_std]

//! # Identity Smart Contract
//!
//! Provides decentralized identity management, social recovery mechanisms via M-of-N guardians,
//! credential binding, and Zero-Knowledge (ZK) credential verification on Soroban / Stellar.
//!
//! ## Key Capabilities
//! - **Social Recovery**: Allows identity owners to configure 3 to 5 guardians and an M-of-N approval threshold.
//! - **Two-Phase Operations**: Offers prepare/commit/rollback workflows for guardian and threshold modifications.
//! - **ZK Integration**: Connects with `zk_verifier` smart contract for privacy-preserving proof validation.
//! - **Credential Binding**: Links off-chain/on-chain credentials to identity addresses.
//! - **Credential Revocation**: Lets issuers revoke credentials without exposing their holders.

pub mod credential;
pub mod did;
pub mod events;
pub mod recovery;
pub mod revocation;

use credential::CredentialError;
use did::{DidKey, DidKeyType, DidResolution, DidService};
use recovery::{RecoveryError, RecoveryRequest};
use soroban_sdk::{
    contract, contractimpl, contracttype, symbol_short, Address, Bytes, BytesN, Env, String,
    Symbol, Vec,
};

/// Preparation data for two-phase commit (2PC) guardian addition.
///
/// Stored in temporary storage during `prepare_add_guardian` and validated/consumed
/// during `commit_add_guardian` or cleaned up by `rollback_add_guardian`.
#[contracttype]
#[derive(Clone, Debug)]
pub struct PrepareGuardianAddition {
    /// The active identity owner requesting guardian addition.
    pub caller: Address,
    /// The candidate guardian address to be added.
    pub guardian: Address,
    /// Ledger timestamp when the prepare phase was executed.
    pub timestamp: u64,
}

/// Preparation data for two-phase commit (2PC) guardian removal.
///
/// Stored in temporary storage during `prepare_remove_guardian` and validated/consumed
/// during `commit_remove_guardian` or cleaned up by `rollback_remove_guardian`.
#[contracttype]
#[derive(Clone, Debug)]
pub struct PrepareGuardianRemoval {
    /// The active identity owner requesting guardian removal.
    pub caller: Address,
    /// The guardian address to be removed.
    pub guardian: Address,
    /// Ledger timestamp when the prepare phase was executed.
    pub timestamp: u64,
}

/// Preparation data for two-phase commit (2PC) recovery threshold change.
///
/// Stored in temporary storage during `prepare_set_recovery_threshold` and validated/consumed
/// during `commit_set_recovery_threshold` or cleaned up by `rollback_set_recovery_threshold`.
#[contracttype]
#[derive(Clone, Debug)]
pub struct PrepareThresholdChange {
    /// The active identity owner requesting threshold update.
    pub caller: Address,
    /// The proposed M-of-N threshold value (must be 1 <= threshold <= guardian count).
    pub threshold: u32,
    /// Ledger timestamp when the prepare phase was executed.
    pub timestamp: u64,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DidDocument {
    pub did: String,
    pub controller: Address,
    pub credential_ids: Vec<BytesN<32>>,
    pub deactivated: bool,
    pub updated_at: u64,
}

// ── Storage keys ─────────────────────────────────────────────────────────────

const ADMIN: Symbol = symbol_short!("ADMIN");
const INITIALIZED: Symbol = symbol_short!("INIT");
const HOLDER_BIND_PREFIX: &str = "HLD_BIND";
const DID_PREFIX: &str = "DID_DOC";

/// Re-export credential error for downstream consumers.
pub use credential::CredentialError as CredentialVerificationError;

// ── Contract ─────────────────────────────────────────────────────────────────

#[contract]
pub struct IdentityContract;

#[contractimpl]
impl IdentityContract {
    /// Initialize the identity contract with an owner address.
    pub fn initialize(env: Env, owner: Address) -> Result<(), RecoveryError> {
        if env.storage().instance().has(&INITIALIZED) {
            return Err(RecoveryError::AlreadyInitialized);
        }

        env.storage().instance().set(&ADMIN, &owner);
        env.storage().instance().set(&INITIALIZED, &true);
        recovery::set_owner_active(&env, &owner);
        events::emit_owner_status_changed(&env, owner, true);

        Ok(())
    }

    /// Create a RetinaX DID document for an active identity owner.
    pub fn create_did(env: Env, owner: Address, did: String) -> Result<(), RecoveryError> {
        owner.require_auth();
        Self::require_active_owner(&env, &owner)?;
        if !Self::valid_retinax_did(&env, &did) {
            return Err(RecoveryError::InvalidInput);
        }

        let did_key = (Symbol::new(&env, DID_PREFIX), did.clone());
        if env.storage().persistent().has(&did_key) {
            return Err(RecoveryError::AlreadyExists);
        }
        let owner_key = (Symbol::new(&env, DID_PREFIX), owner.clone());
        if env.storage().persistent().has(&owner_key) {
            return Err(RecoveryError::AlreadyExists);
        }

        let cred_key = (Symbol::new(&env, HOLDER_BIND_PREFIX), owner.clone());
        let credentials: Vec<BytesN<32>> = env
            .storage()
            .persistent()
            .get(&cred_key)
            .unwrap_or_else(|| Vec::new(&env));
        let document = DidDocument {
            did: did.clone(),
            controller: owner.clone(),
            credential_ids: credentials,
            deactivated: false,
            updated_at: env.ledger().timestamp(),
        };
        env.storage().persistent().set(&did_key, &document);
        env.storage().persistent().set(&owner_key, &document);
        did::set_created(&env, &did, document.updated_at);
        Ok(())
    }

    /// Resolve a `did:retinax:*` DID to its DID document.
    pub fn resolve_did(env: Env, did: String) -> Option<DidDocument> {
        if !Self::valid_retinax_did(&env, &did) {
            return None;
        }
        env.storage()
            .persistent()
            .get(&(Symbol::new(&env, DID_PREFIX), did))
    }

    /// Update DID document credentials and metadata. Controller authorization is required.
    pub fn update_did(env: Env, owner: Address, did: String) -> Result<(), RecoveryError> {
        owner.require_auth();
        Self::require_active_owner(&env, &owner)?;
        let did_key = (Symbol::new(&env, DID_PREFIX), did.clone());
        let mut document: DidDocument = env
            .storage()
            .persistent()
            .get(&did_key)
            .ok_or(RecoveryError::RecordNotFound)?;
        if document.controller != owner {
            return Err(RecoveryError::Unauthorized);
        }
        if document.deactivated {
            return Err(RecoveryError::OwnerDeactivated);
        }
        let cred_key = (Symbol::new(&env, HOLDER_BIND_PREFIX), owner.clone());
        document.credential_ids = env
            .storage()
            .persistent()
            .get(&cred_key)
            .unwrap_or_else(|| Vec::new(&env));
        document.updated_at = env.ledger().timestamp();
        env.storage().persistent().set(&did_key, &document);
        let owner_key = (Symbol::new(&env, DID_PREFIX), owner);
        env.storage().persistent().set(&owner_key, &document);
        Ok(())
    }

    /// Deactivate a DID document.
    pub fn deactivate_did(env: Env, owner: Address, did: String) -> Result<(), RecoveryError> {
        owner.require_auth();
        let did_key = (Symbol::new(&env, DID_PREFIX), did);
        let mut document: DidDocument = env
            .storage()
            .persistent()
            .get(&did_key)
            .ok_or(RecoveryError::RecordNotFound)?;
        if document.controller != owner {
            return Err(RecoveryError::Unauthorized);
        }
        if document.deactivated {
            return Err(RecoveryError::OwnerDeactivated);
        }
        document.deactivated = true;
        document.updated_at = env.ledger().timestamp();
        env.storage().persistent().set(&did_key, &document);
        let owner_key = (Symbol::new(&env, DID_PREFIX), owner);
        env.storage().persistent().set(&owner_key, &document);
        Ok(())
    }

    // ── W3C DID document resolution (#449) ──────────────────────────────────

    /// Register a verification key on a DID. `key_id` is the fragment used in
    /// the key's DID URL (`<did>#<key_id>`); `purposes` is a bitmask of
    /// `did::PURPOSE_*` verification relationships. Controller only.
    pub fn add_did_key(
        env: Env,
        owner: Address,
        did: String,
        key_id: String,
        key_type: DidKeyType,
        public_key: Bytes,
        purposes: u32,
    ) -> Result<(), RecoveryError> {
        let document = Self::load_controlled_did(&env, &owner, &did)?;
        did::add_key(&env, &did, key_id.clone(), key_type, public_key, purposes)?;
        Self::touch_did(&env, document);
        events::emit_did_key_changed(&env, did, key_id, true);
        Ok(())
    }

    /// Remove a verification key from a DID. Controller only.
    pub fn remove_did_key(
        env: Env,
        owner: Address,
        did: String,
        key_id: String,
    ) -> Result<(), RecoveryError> {
        let document = Self::load_controlled_did(&env, &owner, &did)?;
        did::remove_key(&env, &did, &key_id)?;
        Self::touch_did(&env, document);
        events::emit_did_key_changed(&env, did, key_id, false);
        Ok(())
    }

    /// Register a service endpoint on a DID. Controller only.
    pub fn add_did_service(
        env: Env,
        owner: Address,
        did: String,
        service_id: String,
        service_type: String,
        endpoint: String,
    ) -> Result<(), RecoveryError> {
        let document = Self::load_controlled_did(&env, &owner, &did)?;
        did::add_service(&env, &did, service_id.clone(), service_type, endpoint)?;
        Self::touch_did(&env, document);
        events::emit_did_service_changed(&env, did, service_id, true);
        Ok(())
    }

    /// Remove a service endpoint from a DID. Controller only.
    pub fn remove_did_service(
        env: Env,
        owner: Address,
        did: String,
        service_id: String,
    ) -> Result<(), RecoveryError> {
        let document = Self::load_controlled_did(&env, &owner, &did)?;
        did::remove_service(&env, &did, &service_id)?;
        Self::touch_did(&env, document);
        events::emit_did_service_changed(&env, did, service_id, false);
        Ok(())
    }

    /// Verification keys registered on a DID, in insertion order.
    pub fn get_did_keys(env: Env, did: String) -> Vec<DidKey> {
        did::get_keys(&env, &did)
    }

    /// Service endpoints registered on a DID, in insertion order.
    pub fn get_did_services(env: Env, did: String) -> Vec<DidService> {
        did::get_services(&env, &did)
    }

    /// Resolve a DID to a W3C-style DID document plus document metadata.
    /// See `docs/did-resolution.md` for the field mapping.
    pub fn resolve_did_document(env: Env, did: String) -> Result<DidResolution, RecoveryError> {
        if !Self::valid_retinax_did(&env, &did) {
            return Err(RecoveryError::InvalidInput);
        }
        let record: DidDocument = env
            .storage()
            .persistent()
            .get(&(Symbol::new(&env, DID_PREFIX), did))
            .ok_or(RecoveryError::RecordNotFound)?;
        Ok(did::resolve(&env, &record))
    }

    /// Loads a DID record the caller controls and may still modify.
    fn load_controlled_did(
        env: &Env,
        owner: &Address,
        did: &String,
    ) -> Result<DidDocument, RecoveryError> {
        owner.require_auth();
        Self::require_active_owner(env, owner)?;
        let document: DidDocument = env
            .storage()
            .persistent()
            .get(&(Symbol::new(env, DID_PREFIX), did.clone()))
            .ok_or(RecoveryError::RecordNotFound)?;
        if document.controller != *owner {
            return Err(RecoveryError::Unauthorized);
        }
        if document.deactivated {
            return Err(RecoveryError::OwnerDeactivated);
        }
        Ok(document)
    }

    /// Bumps `updated_at` on both copies of a DID record.
    fn touch_did(env: &Env, mut document: DidDocument) {
        document.updated_at = env.ledger().timestamp();
        env.storage().persistent().set(
            &(Symbol::new(env, DID_PREFIX), document.did.clone()),
            &document,
        );
        env.storage().persistent().set(
            &(Symbol::new(env, DID_PREFIX), document.controller.clone()),
            &document,
        );
    }

    fn valid_retinax_did(_env: &Env, did: &String) -> bool {
        const PREFIX: &[u8] = b"did:retinax:";
        if did.len() as usize <= PREFIX.len() || did.len() > did::MAX_DID_LEN {
            return false;
        }
        let mut bytes = [0u8; did::MAX_DID_LEN as usize];
        let len = did.len() as usize;
        did.copy_into_slice(&mut bytes[..len]);
        &bytes[..PREFIX.len()] == PREFIX
    }

    /// Add a guardian address for social recovery (max 5).
    pub fn add_guardian(env: Env, caller: Address, guardian: Address) -> Result<(), RecoveryError> {
        caller.require_auth();
        Self::require_active_owner(&env, &caller)?;
        let result = recovery::add_guardian(&env, &caller, guardian.clone());
        if result.is_ok() {
            events::emit_guardian_changed(&env, caller, guardian, true);
        }
        result
    }

    /// Remove a guardian address.
    pub fn remove_guardian(
        env: Env,
        caller: Address,
        guardian: Address,
    ) -> Result<(), RecoveryError> {
        caller.require_auth();
        Self::require_active_owner(&env, &caller)?;
        let result = recovery::remove_guardian(&env, &caller, &guardian);
        if result.is_ok() {
            events::emit_guardian_changed(&env, caller, guardian, false);
        }
        result
    }

    /// Set the M-of-N approval threshold for recovery.
    pub fn set_recovery_threshold(
        env: Env,
        caller: Address,
        threshold: u32,
    ) -> Result<(), RecoveryError> {
        caller.require_auth();
        Self::require_active_owner(&env, &caller)?;
        recovery::set_threshold(&env, &caller, threshold)
    }

    /// A guardian initiates recovery, proposing a new address.
    /// The initiating guardian counts as the first approval.
    pub fn initiate_recovery(
        env: Env,
        guardian: Address,
        owner: Address,
        new_address: Address,
    ) -> Result<(), RecoveryError> {
        guardian.require_auth();
        let result = recovery::initiate_recovery(&env, &guardian, &owner, new_address.clone());
        if result.is_ok() {
            events::emit_recovery_initiated(&env, owner, new_address, guardian);
        }
        result
    }

    /// A guardian approves an active recovery request.
    pub fn approve_recovery(
        env: Env,
        guardian: Address,
        owner: Address,
    ) -> Result<(), RecoveryError> {
        guardian.require_auth();
        recovery::approve_recovery(&env, &guardian, &owner)
    }

    /// Execute recovery after cooldown and sufficient approvals.
    /// Transfers identity ownership and deactivates the old address.
    pub fn execute_recovery(
        env: Env,
        caller: Address,
        owner: Address,
    ) -> Result<Address, RecoveryError> {
        caller.require_auth();
        let result = recovery::execute_recovery(&env, &owner);
        if let Ok(ref new_addr) = result {
            Self::migrate_bound_credentials(&env, &owner, new_addr);
            events::emit_recovery_executed(&env, owner.clone(), new_addr.clone());
            events::emit_owner_status_changed(&env, owner, false);
            events::emit_owner_status_changed(&env, new_addr.clone(), true);
        }
        result
    }

    /// Owner cancels an active recovery request.
    pub fn cancel_recovery(env: Env, caller: Address) -> Result<(), RecoveryError> {
        caller.require_auth();
        Self::require_active_owner(&env, &caller)?;
        let result = recovery::cancel_recovery(&env, &caller);
        if result.is_ok() {
            events::emit_recovery_cancelled(&env, caller);
        }
        result
    }

    /// Check if an address is an active identity owner.
    pub fn is_owner_active(env: Env, owner: Address) -> bool {
        recovery::is_owner_active(&env, &owner)
    }

    /// Get the list of guardians for an owner.
    pub fn get_guardians(env: Env, owner: Address) -> Vec<Address> {
        recovery::get_guardians(&env, &owner)
    }

    /// Check if a guardian is registered for an owner.
    pub fn is_guardian(env: Env, owner: Address, guardian: Address) -> bool {
        recovery::get_guardians(&env, &owner).contains(&guardian)
    }

    /// Get the recovery threshold for an owner.
    pub fn get_recovery_threshold(env: Env, owner: Address) -> u32 {
        recovery::get_threshold(&env, &owner)
    }

    /// Get the active recovery request for an owner, if any.
    pub fn get_recovery_request(env: Env, owner: Address) -> Option<RecoveryRequest> {
        recovery::get_recovery_request(&env, &owner)
    }

    // ===== Two-Phase Commit Hooks =====

    /// Prepare phase for add_guardian operation
    pub fn prepare_add_guardian(
        env: Env,
        caller: Address,
        guardian: Address,
    ) -> Result<(), RecoveryError> {
        // Validate all inputs without making state changes
        Self::require_active_owner(&env, &caller)?;

        // Check if guardian already exists
        let guardians = recovery::get_guardians(&env, &caller);
        if guardians.contains(&guardian) {
            return Err(RecoveryError::DuplicateGuardian);
        }

        // Check guardian limit
        if guardians.len() >= 5 {
            return Err(RecoveryError::MaxGuardiansReached);
        }

        // Store temporary preparation data
        let prep_key = (
            Symbol::new(&env, "PREP_ADD_GUARD"),
            caller.clone(),
            guardian.clone(),
        );
        let prep_data = PrepareGuardianAddition {
            caller: caller.clone(),
            guardian: guardian.clone(),
            timestamp: env.ledger().timestamp(),
        };
        env.storage().temporary().set(&prep_key, &prep_data);

        Ok(())
    }

    /// Commit phase for add_guardian operation
    pub fn commit_add_guardian(
        env: Env,
        caller: Address,
        guardian: Address,
    ) -> Result<(), RecoveryError> {
        // Retrieve preparation data
        let prep_key = (
            Symbol::new(&env, "PREP_ADD_GUARD"),
            caller.clone(),
            guardian.clone(),
        );
        let prep_data: PrepareGuardianAddition = env
            .storage()
            .temporary()
            .get(&prep_key)
            .ok_or(RecoveryError::Unauthorized)?; // Using Unauthorized as InvalidPhase equivalent

        // Verify preparation data matches commit parameters
        if prep_data.caller != caller || prep_data.guardian != guardian {
            return Err(RecoveryError::Unauthorized);
        }

        // Execute the actual guardian addition
        recovery::add_guardian(&env, &caller, guardian.clone())?;

        // Clean up preparation data
        env.storage().temporary().remove(&prep_key);

        Ok(())
    }

    /// Rollback for add_guardian operation
    pub fn rollback_add_guardian(
        env: Env,
        caller: Address,
        guardian: Address,
    ) -> Result<(), RecoveryError> {
        // Clean up preparation data
        let prep_key = (Symbol::new(&env, "PREP_ADD_GUARD"), caller, guardian);
        env.storage().temporary().remove(&prep_key);

        Ok(())
    }

    /// Prepare phase for remove_guardian operation
    pub fn prepare_remove_guardian(
        env: Env,
        caller: Address,
        guardian: Address,
    ) -> Result<(), RecoveryError> {
        // Validate all inputs without making state changes
        Self::require_active_owner(&env, &caller)?;

        // Check if guardian exists
        let guardians = recovery::get_guardians(&env, &caller);
        if !guardians.contains(&guardian) {
            return Err(RecoveryError::GuardianNotFound);
        }

        // Store temporary preparation data
        let prep_key = (
            Symbol::new(&env, "PREP_REM_GUARD"),
            caller.clone(),
            guardian.clone(),
        );
        let prep_data = PrepareGuardianRemoval {
            caller: caller.clone(),
            guardian: guardian.clone(),
            timestamp: env.ledger().timestamp(),
        };
        env.storage().temporary().set(&prep_key, &prep_data);

        Ok(())
    }

    /// Commit phase for remove_guardian operation
    pub fn commit_remove_guardian(
        env: Env,
        caller: Address,
        guardian: Address,
    ) -> Result<(), RecoveryError> {
        // Retrieve preparation data
        let prep_key = (
            Symbol::new(&env, "PREP_REM_GUARD"),
            caller.clone(),
            guardian.clone(),
        );
        let prep_data: PrepareGuardianRemoval = env
            .storage()
            .temporary()
            .get(&prep_key)
            .ok_or(RecoveryError::Unauthorized)?;

        // Verify preparation data matches commit parameters
        if prep_data.caller != caller || prep_data.guardian != guardian {
            return Err(RecoveryError::Unauthorized);
        }

        // Execute the actual guardian removal
        recovery::remove_guardian(&env, &caller, &guardian)?;

        // Clean up preparation data
        env.storage().temporary().remove(&prep_key);

        Ok(())
    }

    /// Rollback for remove_guardian operation
    pub fn rollback_remove_guardian(
        env: Env,
        caller: Address,
        guardian: Address,
    ) -> Result<(), RecoveryError> {
        // Clean up preparation data
        let prep_key = (Symbol::new(&env, "PREP_REM_GUARD"), caller, guardian);
        env.storage().temporary().remove(&prep_key);

        Ok(())
    }

    /// Prepare phase for set_recovery_threshold operation
    pub fn prepare_set_recovery_threshold(
        env: Env,
        caller: Address,
        threshold: u32,
    ) -> Result<(), RecoveryError> {
        // Validate all inputs without making state changes
        Self::require_active_owner(&env, &caller)?;

        // Validate threshold
        if threshold == 0 || threshold > 5 {
            return Err(RecoveryError::InvalidThreshold);
        }

        // Check current guardians count
        let guardians = recovery::get_guardians(&env, &caller);
        if threshold > guardians.len() {
            return Err(RecoveryError::InvalidThreshold);
        }

        // Store temporary preparation data
        let prep_key = (Symbol::new(&env, "PREP_SET_THRESH"), caller.clone());
        let prep_data = PrepareThresholdChange {
            caller: caller.clone(),
            threshold,
            timestamp: env.ledger().timestamp(),
        };
        env.storage().temporary().set(&prep_key, &prep_data);

        Ok(())
    }

    /// Commit phase for set_recovery_threshold operation
    pub fn commit_set_recovery_threshold(
        env: Env,
        caller: Address,
        threshold: u32,
    ) -> Result<(), RecoveryError> {
        // Retrieve preparation data
        let prep_key = (Symbol::new(&env, "PREP_SET_THRESH"), caller.clone());
        let prep_data: PrepareThresholdChange = env
            .storage()
            .temporary()
            .get(&prep_key)
            .ok_or(RecoveryError::Unauthorized)?;

        // Verify preparation data matches commit parameters
        if prep_data.caller != caller || prep_data.threshold != threshold {
            return Err(RecoveryError::Unauthorized);
        }

        // Execute the actual threshold change
        recovery::set_threshold(&env, &caller, threshold)?;

        // Clean up preparation data
        env.storage().temporary().remove(&prep_key);

        Ok(())
    }

    /// Rollback for set_recovery_threshold operation
    pub fn rollback_set_recovery_threshold(
        env: Env,
        caller: Address,
        _threshold: u32,
    ) -> Result<(), RecoveryError> {
        // Clean up preparation data
        let prep_key = (Symbol::new(&env, "PREP_SET_THRESH"), caller);
        env.storage().temporary().remove(&prep_key);

        Ok(())
    }
    // ── ZK credential verification ────────────────────────────────────────────

    /// Set the address of the deployed `zk_verifier` contract.
    /// Only an active owner can call this.
    pub fn set_zk_verifier(
        env: Env,
        caller: Address,
        verifier_id: Address,
    ) -> Result<(), RecoveryError> {
        caller.require_auth();
        Self::require_active_owner(&env, &caller)?;
        credential::set_zk_verifier(&env, &verifier_id);
        Ok(())
    }

    /// Get the stored `zk_verifier` contract address.
    pub fn get_zk_verifier(env: Env) -> Option<Address> {
        credential::get_zk_verifier(&env)
    }

    /// Verify a ZK credential proof without revealing the credential on-chain.
    ///
    /// Delegates verification to the configured `zk_verifier` contract via a
    /// cross-contract call. Only the verification result and a privacy-preserving
    /// event (user + resource hash) are recorded.
    #[allow(clippy::too_many_arguments)]
    pub fn verify_zk_credential(
        env: Env,
        user: Address,
        resource_id: BytesN<32>,
        proof_a: soroban_sdk::Bytes,
        proof_b: soroban_sdk::Bytes,
        proof_c: soroban_sdk::Bytes,
        public_inputs: Vec<BytesN<32>>,
        expires_at: u64,
    ) -> Result<bool, CredentialError> {
        user.require_auth();
        let result = credential::verify_zk_credential(
            &env,
            &user,
            resource_id,
            proof_a,
            proof_b,
            proof_c,
            public_inputs,
            expires_at,
            0, // Default nonce; caller should set appropriately for replay protection
        );
        if let Ok(verified) = result {
            events::emit_zk_credential_verified(&env, user, verified);
        }
        result
    }

    // ── Credential holder binding ────────────────────────────────────────────

    /// Bind a credential to this identity. Only the identity owner can bind.
    ///
    /// This establishes an on-chain link between the holder's DID and a
    /// credential ID issued by the ZK verifier contract. The binding ensures
    /// only the rightful identity owner can present the credential.
    pub fn bind_credential(
        env: Env,
        caller: Address,
        credential_id: BytesN<32>,
    ) -> Result<(), RecoveryError> {
        caller.require_auth();
        Self::require_active_owner(&env, &caller)?;

        let key = (Symbol::new(&env, HOLDER_BIND_PREFIX), caller.clone());
        let mut creds: Vec<BytesN<32>> = env
            .storage()
            .persistent()
            .get(&key)
            .unwrap_or_else(|| Vec::new(&env));

        // Prevent duplicate bindings.
        if !creds.contains(&credential_id) {
            creds.push_back(credential_id.clone());
            env.storage().persistent().set(&key, &creds);
        }

        #[allow(deprecated)]
        env.events()
            .publish((symbol_short!("CRD_BIND"), caller), credential_id);

        Ok(())
    }

    /// Unbind a credential from this identity. Only the identity owner can unbind.
    pub fn unbind_credential(
        env: Env,
        caller: Address,
        credential_id: BytesN<32>,
    ) -> Result<(), RecoveryError> {
        caller.require_auth();
        Self::require_active_owner(&env, &caller)?;

        let key = (Symbol::new(&env, HOLDER_BIND_PREFIX), caller.clone());
        let creds: Vec<BytesN<32>> = env
            .storage()
            .persistent()
            .get(&key)
            .unwrap_or_else(|| Vec::new(&env));

        let mut new_creds = Vec::new(&env);
        for c in creds.iter() {
            if c != credential_id {
                new_creds.push_back(c);
            }
        }
        env.storage().persistent().set(&key, &new_creds);

        #[allow(deprecated)]
        env.events()
            .publish((symbol_short!("CRD_UBND"), caller), credential_id);

        Ok(())
    }

    /// Get all credential IDs bound to an identity.
    pub fn get_bound_credentials(env: Env, holder: Address) -> Vec<BytesN<32>> {
        let key = (Symbol::new(&env, HOLDER_BIND_PREFIX), holder);
        env.storage()
            .persistent()
            .get(&key)
            .unwrap_or_else(|| Vec::new(&env))
    }

    /// Check if a specific credential is bound to an identity.
    pub fn is_credential_bound(env: Env, holder: Address, credential_id: BytesN<32>) -> bool {
        let key = (Symbol::new(&env, HOLDER_BIND_PREFIX), holder);
        let creds: Vec<BytesN<32>> = env
            .storage()
            .persistent()
            .get(&key)
            .unwrap_or_else(|| Vec::new(&env));
        creds.contains(&credential_id)
    }

    // ── Credential revocation ────────────────────────────────────────────────

    /// Permanently revoke the credential at `index` of the issuer's own registry.
    ///
    /// Every issuer has a separate registry, so an issuer can only revoke credentials it issued.
    pub fn revoke(env: Env, issuer: Address, index: u64) {
        issuer.require_auth();
        revocation::revoke(&env, &issuer, index);
    }

    /// Check if `issuer` has revoked its credential at `index`.
    pub fn is_revoked(env: Env, issuer: Address, index: u64) -> bool {
        revocation::is_revoked(&env, &issuer, index)
    }

    // ── Internal helpers ─────────────────────────────────────────────────────

    /// Move credential bindings from a recovered identity to its new address.
    fn migrate_bound_credentials(env: &Env, from: &Address, to: &Address) {
        let from_key = (Symbol::new(env, HOLDER_BIND_PREFIX), from.clone());
        let creds: Option<Vec<BytesN<32>>> = env.storage().persistent().get(&from_key);
        if let Some(creds) = creds {
            let to_key = (Symbol::new(env, HOLDER_BIND_PREFIX), to.clone());
            let mut merged: Vec<BytesN<32>> = env
                .storage()
                .persistent()
                .get(&to_key)
                .unwrap_or_else(|| Vec::new(env));
            for cred in creds.iter() {
                if !merged.contains(&cred) {
                    merged.push_back(cred);
                }
            }
            env.storage().persistent().set(&to_key, &merged);
            env.storage().persistent().remove(&from_key);
        }
    }

    /// Revoke a credential by its index. Only the issuer can revoke.
    pub fn revoke(env: Env, issuer: Address, index: u64) -> Result<(), CredentialError> {
        issuer.require_auth();
        revocation::revoke(&env, &issuer, index);
        Ok(())
    }

    /// Check if a credential has been revoked by its issuer.
    pub fn is_revoked(env: Env, issuer: Address, index: u64) -> bool {
        revocation::is_revoked(&env, &issuer, index)
    }

    fn require_active_owner(env: &Env, caller: &Address) -> Result<(), RecoveryError> {
        if !recovery::is_owner_active(env, caller) {
            return Err(RecoveryError::Unauthorized);
        }
        Ok(())
    }
}
