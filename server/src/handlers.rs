use axum::{
    extract::State,
    http::{HeaderValue, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use serde::Deserialize;
use serde_json::{json, Value};

use crate::AppState;
use crate::auth::{AuthDevice, require_subscribed};
use crate::plaid::PlaidError;

pub async fn health() -> Json<Value> {
    Json(json!({ "status": "ok" }))
}

fn plaid_err_response(err: PlaidError) -> (StatusCode, Json<Value>) {
    match err {
        PlaidError::ApiError { status, body } => {
            let code = StatusCode::from_u16(status).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
            let msg = serde_json::from_str::<Value>(&body)
                .ok()
                .and_then(|v| v.get("error_message").and_then(|m| m.as_str()).map(str::to_owned))
                .unwrap_or(body);
            (code, Json(json!({ "error": msg })))
        }
        PlaidError::Network(e) => {
            tracing::warn!("plaid network error: {e}");
            (StatusCode::BAD_GATEWAY, Json(json!({ "error": "upstream unavailable" })))
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct LinkTokenRequest {
    pub client_user_id: Option<String>,
    pub redirect_uri: Option<String>,
}

pub async fn link_token(
    State(state): State<AppState>,
    auth: AuthDevice,
    Json(req): Json<LinkTokenRequest>,
) -> Response {
    if let Err(e) = require_subscribed(&state, &auth.device_id).await {
        return e.into_response();
    }

    let Some(client_user_id) = req.client_user_id else {
        return (StatusCode::BAD_REQUEST, Json(json!({ "error": "missing required field: client_user_id" }))).into_response();
    };

    let mut body = json!({
        "user": { "client_user_id": client_user_id },
        "client_name": "Manila",
        "products": ["transactions"],
        "country_codes": ["US"],
        "language": "en"
    });

    if let Some(uri) = req.redirect_uri {
        body["redirect_uri"] = Value::String(uri);
    }

    match state.plaid.call("/link/token/create", body).await {
        Ok(resp) => {
            let link_token = resp
                .get("link_token")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_owned();
            with_sub_exp_header(
                (StatusCode::OK, Json(json!({ "link_token": link_token }))).into_response(),
                sub_exp_for(&state, &auth.device_id).await,
            )
        }
        Err(e) => plaid_err_response(e).into_response(),
    }
}

#[derive(Debug, Deserialize)]
pub struct ExchangeRequest {
    pub public_token: Option<String>,
}

pub async fn exchange(
    State(state): State<AppState>,
    auth: AuthDevice,
    Json(req): Json<ExchangeRequest>,
) -> Response {
    if let Err(e) = require_subscribed(&state, &auth.device_id).await {
        return e.into_response();
    }

    let Some(public_token) = req.public_token else {
        return (StatusCode::BAD_REQUEST, Json(json!({ "error": "missing required field: public_token" }))).into_response();
    };

    let body = json!({ "public_token": public_token });

    match state.plaid.call("/item/public_token/exchange", body).await {
        Ok(resp) => {
            let access_token = resp
                .get("access_token")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_owned();
            let item_id = resp
                .get("item_id")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_owned();
            with_sub_exp_header(
                (StatusCode::OK, Json(json!({ "access_token": access_token, "item_id": item_id }))).into_response(),
                sub_exp_for(&state, &auth.device_id).await,
            )
        }
        Err(e) => plaid_err_response(e).into_response(),
    }
}

#[derive(Debug, Deserialize)]
pub struct TransactionsRequest {
    pub access_token: Option<String>,
    pub cursor: Option<String>,
}

pub async fn transactions(
    State(state): State<AppState>,
    auth: AuthDevice,
    Json(req): Json<TransactionsRequest>,
) -> Response {
    if let Err(e) = require_subscribed(&state, &auth.device_id).await {
        return e.into_response();
    }

    let Some(access_token) = req.access_token else {
        return (StatusCode::BAD_REQUEST, Json(json!({ "error": "missing required field: access_token" }))).into_response();
    };

    let mut body = json!({ "access_token": access_token });
    if let Some(cursor) = req.cursor {
        body["cursor"] = Value::String(cursor);
    }

    match state.plaid.call("/transactions/sync", body).await {
        Ok(resp) => with_sub_exp_header(
            (StatusCode::OK, Json(resp)).into_response(),
            sub_exp_for(&state, &auth.device_id).await,
        ),
        Err(e) => plaid_err_response(e).into_response(),
    }
}

async fn sub_exp_for(state: &AppState, device_id: &str) -> Option<String> {
    let db = state.db.clone();
    let did = device_id.to_owned();
    tokio::task::spawn_blocking(move || db.find_device_subscription(&did))
        .await
        .ok()
        .and_then(|r| r.ok())
        .flatten()
}

fn with_sub_exp_header(mut resp: Response, sub_exp: Option<String>) -> Response {
    if let Some(exp) = sub_exp {
        if let Ok(val) = HeaderValue::from_str(&exp) {
            resp.headers_mut().insert("x-manila-sub-exp", val);
        }
    }
    resp
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{Router, body::Body, http::Request, routing::post};
    use ed25519_dalek::{SigningKey, Signer};
    use serde_json::from_str;
    use std::sync::Arc;
    use std::time::{SystemTime, UNIX_EPOCH};
    use tower::ServiceExt;

    use crate::db::Db;
    use crate::plaid::PlaidClient;

    fn sk() -> SigningKey {
        SigningKey::from_bytes(&[0x77u8; 32])
    }

    fn now_ts() -> String {
        (SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64)
            .to_string()
    }

    fn unsubscribed_state() -> AppState {
        let db = Db::open_in_memory().unwrap();
        let vk_hex = hex::encode(sk().verifying_key().to_bytes());
        db.get_or_create_device("auth-device", &vk_hex, "2024-01-01T00:00:00Z").unwrap();
        AppState {
            plaid: Arc::new(PlaidClient::new("id".into(), "secret".into(), "sandbox")),
            db,
            admin_secret: "test-secret".into(),
        }
    }

    fn subscribed_state() -> AppState {
        let state = unsubscribed_state();
        state.db.set_subscription("auth-device", "2099-01-01T00:00:00Z").unwrap();
        state
    }

    fn plaid_app(state: AppState) -> Router {
        Router::new()
            .route("/plaid/link-token", post(link_token))
            .route("/plaid/exchange", post(exchange))
            .route("/plaid/transactions", post(transactions))
            .with_state(state)
    }

    fn signed_post(path: &str, body_str: &str, ts: &str) -> Request<Body> {
        let msg = format!("POST\n{path}\n{ts}");
        let sig = hex::encode(sk().sign(msg.as_bytes()).to_bytes());
        Request::builder()
            .method("POST")
            .uri(path)
            .header("content-type", "application/json")
            .header("x-manila-device-id", "auth-device")
            .header("x-manila-timestamp", ts)
            .header("x-manila-sig", sig)
            .body(Body::from(body_str.to_owned()))
            .unwrap()
    }

    #[tokio::test]
    async fn link_token_unsubscribed_returns_402() {
        let ts = now_ts();
        let req = signed_post("/plaid/link-token", r#"{"client_user_id":"uid"}"#, &ts);
        let resp = plaid_app(unsubscribed_state()).oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::PAYMENT_REQUIRED);
    }

    #[tokio::test]
    async fn exchange_unsubscribed_returns_402() {
        let ts = now_ts();
        let req = signed_post("/plaid/exchange", r#"{"public_token":"tok"}"#, &ts);
        let resp = plaid_app(unsubscribed_state()).oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::PAYMENT_REQUIRED);
    }

    #[tokio::test]
    async fn transactions_unsubscribed_returns_402() {
        let ts = now_ts();
        let req = signed_post("/plaid/transactions", r#"{"access_token":"tok"}"#, &ts);
        let resp = plaid_app(unsubscribed_state()).oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::PAYMENT_REQUIRED);
    }

    #[tokio::test]
    async fn link_token_subscribed_passes_gate() {
        // Passes the subscription gate; Plaid returns a network/API error since
        // fake credentials are used, but the response must not be 402.
        let ts = now_ts();
        let req = signed_post("/plaid/link-token", r#"{"client_user_id":"uid"}"#, &ts);
        let resp = plaid_app(subscribed_state()).oneshot(req).await.unwrap();
        assert_ne!(resp.status(), StatusCode::PAYMENT_REQUIRED);
    }

    #[tokio::test]
    async fn exchange_subscribed_passes_gate() {
        let ts = now_ts();
        let req = signed_post("/plaid/exchange", r#"{"public_token":"tok"}"#, &ts);
        let resp = plaid_app(subscribed_state()).oneshot(req).await.unwrap();
        assert_ne!(resp.status(), StatusCode::PAYMENT_REQUIRED);
    }

    #[tokio::test]
    async fn transactions_subscribed_passes_gate() {
        let ts = now_ts();
        let req = signed_post("/plaid/transactions", r#"{"access_token":"tok"}"#, &ts);
        let resp = plaid_app(subscribed_state()).oneshot(req).await.unwrap();
        assert_ne!(resp.status(), StatusCode::PAYMENT_REQUIRED);
    }

    #[test]
    fn link_token_request_missing_user_id() {
        let raw = r#"{"redirect_uri": "manila://plaid-oauth"}"#;
        let req: LinkTokenRequest = from_str(raw).unwrap();
        assert!(req.client_user_id.is_none());
        assert_eq!(req.redirect_uri.as_deref(), Some("manila://plaid-oauth"));
    }

    #[test]
    fn link_token_request_full() {
        let raw = r#"{"client_user_id": "vault-abc", "redirect_uri": "manila://plaid-oauth"}"#;
        let req: LinkTokenRequest = from_str(raw).unwrap();
        assert_eq!(req.client_user_id.as_deref(), Some("vault-abc"));
        assert_eq!(req.redirect_uri.as_deref(), Some("manila://plaid-oauth"));
    }

    #[test]
    fn exchange_request_missing_token() {
        let raw = r#"{}"#;
        let req: ExchangeRequest = from_str(raw).unwrap();
        assert!(req.public_token.is_none());
    }

    #[test]
    fn exchange_request_with_token() {
        let raw = r#"{"public_token": "public-sandbox-abc"}"#;
        let req: ExchangeRequest = from_str(raw).unwrap();
        assert_eq!(req.public_token.as_deref(), Some("public-sandbox-abc"));
    }

    #[test]
    fn transactions_request_no_cursor() {
        let raw = r#"{"access_token": "access-sandbox-abc", "cursor": null}"#;
        let req: TransactionsRequest = from_str(raw).unwrap();
        assert_eq!(req.access_token.as_deref(), Some("access-sandbox-abc"));
        assert!(req.cursor.is_none());
    }

    #[test]
    fn transactions_request_with_cursor() {
        let raw = r#"{"access_token": "access-sandbox-abc", "cursor": "xyz"}"#;
        let req: TransactionsRequest = from_str(raw).unwrap();
        assert_eq!(req.cursor.as_deref(), Some("xyz"));
    }

    #[test]
    fn transactions_request_missing_token() {
        let raw = r#"{"cursor": "xyz"}"#;
        let req: TransactionsRequest = from_str(raw).unwrap();
        assert!(req.access_token.is_none());
    }
}
