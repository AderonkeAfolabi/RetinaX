# Error Handling and Recovery

This guide describes how integrators and maintainers should handle contract errors. Numeric codes and variant names are maintained in [Contract Error Code Reference](error-codes.md). Example negative tests are documented in [Error Handling and Negative Test Scenarios](error_scenarios.md).

## Handling Contract Results

Match the exported error type for the contract being called. Do not assume that the same numeric code has the same meaning in every contract.

```rust
match client.add_record(...) {
    Ok(record_id) => Ok(record_id),
    Err(error) => match error {
        ContractError::InvalidInput => Err("invalid request".into()),
        ContractError::AccessDenied => Err("caller is not authorized".into()),
        ContractError::TransientFailure => Err("retry later".into()),
        other => Err(other),
    },
}
```

## Retry Strategy

Retry only errors documented as transient or retryable, and use bounded exponential backoff. Preserve the original operation context and reset retry state after success.

- Maximum retries should be application-defined.
- Do not retry authorization, validation, not-found, or state-conflict errors.
- Do not retry an operation merely because it returned an unknown error.

## Authorization and State Conflicts

For authorization errors:

1. Verify the caller role and required permissions.
2. Check expiration and delegation state.
3. Request authorization from the contract administrator when appropriate.

For state-conflict errors:

1. Read current state before retrying.
2. Resolve duplicate or expired resources explicitly.
3. Preserve state consistency; never retry with the assumption that a failed transaction changed state.

## Logging and Events

Log the contract name, exported error variant, numeric code, operation, and caller when available. Publish only data that is safe to expose off-chain. Applications should correlate transaction events with a single request identifier.

Use alerts for:

- Critical contract state errors
- Repeated transient failures
- Storage errors
- Elevated error rates

## Testing

Every error branch should have a focused negative test. Test successful recovery separately from the original failure, and avoid assertions that only check `assert!(true)`.
