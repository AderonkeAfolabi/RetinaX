# DID Resolution

The identity contract resolves `did:retinax:*` identifiers to documents that follow the [W3C DID Core](https://www.w3.org/TR/did-core/) data model. Everything in the document comes from on-chain state, so any client can rebuild it with a single read-only call:

```rust
let res: DidResolution = identity.resolve_did_document(&did);
```

The contract types live in `contracts/identity/src/did.rs`. The entry points are documented in the [Identity API](api/identity.md#did-documents).

## Lifecycle

1. The active identity owner calls `create_did(owner, "did:retinax:<id>")`. The owner's Stellar account becomes the DID's on-chain controller.
2. The controller adds keys with `add_did_key` and services with `add_did_service`, and removes them with `remove_did_key` and `remove_did_service`. Each write bumps `metadata.updated`.
3. `update_did` refreshes the bound credential list from `bind_credential`.
4. `deactivate_did` is final. Resolution still works, but returns an empty document with `metadata.deactivated = true`, and the DID can no longer be modified.

## On-chain state

| Storage key | Type | Holds |
| --- | --- | --- |
| `("DID_DOC", did)` | `DidDocument` | Controller account, bound credential ids, `deactivated`, `updated_at` |
| `("DID_KEYS", did)` | `Vec<DidKey>` | `key_id`, `key_type`, raw `public_key`, `purposes` bitmask, `added_at` |
| `("DID_SVCS", did)` | `Vec<DidService>` | `service_id`, `service_type`, `endpoint` |
| `("DID_CRT", did)` | `u64` | Creation timestamp |

## Field mapping

`resolve_did_document` returns a `DidResolution { did_document, metadata }`. Contract field names are snake_case, and `type` is a reserved word in Rust, so the contract uses `method_type` and `service_type` instead. A JSON serializer maps them as follows.

### DID document

| W3C property | `ResolvedDidDocument` field | Source |
| --- | --- | --- |
| `@context` | `context` | Always `https://www.w3.org/ns/did/v1`, followed by one suite context for each key type present, in first-use order |
| `id` | `id` | The DID |
| `controller` | `controller` | The DID itself. RetinaX DIDs are self-controlled; the controlling Stellar account is in `metadata.controller_account` |
| `verificationMethod` | `verification_method` | One entry per `DidKey`, in insertion order |
| `authentication` | `authentication` | Ids of keys with `PURPOSE_AUTHENTICATION` (bit 0) |
| `assertionMethod` | `assertion_method` | Ids of keys with `PURPOSE_ASSERTION_METHOD` (bit 1) |
| `keyAgreement` | `key_agreement` | Ids of keys with `PURPOSE_KEY_AGREEMENT` (bit 2) |
| `capabilityInvocation` | `capability_invocation` | Ids of keys with `PURPOSE_CAPABILITY_INVOCATION` (bit 3) |
| `capabilityDelegation` | `capability_delegation` | Ids of keys with `PURPOSE_CAPABILITY_DELEGATION` (bit 4) |
| `service` | `service` | One entry per `DidService`, in insertion order |

Verification relationships refer to methods by id, not by embedding them.

### Verification method

| W3C property | `VerificationMethod` field | Source |
| --- | --- | --- |
| `id` | `id` | `<did>#<key_id>` |
| `type` | `method_type` | From `key_type`. See the table below |
| `controller` | `controller` | The DID |
| `publicKeyMultibase` | `public_key` | Raw key bytes. Serializers must multibase-encode them (see below) |

| `DidKeyType` | Key bytes | `type` | Suite context | Allowed purposes |
| --- | --- | --- | --- | --- |
| `Ed25519` | 32 | `Ed25519VerificationKey2020` | `https://w3id.org/security/suites/ed25519-2020/v1` | Any except key agreement |
| `X25519` | 32 | `X25519KeyAgreementKey2020` | `https://w3id.org/security/suites/x25519-2020/v1` | Key agreement only |
| `Secp256k1` | 33 (compressed, `0x02`/`0x03` prefix) | `EcdsaSecp256k1VerificationKey2019` | `https://w3id.org/security/suites/secp256k1-2019/v1` | Any except key agreement |

To build `publicKeyMultibase`, prefix the raw key with its multicodec code and encode it as base58btc with a leading `z`. The codes are Ed25519 `0xed01`, X25519 `0xec01` and secp256k1 `0xe701`. The contract stores raw bytes so that verifiers on-chain can use them without decoding anything.

### Service

| W3C property | `ServiceEndpoint` field | Source |
| --- | --- | --- |
| `id` | `id` | `<did>#<service_id>` |
| `type` | `service_type` | `service_type` |
| `serviceEndpoint` | `service_endpoint` | `endpoint` |

### Document metadata

| W3C property | `DidDocumentMetadata` field | Source |
| --- | --- | --- |
| `created` | `created` | Ledger timestamp at `create_did`. `0` for DIDs created before this field existed |
| `updated` | `updated` | Ledger timestamp of the last create, update, key or service write, or deactivation |
| `deactivated` | `deactivated` | `true` after `deactivate_did` |
| (RetinaX) | `controller_account` | Stellar account allowed to modify the DID |
| (RetinaX) | `credential_ids` | Credential ids bound to the controller at the last `create_did` or `update_did` |

Timestamps are UNIX seconds. Serializers emit them as XML datetime strings, as DID Core requires.

## Example

A DID with one signing key, one encryption key and one service:

```json
{
  "didDocument": {
    "@context": [
      "https://www.w3.org/ns/did/v1",
      "https://w3id.org/security/suites/ed25519-2020/v1",
      "https://w3id.org/security/suites/x25519-2020/v1"
    ],
    "id": "did:retinax:patient-42",
    "controller": "did:retinax:patient-42",
    "verificationMethod": [
      {
        "id": "did:retinax:patient-42#key-1",
        "type": "Ed25519VerificationKey2020",
        "controller": "did:retinax:patient-42",
        "publicKeyMultibase": "z6Mk..."
      },
      {
        "id": "did:retinax:patient-42#enc-1",
        "type": "X25519KeyAgreementKey2020",
        "controller": "did:retinax:patient-42",
        "publicKeyMultibase": "z6LS..."
      }
    ],
    "authentication": ["did:retinax:patient-42#key-1"],
    "assertionMethod": ["did:retinax:patient-42#key-1"],
    "keyAgreement": ["did:retinax:patient-42#enc-1"],
    "service": [
      {
        "id": "did:retinax:patient-42#records",
        "type": "RetinaXRecords",
        "serviceEndpoint": "https://records.retinax.example/patients/42"
      }
    ]
  },
  "didDocumentMetadata": {
    "created": "2023-11-14T22:13:20Z",
    "updated": "2023-11-14T22:21:40Z",
    "deactivated": false
  }
}
```

Empty relationship arrays (here `capabilityInvocation`, `capabilityDelegation`) are left out of the JSON.

## Validation and limits

| Rule | Limit | Error |
| --- | --- | --- |
| DID format | Starts with `did:retinax:`, method-specific id is non-empty, at most 128 bytes in total | `InvalidInput` |
| Key and service fragments | 1-32 characters of `[A-Za-z0-9._-]`, unique within the DID across keys and services | `InvalidInput`, `AlreadyExists` |
| Service type | 1-64 alphanumeric characters | `InvalidInput` |
| Service endpoint | 1-256 visible ASCII characters with a `scheme:` prefix | `InvalidInput` |
| Keys per DID | 10 | `LimitExceeded` |
| Services per DID | 10 | `LimitExceeded` |

## Known limitations

- After social recovery (`execute_recovery`), a DID's controller is still the old, now deactivated account, so the DID can no longer be modified. Moving DID control to the recovered address is a separate change.
- Resolution covers DID documents only. DID URL dereferencing (for example fetching `#key-1` directly) is up to the client, which can do it by filtering `verification_method` or `service`.
