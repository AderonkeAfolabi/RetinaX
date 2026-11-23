## [Unreleased]

### Features

* **ai_integration:** add time-decayed provider reputation (#464).
* **fhir:** map a minimal pseudonymous Patient projection (#467).
* **fhir:** validate and map DiagnosticReport metadata and result references (#466).
* **emr_bridge:** queue failed syncs for bounded exponential retries (#468).
* **sdk/zk_prover:** add `wasm32-unknown-unknown` WASM build target and browser JS usage guide ([#471](https://github.com/RetinaX-Org/RetinaX/issues/471))
* **offchain-server:** add cursor pagination and Stellar Ed25519 signature header authentication for `/api/audit-logs` ([#472](https://github.com/RetinaX-Org/RetinaX/issues/472))
* **offchain-server:** add OpenAPI 3.0 specification, Swagger UI documentation at `/docs`, and database migration tooling ([#473](https://github.com/RetinaX-Org/RetinaX/issues/473))
* **website:** audit and enforce WCAG 2.1 AA compliant color contrast tokens for light and dark themes ([#474](https://github.com/RetinaX-Org/RetinaX/issues/474))

## 1.0.0 (2026-02-21)

### Features

* add automated release management workflow ([6856459](https://github.com/ChukwuemekaP1/RetinaX-Contracts/commit/6856459e18c1000b2121171b11112bb64f5aaa8e))
* implement event monitoring system ([d6f6c25](https://github.com/ChukwuemekaP1/RetinaX-Contracts/commit/d6f6c2546805cfb0b92f42fd455f08f7793b98b8))
* implement record versioning and history tracking ([5b6a6ea](https://github.com/ChukwuemekaP1/RetinaX-Contracts/commit/5b6a6ea5d2089510641b23e9b9ec8fa1347f0175))
* implement role based access control ([ef86cbf](https://github.com/ChukwuemekaP1/RetinaX-Contracts/commit/ef86cbf68b3db9a246901c06e146824d01d7c2cf))
* initial project scaffolding for RetinaX-Org vision care platform ([aca3c01](https://github.com/ChukwuemekaP1/RetinaX-Contracts/commit/aca3c015fa100b765343bac881beffd165deb2e2))
## [1.2.2](https://github.com/RetinaX-Org/RetinaX/compare/v1.2.1...v1.2.2) (2026-02-22)

### Bug Fixes

* **rbac:** correctly persist user roles to storage on initialization and registration ([fbfae85](https://github.com/RetinaX-Org/RetinaX/commit/fbfae859eb167d0f9be98412160a4c7bc3b7945e))
* **test:** remove corrupted inline unit test module from lib.rs ([2fa262b](https://github.com/RetinaX-Org/RetinaX/commit/2fa262b8d5df8ce3ec0035643dcd894f245915e8))

## [1.1.0](https://github.com/precious-akpan/RetinaX-Contracts/compare/v1.0.0...v1.1.0) (2026-02-22)

### Features

* implement mutation testing and tests coverage for [#27](https://github.com/precious-akpan/RetinaX-Contracts/issues/27) ([ced0105](https://github.com/precious-akpan/RetinaX-Contracts/commit/ced010556c7c24a44e92b3609a3d4bd7c5096b01))
* **zk:** implement Issue [#43](https://github.com/precious-akpan/RetinaX-Contracts/issues/43) ZK Verifier integration ([eefb197](https://github.com/precious-akpan/RetinaX-Contracts/commit/eefb197f9f256c8788fb912dbafbd4ebadf8078b))

### Bug Fixes

* **ci:** Resolve clippy warning and missing toolchain input in mutation.yml ([d477d70](https://github.com/precious-akpan/RetinaX-Contracts/commit/d477d70c15e682e874f0cfb1951e1c4969e150ff))
* **test:** remove corrupted master test block in lib.rs ([f800c4b](https://github.com/precious-akpan/RetinaX-Contracts/commit/f800c4b3c979c3b30765f911d5ee6cd809c77b77))

## [1.2.1](https://github.com/RetinaX-Org/RetinaX/compare/v1.2.0...v1.2.1) (2026-02-22)

### Bug Fixes

* **test:** remove corrupted master test block in lib.rs ([f800c4b](https://github.com/RetinaX-Org/RetinaX/commit/f800c4b3c979c3b30765f911d5ee6cd809c77b77))

## [1.2.0](https://github.com/RetinaX-Org/RetinaX/compare/v1.1.0...v1.2.0) (2026-02-22)

### Features

* implement mutation testing and tests coverage for [#27](https://github.com/RetinaX-Org/RetinaX/issues/27) ([ced0105](https://github.com/RetinaX-Org/RetinaX/commit/ced010556c7c24a44e92b3609a3d4bd7c5096b01))
* **zk:** implement Issue [#43](https://github.com/RetinaX-Org/RetinaX/issues/43) ZK Verifier integration ([eefb197](https://github.com/RetinaX-Org/RetinaX/commit/eefb197f9f256c8788fb912dbafbd4ebadf8078b))

### Bug Fixes

* **ci:** Resolve clippy warning and missing toolchain input in mutation.yml ([d477d70](https://github.com/RetinaX-Org/RetinaX/commit/d477d70c15e682e874f0cfb1951e1c4969e150ff))

## [1.1.0](https://github.com/RetinaX-Org/RetinaX/compare/v1.0.0...v1.1.0) (2026-02-21)

### Features

* implement record versioning and history tracking ([5b6a6ea](https://github.com/RetinaX-Org/RetinaX/commit/5b6a6ea5d2089510641b23e9b9ec8fa1347f0175))

## [Unreleased]

### Features

* **ai_integration:** add time-decayed provider reputation (#464).
* **fhir:** map a minimal pseudonymous Patient projection (#467).
* **fhir:** validate and map DiagnosticReport metadata and result references (#466).
* **emr_bridge:** queue failed syncs for bounded exponential retries (#468).
* **sdk/zk_prover:** add `wasm32-unknown-unknown` WASM build target and browser JS usage guide ([#471](https://github.com/RetinaX-Org/RetinaX/issues/471))
* **offchain-server:** add cursor pagination and Stellar Ed25519 signature header authentication for `/api/audit-logs` ([#472](https://github.com/RetinaX-Org/RetinaX/issues/472))
* **offchain-server:** add OpenAPI 3.0 specification, Swagger UI documentation at `/docs`, and database migration tooling ([#473](https://github.com/RetinaX-Org/RetinaX/issues/473))
* **website:** audit and enforce WCAG 2.1 AA compliant color contrast tokens for light and dark themes ([#474](https://github.com/RetinaX-Org/RetinaX/issues/474))

## 1.0.0 (2026-02-21)

### Features

* add automated release management workflow ([6856459](https://github.com/RetinaX-Org/RetinaX/commit/6856459e18c1000b2121171b11112bb64f5aaa8e))
* implement event monitoring system ([d6f6c25](https://github.com/RetinaX-Org/RetinaX/commit/d6f6c2546805cfb0b92f42fd455f08f7793b98b8))
* implement role based access control ([ef86cbf](https://github.com/RetinaX-Org/RetinaX/commit/ef86cbf68b3db9a246901c06e146824d01d7c2cf))
* initial project scaffolding for RetinaX-Org vision care platform ([aca3c01](https://github.com/RetinaX-Org/RetinaX/commit/aca3c015fa100b765343bac881beffd165deb2e2))

# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

* **treasury:** recurring grants and streaming payments with configurable rate, cliff, cancellation, and claim with underflow protection (#458).
* **governor:** emergency veto guardian role allowing guardian to cancel malicious queued proposals during the timelock window (#457).
* **governor:** proposal categories with configurable distinct quorum and pass threshold parameters per category (#456).
* **events:** bounded dead-letter queue (DLQ) with configurable capacity, FIFO capacity eviction, and retry mechanism (#455).
* **docs:** ADR-0013 — research spike on Ethereum header/receipt verification on Soroban (light client vs optimistic vs ZK) (#469)
* **ai_integration:** model version pinning per request. Providers record `model_version`, `submit_pinned_analysis_request` refuses a version other than the one pinned, and results carry the version they were made against (#465).
* **vision_records:** cursor-based bulk patient export through `export_patient_data_page`. Records, prescriptions and grants are paged together, so long lists no longer fail the export (#446).
* **analytics:** admin-configurable differential-privacy parameters through `set_dp_params` and `get_dp_params`, bounded to epsilon 1 to 10 and sensitivity 1 to 1,000,000 (#463).
* **metering:** tiered pricing plans with per-plan quotas and base prices, managed with `set_plan` and `assign_plan`. Postpaid invoices include the plan base price (#462).
- Initial project scaffolding
- Vision records smart contract with basic functionality
- User registration and management
- Record creation and retrieval
- Access control system (grant/revoke access)
- Role-based access control (Patient, Optometrist, Ophthalmologist, Admin)
- Automated setup script
- CI/CD workflow with GitHub Actions
- Comprehensive documentation

### Changed
- N/A

### Deprecated
- N/A

### Removed
- N/A

### Fixed
- N/A

### Security
- N/A

## [0.1.0] - 2026-01-27

### Added
- Initial release
- Core vision records contract
- Basic RBAC implementation
- Project documentation
