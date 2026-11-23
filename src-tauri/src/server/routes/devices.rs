use crate::models::{ScreenStateRequest, SetAliasRequest, StartAppRequest, StopAppRequest, VolumeRequest};
use crate::state::AppState;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::routing::{get, post};
use axum::{Json, Router};
use serde_json::json;
use std::sync::Arc;

pub fn router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/api/devices", get(get_all))
        .route("/api/devices/{sn}", get(get_by_sn))
        .route("/api/devices/{sn}/lock", post(lock_device))
        .route("/api/devices/{sn}/unlock", post(unlock_device))
        .route("/api/devices/{sn}/recenter", post(recenter))
        .route("/api/devices/{sn}/find", post(find_device))
        .route("/api/devices/{sn}/voice-call", post(voice_call))
        .route("/api/devices/{sn}/volume", post(set_volume))
        .route("/api/devices/{sn}/alias", post(set_alias))
        .route("/api/devices/{sn}/screen", post(set_screen))
        .route("/api/devices/{sn}/start-app", post(start_app))
        .route("/api/devices/{sn}/stop-app", post(stop_app))
        .route("/api/devices/{sn}/reboot", post(reboot))
        .route("/api/devices/{sn}/shutdown", post(shutdown))
}

async fn get_all(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    Json(state.device_manager.get_all_devices())
}

async fn get_by_sn(State(state): State<Arc<AppState>>, Path(sn): Path<String>) -> impl IntoResponse {
    match state.device_manager.get_device(&sn) {
        Some(dev) => (StatusCode::OK, Json(json!(dev))).into_response(),
        None => (StatusCode::NOT_FOUND, Json(json!({ "message": format!("设备 {} 未找到", sn) }))).into_response(),
    }
}

async fn lock_device(State(state): State<Arc<AppState>>, Path(sn): Path<String>) -> impl IntoResponse {
    let ret = state.device_manager.lock_device(&sn);
    Json(json!({ "success": ret == 0, "ret": ret }))
}

async fn unlock_device(State(state): State<Arc<AppState>>, Path(sn): Path<String>) -> impl IntoResponse {
    let ret = state.device_manager.unlock_device(&sn);
    Json(json!({ "success": ret == 0, "ret": ret }))
}

async fn recenter(State(state): State<Arc<AppState>>, Path(sn): Path<String>) -> impl IntoResponse {
    let ret = state.device_manager.recenter(&sn);
    Json(json!({ "success": ret == 0, "ret": ret }))
}

#[derive(serde::Deserialize, Default)]
struct ToggleActiveRequest {
    active: Option<bool>,
}

async fn find_device(
    State(state): State<Arc<AppState>>,
    Path(sn): Path<String>,
    body: Option<Json<ToggleActiveRequest>>,
) -> impl IntoResponse {
    let active = body.and_then(|Json(b)| b.active).unwrap_or(true);
    let ret = state.device_manager.find_device(&sn, active).await;
    Json(json!({ "success": true, "active": active, "ret": ret }))
}

async fn voice_call(
    State(state): State<Arc<AppState>>,
    Path(sn): Path<String>,
    body: Option<Json<ToggleActiveRequest>>,
) -> impl IntoResponse {
    let active = body.and_then(|Json(b)| b.active).unwrap_or(true);
    let ret = state.device_manager.set_voice_call(&sn, active).await;
    Json(json!({ "success": true, "active": active, "ret": ret }))
}

async fn set_volume(
    State(state): State<Arc<AppState>>,
    Path(sn): Path<String>,
    Json(req): Json<VolumeRequest>,
) -> impl IntoResponse {
    let ret = state.device_manager.set_volume(&sn, req.volume);
    Json(json!({ "success": ret == 0, "ret": ret }))
}

async fn set_alias(
    State(state): State<Arc<AppState>>,
    Path(sn): Path<String>,
    Json(req): Json<SetAliasRequest>,
) -> impl IntoResponse {
    let ret = state.device_manager.set_device_alias(&sn, &req.alias);
    Json(json!({ "success": ret == 0, "ret": ret }))
}

async fn set_screen(
    State(state): State<Arc<AppState>>,
    Path(sn): Path<String>,
    Json(req): Json<ScreenStateRequest>,
) -> impl IntoResponse {
    let ret = state.device_manager.set_screen_state(&sn, req.screen_on).await;
    Json(json!({ "success": true, "screenOn": req.screen_on, "ret": ret }))
}

async fn start_app(
    State(state): State<Arc<AppState>>,
    Path(sn): Path<String>,
    Json(req): Json<StartAppRequest>,
) -> impl IntoResponse {
    let ret = state
        .device_manager
        .start_app(&sn, &req.package_name, req.extra_json.as_deref().unwrap_or(""));
    Json(json!({ "success": ret == 0, "ret": ret }))
}

async fn stop_app(
    State(state): State<Arc<AppState>>,
    Path(sn): Path<String>,
    body: Option<Json<StopAppRequest>>,
) -> impl IntoResponse {
    let pkg = body
        .and_then(|Json(r)| r.package_name)
        .unwrap_or_default();
    let ret = state.device_manager.stop_app(&sn, &pkg);
    Json(json!({ "success": ret == 0, "ret": ret }))
}

async fn reboot(State(state): State<Arc<AppState>>, Path(sn): Path<String>) -> impl IntoResponse {
    let ret = state.device_manager.reboot(&sn);
    Json(json!({ "success": ret == 0, "ret": ret }))
}

async fn shutdown(State(state): State<Arc<AppState>>, Path(sn): Path<String>) -> impl IntoResponse {
    let ret = state.device_manager.shutdown(&sn);
    Json(json!({ "success": ret == 0, "ret": ret }))
}
