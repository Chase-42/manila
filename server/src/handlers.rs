use axum::{
    extract::State,
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use serde::Deserialize;
use serde_json::{json, Value};

use crate::AppState;
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
    Json(req): Json<LinkTokenRequest>,
) -> impl IntoResponse {
    let Some(client_user_id) = req.client_user_id else {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "missing required field: client_user_id" })),
        );
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
            (StatusCode::OK, Json(json!({ "link_token": link_token })))
        }
        Err(e) => plaid_err_response(e),
    }
}

#[derive(Debug, Deserialize)]
pub struct ExchangeRequest {
    pub public_token: Option<String>,
}

pub async fn exchange(
    State(state): State<AppState>,
    Json(req): Json<ExchangeRequest>,
) -> impl IntoResponse {
    let Some(public_token) = req.public_token else {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "missing required field: public_token" })),
        );
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
            (
                StatusCode::OK,
                Json(json!({ "access_token": access_token, "item_id": item_id })),
            )
        }
        Err(e) => plaid_err_response(e),
    }
}

#[derive(Debug, Deserialize)]
pub struct TransactionsRequest {
    pub access_token: Option<String>,
    pub cursor: Option<String>,
}

pub async fn transactions(
    State(state): State<AppState>,
    Json(req): Json<TransactionsRequest>,
) -> impl IntoResponse {
    let Some(access_token) = req.access_token else {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "missing required field: access_token" })),
        );
    };

    let mut body = json!({ "access_token": access_token });
    if let Some(cursor) = req.cursor {
        body["cursor"] = Value::String(cursor);
    }

    match state.plaid.call("/transactions/sync", body).await {
        Ok(resp) => (StatusCode::OK, Json(resp)),
        Err(e) => plaid_err_response(e),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::from_str;

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
