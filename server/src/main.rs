mod handlers;
mod plaid;

use axum::{Router, routing::{get, post}};
use std::net::SocketAddr;
use std::sync::Arc;

use plaid::PlaidClient;

#[derive(Clone)]
pub struct AppState {
    pub plaid: Arc<PlaidClient>,
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    let client_id = require_env("PLAID_CLIENT_ID");
    let secret = require_env("PLAID_SECRET");
    let plaid_env = require_env("PLAID_ENV");
    let port: u16 = std::env::var("PORT")
        .unwrap_or_else(|_| "3000".into())
        .parse()
        .unwrap_or(3000);

    let plaid = Arc::new(PlaidClient::new(client_id, secret, &plaid_env));

    let state = AppState { plaid };

    let app = Router::new()
        .route("/health", get(handlers::health))
        .route("/plaid/link-token", post(handlers::link_token))
        .route("/plaid/exchange", post(handlers::exchange))
        .route("/plaid/transactions", post(handlers::transactions))
        .with_state(state);

    let addr = SocketAddr::from(([0, 0, 0, 0], port));
    tracing::info!("listening on {addr}");
    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}

fn require_env(key: &str) -> String {
    std::env::var(key).unwrap_or_else(|_| {
        eprintln!("error: required environment variable {key} is not set");
        std::process::exit(1);
    })
}
