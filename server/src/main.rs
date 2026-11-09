mod admin;
mod auth;
mod db;
mod handlers;
mod plaid;
mod sync_handlers;

use axum::{Router, extract::DefaultBodyLimit, routing::{delete, get, post}};
use std::net::SocketAddr;
use std::sync::Arc;

use db::Db;
use plaid::PlaidClient;

#[derive(Clone)]
pub struct AppState {
    pub plaid: Arc<PlaidClient>,
    pub db: Db,
    pub admin_secret: String,
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    let client_id = require_env("PLAID_CLIENT_ID");
    let secret = require_env("PLAID_SECRET");
    let plaid_env = require_env("PLAID_ENV");
    let admin_secret = require_env("ADMIN_SECRET");
    if admin_secret.is_empty() {
        eprintln!("error: ADMIN_SECRET must not be empty");
        std::process::exit(1);
    }
    let port: u16 = std::env::var("PORT")
        .unwrap_or_else(|_| "3000".into())
        .parse()
        .unwrap_or(3000);
    let db_path = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "./data/manila-sync.db".into());

    if let Some(dir) = std::path::Path::new(&db_path).parent() {
        if !dir.as_os_str().is_empty() {
            std::fs::create_dir_all(dir).unwrap_or_else(|e| {
                eprintln!("error: could not create data directory {}: {e}", dir.display());
                std::process::exit(1);
            });
        }
    }

    let db = Db::open(&db_path).unwrap_or_else(|e| {
        eprintln!("error: could not open database at {db_path}: {e}");
        std::process::exit(1);
    });

    let plaid = Arc::new(PlaidClient::new(client_id, secret, &plaid_env));

    let state = AppState { plaid, db, admin_secret };

    let app = Router::new()
        .route("/health", get(handlers::health))
        .route("/plaid/link-token", post(handlers::link_token))
        .route("/plaid/exchange", post(handlers::exchange))
        .route("/plaid/transactions", post(handlers::transactions))
        .route("/sync/enroll", post(sync_handlers::enroll).layer(DefaultBodyLimit::max(4 * 1024)))
        .route("/sync/push", post(sync_handlers::push).layer(DefaultBodyLimit::max(4 * 1024 * 1024)))
        .route("/sync/pull", get(sync_handlers::pull))
        .route("/sync/devices/:device_id", delete(sync_handlers::revoke_device))
        .route("/admin/grant", post(admin::grant).layer(DefaultBodyLimit::max(1024)))
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
