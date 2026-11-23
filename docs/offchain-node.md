# RetinaX Offchain Server

The `offchain-server` crate is a high-performance HTTP indexing service that stores and serves audit log events copied from on-chain Soroban contracts into PostgreSQL, as well as providing mock ZK proof generation endpoints.

## Features

- **Automated DB Migrations**: Runs SQL migrations automatically on startup via `sqlx::migrate!("./migrations")` and exposes `run_migrations(&pool)` programmatic tooling.
- **OpenAPI 3.0 Documentation**: Generates full OpenAPI specs served as JSON at `/openapi.json` and interactive Swagger UI at `/docs`.
- **Cursor-Based Pagination**: Supports `limit`, `before`, and `cursor` parameters for deterministic, high-throughput page traversal.
- **Dual Authentication**:
  - **Bearer Token**: `Authorization: Bearer <API_TOKEN>` for indexer and administrative ingestion.
  - **Stellar Ed25519 Signature**: Allows patients and authorized providers to query audit logs directly using cryptographic signature headers without shared API secrets.

## API Endpoints

### 1. Health & Readiness
- **`GET /health`**
  - Public endpoint. Verifies PostgreSQL connectivity and pool responsiveness.

### 2. Interactive Documentation & Spec
- **`GET /docs`** (or `GET /api/docs`)
  - Public Swagger UI explorer.
- **`GET /openapi.json`** (or `GET /api/openapi.json`)
  - OpenAPI 3.0.3 specification JSON.

### 3. Patient Audit Logs
- **`GET /api/audit-logs/{patient_id}`**
  - **Path Parameters**:
    - `patient_id`: 56-character Stellar account or contract address (`G...` or `C...`).
  - **Query Parameters**:
    - `limit` (optional): Page size from 1 to 200 (defaults to 50).
    - `before` / `cursor` (optional): Returns entries with `id` strictly lower than the specified cursor.
  - **Authentication**:
    - Either `Authorization: Bearer <API_TOKEN>`, OR
    - Stellar signature headers:
      - `X-Stellar-Account`: Caller's Stellar public key (must match `patient_id` or an authorized caller).
      - `X-Stellar-Signature`: 64-byte hex-encoded Ed25519 signature (128 hex characters).
      - `X-Stellar-Timestamp`: Fresh Unix timestamp in seconds (within +/- 300s window).

### 4. ZK Access Proof Generation
- **`POST /api/prove`**
  - Generates proof points (`proof_a`, `proof_b`, `proof_c`) and public inputs for submission to the `zk_verifier` Soroban contract.

## Database Migrations

Migrations are stored in `offchain-server/migrations/` and applied automatically on server start:

```rust
// Automatically runs pending SQL migrations
offchain_server::run_migrations(&pool).await?;
```
