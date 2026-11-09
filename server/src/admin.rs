use axum::{
    extract::State,
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
    Json,
};
use serde::Deserialize;
use serde_json::json;

use crate::AppState;

#[derive(Debug, Deserialize)]
pub struct GrantRequest {
    pub device_id: Option<String>,
    pub expires_at: Option<String>,
}

pub async fn grant(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<GrantRequest>,
) -> impl IntoResponse {
    let auth = headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");

    let expected = format!("Bearer {}", state.admin_secret);
    if auth != expected {
        return (StatusCode::UNAUTHORIZED, Json(json!({ "error": "unauthorized" })));
    }

    let Some(device_id) = req.device_id else {
        return (StatusCode::BAD_REQUEST, Json(json!({ "error": "missing required field: device_id" })));
    };
    let Some(expires_at) = req.expires_at else {
        return (StatusCode::BAD_REQUEST, Json(json!({ "error": "missing required field: expires_at" })));
    };
    if !expires_at.ends_with('Z') {
        return (StatusCode::BAD_REQUEST, Json(json!({ "error": "expires_at must be UTC (suffix Z)" })));
    }

    let db = state.db.clone();
    let did = device_id.clone();
    let exp = expires_at.clone();
    match tokio::task::spawn_blocking(move || db.set_subscription(&did, &exp))
        .await
        .map_err(|_| ())
        .and_then(|r| r.map_err(|_| ()))
    {
        Ok(true) => (StatusCode::OK, Json(json!({
            "device_id": device_id,
            "subscription_expires_at": expires_at,
        }))),
        Ok(false) => (StatusCode::NOT_FOUND, Json(json!({ "error": "not found" }))),
        Err(()) => (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({ "error": "internal error" }))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{Router, body::Body, http::Request, routing::post};
    use serde_json::Value;
    use tower::ServiceExt;

    use crate::db::Db;
    use crate::plaid::PlaidClient;
    use std::sync::Arc;

    fn make_state() -> AppState {
        let db = Db::open_in_memory().unwrap();
        db.get_or_create_device("dev-1", "a".repeat(64).as_str(), "2024-01-01T00:00:00Z").unwrap();
        AppState {
            plaid: Arc::new(PlaidClient::new("id".into(), "secret".into(), "sandbox")),
            db,
            admin_secret: "test-secret".into(),
        }
    }

    fn app(state: AppState) -> Router {
        Router::new()
            .route("/admin/grant", post(grant))
            .with_state(state)
    }

    async fn post_json_with_auth(
        app: Router,
        body: &str,
        auth: &str,
    ) -> (StatusCode, Value) {
        let req = Request::builder()
            .method("POST")
            .uri("/admin/grant")
            .header("content-type", "application/json")
            .header("authorization", auth)
            .body(Body::from(body.to_owned()))
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        let status = resp.status();
        let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        (status, serde_json::from_slice(&bytes).unwrap())
    }

    #[tokio::test]
    async fn grant_valid_device_returns_200() {
        let (status, json) = post_json_with_auth(
            app(make_state()),
            r#"{"device_id":"dev-1","expires_at":"2099-01-01T00:00:00Z"}"#,
            "Bearer test-secret",
        ).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(json["device_id"], "dev-1");
        assert_eq!(json["subscription_expires_at"], "2099-01-01T00:00:00Z");
    }

    #[tokio::test]
    async fn grant_wrong_secret_returns_401() {
        let (status, json) = post_json_with_auth(
            app(make_state()),
            r#"{"device_id":"dev-1","expires_at":"2099-01-01T00:00:00Z"}"#,
            "Bearer wrong",
        ).await;
        assert_eq!(status, StatusCode::UNAUTHORIZED);
        assert_eq!(json["error"], "unauthorized");
    }

    #[tokio::test]
    async fn grant_no_auth_returns_401() {
        let req = Request::builder()
            .method("POST")
            .uri("/admin/grant")
            .header("content-type", "application/json")
            .body(Body::from(r#"{"device_id":"dev-1","expires_at":"2099-01-01T00:00:00Z"}"#))
            .unwrap();
        let resp = app(make_state()).oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn grant_unknown_device_returns_404() {
        let (status, json) = post_json_with_auth(
            app(make_state()),
            r#"{"device_id":"ghost","expires_at":"2099-01-01T00:00:00Z"}"#,
            "Bearer test-secret",
        ).await;
        assert_eq!(status, StatusCode::NOT_FOUND);
        assert_eq!(json["error"], "not found");
    }

    #[tokio::test]
    async fn grant_missing_device_id_returns_400() {
        let (status, json) = post_json_with_auth(
            app(make_state()),
            r#"{"expires_at":"2099-01-01T00:00:00Z"}"#,
            "Bearer test-secret",
        ).await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert!(json["error"].as_str().unwrap().contains("device_id"));
    }

    #[tokio::test]
    async fn grant_missing_expires_at_returns_400() {
        let (status, json) = post_json_with_auth(
            app(make_state()),
            r#"{"device_id":"dev-1"}"#,
            "Bearer test-secret",
        ).await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert!(json["error"].as_str().unwrap().contains("expires_at"));
    }

    #[tokio::test]
    async fn grant_non_utc_expires_at_returns_400() {
        let (status, json) = post_json_with_auth(
            app(make_state()),
            r#"{"device_id":"dev-1","expires_at":"2099-01-01T00:00:00+00:00"}"#,
            "Bearer test-secret",
        ).await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert!(json["error"].as_str().unwrap().contains("UTC"));
    }

    #[tokio::test]
    async fn grant_then_subscription_active() {
        let state = make_state();
        post_json_with_auth(
            app(state.clone()),
            r#"{"device_id":"dev-1","expires_at":"2099-01-01T00:00:00Z"}"#,
            "Bearer test-secret",
        ).await;
        assert!(state.db.is_subscribed("dev-1").unwrap());
    }
}
