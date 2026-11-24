# RetinaX Documentation Index

Welcome to the RetinaX documentation. This index groups every guide by the audience it serves: contributors, integrators, operators, and auditors.

## Contributors

- [Getting Started](GETTING_STARTED.md) — Quickstart guide for building and testing RetinaX locally.
- [Glossary](GLOSSARY.md) — Definitions for Soroban, healthcare, and RetinaX domain terms.
- [Architecture Overview](architecture.md) — System architecture, module boundaries, and deployment topology.
- [Data Flows](data-flows.md) — End-to-end data and call flows across contracts and off-chain components.
- [Cross-Contract Interactions](cross-contract-interactions.md) — Cross-contract call paths and dependency contracts.
- [Common Library](common-library.md) — Shared Rust library types, helpers, and utilities.
- [Storage Key Conventions](storage-key-conventions.md) — Rules for naming and checking Soroban storage keys.
- [Error Codes](error-codes.md) — Canonical contract error codes and their meanings.
- [Error Handling](error-handling.md) — Error propagation and handling patterns for contracts.
- [Error Scenarios](error_scenarios.md) — Example failure scenarios and expected contract responses.
- [Testing Guide](testing.md) — Contract test layout and day-to-day testing commands.
- [Testing Strategy](testing-strategy.md) — Test strategy, quality gates, and CI expectations.
- [Fuzzing](fuzzing.md) — Fuzzing setup and targets for contract robustness.
- [Property Testing](property-testing.md) — Property-based testing guidance for invariants.
- [Performance](performance.md) — Performance measurement and optimization guidance.
- [Gas Budget](gas-budget.md) — Gas budget expectations and cost-control techniques.
- [Website Development Guide](website-dev-guide.md) — Local website tooling, tests, and deployment notes.
- [Smart Contract Contributing](smart-contract-contributing.md) — Rust contribution workflow for smart contracts.
- [Vision Records Architecture](vision-records-architecture.md) — Vision Records module layout and record lifecycle.
- [Identity Architecture](identity-architecture.md) — Identity contract design for DIDs and recovery.
- [Key Manager Architecture](key-manager-architecture.md) — Key derivation, rotation, and guardian integration design.
- [Audit Architecture](audit_architecture.md) — Audit log structure and hash-chain design.
- [ZK Architecture](zk-architecture.md) — ZK verifier architecture and proof system abstraction.
- [AI Integration](ai_integration.md) — AI oracle integration, verification, and anomaly flags.
- [Analytics](analytics.md) — On-chain analytics design and aggregate reporting.
- [Governance](governance.md) — Governor, timelock, and admin tier governance model.
- [Timelock Guide](timelock.md) — Timelock contract guide, queue lifecycle, and error reference.
- [Staking](staking.md) — Staking model and validator/reward mechanics.
- [Treasury](treasury.md) — Treasury controls, multisig, and fund management.
- [Vault Guide](vault.md) — Vault contract, multi-guardian thresholds, and deadman grace period.
- [State Channel Design](state_channel.md) — State channel forced settlement, dispute window, and fraud proofs.
- [ADR Template](adr/0000-template.md) — Template for recording architecture decisions.
- [ADR 0001: Storage Design](adr/0001-storage-design.md) — Storage layout and key design decision.
- [ADR 0002: Access Control Design](adr/0002-access-control-design.md) — Access-control model decision.
- [ADR 0003: Event System Design](adr/0003-event-system-design.md) — Event emission and consumption design.
- [ADR 0004: Upgrade Strategy](adr/0004-upgrade-strategy.md) — Contract upgrade strategy and safety constraints.
- [ADR 0005: ADR Process](adr/0005-adr-process.md) — Process for proposing and accepting ADRs.
- [ADR 0006: Governance and Staking Architecture](adr/0006-governance-staking-architecture.md) — Governance and staking architecture decision.
- [ADR 0007: ZK Proof Integration](adr/0007-zk-proof-integration.md) — ZK proof integration decision.
- [ADR 0008: Cross-Chain Bridge Design](adr/0008-cross-chain-bridge-design.md) — Cross-chain bridge architecture decision.
- [ADR 0009: Privacy-Preserving Analytics](adr/0009-privacy-preserving-analytics.md) — Privacy-preserving analytics decision.
- [ADR 0010: AI Integration Security Model](adr/0010-ai-integration-security-model.md) — Security model for AI integration.
- [ADR 0011: FHIR Healthcare Interoperability](adr/0011-fhir-healthcare-interoperability.md) — FHIR interoperability decision.
- [ADR 0012: Concurrency Conflict Resolution](adr/0012-concurrency-conflict-resolution.md) — Concurrency and conflict-resolution strategy.
- [ADR 0013: Ethereum Light Client Verification](adr/0013-ethereum-light-client-verification.md) — Ethereum light-client verification design.
- [ADR 014: Access Control Micro-Contract](adr/014-access-control-micro-contract.md) — Micro-contract extraction of access control.
- [Onboarding Home](onboarding/README.md) — Entry point for contributor setup and first PR workflow.
- [Codebase Tour](onboarding/codebase-tour.md) — Guided tour of the repository and contract crates.
- [Development Environment](onboarding/dev-environment.md) — Local environment setup for Rust, Node, and off-chain tools.
- [First Contribution](onboarding/first-contribution.md) — Step-by-step first contribution checklist.
- [Good First Issue Guide](onboarding/good-first-issues.md) — How to choose and claim a starter issue.
- [Onboarding Glossary](onboarding/glossary.md) — Onboarding glossary for project and tooling terms.
- [Makefile Reference](onboarding/makefile-reference.md) — Reference for available Makefile targets.

## Integrators

- [API Reference](api.md) — Top-level reference for RetinaX contract APIs.
- [Access Control API](api/access_control.md) — Roles, permissions, and access-binding API reference.
- [Clinical Records API](api/clinical_records.md) — Clinical record lifecycle API reference.
- [Analytics API](api/analytics.md) — Analytics aggregation and reporting API reference.
- [Audit API](api/audit.md) — On-chain audit segment and entry API reference.
- [Compliance API](api/compliance.md) — Compliance rule and consent API reference.
- [Cross-Chain API](api/cross_chain.md) — Cross-chain message and proof API reference.
- [EMR Bridge API](api/emr_bridge.md) — EMR synchronization bridge API reference.
- [FHIR API](api/fhir.md) — FHIR resource registration and validation API reference.
- [Governor API](api/governor.md) — Proposal, voting, and execution API reference.
- [Identity API](api/identity.md) — DID, credential, and recovery API reference.
- [Metering API](api/metering.md) — Metering and rate-limit API reference.
- [Staking API](api/staking.md) — Staking, rewards, and tier API reference.
- [Treasury API](api/treasury.md) — Treasury transfer and multisig API reference.
- [ZK Verifier API](api/zk_verifier.md) — Proof verification and verifier management API reference.
- [ZK Voting API](api/zk_voting.md) — Private voting and proof verification API reference.
- [SDK Home](sdk/README.md) — Overview of available RetinaX SDK packages.
- [JavaScript SDK](sdk/javascript.md) — JavaScript client usage and examples.
- [Python SDK](sdk/python.md) — Python client usage and examples.
- [Rust SDK](sdk/rust.md) — Rust client usage and examples.
- [Integration Home](integration/README.md) — Entry point for integration patterns and setup.
- [Integration Troubleshooting](integration/troubleshooting.md) — Common integration failures and fixes.
- [Integrators Guide](integrators.md) — Practical guidance for client and service integrators.
- [ZK Integration Guide](zk-integration-guide.md) — Step-by-step ZK proof integration workflow.
- [ZK Integration Reference](ZK_INTEGRATION.md) — Detailed ZK integration architecture and proof formats.
- [Data Portability](data-portability.md) — Export and portability flows for patient data.
- [Appointment Scheduling](appointment-scheduling.md) — Scheduling integration patterns for clinics.
- [EMR Integration](emr-integration.md) — EMR system connectivity and data mapping.
- [FHIR Guide](fhir.md) — FHIR resource modeling and validation guidance.
- [Indexer](indexer.md) — Event indexing and off-chain data consumption.
- [Cross-Chain Guide](cross_chain.md) — Cross-chain workflow and bridge integration guide.

## Operators

- [Runbook Home](runbook/README.md) — Entry point for operational procedures.
- [Pre-Deployment Checklist](runbook/pre-deployment.md) — Checks to complete before a deployment.
- [Runbook Monitoring](runbook/monitoring.md) — Monitoring dashboards, alerts, and health checks.
- [Incident Response](runbook/incident-response.md) — Incident triage, mitigation, and escalation steps.
- [Access Control Operations](runbook/access-control-ops.md) — Operational access-control changes and reviews.
- [Deployment Runbook](runbook/deployment.md) — Step-by-step deployment procedure.
- [Upgrade Runbook](runbook/upgrades.md) — Safe contract upgrade procedure.
- [Deployment Guide](deployment.md) — Deployment targets, identities, and release commands.
- [Deployment Security](deployment-security.md) — Deployment key handling and operational security.
- [Operations Guide](operations.md) — Day-to-day operational tasks and cost allocation.
- [Monitoring](monitoring.md) — Metrics, logs, and service health expectations.
- [Upgrade Procedures](upgrade_procedures.md) — Versioned migration and upgrade steps.
- [Incident Response Plan](incident-response-plan.md) — Planning and communication for production incidents.
- [Emergency Protocol](emergency-protocol.md) — Emergency pause and recovery protocol.
- [Emergency Contacts](emergency-contacts.md) — Escalation contacts for urgent incidents.
- [Release Management](release-management.md) — Release planning, tagging, and rollout process.
- [RBAC Quick Reference](rbac-quick-reference.md) — Fast reference for role and permission operations.
- [Rate Limiting](rate-limiting.md) — Rate-limit configuration and monitoring guidance.

## Auditors

- [Security Model](security.md) — Security assumptions, controls, and trust boundaries.
- [Security Home](security/README.md) — Entry point for security documentation.
- [Access Control Matrix](security/access-control-matrix.md) — Role and permission matrix for contracts.
- [Contract Security Profiles](security/contract-profiles.md) — Security profile for each contract.
- [Security Overview](security/overview.md) — High-level security architecture overview.
- [Cryptography](security/cryptography.md) — Cryptographic primitives, proofs, and encryption design.
- [Security Rate Limiting](security/rate-limiting.md) — Abuse-prevention and rate-limiting controls.
- [Threat Model](threat-model.md) — Threat actors, attack surfaces, and mitigations.
- [Security Audit Checklist](security-audit-checklist.md) — Checklist for audit readiness and review.
- [Audit Logging](audit-logging.md) — Immutable audit trail design and verification.
- [Timestamp Assumptions](TIMESTAMP_ASSUMPTIONS.md) — Time-source assumptions and related risks.
- [Post-Quantum Migration Tracking](POST_QUANTUM_MIGRATION_TRACKING.md) — Status and roadmap for post-quantum ZK migration.
- [Post-Quantum ZK Research](post-quantum-zk-research.md) — Research notes on post-quantum proof systems.
- [Access Control Architecture](access-control-architecture.md) — Detailed access-control architecture and RBAC design.
- [Compliance Home](compliance/README.md) — Entry point for regulatory and compliance documentation.
- [Compliance Architecture](compliance/architecture.md) — Compliance system architecture and controls.
- [Audit Trail Compliance](compliance/audit-trail.md) — Audit trail controls and evidentiary requirements.
- [Data Classification](compliance/data-classification.md) — Classification scheme for healthcare data.
- [Consent Management](compliance/consent-management.md) — Consent capture, storage, and revocation controls.
- [Compliance Deployment Guide](compliance/deployment-guide.md) — Deployment requirements for compliant environments.
- [HIPAA Mapping](compliance/hipaa-mapping.md) — Mapping of RetinaX controls to HIPAA requirements.
