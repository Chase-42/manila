use axum::{
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use base64::Engine;
use ed25519_dalek::VerifyingKey;
use serde::{Deserialize, Serialize};
use serde_json::json;
use uuid::Uuid;

use crate::AppState;
use crate::auth::{AuthDevice, require_subscribed};

// ── Enrollment ────────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct EnrollRequest {
    pub public_key: Option<String>,
}

pub async fn enroll(
    State(state): State<AppState>,
    Json(req): Json<EnrollRequest>,
) -> impl IntoResponse {
    let Some(pk_hex) = req.public_key else {
        return (StatusCode::BAD_REQUEST, Json(json!({ "error": "missing required field: public_key" })));
    };

    if pk_hex.len() != 64 {
        return (StatusCode::BAD_REQUEST, Json(json!({ "error": "invalid public_key" })));
    }

    let key_bytes = match hex::decode(&pk_hex) {
        Ok(b) if b.len() == 32 => b,
        _ => return (StatusCode::BAD_REQUEST, Json(json!({ "error": "invalid public_key" }))),
    };

    let key_array: [u8; 32] = key_bytes.try_into().unwrap();
    if VerifyingKey::from_bytes(&key_array).is_err() {
        return (StatusCode::BAD_REQUEST, Json(json!({ "error": "invalid public_key" })));
    }

    let pk_hex_lower = pk_hex.to_lowercase();
    let candidate_id = Uuid::new_v4().to_string();
    let enrolled_at = chrono_now();

    let db = state.db.clone();
    let pk = pk_hex_lower.clone();
    let cid = candidate_id.clone();
    let ea = enrolled_at.clone();
    match tokio::task::spawn_blocking(move || db.get_or_create_device(&cid, &pk, &ea))
        .await
        .map_err(|_| ())
        .and_then(|r| r.map_err(|_| ()))
    {
        Ok((device_id, enrolled_at)) => (
            StatusCode::OK,
            Json(json!({ "device_id": device_id, "enrolled_at": enrolled_at })),
        ),
        Err(()) => {
            tracing::error!("db error on get_or_create_device");
            (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({ "error": "internal error" })))
        }
    }
}

// ── Push ──────────────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct PushRequest {
    pub payload: Option<String>,
}

pub async fn push(
    State(state): State<AppState>,
    AuthDevice { device_id }: AuthDevice,
    Json(req): Json<PushRequest>,
) -> Response {
    if let Err(e) = require_subscribed(&state, &device_id).await {
        return e.into_response();
    }

    let Some(payload_b64) = req.payload else {
        return (StatusCode::BAD_REQUEST, Json(json!({ "error": "missing required field: payload" }))).into_response();
    };

    let payload_bytes = match base64::engine::general_purpose::STANDARD.decode(&payload_b64) {
        Ok(b) => b,
        Err(_) => return (StatusCode::BAD_REQUEST, Json(json!({ "error": "invalid payload" }))).into_response(),
    };

    let created_at = chrono_now();
    let db = state.db.clone();
    let result = match tokio::task::spawn_blocking(move || db.insert_blob(&device_id, &payload_bytes, &created_at))
        .await
    {
        Err(_) => {
            tracing::error!("spawn_blocking panicked on insert_blob");
            return (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({ "error": "internal error" }))).into_response();
        }
        Ok(r) => r,
    };

    match result {
        Ok(seq) => (StatusCode::OK, Json(json!({ "seq": seq }))).into_response(),
        Err(e) => {
            tracing::error!("db error on insert_blob: {e}");
            (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({ "error": "internal error" }))).into_response()
        }
    }
}

// ── Pull ──────────────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct PullQuery {
    pub after: Option<i64>,
}

#[derive(Serialize)]
struct BlobEntry {
    seq: i64,
    device_id: String,
    payload: String,
    created_at: String,
}

// All devices belonging to one vault share a single Ed25519 keypair (derived
// deterministically from vault_secret), so they enroll under the same device ID.
// There is no multi-user separation at this layer; all stored blobs belong to
// the authenticated vault-holder. A per-user scope would be required before
// supporting multiple independent vault owners on the same server.
pub async fn pull(
    State(state): State<AppState>,
    AuthDevice { device_id }: AuthDevice,
    axum::extract::Query(query): axum::extract::Query<PullQuery>,
) -> Response {
    if let Err(e) = require_subscribed(&state, &device_id).await {
        return e.into_response();
    }

    let after = query.after.unwrap_or(0);
    let db = state.db.clone();
    let result = match tokio::task::spawn_blocking(move || db.pull_blobs(after))
        .await
    {
        Err(_) => {
            tracing::error!("spawn_blocking panicked on pull_blobs");
            return (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({ "error": "internal error" }))).into_response();
        }
        Ok(r) => r,
    };

    match result {
        Ok(rows) => {
            let blobs: Vec<BlobEntry> = rows
                .into_iter()
                .map(|r| BlobEntry {
                    seq: r.seq,
                    device_id: r.device_id,
                    payload: base64::engine::general_purpose::STANDARD.encode(&r.payload),
                    created_at: r.created_at,
                })
                .collect();
            (StatusCode::OK, Json(json!({ "blobs": blobs }))).into_response()
        }
        Err(e) => {
            tracing::error!("db error on pull_blobs: {e}");
            (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({ "error": "internal error" }))).into_response()
        }
    }
}

// ── Revocation ────────────────────────────────────────────────────────────────

pub async fn revoke_device(
    State(state): State<AppState>,
    AuthDevice { device_id: auth_device_id }: AuthDevice,
    axum::extract::Path(path_device_id): axum::extract::Path<String>,
) -> Response {
    if let Err(e) = require_subscribed(&state, &auth_device_id).await {
        return e.into_response();
    }

    if auth_device_id != path_device_id {
        return (StatusCode::FORBIDDEN, Json(json!({ "error": "device_id mismatch" }))).into_response();
    }

    let db = state.db.clone();
    let id = path_device_id.clone();
    let result = match tokio::task::spawn_blocking(move || db.delete_device(&id))
        .await
    {
        Err(_) => {
            tracing::error!("spawn_blocking panicked on delete_device");
            return (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({ "error": "internal error" }))).into_response();
        }
        Ok(r) => r,
    };

    match result {
        Ok(true) => (StatusCode::OK, Json(json!({}))).into_response(),
        Ok(false) => (StatusCode::NOT_FOUND, Json(json!({ "error": "not found" }))).into_response(),
        Err(e) => {
            tracing::error!("db error on delete_device: {e}");
            (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({ "error": "internal error" }))).into_response()
        }
    }
}

fn chrono_now() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    // RFC 3339 / ISO-8601 UTC without the chrono crate
    let (y, mo, d, h, mi, s) = epoch_to_ymd_hms(secs);
    format!("{y:04}-{mo:02}-{d:02}T{h:02}:{mi:02}:{s:02}Z")
}

fn epoch_to_ymd_hms(secs: u64) -> (u64, u64, u64, u64, u64, u64) {
    let s = secs % 60;
    let total_minutes = secs / 60;
    let mi = total_minutes % 60;
    let total_hours = total_minutes / 60;
    let h = total_hours % 24;
    let mut days = total_hours / 24;

    // Days since 1970-01-01
    let mut y = 1970u64;
    loop {
        let dy = if crate::db::is_leap_year(y) { 366 } else { 365 };
        if days < dy { break; }
        days -= dy;
        y += 1;
    }
    let leap = crate::db::is_leap_year(y);
    let months = [31u64, if leap { 29 } else { 28 }, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];
    let mut mo = 1u64;
    for dm in &months {
        if days < *dm { break; }
        days -= dm;
        mo += 1;
    }
    (y, mo, days + 1, h, mi, s)
}


#[cfg(test)]
mod tests {
    use super::*;
    use axum::{body::Body, http::Request, Router, routing::post};
    use axum::routing::get;
    use tower::ServiceExt;

    use crate::db::Db;
    use serde_json::Value;
    use std::sync::Arc;
    use crate::plaid::PlaidClient;

    fn test_state() -> AppState {
        let db = Db::open_in_memory().unwrap();
        // Seed a device for auth tests
        db.get_or_create_device(
            "test-device-id",
            "a".repeat(64).as_str(),
            "2024-01-01T00:00:00Z",
        ).unwrap();
        AppState {
            plaid: Arc::new(PlaidClient::new(
                "id".into(), "secret".into(), "sandbox",
            )),
            db,
            admin_secret: "test-secret".into(),
        }
    }

    fn app(state: AppState) -> Router {
        Router::new()
            .route("/sync/enroll", post(enroll))
            .with_state(state)
    }

    fn push_pull_app(state: AppState) -> Router {
        Router::new()
            .route("/sync/push", post(push))
            .route("/sync/pull", get(pull))
            .with_state(state)
    }

    async fn post_json(app: Router, uri: &str, body: &str) -> (StatusCode, Value) {
        let req = Request::builder()
            .method("POST")
            .uri(uri)
            .header("content-type", "application/json")
            .body(Body::from(body.to_owned()))
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        let status = resp.status();
        let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        let json: Value = serde_json::from_slice(&bytes).unwrap();
        (status, json)
    }

    // ── Enrollment tests ──────────────────────────────────────────────────────

    fn valid_pubkey() -> String {
        // 32-byte all-ones point is invalid; use the compressed base point for Ed25519 (hex)
        // We just need 64 valid hex chars that form a valid Ed25519 verifying key.
        // Ed25519 base point compressed y-coordinate (little-endian):
        "5866666666666666666666666666666666666666666666666666666666666666".to_string()
    }

    #[tokio::test]
    async fn enroll_valid_key() {
        let state = test_state();
        let a = app(state);
        let body = format!(r#"{{"public_key": "{}"}}"#, valid_pubkey());
        let (status, json) = post_json(a, "/sync/enroll", &body).await;
        assert_eq!(status, StatusCode::OK);
        assert!(json.get("device_id").is_some());
        assert!(json.get("enrolled_at").is_some());
    }

    #[tokio::test]
    async fn enroll_duplicate_returns_same_device_id() {
        let state = test_state();
        let pk = valid_pubkey();
        let body = format!(r#"{{"public_key": "{}"}}"#, pk);

        let a1 = app(state.clone());
        let (_, json1) = post_json(a1, "/sync/enroll", &body).await;
        let id1 = json1["device_id"].as_str().unwrap().to_owned();

        let a2 = app(state);
        let (_, json2) = post_json(a2, "/sync/enroll", &body).await;
        let id2 = json2["device_id"].as_str().unwrap().to_owned();

        assert_eq!(id1, id2);
    }

    #[tokio::test]
    async fn enroll_too_short_hex_returns_400() {
        let (status, json) = post_json(
            app(test_state()),
            "/sync/enroll",
            r#"{"public_key": "abcd"}"#,
        ).await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert_eq!(json["error"], "invalid public_key");
    }

    #[tokio::test]
    async fn enroll_non_hex_chars_returns_400() {
        let (status, json) = post_json(
            app(test_state()),
            "/sync/enroll",
            &format!(r#"{{"public_key": "{}"}}"#, "ZZ".repeat(32)),
        ).await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert_eq!(json["error"], "invalid public_key");
    }

    #[tokio::test]
    async fn enroll_missing_field_returns_400() {
        let (status, _) = post_json(app(test_state()), "/sync/enroll", r#"{}"#).await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
    }

    // ── Push/pull tests ────────────────────────────────────────────────────────

    use ed25519_dalek::{SigningKey, Signer};
    use std::time::{SystemTime, UNIX_EPOCH};

    fn sk() -> SigningKey {
        SigningKey::from_bytes(&[0x77u8; 32])
    }

    fn auth_state() -> AppState {
        let state = test_state();
        let vk_hex = hex::encode(sk().verifying_key().to_bytes());
        state.db.get_or_create_device("auth-device", &vk_hex, "2024-01-01T00:00:00Z").unwrap();
        // Grant an active subscription so push/pull/revoke tests pass the gate.
        state.db.set_subscription("auth-device", "2099-01-01T00:00:00Z").unwrap();
        state
    }

    fn now_ts() -> String {
        (SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64)
            .to_string()
    }

    fn signed_push(payload_b64: &str, ts: &str) -> Request<Body> {
        let msg = format!("POST\n/sync/push\n{ts}");
        let sig = hex::encode(sk().sign(msg.as_bytes()).to_bytes());
        Request::builder()
            .method("POST")
            .uri("/sync/push")
            .header("content-type", "application/json")
            .header("x-manila-device-id", "auth-device")
            .header("x-manila-timestamp", ts)
            .header("x-manila-sig", sig)
            .body(Body::from(format!(r#"{{"payload": "{payload_b64}"}}"#)))
            .unwrap()
    }

    fn signed_pull(after: Option<i64>, ts: &str) -> Request<Body> {
        let uri = match after {
            Some(n) => format!("/sync/pull?after={n}"),
            None => "/sync/pull".to_owned(),
        };
        let msg = format!("GET\n{uri}\n{ts}");
        let sig = hex::encode(sk().sign(msg.as_bytes()).to_bytes());
        Request::builder()
            .method("GET")
            .uri(uri)
            .header("x-manila-device-id", "auth-device")
            .header("x-manila-timestamp", ts)
            .header("x-manila-sig", sig)
            .body(Body::empty())
            .unwrap()
    }

    #[tokio::test]
    async fn push_stores_blob_and_returns_seq() {
        let state = auth_state();
        let ts = now_ts();
        let resp = push_pull_app(state)
            .oneshot(signed_push("aGVsbG8=", &ts))
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        let json: Value = serde_json::from_slice(&bytes).unwrap();
        assert!(json["seq"].as_i64().unwrap() >= 1);
    }

    #[tokio::test]
    async fn push_bad_base64_returns_400() {
        let state = auth_state();
        let ts = now_ts();
        let resp = push_pull_app(state)
            .oneshot(signed_push("not-valid-base64!!!", &ts))
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn push_without_auth_returns_400() {
        let req = Request::builder()
            .method("POST")
            .uri("/sync/push")
            .header("content-type", "application/json")
            .body(Body::from(r#"{"payload": "aGVsbG8="}"#))
            .unwrap();
        let resp = push_pull_app(auth_state()).oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn pull_returns_blobs_in_order() {
        let state = auth_state();
        let ts = now_ts();
        // push two blobs
        push_pull_app(state.clone())
            .oneshot(signed_push("AAAA", &ts))
            .await.unwrap();
        push_pull_app(state.clone())
            .oneshot(signed_push("BBBB", &ts))
            .await.unwrap();

        let resp = push_pull_app(state)
            .oneshot(signed_pull(Some(0), &ts))
            .await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        let json: Value = serde_json::from_slice(&bytes).unwrap();
        let blobs = json["blobs"].as_array().unwrap();
        assert_eq!(blobs.len(), 2);
        assert!(blobs[0]["seq"].as_i64().unwrap() < blobs[1]["seq"].as_i64().unwrap());
    }

    #[tokio::test]
    async fn pull_after_filters_correctly() {
        let state = auth_state();
        let ts = now_ts();
        // push blob 1
        let r1 = push_pull_app(state.clone())
            .oneshot(signed_push("AAAA", &ts))
            .await.unwrap();
        let b1 = axum::body::to_bytes(r1.into_body(), usize::MAX).await.unwrap();
        let seq1 = serde_json::from_slice::<Value>(&b1).unwrap()["seq"].as_i64().unwrap();

        // push blob 2
        push_pull_app(state.clone())
            .oneshot(signed_push("BBBB", &ts))
            .await.unwrap();

        // pull after seq1 should return only blob 2
        let resp = push_pull_app(state)
            .oneshot(signed_pull(Some(seq1), &ts))
            .await.unwrap();
        let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        let json: Value = serde_json::from_slice(&bytes).unwrap();
        let blobs = json["blobs"].as_array().unwrap();
        assert_eq!(blobs.len(), 1);
        assert!(blobs[0]["seq"].as_i64().unwrap() > seq1);
    }

    #[tokio::test]
    async fn pull_empty_returns_empty_array() {
        let state = auth_state();
        let ts = now_ts();
        let resp = push_pull_app(state)
            .oneshot(signed_pull(None, &ts))
            .await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        let json: Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(json["blobs"].as_array().unwrap().len(), 0);
    }

    #[tokio::test]
    async fn pull_without_auth_returns_400() {
        let req = Request::builder()
            .method("GET")
            .uri("/sync/pull")
            .body(Body::empty())
            .unwrap();
        let resp = push_pull_app(auth_state()).oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }

    // ── Revocation tests ──────────────────────────────────────────────────────

    fn revoke_app(state: AppState) -> Router {
        Router::new()
            .route("/sync/devices/:device_id", axum::routing::delete(revoke_device))
            .with_state(state)
    }

    fn signed_delete(device_id: &str, ts: &str) -> Request<Body> {
        let uri = format!("/sync/devices/{device_id}");
        let msg = format!("DELETE\n{uri}\n{ts}");
        let sig = hex::encode(sk().sign(msg.as_bytes()).to_bytes());
        Request::builder()
            .method("DELETE")
            .uri(uri)
            .header("x-manila-device-id", "auth-device")
            .header("x-manila-timestamp", ts)
            .header("x-manila-sig", sig)
            .body(Body::empty())
            .unwrap()
    }

    #[tokio::test]
    async fn revoke_own_device_returns_200_and_removes_device() {
        let state = auth_state();
        let ts = now_ts();
        let resp = revoke_app(state.clone())
            .oneshot(signed_delete("auth-device", &ts))
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        assert!(state.db.find_device_by_id("auth-device").unwrap().is_none());
    }

    #[tokio::test]
    async fn revoke_mismatched_device_returns_403() {
        let state = auth_state();
        // Enroll a second device so it exists.
        state.db.get_or_create_device("other-device", &"bb".repeat(32), "2024-01-01T00:00:00Z").unwrap();
        let ts = now_ts();
        let resp = revoke_app(state)
            .oneshot(signed_delete("other-device", &ts))
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn revoke_then_retry_returns_auth_failure() {
        // After revocation the device is gone from the registry; a second
        // attempt fails at the auth layer (401), not the handler layer.
        let state = auth_state();
        let ts = now_ts();
        revoke_app(state.clone())
            .oneshot(signed_delete("auth-device", &ts))
            .await
            .unwrap();

        let ts2 = now_ts();
        let resp = revoke_app(state)
            .oneshot(signed_delete("auth-device", &ts2))
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn revoke_without_auth_returns_400() {
        let req = Request::builder()
            .method("DELETE")
            .uri("/sync/devices/auth-device")
            .body(Body::empty())
            .unwrap();
        let resp = revoke_app(auth_state()).oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }

    // ── Subscription gate tests ───────────────────────────────────────────────

    fn unsubscribed_auth_state() -> AppState {
        // Enrolled but no subscription set.
        let state = test_state();
        let vk_hex = hex::encode(sk().verifying_key().to_bytes());
        state.db.get_or_create_device("auth-device", &vk_hex, "2024-01-01T00:00:00Z").unwrap();
        state
    }

    #[tokio::test]
    async fn push_unsubscribed_returns_402() {
        let state = unsubscribed_auth_state();
        let ts = now_ts();
        let resp = push_pull_app(state)
            .oneshot(signed_push("aGVsbG8=", &ts))
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::PAYMENT_REQUIRED);
    }

    #[tokio::test]
    async fn pull_unsubscribed_returns_402() {
        let state = unsubscribed_auth_state();
        let ts = now_ts();
        let resp = push_pull_app(state)
            .oneshot(signed_pull(None, &ts))
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::PAYMENT_REQUIRED);
    }

    #[tokio::test]
    async fn revoke_unsubscribed_returns_402() {
        let state = unsubscribed_auth_state();
        let ts = now_ts();
        let resp = revoke_app(state)
            .oneshot(signed_delete("auth-device", &ts))
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::PAYMENT_REQUIRED);
    }

    #[tokio::test]
    async fn enroll_no_subscription_required_returns_200() {
        // Enrollment must succeed without a subscription - device needs an
        // identity before subscription can be granted.
        let state = test_state();
        let body = format!(r#"{{"public_key": "{}"}}"#, valid_pubkey());
        let (status, _) = post_json(app(state), "/sync/enroll", &body).await;
        assert_eq!(status, StatusCode::OK);
    }
}
