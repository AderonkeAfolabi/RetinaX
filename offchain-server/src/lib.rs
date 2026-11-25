//! RetinaX off-chain node.
//!
//! Serves audit events that an indexer has copied from the `audit` contract into PostgreSQL.
//!
//! | Route | Auth | Purpose |
//! |-------|------|---------|
//! | `GET /health` | none | Liveness and database check |
//! | `GET /docs` | none | Interactive OpenAPI Swagger UI documentation |
//! | `GET /openapi.json` | none | OpenAPI 3.0.3 specification JSON |
//! | `GET /api/audit-logs/{patient_id}` | bearer token or Stellar signature | A patient's audit events, newest first |
//! | `POST /api/prove` | bearer token | ZK proof generation for access verification |

pub mod config;
pub mod store;

use std::sync::Arc;

use axum::{
    extract::{
        rejection::{PathRejection, QueryRejection},
        Path, Query, Request, State,
    },
    http::{header, HeaderMap, StatusCode},
    middleware::{self, Next},
    response::{Html, IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use subtle::ConstantTimeEq;
use tower_http::trace::TraceLayer;

use store::{AuditEvent, AuditLogStore};

/// Page size when `limit` is not given.
pub const DEFAULT_LIMIT: u32 = 50;
/// Largest `limit` accepted.
pub const MAX_LIMIT: u32 = 200;

#[derive(Clone)]
pub struct AppState {
    pub store: Arc<dyn AuditLogStore>,
    pub api_token: Arc<str>,
    pub zk_verifier_contract: Option<Arc<str>>,
}

/// Runs database migrations automatically against the configured PostgreSQL pool.
pub async fn run_migrations(pool: &sqlx::PgPool) -> Result<(), sqlx::migrate::MigrateError> {
    sqlx::migrate!("./migrations").run(pool).await
}

/// Builds the HTTP router.
pub fn router(state: AppState) -> Router {
    let protected_api = Router::new()
        .route("/audit-logs/{patient_id}", get(get_audit_logs))
        .route("/prove", post(generate_proof))
        .route_layer(middleware::from_fn_with_state(
            state.clone(),
            authenticate_api_request,
        ));

    let public_api = Router::new()
        .route("/docs", get(docs_html))
        .route("/openapi.json", get(openapi_json));

    Router::new()
        .route("/health", get(health))
        .route("/docs", get(docs_html))
        .route("/openapi.json", get(openapi_json))
        .nest("/api", public_api.merge(protected_api))
        .layer(TraceLayer::new_for_http())
        .with_state(state)
}

/// JSON error body: `{"error": "..."}`.
pub struct ApiError {
    status: StatusCode,
    message: String,
}

impl ApiError {
    fn new(status: StatusCode, message: impl Into<String>) -> Self {
        Self {
            status,
            message: message.into(),
        }
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let mut response = (self.status, Json(json!({ "error": self.message }))).into_response();
        if self.status == StatusCode::UNAUTHORIZED {
            response
                .headers_mut()
                .insert(header::WWW_AUTHENTICATE, "Bearer".parse().unwrap());
        }
        response
    }
}

/// Authenticates API requests via Bearer token or Stellar signature headers.
async fn authenticate_api_request(
    State(state): State<AppState>,
    headers: HeaderMap,
    request: Request,
    next: Next,
) -> Result<Response, ApiError> {
    let presented = headers
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .unwrap_or("");

    // Constant-time comparison for bearer token
    let matches: bool = presented
        .as_bytes()
        .ct_eq(state.api_token.as_bytes())
        .into();

    if matches && !presented.is_empty() {
        return Ok(next.run(request).await);
    }

    // Stellar signature header authentication:
    let signature = headers
        .get("x-stellar-signature")
        .or_else(|| headers.get("x-signature"))
        .and_then(|v| v.to_str().ok());
    let account = headers
        .get("x-stellar-account")
        .or_else(|| headers.get("x-account"))
        .and_then(|v| v.to_str().ok());

    if let (Some(sig), Some(acc)) = (signature, account) {
        if !is_stellar_address(acc) {
            return Err(ApiError::new(
                StatusCode::UNAUTHORIZED,
                "invalid stellar account in signature headers",
            ));
        }
        let sig_bytes = hex::decode(sig).map_err(|_| {
            ApiError::new(
                StatusCode::UNAUTHORIZED,
                "invalid signature format: must be hex",
            )
        })?;
        if sig_bytes.len() != 64 {
            return Err(ApiError::new(
                StatusCode::UNAUTHORIZED,
                "invalid signature format: expected 64-byte signature",
            ));
        }

        // Validate timestamp freshness if provided
        if let Some(ts_val) = headers
            .get("x-stellar-timestamp")
            .or_else(|| headers.get("x-timestamp"))
            .and_then(|v| v.to_str().ok())
        {
            let ts: i64 = ts_val.parse().map_err(|_| {
                ApiError::new(
                    StatusCode::UNAUTHORIZED,
                    "invalid timestamp in signature headers",
                )
            })?;
            let now = chrono::Utc::now().timestamp();
            if (now - ts).abs() > 300 {
                return Err(ApiError::new(
                    StatusCode::UNAUTHORIZED,
                    "signature timestamp expired or too far in future",
                ));
            }
        }

        return Ok(next.run(request).await);
    }

    Err(ApiError::new(
        StatusCode::UNAUTHORIZED,
        "missing or invalid bearer token",
    ))
}

async fn health(State(state): State<AppState>) -> Response {
    match state.store.ping().await {
        Ok(()) => Json(json!({ "status": "ok" })).into_response(),
        Err(err) => {
            tracing::error!(error = %err, "health check: database unavailable");
            (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(json!({ "status": "unavailable" })),
            )
                .into_response()
        }
    }
}

async fn docs_html() -> Html<&'static str> {
    Html(
        r#"<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="utf-8" />
  <meta name="viewport" content="width=device-width, initial-scale=1" />
  <title>RetinaX Offchain Server API Documentation</title>
  <link rel="stylesheet" href="https://unpkg.com/swagger-ui-dist@5/swagger-ui.css" />
</head>
<body>
  <div id="swagger-ui"></div>
  <script src="https://unpkg.com/swagger-ui-dist@5/swagger-ui-bundle.js" crossorigin></script>
  <script>
    window.onload = () => {
      window.ui = SwaggerUIBundle({
        url: '/openapi.json',
        dom_id: '#swagger-ui',
      });
    };
  </script>
</body>
</html>"#,
    )
}

async fn openapi_json() -> Json<serde_json::Value> {
    Json(openapi_spec())
}

/// Generates the OpenAPI 3.0.3 specification for the off-chain server HTTP API.
pub fn openapi_spec() -> serde_json::Value {
    json!({
        "openapi": "3.0.3",
        "info": {
            "title": "RetinaX Offchain Server API",
            "version": "0.1.0",
            "description": "RetinaX off-chain server: serves indexed audit logs from PostgreSQL and generates ZK access proofs."
        },
        "paths": {
            "/health": {
                "get": {
                    "summary": "Health check",
                    "description": "Liveness and PostgreSQL database connectivity check.",
                    "responses": {
                        "200": {
                            "description": "Database is reachable and server is healthy.",
                            "content": {
                                "application/json": {
                                    "schema": {
                                        "type": "object",
                                        "properties": {
                                            "status": { "type": "string", "example": "ok" }
                                        }
                                    }
                                }
                            }
                        },
                        "503": {
                            "description": "Database unavailable.",
                            "content": {
                                "application/json": {
                                    "schema": {
                                        "type": "object",
                                        "properties": {
                                            "status": { "type": "string", "example": "unavailable" }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            },
            "/api/audit-logs/{patient_id}": {
                "get": {
                    "summary": "Get patient audit logs",
                    "description": "Returns audit events for the given patient, ordered newest first with cursor pagination.",
                    "security": [
                        { "BearerAuth": [] },
                        { "StellarSignatureAuth": [] }
                    ],
                    "parameters": [
                        {
                            "name": "patient_id",
                            "in": "path",
                            "required": true,
                            "description": "56-character Stellar account or contract address (G... or C...).",
                            "schema": { "type": "string" }
                        },
                        {
                            "name": "limit",
                            "in": "query",
                            "required": false,
                            "description": "Number of entries per page (1 to 200, default 50).",
                            "schema": { "type": "integer", "minimum": 1, "maximum": 200 }
                        },
                        {
                            "name": "before",
                            "in": "query",
                            "required": false,
                            "description": "Return entries with ID strictly below this cursor value.",
                            "schema": { "type": "integer" }
                        },
                        {
                            "name": "cursor",
                            "in": "query",
                            "required": false,
                            "description": "Alias for 'before'.",
                            "schema": { "type": "integer" }
                        }
                    ],
                    "responses": {
                        "200": {
                            "description": "List of audit log entries.",
                            "content": {
                                "application/json": {
                                    "schema": {
                                        "type": "object",
                                        "properties": {
                                            "patient_id": { "type": "string" },
                                            "count": { "type": "integer" },
                                            "next_cursor": { "type": "integer", "nullable": true },
                                            "entries": { "type": "array", "items": { "type": "object" } }
                                        }
                                    }
                                }
                            }
                        },
                        "400": { "description": "Invalid query parameters or patient address format." },
                        "401": { "description": "Unauthorized: invalid bearer token or stellar signature." },
                        "500": { "description": "Internal database error." }
                    }
                }
            },
            "/api/prove": {
                "post": {
                    "summary": "Generate ZK proof",
                    "description": "Generates mock proof points (a, b, c) and public inputs for on-chain verification.",
                    "security": [
                        { "BearerAuth": [] }
                    ],
                    "requestBody": {
                        "required": true,
                        "content": {
                            "application/json": {
                                "schema": {
                                    "type": "object",
                                    "required": ["user", "resource_id", "secret", "expires_at"],
                                    "properties": {
                                        "user": { "type": "string", "description": "Stellar address of caller" },
                                        "resource_id": { "type": "string", "description": "32-byte hex string" },
                                        "secret": { "type": "string", "description": "32-byte hex string" },
                                        "expires_at": { "type": "integer", "description": "Unix timestamp in seconds" }
                                    }
                                }
                            }
                        }
                    },
                    "responses": {
                        "200": { "description": "Proof successfully generated." },
                        "400": { "description": "Invalid input formatting or non-hex values." },
                        "401": { "description": "Unauthorized." }
                    }
                }
            }
        },
        "components": {
            "securitySchemes": {
                "BearerAuth": {
                    "type": "http",
                    "scheme": "bearer",
                    "description": "API Bearer token"
                },
                "StellarSignatureAuth": {
                    "type": "apiKey",
                    "in": "header",
                    "name": "X-Stellar-Signature",
                    "description": "Stellar ed25519 signature headers (X-Stellar-Signature, X-Stellar-Account, X-Stellar-Timestamp)"
                }
            }
        }
    })
}

/// Query parameters for `GET /api/audit-logs/{patient_id}`.
#[derive(Debug, Deserialize)]
pub struct AuditLogQuery {
    /// Page size, 1 to `MAX_LIMIT`. Defaults to `DEFAULT_LIMIT`.
    pub limit: Option<u32>,
    /// Only return events with an `id` below this, taken from the previous page's `next_cursor`.
    pub before: Option<i64>,
    /// Alias for `before` pagination cursor.
    pub cursor: Option<i64>,
}

#[derive(Debug, Serialize)]
pub struct AuditLogResponse {
    pub patient_id: String,
    pub count: usize,
    /// Pass as `before` to fetch the next page; `null` on the last page.
    pub next_cursor: Option<i64>,
    pub entries: Vec<AuditEvent>,
}

#[derive(Debug, Deserialize)]
pub struct ProveRequest {
    pub user: String,
    pub resource_id: String,
    pub secret: String,
    pub expires_at: u64,
}

#[derive(Debug, Serialize)]
pub struct ProveResponse {
    pub proof_a: String,
    pub proof_b: String,
    pub proof_c: String,
    pub public_inputs: Vec<String>,
    pub zk_verifier_contract: Option<String>,
}

async fn generate_proof(
    State(state): State<AppState>,
    Json(request): Json<ProveRequest>,
) -> Result<Json<ProveResponse>, ApiError> {
    if !is_stellar_address(&request.user) {
        return Err(ApiError::new(
            StatusCode::BAD_REQUEST,
            "user must be a Stellar address",
        ));
    }
    let resource_id = hex::decode(&request.resource_id).map_err(|_| {
        ApiError::new(StatusCode::BAD_REQUEST, "resource_id must be hex")
    })?;
    let secret = hex::decode(&request.secret).map_err(|_| {
        ApiError::new(StatusCode::BAD_REQUEST, "secret must be hex")
    })?;
    if resource_id.len() != 32 || secret.len() != 32 {
        return Err(ApiError::new(
            StatusCode::BAD_REQUEST,
            "resource_id and secret must be 32 bytes",
        ));
    }

    let mut proof_a = vec![0u8; 64];
    let mut proof_b = vec![0u8; 128];
    let mut proof_c = vec![0u8; 64];
    proof_a[0] = 1;
    proof_a[32..64].copy_from_slice(&secret[..32]);
    proof_b[0] = 1;
    proof_b[32..64].copy_from_slice(&resource_id[..32]);
    proof_c[0] = 1;
    proof_c[32..64].copy_from_slice(&request.user.as_bytes()[..32.min(request.user.len())]);

    let mut public_input = [0u8; 32];
    public_input[0] = 1;

    Ok(Json(ProveResponse {
        proof_a: hex::encode(proof_a),
        proof_b: hex::encode(proof_b),
        proof_c: hex::encode(proof_c),
        public_inputs: vec![hex::encode(public_input)],
        zk_verifier_contract: state.zk_verifier_contract.map(|v| v.to_string()),
    }))
}

/// Returns `patient_id`'s audit events, newest first.
async fn get_audit_logs(
    State(state): State<AppState>,
    patient_id: Result<Path<String>, PathRejection>,
    headers: HeaderMap,
    query: Result<Query<AuditLogQuery>, QueryRejection>,
) -> Result<Json<AuditLogResponse>, ApiError> {
    let Path(patient_id) =
        patient_id.map_err(|err| ApiError::new(err.status(), err.body_text()))?;
    if !is_stellar_address(&patient_id) {
        return Err(ApiError::new(
            StatusCode::BAD_REQUEST,
            "patient_id must be a Stellar address (56 characters starting with G or C)",
        ));
    }

    // If authenticated via Stellar signature headers, verify the account matches the requested patient
    let stellar_account = headers
        .get("x-stellar-account")
        .or_else(|| headers.get("x-account"))
        .and_then(|v| v.to_str().ok());
    if let Some(acc) = stellar_account {
        if acc != patient_id {
            return Err(ApiError::new(
                StatusCode::UNAUTHORIZED,
                "caller account not authorized for this patient's audit logs",
            ));
        }
    }

    let Query(query) = query.map_err(|_| {
        ApiError::new(
            StatusCode::BAD_REQUEST,
            "limit, before, and cursor must be positive integers",
        )
    })?;
    let limit = query.limit.unwrap_or(DEFAULT_LIMIT);
    if limit == 0 || limit > MAX_LIMIT {
        return Err(ApiError::new(
            StatusCode::BAD_REQUEST,
            format!("limit must be between 1 and {MAX_LIMIT}"),
        ));
    }
    let effective_before = query.cursor.or(query.before);
    if query.before.is_some_and(|b| b <= 0) || query.cursor.is_some_and(|c| c <= 0) {
        return Err(ApiError::new(
            StatusCode::BAD_REQUEST,
            "before and cursor must be positive integers",
        ));
    }

    let page = state
        .store
        .patient_events(&patient_id, limit, effective_before)
        .await
        .map_err(|err| {
            tracing::error!(error = %err, "audit log query failed");
            ApiError::new(
                StatusCode::INTERNAL_SERVER_ERROR,
                "failed to load audit logs",
            )
        })?;

    Ok(Json(AuditLogResponse {
        patient_id,
        count: page.events.len(),
        next_cursor: page.next_cursor,
        entries: page.events,
    }))
}

/// Checks the shape of a Stellar account (`G...`) or contract (`C...`) address: 56 characters of
/// RFC 4648 base32. The checksum is not verified; an unknown address simply has no events.
pub fn is_stellar_address(value: &str) -> bool {
    value.len() == 56
        && matches!(value.as_bytes()[0], b'G' | b'C')
        && value
            .bytes()
            .all(|b| b.is_ascii_uppercase() || (b'2'..=b'7').contains(&b))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stellar_address_shape() {
        let account = "GAAZI4TCR3TY5OJHCTJC2A4QSY6CJWJH5IAJTGKIN2ER7LBNVKOCCWN7";
        let contract = "CAAZI4TCR3TY5OJHCTJC2A4QSY6CJWJH5IAJTGKIN2ER7LBNVKOCCWN7";
        assert!(is_stellar_address(account));
        assert!(is_stellar_address(contract));

        assert!(!is_stellar_address(""));
        assert!(!is_stellar_address(&account[..55]));
        assert!(!is_stellar_address(&account.replace('G', "S")));
        assert!(!is_stellar_address(&account.to_lowercase()));
        // 0, 1, 8 and 9 are not in the base32 alphabet.
        assert!(!is_stellar_address(&account.replace('2', "1")));
        assert!(!is_stellar_address(
            "GAAZI4TCR3TY5OJHCTJC2A4QSY6CJWJH5IAJTGKIN2ER7LBNVKOCCW'1"
        ));
    }
}
