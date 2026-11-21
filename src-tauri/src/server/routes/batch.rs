use crate::models::{BatchCommandRequest, BatchScreenRequest};
use crate::state::AppState;
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::routing::post;
use axum::{Json, Router};
use serde_json::json;
use std::sync::Arc;

pub fn router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/api/batch/start-app", post(batch_start_app))
        .route("/api/batch/stop-app", post(batch_stop_app))
        .route("/api/batch/pause-video", post(batch_pause_video))
        .route("/api/batch/resume-video", post(batch_resume_video))
        .route("/api/batch/volume", post(batch_volume))
        .route("/api/batch/reboot", post(batch_reboot))
        .route("/api/batch/screen", post(batch_screen))
        .route("/api/batch/recenter", post(batch_recenter))
        .route("/api/batch/find", post(batch_find))
        .route("/api/batch/shutdown", post(batch_shutdown))
}

async fn batch_start_app(
    State(state): State<Arc<AppState>>,
    Json(req): Json<BatchCommandRequest>,
) -> impl IntoResponse {
    if req.sn_list.is_empty() {
        return (StatusCode::BAD_REQUEST, Json(json!({ "message": "请至少选择一台设备" })));
    }
    if req.package_name.trim().is_empty() {
        return (StatusCode::BAD_REQUEST, Json(json!({ "message": "必须提供应用包名" })));
    }
    let count = state
        .device_manager
        .batch_start_app(&req.sn_list, &req.package_name, &req.extra_json)
        .await;
    (StatusCode::OK, Json(json!({ "success": true, "processedCount": count })))
}

async fn batch_stop_app(
    State(state): State<Arc<AppState>>,
    Json(req): Json<BatchCommandRequest>,
) -> impl IntoResponse {
    if req.sn_list.is_empty() {
        return (StatusCode::BAD_REQUEST, Json(json!({ "message": "请至少选择一台设备" })));
    }
    let count = state.device_manager.batch_stop_app(&req.sn_list).await;
    (StatusCode::OK, Json(json!({ "success": true, "processedCount": count })))
}

async fn batch_pause_video(
    State(state): State<Arc<AppState>>,
    Json(req): Json<BatchCommandRequest>,
) -> impl IntoResponse {
    if req.sn_list.is_empty() {
        return (StatusCode::BAD_REQUEST, Json(json!({ "message": "请至少选择一台设备" })));
    }
    let count = state.device_manager.batch_pause_video(&req.sn_list).await;
    (StatusCode::OK, Json(json!({ "success": true, "processedCount": count })))
}

async fn batch_resume_video(
    State(state): State<Arc<AppState>>,
    Json(req): Json<BatchCommandRequest>,
) -> impl IntoResponse {
    if req.sn_list.is_empty() {
        return (StatusCode::BAD_REQUEST, Json(json!({ "message": "请至少选择一台设备" })));
    }
    let count = state.device_manager.batch_continue_video(&req.sn_list).await;
    (StatusCode::OK, Json(json!({ "success": true, "processedCount": count })))
}

async fn batch_volume(
    State(state): State<Arc<AppState>>,
    Json(req): Json<BatchCommandRequest>,
) -> impl IntoResponse {
    if req.sn_list.is_empty() {
        return (StatusCode::BAD_REQUEST, Json(json!({ "message": "请至少选择一台设备" })));
    }
    let count = state.device_manager.batch_set_volume(&req.sn_list, req.volume).await;
    (StatusCode::OK, Json(json!({ "success": true, "processedCount": count })))
}

async fn batch_screen(
    State(state): State<Arc<AppState>>,
    Json(req): Json<BatchScreenRequest>,
) -> impl IntoResponse {
    if req.sn_list.is_empty() {
        return (StatusCode::BAD_REQUEST, Json(json!({ "message": "请至少选择一台设备" })));
    }
    let count = state.device_manager.batch_set_screen(&req.sn_list, req.screen_on).await;
    (
        StatusCode::OK,
        Json(json!({ "success": true, "processedCount": count, "screenOn": req.screen_on })),
    )
}

async fn batch_reboot(
    State(state): State<Arc<AppState>>,
    Json(req): Json<BatchCommandRequest>,
) -> impl IntoResponse {
    if req.sn_list.is_empty() {
        return (StatusCode::BAD_REQUEST, Json(json!({ "message": "请至少选择一台设备" })));
    }
    let count = state.device_manager.batch_reboot(&req.sn_list).await;
    (StatusCode::OK, Json(json!({ "success": true, "processedCount": count })))
}

async fn batch_recenter(
    State(state): State<Arc<AppState>>,
    Json(req): Json<BatchCommandRequest>,
) -> impl IntoResponse {
    if req.sn_list.is_empty() {
        return (StatusCode::BAD_REQUEST, Json(json!({ "message": "请至少选择一台设备" })));
    }
    let count = state.device_manager.batch_recenter(&req.sn_list).await;
    (StatusCode::OK, Json(json!({ "success": true, "processedCount": count })))
}

async fn batch_find(
    State(state): State<Arc<AppState>>,
    Json(req): Json<BatchCommandRequest>,
) -> impl IntoResponse {
    if req.sn_list.is_empty() {
        return (StatusCode::BAD_REQUEST, Json(json!({ "message": "请至少选择一台设备" })));
    }
    let count = state.device_manager.batch_find(&req.sn_list).await;
    (StatusCode::OK, Json(json!({ "success": true, "processedCount": count })))
}

async fn batch_shutdown(
    State(state): State<Arc<AppState>>,
    Json(req): Json<BatchCommandRequest>,
) -> impl IntoResponse {
    if req.sn_list.is_empty() {
        return (StatusCode::BAD_REQUEST, Json(json!({ "message": "请至少选择一台设备" })));
    }
    let count = state.device_manager.batch_shutdown(&req.sn_list).await;
    (StatusCode::OK, Json(json!({ "success": true, "processedCount": count })))
}
