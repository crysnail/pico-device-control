use crate::models::{
    AdbConnectRequest, AdbLocalInstallRequest, AdbLocalPushRequest, AdbMkdirRequest,
    AdbScanRequest, AdbScreenRequest, AdbTargetRequest, AdbUninstallRequest,
};
use crate::services::adb_service::AdbService;
use crate::services::safe_zip;
use crate::state::AppState;
use axum::extract::{DefaultBodyLimit, Multipart, Query, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::Deserialize;
use serde_json::json;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::fs::File;
use tokio::io::AsyncWriteExt;
use tokio::sync::Semaphore;
use uuid::Uuid;

pub fn router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/api/adb/devices", get(get_devices))
        .route("/api/adb/subnets", get(get_subnets))
        .route("/api/adb/connect", post(connect))
        .route("/api/adb/disconnect", post(disconnect))
        .route("/api/adb/scan", post(scan))
        .route("/api/adb/read-sn", post(read_sn))
        .route("/api/adb/reboot", post(reboot))
        .route("/api/adb/sync-to-config", post(sync_to_config))
        .route("/api/adb/packages", get(get_packages))
        .route(
            "/api/adb/upload-install",
            post(upload_install).layer(DefaultBodyLimit::disable()),
        )
        .route("/api/adb/local-install", post(local_install))
        .route("/api/adb/uninstall", post(uninstall))
        .route(
            "/api/adb/upload-push",
            post(upload_push).layer(DefaultBodyLimit::disable()),
        )
        .route("/api/adb/local-push", post(local_push))
        .route("/api/adb/mkdir", post(mkdir))
        .route("/api/adb/screen", post(set_screen))
}

async fn get_devices(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    let devices = state.adb_service.get_connected_devices().await;
    Json(json!({ "success": true, "count": devices.len(), "devices": devices }))
}

async fn get_subnets(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    let subnets = state.adb_service.detect_lan_subnets();
    Json(json!({ "success": true, "subnets": subnets }))
}

async fn connect(
    State(state): State<Arc<AppState>>,
    Json(req): Json<AdbConnectRequest>,
) -> impl IntoResponse {
    let ip = req.ip.trim();
    if ip.is_empty() {
        return (StatusCode::BAD_REQUEST, Json(json!({ "success": false, "message": "IP 地址不能为空" })));
    }
    let port = if req.port > 0 { req.port } else { 5555 };
    let ok = state.adb_service.connect(ip, port).await;
    if ok {
        let target = if port == 5555 {
            ip.to_string()
        } else {
            format!("{}:{}", ip, port)
        };
        let sn = state.adb_service.read_device_sn(&target).await;
        (
            StatusCode::OK,
            Json(json!({
                "success": true,
                "target": target,
                "sn": sn,
                "message": format!("成功连接无线 ADB ({})", target)
            })),
        )
    } else {
        (
            StatusCode::OK,
            Json(json!({
                "success": false,
                "message": format!("连接失败，请确认头戴已开机并处于同一局域网内 (TCP {})", port)
            })),
        )
    }
}

async fn disconnect(
    State(state): State<Arc<AppState>>,
    Json(req): Json<AdbTargetRequest>,
) -> impl IntoResponse {
    if req.target.trim().is_empty() {
        return (StatusCode::BAD_REQUEST, Json(json!({ "success": false })));
    }
    let ok = state.adb_service.disconnect(req.target.trim()).await;
    (StatusCode::OK, Json(json!({ "success": ok })))
}

async fn scan(
    State(state): State<Arc<AppState>>,
    body: Option<Json<AdbScanRequest>>,
) -> impl IntoResponse {
    let subnet = body.and_then(|Json(r)| r.subnet);
    let devices = state.adb_service.scan_and_auto_connect(subnet, 600).await;
    Json(json!({
        "success": true,
        "discoveredCount": devices.len(),
        "devices": devices
    }))
}

async fn read_sn(
    State(state): State<Arc<AppState>>,
    Json(req): Json<AdbTargetRequest>,
) -> impl IntoResponse {
    if req.target.trim().is_empty() {
        return (StatusCode::BAD_REQUEST, Json(json!({ "success": false })));
    }
    let sn = state.adb_service.read_device_sn(req.target.trim()).await;
    (StatusCode::OK, Json(json!({ "success": !sn.is_empty(), "sn": sn })))
}

async fn reboot(
    State(state): State<Arc<AppState>>,
    Json(req): Json<AdbTargetRequest>,
) -> impl IntoResponse {
    if req.target.trim().is_empty() {
        return (StatusCode::BAD_REQUEST, Json(json!({ "success": false })));
    }
    let ok = state.adb_service.reboot(req.target.trim()).await;
    (StatusCode::OK, Json(json!({ "success": ok })))
}

#[derive(Deserialize, Default)]
struct SyncConfigRequest {
    #[serde(default)]
    targets: Option<Vec<String>>,
}

async fn sync_to_config(
    State(state): State<Arc<AppState>>,
    body: Option<Json<SyncConfigRequest>>,
) -> impl IntoResponse {
    let filter_targets = body.and_then(|Json(r)| r.targets).filter(|t| !t.is_empty());
    let devices = state.adb_service.get_connected_devices().await;
    let existing_count = state.device_manager.get_all_devices().len();
    let mut synced = 0usize;
    for dev in devices.into_iter().filter(|d| d.is_connected && !d.sn.is_empty()) {
        if let Some(ref allow) = filter_targets {
            if !allow.iter().any(|t| t == &dev.target || t == &dev.sn) {
                continue;
            }
        }
        let existing = state.config_service.get_alias(&dev.sn);
        if existing == dev.sn {
            let new_alias = format!("{:02}号机", existing_count + synced + 1);
            state
                .device_manager
                .register_headset(&dev.sn, &new_alias, Some(&dev.model));
            synced += 1;
        }
    }
    Json(json!({ "success": true, "syncedCount": synced }))
}

#[derive(Deserialize)]
struct PkgQuery {
    target: String,
}

async fn get_packages(
    State(state): State<Arc<AppState>>,
    Query(q): Query<PkgQuery>,
) -> impl IntoResponse {
    if q.target.trim().is_empty() {
        return (StatusCode::BAD_REQUEST, Json(json!({ "success": false, "message": "目标设备不能为空" })));
    }
    let pkgs = state.adb_service.list_installed_packages(q.target.trim()).await;
    (StatusCode::OK, Json(json!({ "success": true, "packages": pkgs })))
}

fn parse_targets_str(raw: &str) -> Vec<String> {
    let mut list = Vec::new();
    for p in raw.split(|c| matches!(c, ',' | ';' | '\r' | '\n')) {
        let clean = AdbService::sanitize_shell_arg(p);
        if !clean.is_empty() && !list.contains(&clean) {
            list.push(clean);
        }
    }
    list
}

async fn resolve_target_list(state: &Arc<AppState>, provided: Vec<String>) -> Vec<String> {
    if !provided.is_empty() {
        return provided;
    }
    state
        .adb_service
        .get_connected_devices()
        .await
        .into_iter()
        .filter(|d| d.is_connected)
        .map(|d| d.target)
        .collect()
}

async fn upload_install(
    State(state): State<Arc<AppState>>,
    mut multipart: Multipart,
) -> impl IntoResponse {
    let session_dir = std::env::temp_dir()
        .join("PicoManager_Apks")
        .join(Uuid::new_v4().simple().to_string());
    if let Err(e) = tokio::fs::create_dir_all(&session_dir).await {
        return (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({ "success": false, "message": e.to_string() })));
    }

    let mut saved_apk: Option<PathBuf> = None;
    let mut targets_raw = String::new();

    while let Ok(Some(mut field)) = multipart.next_field().await {
        let name = field.name().unwrap_or("").to_string();
        if name == "file" {
            let file_name = field
                .file_name()
                .map(|s| Path::new(s).file_name().and_then(|n| n.to_str()).unwrap_or("app.apk").to_string())
                .unwrap_or_else(|| "app.apk".to_string());
            if !file_name.to_ascii_lowercase().ends_with(".apk") {
                let _ = tokio::fs::remove_dir_all(&session_dir).await;
                return (StatusCode::BAD_REQUEST, Json(json!({ "success": false, "message": "仅支持 .apk 安装包格式" })));
            }
            let dest = session_dir.join(&file_name);
            let Ok(mut f) = File::create(&dest).await else {
                let _ = tokio::fs::remove_dir_all(&session_dir).await;
                return (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({ "success": false, "message": "创建临时 APK 文件失败" })));
            };
            while let Ok(Some(chunk)) = field.chunk().await {
                if f.write_all(&chunk).await.is_err() {
                    let _ = tokio::fs::remove_dir_all(&session_dir).await;
                    return (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({ "success": false, "message": "写入上传文件失败" })));
                }
            }
            saved_apk = Some(dest);
        } else if name == "targets" {
            if let Ok(txt) = field.text().await {
                targets_raw = txt;
            }
        }
    }

    let Some(apk_path) = saved_apk else {
        let _ = tokio::fs::remove_dir_all(&session_dir).await;
        return (StatusCode::BAD_REQUEST, Json(json!({ "success": false, "message": "请先选择要上传安装的 APK 文件" })));
    };

    let targets = resolve_target_list(&state, parse_targets_str(&targets_raw)).await;
    if targets.is_empty() {
        let _ = tokio::fs::remove_dir_all(&session_dir).await;
        return (StatusCode::BAD_REQUEST, Json(json!({ "success": false, "message": "未检测到可安装的已连接 ADB 设备" })));
    }

    let results = run_parallel_install(&state, targets.clone(), apk_path).await;
    let _ = tokio::fs::remove_dir_all(&session_dir).await;
    (StatusCode::OK, Json(json!({ "success": true, "total": targets.len(), "results": results })))
}

async fn local_install(
    State(state): State<Arc<AppState>>,
    Json(req): Json<AdbLocalInstallRequest>,
) -> impl IntoResponse {
    let path = PathBuf::from(req.local_path.trim());
    if !path.exists() || !path.is_file() {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({ "success": false, "message": format!("服务器本地 APK 文件不存在: {}", req.local_path) })),
        );
    }
    if !req.local_path.trim().to_ascii_lowercase().ends_with(".apk") {
        return (StatusCode::BAD_REQUEST, Json(json!({ "success": false, "message": "仅支持 .apk 安装包格式" })));
    }
    let targets = resolve_target_list(&state, req.targets.unwrap_or_default()).await;
    if targets.is_empty() {
        return (StatusCode::BAD_REQUEST, Json(json!({ "success": false, "message": "未检测到可安装的已连接 ADB 设备" })));
    }
    let results = run_parallel_install(&state, targets.clone(), path).await;
    (StatusCode::OK, Json(json!({ "success": true, "total": targets.len(), "results": results })))
}

async fn run_parallel_install(
    state: &Arc<AppState>,
    targets: Vec<String>,
    apk_path: PathBuf,
) -> Vec<serde_json::Value> {
    let sem = Arc::new(Semaphore::new(4));
    let mut handles = Vec::new();
    for t in targets {
        let adb = state.adb_service.clone();
        let path = apk_path.clone();
        let sem_c = sem.clone();
        handles.push(tokio::spawn(async move {
            let _p = sem_c.acquire().await;
            let (ok, msg) = adb.install_apk(&t, &path).await;
            json!({ "target": t, "success": ok, "message": msg })
        }));
    }
    let mut results = Vec::new();
    for h in handles {
        if let Ok(v) = h.await {
            results.push(v);
        }
    }
    results
}

async fn uninstall(
    State(state): State<Arc<AppState>>,
    Json(req): Json<AdbUninstallRequest>,
) -> impl IntoResponse {
    if req.package_name.trim().is_empty() {
        return (StatusCode::BAD_REQUEST, Json(json!({ "success": false, "message": "包名不能为空" })));
    }
    let targets = resolve_target_list(&state, req.targets.unwrap_or_default()).await;
    if targets.is_empty() {
        return (StatusCode::BAD_REQUEST, Json(json!({ "success": false, "message": "未检测到可操作的已连接 ADB 设备" })));
    }
    let mut results = Vec::new();
    for t in &targets {
        let (ok, msg) = state.adb_service.uninstall_app(t, &req.package_name).await;
        results.push(json!({ "target": t, "success": ok, "message": msg }));
    }
    (StatusCode::OK, Json(json!({ "success": true, "total": targets.len(), "results": results })))
}

async fn upload_push(
    State(state): State<Arc<AppState>>,
    mut multipart: Multipart,
) -> impl IntoResponse {
    let session_dir = std::env::temp_dir()
        .join("PicoManager_Transfers")
        .join(Uuid::new_v4().simple().to_string());
    if let Err(e) = tokio::fs::create_dir_all(&session_dir).await {
        return (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({ "success": false, "message": e.to_string() })));
    }

    let mut saved_file: Option<(PathBuf, String)> = None;
    let mut remote_path = "/sdcard/Download/".to_string();
    let mut targets_raw = String::new();
    let mut overwrite = true;
    let mut flatten = false;

    while let Ok(Some(mut field)) = multipart.next_field().await {
        let name = field.name().unwrap_or("").to_string();
        match name.as_str() {
            "file" => {
                let file_name = field
                    .file_name()
                    .map(|s| Path::new(s).file_name().and_then(|n| n.to_str()).unwrap_or("upload.bin").to_string())
                    .unwrap_or_else(|| "upload.bin".to_string());
                let dest = session_dir.join(&file_name);
                let Ok(mut f) = File::create(&dest).await else {
                    let _ = tokio::fs::remove_dir_all(&session_dir).await;
                    return (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({ "success": false, "message": "创建临时文件失败" })));
                };
                while let Ok(Some(chunk)) = field.chunk().await {
                    if f.write_all(&chunk).await.is_err() {
                        let _ = tokio::fs::remove_dir_all(&session_dir).await;
                        return (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({ "success": false, "message": "写入上传文件失败" })));
                    }
                }
                saved_file = Some((dest, file_name));
            }
            "remotePath" => {
                if let Ok(txt) = field.text().await {
                    if !txt.trim().is_empty() {
                        remote_path = txt.trim().to_string();
                    }
                }
            }
            "targets" => {
                if let Ok(txt) = field.text().await {
                    targets_raw = txt;
                }
            }
            "overwrite" => {
                if let Ok(txt) = field.text().await {
                    overwrite = !txt.eq_ignore_ascii_case("false");
                }
            }
            "flatten" => {
                if let Ok(txt) = field.text().await {
                    flatten = txt.eq_ignore_ascii_case("true");
                }
            }
            _ => {}
        }
    }

    let Some((temp_file, orig_name)) = saved_file else {
        let _ = tokio::fs::remove_dir_all(&session_dir).await;
        return (StatusCode::BAD_REQUEST, Json(json!({ "success": false, "message": "请先选择要上传的文件" })));
    };

    let mut actual_push_path = temp_file.clone();
    if orig_name.to_ascii_lowercase().ends_with(".zip") {
        let stem = Path::new(&orig_name)
            .file_stem()
            .and_then(|s| s.to_str())
            .filter(|s| !s.is_empty())
            .unwrap_or("Archive");
        let extract_dir = if flatten {
            session_dir.join("unzipped")
        } else {
            session_dir.join(stem)
        };
        let tf = temp_file.clone();
        let ed = extract_dir.clone();
        if let Ok(Ok(())) = tokio::task::spawn_blocking(move || safe_zip::extract_zip_safe(&tf, &ed)).await {
            actual_push_path = extract_dir;
        }
    }

    let targets = resolve_target_list(&state, parse_targets_str(&targets_raw)).await;
    if targets.is_empty() {
        let _ = tokio::fs::remove_dir_all(&session_dir).await;
        return (StatusCode::BAD_REQUEST, Json(json!({ "success": false, "message": "未检测到可推送的已连接 ADB 设备" })));
    }

    let results = run_parallel_push(&state, targets.clone(), actual_push_path, remote_path, overwrite, flatten).await;
    let _ = tokio::fs::remove_dir_all(&session_dir).await;
    (StatusCode::OK, Json(json!({ "success": true, "total": targets.len(), "results": results })))
}

async fn local_push(
    State(state): State<Arc<AppState>>,
    Json(req): Json<AdbLocalPushRequest>,
) -> impl IntoResponse {
    let path = PathBuf::from(req.local_path.trim());
    if !path.exists() {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({ "success": false, "message": format!("服务器本地路径不存在: {}", req.local_path) })),
        );
    }
    let remote = req
        .remote_path
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| "/sdcard/Download/".to_string());
    let targets = resolve_target_list(&state, req.targets.unwrap_or_default()).await;
    if targets.is_empty() {
        return (StatusCode::BAD_REQUEST, Json(json!({ "success": false, "message": "未检测到可推送的已连接 ADB 设备" })));
    }
    let results = run_parallel_push(&state, targets.clone(), path, remote, req.overwrite, req.flatten).await;
    (StatusCode::OK, Json(json!({ "success": true, "total": targets.len(), "results": results })))
}

async fn run_parallel_push(
    state: &Arc<AppState>,
    targets: Vec<String>,
    local_path: PathBuf,
    remote_path: String,
    overwrite: bool,
    flatten: bool,
) -> Vec<serde_json::Value> {
    let sem = Arc::new(Semaphore::new(4));
    let mut handles = Vec::new();
    for t in targets {
        let adb = state.adb_service.clone();
        let lp = local_path.clone();
        let rp = remote_path.clone();
        let sem_c = sem.clone();
        handles.push(tokio::spawn(async move {
            let _p = sem_c.acquire().await;
            let (ok, msg) = adb.push_file_or_folder(&t, &lp, &rp, overwrite, flatten).await;
            json!({ "target": t, "success": ok, "message": msg })
        }));
    }
    let mut results = Vec::new();
    for h in handles {
        if let Ok(v) = h.await {
            results.push(v);
        }
    }
    results
}

async fn mkdir(
    State(state): State<Arc<AppState>>,
    Json(req): Json<AdbMkdirRequest>,
) -> impl IntoResponse {
    if req.remote_path.trim().is_empty() {
        return (StatusCode::BAD_REQUEST, Json(json!({ "success": false, "message": "远程目录路径不能为空" })));
    }
    let targets = resolve_target_list(&state, req.targets.unwrap_or_default()).await;
    if targets.is_empty() {
        return (StatusCode::BAD_REQUEST, Json(json!({ "success": false, "message": "未检测到可操作的已连接 ADB 设备" })));
    }
    let mut results = Vec::new();
    for t in &targets {
        let (ok, msg) = state.adb_service.create_folder(t, &req.remote_path).await;
        results.push(json!({ "target": t, "success": ok, "message": msg }));
    }
    (StatusCode::OK, Json(json!({ "success": true, "total": targets.len(), "results": results })))
}

async fn set_screen(
    State(state): State<Arc<AppState>>,
    Json(req): Json<AdbScreenRequest>,
) -> impl IntoResponse {
    if req.target.trim().is_empty() {
        return (StatusCode::BAD_REQUEST, Json(json!({ "success": false })));
    }
    state.adb_service.set_screen_state(req.target.trim(), req.screen_on).await;
    (StatusCode::OK, Json(json!({ "success": true })))
}
