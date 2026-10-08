use serde_json::Value;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum PlaidError {
    #[error("plaid error {status}: {body}")]
    ApiError { status: u16, body: String },
    #[error("network error: {0}")]
    Network(#[from] reqwest::Error),
}

pub struct PlaidClient {
    client_id: String,
    secret: String,
    base_url: String,
    http: reqwest::Client,
}

impl PlaidClient {
    pub fn new(client_id: String, secret: String, plaid_env: &str) -> Self {
        let base_url = resolve_base_url(plaid_env).to_string();
        Self {
            client_id,
            secret,
            base_url,
            http: reqwest::Client::new(),
        }
    }

    pub async fn call(&self, path: &str, mut body: Value) -> Result<Value, PlaidError> {
        let obj = body.as_object_mut().expect("body must be an object");
        obj.insert("client_id".into(), Value::String(self.client_id.clone()));
        obj.insert("secret".into(), Value::String(self.secret.clone()));

        let url = format!("{}{}", self.base_url, path);
        let resp = self.http.post(&url).json(&body).send().await?;
        let status = resp.status();
        let text = resp.text().await.map_err(PlaidError::Network)?;

        if status.is_success() {
            let value: Value = serde_json::from_str(&text).unwrap_or(Value::Null);
            Ok(value)
        } else {
            Err(PlaidError::ApiError {
                status: status.as_u16(),
                body: text,
            })
        }
    }
}

pub fn resolve_base_url(plaid_env: &str) -> &'static str {
    match plaid_env {
        "sandbox" => "https://sandbox.plaid.com",
        "development" => "https://development.plaid.com",
        "production" => "https://production.plaid.com",
        other => {
            eprintln!("error: unrecognized PLAID_ENV value '{other}'; expected sandbox, development, or production");
            std::process::exit(1);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sandbox_url() {
        assert_eq!(resolve_base_url("sandbox"), "https://sandbox.plaid.com");
    }

    #[test]
    fn development_url() {
        assert_eq!(resolve_base_url("development"), "https://development.plaid.com");
    }

    #[test]
    fn production_url() {
        assert_eq!(resolve_base_url("production"), "https://production.plaid.com");
    }
}
