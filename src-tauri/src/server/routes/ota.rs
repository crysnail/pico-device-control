use crate::models::{OtaDownloadRequest, OtaVersionInfo};
use crate::state::AppState;
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::routing::{get, post};
use axum::{Json, Router};
use serde_json::json;
use std::sync::Arc;

pub fn router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/api/ota/version", get(get_version))
        .route("/api/ota/upload", post(upload_update))
        .route("/api/ota/download", post(download_update))
}

async fn get_version() -> impl IntoResponse {
    Json(OtaVersionInfo {
        version: format!("v{}", env!("CARGO_PKG_VERSION")),
        build_time: "2026-11-20".to_string(),
        channel: "Rust 原生一体版 (Release)".to_string(),
        os_platform: format!("{} {}", std::env::consts::OS, std::env::consts::ARCH),
        runtime: "Rust + Tauri v2 + Axum".to_string(),
    })
}

async fn upload_update() -> impl IntoResponse {
    (
        StatusCode::BAD_REQUEST,
        Json(json!({
            "success": false,
            "message": "出于安全加固考量，当前 Rust 一体化客户端已禁用未签名热替换；请直接替换 Build 目录中的新版 PicoVR-Manager.exe 后重启。"
        })),
    )
}

async fn download_update(Json(_req): Json<OtaDownloadRequest>) -> impl IntoResponse {
    (
        StatusCode::FORBIDDEN,
        Json(json!({
            "success": false,
            "message": "为防止局域网未鉴权远程代码执行 (RCE)，已关闭任意远程 URL 自动热替换，请通过可信渠道更新客户端安装包。"
        })),
    )
}
