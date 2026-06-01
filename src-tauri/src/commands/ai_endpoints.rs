//! ai_endpoints Tauri commands (Phase 5 brought forward for Phase 2).

use std::time::Instant;

use serde::Serialize;
use tauri::State;

use crate::error::Result;
use crate::models::AiEndpoint;
use crate::repo::ai_endpoints as repo;
use crate::repo::ai_endpoints::{CreateEndpointInput, UpdateEndpointInput};
use crate::AppState;

#[tauri::command]
pub async fn list_endpoints(
    state: State<'_, AppState>,
    kind: Option<String>,
) -> Result<Vec<AiEndpoint>> {
    repo::list(&state.db, kind.as_deref()).await
}

#[tauri::command]
pub async fn create_endpoint(
    state: State<'_, AppState>,
    input: CreateEndpointInput,
) -> Result<AiEndpoint> {
    repo::create(&state.db, input).await
}

#[tauri::command]
pub async fn update_endpoint(
    state: State<'_, AppState>,
    id: String,
    input: UpdateEndpointInput,
) -> Result<AiEndpoint> {
    repo::update(&state.db, &id, input).await
}

#[tauri::command]
pub async fn delete_endpoint(state: State<'_, AppState>, id: String) -> Result<()> {
    repo::delete(&state.db, &id).await
}

#[tauri::command]
pub async fn activate_endpoint(state: State<'_, AppState>, id: String) -> Result<AiEndpoint> {
    repo::activate(&state.db, &id).await
}

#[derive(Serialize)]
pub struct TestResult {
    pub success: bool,
    pub message: String,
    pub response_time_ms: Option<u128>,
}

/// Connectivity probe — GET {api_base_url}/models with bearer auth.
/// Mirrors old admin `POST /admin/models/{id}/test`.
#[tauri::command]
pub async fn test_endpoint(state: State<'_, AppState>, id: String) -> Result<TestResult> {
    let ep = repo::get(&state.db, &id).await?;
    let url = format!("{}/models", ep.api_base_url.trim_end_matches('/'));

    let client = reqwest::Client::builder()
        .danger_accept_invalid_certs(true) // dev / self-hosted endpoints
        .timeout(std::time::Duration::from_secs(15))
        .build()
        .map_err(|e| crate::error::Error::Other(format!("http client: {e}")))?;

    let started = Instant::now();
    let req = client.get(&url).bearer_auth(&ep.api_key);
    match req.send().await {
        Ok(resp) => {
            let ms = started.elapsed().as_millis();
            let status = resp.status();
            Ok(TestResult {
                success: status.is_success(),
                message: if status.is_success() {
                    format!("연결 성공 ({})", status.as_u16())
                } else {
                    format!("응답 코드 {}", status.as_u16())
                },
                response_time_ms: Some(ms),
            })
        }
        Err(e) => Ok(TestResult {
            success: false,
            message: format!("연결 실패: {e}"),
            response_time_ms: None,
        }),
    }
}
