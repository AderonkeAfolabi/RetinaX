# Error Handling and Negative Test Scenarios

This file defines the roles of negative tests. Variant names and codes belong in [Contract Error Code Reference](error-codes.md); handling and recovery patterns belong in [Error Handling and Recovery](error-handling.md).

## Scenario Matrix

| Scenario | Expected behavior | Reference error |
|---|---|---|
| Unauthorized caller | Operation is rejected and no state changes | Contract-specific authorization variant |
| Invalid input | Operation rejects malformed or out-of-range data | `InvalidInput` |
| Missing resource | Operation reports that the entity does not exist | `UserNotFound`, `RecordNotFound`, or contract-specific not-found variant |
| State conflict | Operation is rejected without overwriting current state | `AlreadyInitialized`, `DuplicateRecord`, or contract-specific conflict variant |
| Transient failure | Operation can be retried with bounded backoff | `TransientFailure`, `StorageError`, or contract-specific transient variant |

## Test Location

Workspace negative and upgrade scenarios are maintained under [`test/negative/`](../test/negative) and [`test/upgrade/`](../test/upgrade). Contract-specific integration scenarios belong in the affected contract's `tests/` directory.

## Test Requirements

Each scenario must:

- Exercise the public contract operation, not only a helper
- Assert the exact exported error variant or code
- Verify state preservation after the failure
- Avoid placeholder assertions or tests that cannot fail
