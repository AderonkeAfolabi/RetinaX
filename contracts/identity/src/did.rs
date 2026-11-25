//! W3C-style DID document resolution (#449).
//!
//! A `did:retinax:*` identifier is backed by on-chain state:
//! - the [`crate::DidDocument`] record (controller, credentials, status),
//! - the verification keys registered with `add_did_key`,
//! - the service endpoints registered with `add_did_service`.
//!
//! [`resolve`] assembles that state into a [`ResolvedDidDocument`] whose fields
//! map one-to-one onto the W3C DID Core data model. The mapping is documented
//! in `docs/did-resolution.md`.

use soroban_sdk::{contracttype, Address, Bytes, BytesN, Env, String, Symbol, Vec};

use crate::recovery::RecoveryError;
use crate::DidDocument;

// ── Limits ──────────────────────────────────────────────────────────────────

/// Maximum length of a `did:retinax:*` identifier, in bytes.
pub const MAX_DID_LEN: u32 = 128;
/// Maximum length of a key or service fragment (the part after `#`).
pub const MAX_FRAGMENT_LEN: u32 = 32;
/// Maximum length of a service `type`.
pub const MAX_SERVICE_TYPE_LEN: u32 = 64;
/// Maximum length of a service endpoint URI.
pub const MAX_ENDPOINT_LEN: u32 = 256;
/// Maximum verification keys per DID.
pub const MAX_DID_KEYS: u32 = 10;
/// Maximum services per DID.
pub const MAX_DID_SERVICES: u32 = 10;

// ── Verification relationships (bit flags for `DidKey::purposes`) ───────────

pub const PURPOSE_AUTHENTICATION: u32 = 1 << 0;
pub const PURPOSE_ASSERTION_METHOD: u32 = 1 << 1;
pub const PURPOSE_KEY_AGREEMENT: u32 = 1 << 2;
pub const PURPOSE_CAPABILITY_INVOCATION: u32 = 1 << 3;
pub const PURPOSE_CAPABILITY_DELEGATION: u32 = 1 << 4;
pub const ALL_PURPOSES: u32 = PURPOSE_AUTHENTICATION
    | PURPOSE_ASSERTION_METHOD
    | PURPOSE_KEY_AGREEMENT
    | PURPOSE_CAPABILITY_INVOCATION
    | PURPOSE_CAPABILITY_DELEGATION;

// ── Storage ─────────────────────────────────────────────────────────────────

const DID_KEYS_PREFIX: &str = "DID_KEYS";
const DID_SVCS_PREFIX: &str = "DID_SVCS";
const DID_CREATED_PREFIX: &str = "DID_CRT";

const DID_CONTEXT: &str = "https://www.w3.org/ns/did/v1";

// ── Stored types ────────────────────────────────────────────────────────────

/// Cryptographic suite of a verification key.
#[contracttype]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum DidKeyType {
    /// 32-byte Ed25519 public key (signing).
    Ed25519 = 0,
    /// 32-byte X25519 public key (key agreement only).
    X25519 = 1,
    /// 33-byte compressed secp256k1 public key (signing).
    Secp256k1 = 2,
}

impl DidKeyType {
    fn key_len(self) -> u32 {
        match self {
            DidKeyType::Ed25519 | DidKeyType::X25519 => 32,
            DidKeyType::Secp256k1 => 33,
        }
    }

    /// W3C verification method `type`.
    fn method_type(self) -> &'static str {
        match self {
            DidKeyType::Ed25519 => "Ed25519VerificationKey2020",
            DidKeyType::X25519 => "X25519KeyAgreementKey2020",
            DidKeyType::Secp256k1 => "EcdsaSecp256k1VerificationKey2019",
        }
    }

    /// JSON-LD context for the suite.
    fn context(self) -> &'static str {
        match self {
            DidKeyType::Ed25519 => "https://w3id.org/security/suites/ed25519-2020/v1",
            DidKeyType::X25519 => "https://w3id.org/security/suites/x25519-2020/v1",
            DidKeyType::Secp256k1 => "https://w3id.org/security/suites/secp256k1-2019/v1",
        }
    }
}

/// A verification key registered against a DID.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DidKey {
    /// Fragment identifying the key within the document (e.g. `key-1`).
    pub key_id: String,
    pub key_type: DidKeyType,
    /// Raw public key bytes.
    pub public_key: Bytes,
    /// Bitmask of `PURPOSE_*` verification relationships.
    pub purposes: u32,
    pub added_at: u64,
}

/// A service endpoint registered against a DID.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DidService {
    /// Fragment identifying the service within the document (e.g. `records`).
    pub service_id: String,
    pub service_type: String,
    pub endpoint: String,
}

// ── Resolved (W3C-style) types ──────────────────────────────────────────────

/// W3C `verificationMethod` entry.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VerificationMethod {
    /// Absolute DID URL: `<did>#<key_id>`.
    pub id: String,
    /// W3C `type`, e.g. `Ed25519VerificationKey2020`.
    pub method_type: String,
    /// The DID that controls this key (the document's own DID).
    pub controller: String,
    /// Raw public key bytes. Serializers encode this as `publicKeyMultibase`.
    pub public_key: Bytes,
}

/// W3C `service` entry.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ServiceEndpoint {
    /// Absolute DID URL: `<did>#<service_id>`.
    pub id: String,
    /// W3C `type`.
    pub service_type: String,
    /// W3C `serviceEndpoint`.
    pub service_endpoint: String,
}

/// A DID document following the W3C DID Core data model.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ResolvedDidDocument {
    /// `@context`: the DID v1 context, then one suite context per key type used.
    pub context: Vec<String>,
    pub id: String,
    /// The controlling DID. RetinaX DIDs are self-controlled.
    pub controller: String,
    pub verification_method: Vec<VerificationMethod>,
    /// Verification method ids (DID URLs) per relationship.
    pub authentication: Vec<String>,
    pub assertion_method: Vec<String>,
    pub key_agreement: Vec<String>,
    pub capability_invocation: Vec<String>,
    pub capability_delegation: Vec<String>,
    pub service: Vec<ServiceEndpoint>,
}

/// W3C DID document metadata.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DidDocumentMetadata {
    /// Ledger timestamp at creation; `0` for DIDs created before #449.
    pub created: u64,
    pub updated: u64,
    pub deactivated: bool,
    /// The Stellar account that controls the DID on-chain.
    pub controller_account: Address,
    /// Credential ids bound to the controller (RetinaX extension).
    pub credential_ids: Vec<BytesN<32>>,
}

/// The result of resolving a DID.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DidResolution {
    pub did_document: ResolvedDidDocument,
    pub metadata: DidDocumentMetadata,
}

// ── Storage helpers ─────────────────────────────────────────────────────────

pub fn get_keys(env: &Env, did: &String) -> Vec<DidKey> {
    env.storage()
        .persistent()
        .get(&(Symbol::new(env, DID_KEYS_PREFIX), did.clone()))
        .unwrap_or_else(|| Vec::new(env))
}

pub fn get_services(env: &Env, did: &String) -> Vec<DidService> {
    env.storage()
        .persistent()
        .get(&(Symbol::new(env, DID_SVCS_PREFIX), did.clone()))
        .unwrap_or_else(|| Vec::new(env))
}

fn set_keys(env: &Env, did: &String, keys: &Vec<DidKey>) {
    env.storage()
        .persistent()
        .set(&(Symbol::new(env, DID_KEYS_PREFIX), did.clone()), keys);
}

fn set_services(env: &Env, did: &String, services: &Vec<DidService>) {
    env.storage()
        .persistent()
        .set(&(Symbol::new(env, DID_SVCS_PREFIX), did.clone()), services);
}

pub fn set_created(env: &Env, did: &String, at: u64) {
    env.storage()
        .persistent()
        .set(&(Symbol::new(env, DID_CREATED_PREFIX), did.clone()), &at);
}

fn get_created(env: &Env, did: &String) -> u64 {
    env.storage()
        .persistent()
        .get(&(Symbol::new(env, DID_CREATED_PREFIX), did.clone()))
        .unwrap_or(0)
}

// ── Mutations ───────────────────────────────────────────────────────────────

pub fn add_key(
    env: &Env,
    did: &String,
    key_id: String,
    key_type: DidKeyType,
    public_key: Bytes,
    purposes: u32,
) -> Result<(), RecoveryError> {
    if !valid_fragment(&key_id) {
        return Err(RecoveryError::InvalidInput);
    }
    if public_key.len() != key_type.key_len() {
        return Err(RecoveryError::InvalidInput);
    }
    if key_type == DidKeyType::Secp256k1 {
        let prefix = public_key.get(0).unwrap_or(0);
        if prefix != 0x02 && prefix != 0x03 {
            return Err(RecoveryError::InvalidInput);
        }
    }
    if !valid_purposes(key_type, purposes) {
        return Err(RecoveryError::InvalidInput);
    }

    let mut keys = get_keys(env, did);
    if fragment_in_use(env, did, &key_id) {
        return Err(RecoveryError::AlreadyExists);
    }
    if keys.len() >= MAX_DID_KEYS {
        return Err(RecoveryError::LimitExceeded);
    }
    keys.push_back(DidKey {
        key_id,
        key_type,
        public_key,
        purposes,
        added_at: env.ledger().timestamp(),
    });
    set_keys(env, did, &keys);
    Ok(())
}

pub fn remove_key(env: &Env, did: &String, key_id: &String) -> Result<(), RecoveryError> {
    let keys = get_keys(env, did);
    let mut kept = Vec::new(env);
    let mut found = false;
    for k in keys.iter() {
        if k.key_id == *key_id {
            found = true;
        } else {
            kept.push_back(k);
        }
    }
    if !found {
        return Err(RecoveryError::RecordNotFound);
    }
    set_keys(env, did, &kept);
    Ok(())
}

pub fn add_service(
    env: &Env,
    did: &String,
    service_id: String,
    service_type: String,
    endpoint: String,
) -> Result<(), RecoveryError> {
    if !valid_fragment(&service_id)
        || !valid_service_type(&service_type)
        || !valid_endpoint(&endpoint)
    {
        return Err(RecoveryError::InvalidInput);
    }
    let mut services = get_services(env, did);
    if fragment_in_use(env, did, &service_id) {
        return Err(RecoveryError::AlreadyExists);
    }
    if services.len() >= MAX_DID_SERVICES {
        return Err(RecoveryError::LimitExceeded);
    }
    services.push_back(DidService {
        service_id,
        service_type,
        endpoint,
    });
    set_services(env, did, &services);
    Ok(())
}

pub fn remove_service(env: &Env, did: &String, service_id: &String) -> Result<(), RecoveryError> {
    let services = get_services(env, did);
    let mut kept = Vec::new(env);
    let mut found = false;
    for s in services.iter() {
        if s.service_id == *service_id {
            found = true;
        } else {
            kept.push_back(s);
        }
    }
    if !found {
        return Err(RecoveryError::RecordNotFound);
    }
    set_services(env, did, &kept);
    Ok(())
}

/// Key and service fragments share one namespace: W3C requires every `id`
/// in a document to be unique.
fn fragment_in_use(env: &Env, did: &String, fragment: &String) -> bool {
    get_keys(env, did).iter().any(|k| k.key_id == *fragment)
        || get_services(env, did)
            .iter()
            .any(|s| s.service_id == *fragment)
}

// ── Resolution ──────────────────────────────────────────────────────────────

/// Builds the W3C-style document and metadata for a stored DID record.
///
/// A deactivated DID resolves with empty verification methods, relationships
/// and services, and `metadata.deactivated = true`, so its keys can no longer
/// be used to authenticate.
pub fn resolve(env: &Env, record: &DidDocument) -> DidResolution {
    let did = &record.did;
    let mut context = Vec::new(env);
    context.push_back(String::from_str(env, DID_CONTEXT));

    let mut verification_method = Vec::new(env);
    let mut authentication = Vec::new(env);
    let mut assertion_method = Vec::new(env);
    let mut key_agreement = Vec::new(env);
    let mut capability_invocation = Vec::new(env);
    let mut capability_delegation = Vec::new(env);
    let mut service = Vec::new(env);

    if !record.deactivated {
        for key in get_keys(env, did).iter() {
            let suite = String::from_str(env, key.key_type.context());
            if !context.contains(&suite) {
                context.push_back(suite);
            }

            let id = did_url(env, did, &key.key_id);
            verification_method.push_back(VerificationMethod {
                id: id.clone(),
                method_type: String::from_str(env, key.key_type.method_type()),
                controller: did.clone(),
                public_key: key.public_key.clone(),
            });

            let relationships = [
                (PURPOSE_AUTHENTICATION, &mut authentication),
                (PURPOSE_ASSERTION_METHOD, &mut assertion_method),
                (PURPOSE_KEY_AGREEMENT, &mut key_agreement),
                (PURPOSE_CAPABILITY_INVOCATION, &mut capability_invocation),
                (PURPOSE_CAPABILITY_DELEGATION, &mut capability_delegation),
            ];
            for (flag, list) in relationships {
                if key.purposes & flag != 0 {
                    list.push_back(id.clone());
                }
            }
        }

        for svc in get_services(env, did).iter() {
            service.push_back(ServiceEndpoint {
                id: did_url(env, did, &svc.service_id),
                service_type: svc.service_type.clone(),
                service_endpoint: svc.endpoint.clone(),
            });
        }
    }

    DidResolution {
        did_document: ResolvedDidDocument {
            context,
            id: did.clone(),
            controller: did.clone(),
            verification_method,
            authentication,
            assertion_method,
            key_agreement,
            capability_invocation,
            capability_delegation,
            service,
        },
        metadata: DidDocumentMetadata {
            created: get_created(env, did),
            updated: record.updated_at,
            deactivated: record.deactivated,
            controller_account: record.controller.clone(),
            credential_ids: record.credential_ids.clone(),
        },
    }
}

/// `<did>#<fragment>`. Both parts are length-checked on write.
fn did_url(env: &Env, did: &String, fragment: &String) -> String {
    const CAP: usize = (MAX_DID_LEN + 1 + MAX_FRAGMENT_LEN) as usize;
    let mut buf = [0u8; CAP];
    let did_len = (did.len() as usize).min(MAX_DID_LEN as usize);
    let frag_len = (fragment.len() as usize).min(MAX_FRAGMENT_LEN as usize);
    did.copy_into_slice(&mut buf[..did_len]);
    buf[did_len] = b'#';
    let start = did_len + 1;
    fragment.copy_into_slice(&mut buf[start..start + frag_len]);
    String::from_bytes(env, &buf[..start + frag_len])
}

// ── Validation ──────────────────────────────────────────────────────────────

fn valid_purposes(key_type: DidKeyType, purposes: u32) -> bool {
    if purposes == 0 || purposes & !ALL_PURPOSES != 0 {
        return false;
    }
    match key_type {
        // Encryption keys are only valid for key agreement.
        DidKeyType::X25519 => purposes == PURPOSE_KEY_AGREEMENT,
        // Signing keys must not be used for key agreement.
        DidKeyType::Ed25519 | DidKeyType::Secp256k1 => purposes & PURPOSE_KEY_AGREEMENT == 0,
    }
}

fn copy_small(s: &String, max: u32, buf: &mut [u8; 256]) -> Option<usize> {
    let len = s.len();
    if len == 0 || len > max || len as usize > buf.len() {
        return None;
    }
    let len = len as usize;
    s.copy_into_slice(&mut buf[..len]);
    Some(len)
}

/// 1–32 chars of `[A-Za-z0-9._-]`.
fn valid_fragment(s: &String) -> bool {
    let mut buf = [0u8; 256];
    match copy_small(s, MAX_FRAGMENT_LEN, &mut buf) {
        Some(len) => buf[..len]
            .iter()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, b'.' | b'_' | b'-')),
        None => false,
    }
}

/// 1–64 chars of `[A-Za-z0-9]`, e.g. `LinkedDomains`, `RetinaXRecords`.
fn valid_service_type(s: &String) -> bool {
    let mut buf = [0u8; 256];
    match copy_small(s, MAX_SERVICE_TYPE_LEN, &mut buf) {
        Some(len) => buf[..len].iter().all(|c| c.is_ascii_alphanumeric()),
        None => false,
    }
}

/// 1–256 chars of visible ASCII containing a `:` scheme separator.
fn valid_endpoint(s: &String) -> bool {
    let mut buf = [0u8; 256];
    match copy_small(s, MAX_ENDPOINT_LEN, &mut buf) {
        Some(len) => {
            let bytes = &buf[..len];
            bytes.iter().all(|c| (0x21..=0x7e).contains(c))
                && bytes.iter().position(|c| *c == b':').is_some_and(|i| i > 0)
        }
        None => false,
    }
}
