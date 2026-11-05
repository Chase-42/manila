use std::path::Path;

use base64::{engine::general_purpose::STANDARD as B64, Engine};
use ed25519_dalek::SigningKey;
use serde::Deserialize;

use crate::crypto::field::seal_bytes;
use crate::storage::vault_file::{read_vault_file, write_vault_file};
use crate::sync::signing::{keypair_from_seed, public_key_hex, sign_request};

pub async fn enroll(
    http: &reqwest::Client,
    app_dir: &Path,
    server_url: &str,
    sync_signing_seed: &[u8; 32],
) -> Result<String, String> {
    let (_, vk) = keypair_from_seed(sync_signing_seed);
    let public_key = public_key_hex(&vk);

    let url = format!("{}/sync/enroll", server_url.trim_end_matches('/'));
    let resp = http
        .post(&url)
        .json(&serde_json::json!({ "public_key": public_key }))
        .send()
        .await
        .map_err(|e| e.to_string())?;

    let status = resp.status();
    let body = resp.text().await.map_err(|e| e.to_string())?;

    if !status.is_success() {
        return Err(format!("{status}: {body}"));
    }

    #[derive(Deserialize)]
    struct EnrollResponse {
        device_id: String,
    }

    let enroll_resp: EnrollResponse =
        serde_json::from_str(&body).map_err(|e| e.to_string())?;
    let device_id = enroll_resp.device_id;

    // Persist device_id to vault.json.
    let mut cfg = read_vault_file(app_dir)?
        .ok_or_else(|| "vault.json not found".to_string())?;
    cfg.sync_device_id = Some(device_id.clone());
    write_vault_file(app_dir, &cfg)?;

    Ok(device_id)
}

pub async fn push_bundle(
    http: &reqwest::Client,
    server_url: &str,
    bundle_bytes: &[u8],
    data_key: &[u8; 32],
    device_id: &str,
    signing_key: &SigningKey,
) -> Result<i64, String> {
    let ciphertext = seal_bytes(data_key, bundle_bytes);
    let payload = B64.encode(&ciphertext);

    let path_and_query = "/sync/push";
    let url = format!("{}{}", server_url.trim_end_matches('/'), path_and_query);

    let resp = authenticated_request(
        http,
        "POST",
        &url,
        path_and_query,
        Some(serde_json::json!({ "payload": payload })),
        device_id,
        signing_key,
    )
    .await?;

    let status = resp.status();
    let body = resp.text().await.map_err(|e| e.to_string())?;

    if !status.is_success() {
        return Err(format!("{status}: {body}"));
    }

    #[derive(Deserialize)]
    struct PushResponse {
        seq: i64,
    }

    let push_resp: PushResponse =
        serde_json::from_str(&body).map_err(|e| e.to_string())?;
    Ok(push_resp.seq)
}

#[derive(Deserialize)]
pub struct PullResponse {
    pub blobs: Vec<PullBlob>,
}

#[derive(Deserialize)]
pub struct PullBlob {
    pub seq: i64,
    pub device_id: String,
    pub payload: String,
    pub created_at: String,
}

pub async fn pull_blobs(
    http: &reqwest::Client,
    server_url: &str,
    after: i64,
    device_id: &str,
    signing_key: &SigningKey,
) -> Result<PullResponse, String> {
    let path_and_query = format!("/sync/pull?after={after}");
    let url = format!("{}{}", server_url.trim_end_matches('/'), path_and_query);

    let resp = authenticated_request(
        http,
        "GET",
        &url,
        &path_and_query,
        None,
        device_id,
        signing_key,
    )
    .await?;

    let status = resp.status();
    let body = resp.text().await.map_err(|e| e.to_string())?;

    if !status.is_success() {
        return Err(format!("{status}: {body}"));
    }

    serde_json::from_str(&body).map_err(|e| e.to_string())
}

pub async fn revoke_device(
    http: &reqwest::Client,
    server_url: &str,
    device_id: &str,
    signing_key: &SigningKey,
) -> Result<(), String> {
    let path_and_query = format!("/sync/devices/{device_id}");
    let url = format!("{}{}", server_url.trim_end_matches('/'), path_and_query);

    let resp = authenticated_request(
        http,
        "DELETE",
        &url,
        &path_and_query,
        None,
        device_id,
        signing_key,
    )
    .await?;

    let status = resp.status();
    let body = resp.text().await.map_err(|e| e.to_string())?;

    if !status.is_success() {
        return Err(format!("{status}: {body}"));
    }

    Ok(())
}

async fn authenticated_request(
    http: &reqwest::Client,
    method: &str,
    url: &str,
    path_and_query: &str,
    json_body: Option<serde_json::Value>,
    device_id: &str,
    signing_key: &SigningKey,
) -> Result<reqwest::Response, String> {
    let timestamp_secs: i64 = chrono::Utc::now().timestamp();
    let sig = sign_request(signing_key, method, path_and_query, timestamp_secs);

    let mut req = match method {
        "POST" => http.post(url),
        "GET" => http.get(url),
        "DELETE" => http.delete(url),
        other => return Err(format!("unsupported method: {other}")),
    };

    req = req
        .header("X-Manila-Device-Id", device_id)
        .header("X-Manila-Timestamp", timestamp_secs.to_string())
        .header("X-Manila-Sig", sig);

    if let Some(body) = json_body {
        req = req.json(&body);
    }

    req.send().await.map_err(|e| e.to_string())
}
