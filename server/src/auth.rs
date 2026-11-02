use axum::{
    async_trait,
    extract::FromRequestParts,
    http::{request::Parts, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use ed25519_dalek::{Signature, VerifyingKey};
use serde_json::json;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::AppState;

pub struct AuthDevice {
    pub device_id: String,
}

pub struct AuthError(StatusCode, &'static str);

impl IntoResponse for AuthError {
    fn into_response(self) -> Response {
        (self.0, Json(json!({ "error": self.1 }))).into_response()
    }
}

#[async_trait]
impl FromRequestParts<AppState> for AuthDevice {
    type Rejection = AuthError;

    async fn from_request_parts(parts: &mut Parts, state: &AppState) -> Result<Self, AuthError> {
        let device_id = header_str(parts, "x-manila-device-id")
            .ok_or(AuthError(StatusCode::BAD_REQUEST, "missing X-Manila-Device-Id"))?;
        let timestamp_str = header_str(parts, "x-manila-timestamp")
            .ok_or(AuthError(StatusCode::BAD_REQUEST, "missing X-Manila-Timestamp"))?;
        let sig_hex = header_str(parts, "x-manila-sig")
            .ok_or(AuthError(StatusCode::BAD_REQUEST, "missing X-Manila-Sig"))?;

        let timestamp: i64 = timestamp_str
            .parse()
            .map_err(|_| AuthError(StatusCode::BAD_REQUEST, "malformed X-Manila-Timestamp"))?;

        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as i64;
        if (now - timestamp).abs() > 300 {
            return Err(AuthError(StatusCode::UNAUTHORIZED, "timestamp expired"));
        }

        let db = state.db.clone();
        let did = device_id.clone();
        let pubkey_hex = tokio::task::spawn_blocking(move || db.find_device_by_id(&did))
            .await
            .map_err(|_| AuthError(StatusCode::INTERNAL_SERVER_ERROR, "internal error"))?
            .map_err(|_| AuthError(StatusCode::INTERNAL_SERVER_ERROR, "internal error"))?
            .ok_or(AuthError(StatusCode::UNAUTHORIZED, "unknown device"))?;

        let key_bytes: [u8; 32] = hex::decode(&pubkey_hex)
            .ok()
            .and_then(|b| b.try_into().ok())
            .ok_or(AuthError(StatusCode::INTERNAL_SERVER_ERROR, "internal error"))?;
        let verifying_key = VerifyingKey::from_bytes(&key_bytes)
            .map_err(|_| AuthError(StatusCode::INTERNAL_SERVER_ERROR, "internal error"))?;

        let sig_bytes: [u8; 64] = hex::decode(&sig_hex)
            .ok()
            .and_then(|b| b.try_into().ok())
            .ok_or(AuthError(StatusCode::UNAUTHORIZED, "invalid signature"))?;
        let signature = Signature::from_bytes(&sig_bytes);

        let method = parts.method.as_str();
        let path_and_query = parts.uri.path_and_query().map(|pq| pq.as_str()).unwrap_or(parts.uri.path());
        let message = format!("{method}\n{path_and_query}\n{timestamp_str}");
        // Body is not covered by the signature; transport integrity relies on TLS.
        // verify_strict rejects low-order (weak) keys and non-canonical signatures,
        // preventing signature forgery via small-subgroup public keys.
        verifying_key
            .verify_strict(message.as_bytes(), &signature)
            .map_err(|_| AuthError(StatusCode::UNAUTHORIZED, "invalid signature"))?;

        Ok(AuthDevice { device_id })
    }
}

fn header_str(parts: &Parts, name: &str) -> Option<String> {
    parts
        .headers
        .get(name)
        .and_then(|v| v.to_str().ok())
        .map(str::to_owned)
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{body::Body, http::Request, Router, routing::get};
    use ed25519_dalek::{SigningKey, Signer};
    use tower::ServiceExt;

    use crate::db::Db;
    use crate::plaid::PlaidClient;
    use std::sync::Arc;

    fn signing_key() -> SigningKey {
        SigningKey::from_bytes(&[0x42u8; 32])
    }

    fn verifying_key_hex() -> String {
        hex::encode(signing_key().verifying_key().to_bytes())
    }

    fn make_state() -> AppState {
        let db = Db::open_in_memory().unwrap();
        db.get_or_create_device("dev-abc", &verifying_key_hex(), "2024-01-01T00:00:00Z").unwrap();
        AppState {
            plaid: Arc::new(PlaidClient::new("id".into(), "secret".into(), "sandbox")),
            db,
        }
    }

    fn probe_app(state: AppState) -> Router {
        Router::new()
            .route("/sync/pull", get(|_: AuthDevice| async { "ok" }))
            .with_state(state)
    }

    fn fresh_sig(timestamp_str: &str) -> String {
        let sk = signing_key();
        let msg = format!("GET\n/sync/pull\n{timestamp_str}");
        hex::encode(sk.sign(msg.as_bytes()).to_bytes())
    }

    fn now_secs() -> i64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64
    }

    #[tokio::test]
    async fn valid_signature_passes() {
        let ts = now_secs().to_string();
        let sig = fresh_sig(&ts);
        let req = Request::builder()
            .method("GET").uri("/sync/pull")
            .header("x-manila-device-id", "dev-abc")
            .header("x-manila-timestamp", &ts)
            .header("x-manila-sig", &sig)
            .body(Body::empty()).unwrap();
        let status = probe_app(make_state()).oneshot(req).await.unwrap().status();
        assert_eq!(status, StatusCode::OK);
    }

    #[tokio::test]
    async fn expired_timestamp_rejected() {
        let old_ts = (now_secs() - 400).to_string();
        let sig = fresh_sig(&old_ts);
        let req = Request::builder()
            .method("GET").uri("/sync/pull")
            .header("x-manila-device-id", "dev-abc")
            .header("x-manila-timestamp", &old_ts)
            .header("x-manila-sig", &sig)
            .body(Body::empty()).unwrap();
        let status = probe_app(make_state()).oneshot(req).await.unwrap().status();
        assert_eq!(status, StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn future_timestamp_rejected() {
        let future_ts = (now_secs() + 400).to_string();
        let sig = fresh_sig(&future_ts);
        let req = Request::builder()
            .method("GET").uri("/sync/pull")
            .header("x-manila-device-id", "dev-abc")
            .header("x-manila-timestamp", &future_ts)
            .header("x-manila-sig", &sig)
            .body(Body::empty()).unwrap();
        let status = probe_app(make_state()).oneshot(req).await.unwrap().status();
        assert_eq!(status, StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn unknown_device_rejected() {
        let ts = now_secs().to_string();
        let sig = fresh_sig(&ts);
        let req = Request::builder()
            .method("GET").uri("/sync/pull")
            .header("x-manila-device-id", "unknown-id")
            .header("x-manila-timestamp", &ts)
            .header("x-manila-sig", &sig)
            .body(Body::empty()).unwrap();
        let status = probe_app(make_state()).oneshot(req).await.unwrap().status();
        assert_eq!(status, StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn tampered_signature_rejected() {
        let ts = now_secs().to_string();
        let req = Request::builder()
            .method("GET").uri("/sync/pull")
            .header("x-manila-device-id", "dev-abc")
            .header("x-manila-timestamp", &ts)
            .header("x-manila-sig", "aa".repeat(64))
            .body(Body::empty()).unwrap();
        let status = probe_app(make_state()).oneshot(req).await.unwrap().status();
        assert_eq!(status, StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn missing_auth_headers_returns_400() {
        let req = Request::builder()
            .method("GET").uri("/sync/pull")
            .body(Body::empty()).unwrap();
        let status = probe_app(make_state()).oneshot(req).await.unwrap().status();
        assert_eq!(status, StatusCode::BAD_REQUEST);
    }
}
