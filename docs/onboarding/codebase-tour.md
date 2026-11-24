# Codebase Orientation Tour

This is a guided walkthrough of the repository structure and how the contract suite fits together.

## Root-Level Files and Directories

- `Cargo.toml` - Rust workspace definition and shared dependencies.
- `Makefile` - Common build, test, and deployment commands.
- `deny.toml` - Dependency policy rules for `cargo deny`.
- `.coderabbit.yaml` - Code review automation configuration.
- `.pre-commit-config.yaml` - Pre-commit hooks configuration.
- `.github/workflows/` - GitHub Actions pipelines.
- `setup.sh` - Automated environment setup.
- `contracts/` - Soroban smart contract workspace crates.
- `sdk/zk_prover/` - Off-chain ZK proof generation SDK.
- `offchain-server/` - Standalone Rust server for audit logs and proof endpoints.
- `website/` - Static project website and `node:test` suite.
- `test/` and `tests/integration/` - Shared and cross-contract test suites.
- `fuzz/` - Cargo fuzz targets.
- `docs/` - Architecture, API, onboarding, runbooks, compliance, and the [documentation index](../README.md).

## Contracts Workspace

All contract crates live in `contracts/` and are part of the workspace. Each contract is a separate Rust crate with its own `Cargo.toml`, and they share utilities from the common library.

### Contract Suite (26 crates)

- `contracts/access_control` - Role and permission management for micro-contracts.
- `contracts/ai_integration` - AI-assisted vision analysis and anomaly handling.
- `contracts/analytics` - Aggregated metrics and reporting for vision data.
- `contracts/audit` - Audit trail and access logging.
- `contracts/benches` - Benchmark crate; not a deployed contract.
- `contracts/clinical_records` - Clinical record storage and access rules.
- `contracts/common` - Shared utilities, error types, and security helpers; not a deployed contract.
- `contracts/compliance` - Compliance checks and enforcement for policy rules.
- `contracts/cross_chain` - Cross-chain relay and data portability helpers.
- `contracts/delegation` - Delegated authority and scoped permissions.
- `contracts/emr_bridge` - EMR integration and data interoperability.
- `contracts/events` - Event streaming and contract telemetry.
- `contracts/fhir` - FHIR-compatible data transforms.
- `contracts/governor` - Governance proposals and voting.
- `contracts/identity` - Identity and verification primitives.
- `contracts/key_manager` - Key derivation and key rotation.
- `contracts/metering` - Usage metering and quotas.
- `contracts/orchestrator` - Cross-contract workflow orchestration.
- `contracts/staking` - Staking and incentives.
- `contracts/state_channel` - Off-chain state channel primitives.
- `contracts/timelock` - Timelock execution for governance actions.
- `contracts/treasury` - Treasury controls and funds management.
- `contracts/vault` - Secure value and secret holding helpers.
- `contracts/vision_records` - Core vision record management contract.
- `contracts/zk_verifier` - ZK proof verification.
- `contracts/zk_voting` - ZK-enabled governance voting.

## Vision Records Contract Deep Dive

The main contract lives in `contracts/vision_records`.

### Module Structure

- `lib.rs` - Contract entry points, shared state, and public API.
- `appointment.rs` - Appointment scheduling and visit tracking.
- `audit.rs` - Access auditing and usage tracing.
- `circuit_breaker.rs` - Emergency pause and safety controls.
- `consent_management.rs` - Patient consent lifecycle.
- `emergency.rs` - Emergency access and break-glass flows.
- `errors.rs` - Contract-specific error definitions.
- `events.rs` - Event emissions for observability.
- `examination.rs` - Vision examination structures and logic.
- `export.rs` - Patient data export helpers.
- `insurance.rs` - Insurance metadata and claim support.
- `multisig.rs` - Multi-party approval helpers.
- `patient_profile.rs` - Patient profile storage and updates.
- `portability.rs` - Data portability flows.
- `prescription.rs` - Prescription creation and update logic.
- `provenance.rs` - Record provenance tracking.
- `rbac.rs` - Role-based access control and policy evaluation.
- `transactions.rs` - Internal transaction helpers.
- `types.rs` - Common types and enums.
- `user.rs` - User registration and profile helpers.

Shared migration helpers live in `contracts/common/src/migration.rs`.

### How the Modules Interconnect

- `lib.rs` provides the public contract API and wires in storage keys.
- `rbac.rs` enforces permissions across patient and provider operations.
- `audit.rs` and `events.rs` record access and emit trace events.
- `emergency.rs` and `circuit_breaker.rs` provide break-glass and pause controls.
- `examination.rs` and `prescription.rs` hold the domain model for care records.

### Storage Key Organization

Storage keys use the Soroban `symbol_short!` convention. See [storage key conventions](../storage-key-conventions.md).

## Documentation

All documentation lives in `docs/`. Start with:

- [Documentation index](../README.md)
- [Architecture overview](../architecture.md)
- [API reference](../api.md)
- [Security model](../security.md)

## Scripts

Utility scripts live in `scripts/`:

- `deploy.sh` - Deployment helper for local, testnet, and mainnet.
- `check_storage_keys.sh` - Detects storage key collisions.
- `run_coverage.sh` - Coverage runs for CI.

## Fuzzing

Fuzz testing is in `fuzz/` and uses `cargo-fuzz`. This is how we stress test contract invariants.

## CI/CD

GitHub Actions workflows live in `.github/workflows/` and include Rust tests, formatting, contract checks, website linting, Markdown link checks, security scans, and deployment automation.
