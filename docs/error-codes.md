# Contract Error Code Reference

This document is the canonical reference for Soroban `#[contracterror]` enums in RetinaX.
For handling and recovery behavior, see [Error Handling and Recovery](error-handling.md). For example negative-test scenarios, see [Error Handling and Negative Test Scenarios](error_scenarios.md).

## Shared `CommonError`

All contracts that use `common::CommonError` share these codes:

| Code | Variant |
|---:|---|
| 1 | `NotInitialized` |
| 2 | `AlreadyInitialized` |
| 10 | `AccessDenied` |
| 20 | `UserNotFound` |
| 21 | `RecordNotFound` |
| 30 | `InvalidInput` |
| 31 | `InvalidNonce` |
| 32 | `NonceOverflow` |
| 40 | `Paused` |
| 50 | `InvalidChannelState` |
| 51 | `InvalidSignature` |
| 52 | `InvalidTransition` |
| 53 | `ChallengePeriodActive` |
| 54 | `AlreadySettled` |

## Contract-Specific Error Enums

Contract-specific codes belong to their executing contract and should be matched using that contract's exported error type.

### Access Control

| Code | Variant |
|---:|---|
| 1 | `Unauthorized` |
| 2 | `InvalidInput` |
| 3 | `NotFound` |
| 4 | `DelegationExpired` |
| 5 | `Paused` |
| 6 | `AlreadyExists` |
| 7 | `AlreadyInitialized` |
| 8 | `DelegationDepthExceeded` |

### Clinical Records

| Code | Variant |
|---:|---|
| 1 | `AlreadyInitialized` |
| 2 | `NotInitialized` |
| 3 | `Unauthorized` |
| 4 | `InvalidInput` |
| 5 | `InvalidRecordType` |
| 6 | `RecordNotFound` |
| 7 | `InvalidPhase` |
| 8 | `SessionExpired` |
| 9 | `InsufficientAuthLevel` |

### Vision Records

| Code | Variant |
|---:|---|
| 1 | `NotInitialized` |
| 2 | `AlreadyInitialized` |
| 3 | `Unauthorized` |
| 4 | `UserNotFound` |
| 5 | `RecordNotFound` |
| 6 | `InvalidInput` |
| 7 | `AccessDenied` |
| 8 | `Paused` |
| 9 | `ProviderNotFound` |
| 10 | `ProviderAlreadyRegistered` |
| 11 | `InvalidVerificationStatus` |
| 12 | `InvalidAddress` |
| 13 | `InvalidTimestamp` |
| 14 | `StorageError` |
| 15 | `RateLimitExceeded` |
| 16 | `ExpiredAccess` |
| 17 | `InvalidRole` |
| 18 | `InvalidPermission` |
| 19 | `DelegationExpired` |
| 20 | `InvalidDataHash` |
| 21 | `DuplicateRecord` |
| 22 | `InvalidRecordType` |
| 23 | `ContractPaused` |
| 24 | `InsufficientPermissions` |
| 25 | `TransientFailure` |

### Other Contract Errors

Other contract crates define their own `#[contracterror]` enums, including `audit`, `key_manager`, `staking`, `treasury`, `governor`, `timelock`, `identity`, `zk_verifier`, `zk_voting`, `ai_integration`, `fhir`, `cross_chain`, `analytics`, `emr_bridge`, `vault`, `metering`, `orchestrator`, `events`, `delegation`, and `compliance`. Source enum declarations are the authoritative inventory while this reference is expanded; locate declarations with:

```bash
rg '#\[.*contracterror\]' contracts
```

When a contract adds or changes an enum, update this reference in the same pull request.
