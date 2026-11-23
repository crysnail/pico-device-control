use crate::models::{
    AppProject, BindHeadsetRequest, DeviceGroup, DeviceProjectRequest, SetAliasRequest,
    SetServiceModeRequest, SystemSettingsRequest, UdpCommandMessage,
};
use crate::services::service_mode;
use crate::state::AppState;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::routing::{delete, get, post};
use axum::{Json, Router};
use serde_json::json;
use std::sync::Arc;
use tokio::net::UdpSocket;
use tokio::time::{timeout, Duration};
use uuid::Uuid;

pub fn router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/api/config", get(get_config))
        .route("/api/config/projects", post(save_project))
        .route("/api/config/projects/reorder", post(reorder_projects))
        .route("/api/config/projects/{id}", delete(delete_project))
        .route("/api/config/groups", post(save_group))
        .route("/api/config/groups/{id}", delete(delete_group))
        .route("/api/config/alias", post(set_alias))
        .route("/api/config/bind-headset", post(bind_headset))
        .route("/api/config/bind-headset/{sn}", delete(unbind_headset))
        .route("/api/config/device-project", post(set_device_project))
        .route("/api/config/test-udp", post(test_udp))
        .route("/api/config/system", post(update_system_settings))
        .route("/api/config/seed-demo", post(seed_demo))
        .route("/api/config/clear-demo", post(clear_demo))
        .route("/api/config/pico-service-mode", get(get_pico_service_mode).post(set_pico_service_mode))
        .route("/api/config/pico-service-mode/restart", post(restart_pico_service))
}

async fn get_config(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    Json(state.config_service.get_config())
}

async fn save_project(
    State(state): State<Arc<AppState>>,
    Json(mut project): Json<AppProject>,
) -> impl IntoResponse {
    if project.name.trim().is_empty() || project.package_name.trim().is_empty() {
        return (StatusCode::BAD_REQUEST, Json(json!({ "message": "项目名称和包名不能为空" })));
    }
    if project.icon.trim().is_empty() {
        project.icon = "🎮".to_string();
    }
    if project.id.trim().is_empty() {
        project.id = format!("proj-{}", &Uuid::new_v4().simple().to_string()[..8]);
    }
    let saved = project.clone();
    let _ = state.config_service.update_config(|cfg| {
        if let Some(existing) = cfg.projects.iter_mut().find(|p| p.id == saved.id) {
            *existing = saved.clone();
        } else {
            cfg.projects.push(saved.clone());
        }
    });
    (StatusCode::OK, Json(json!({ "success": true, "project": project })))
}

#[derive(serde::Deserialize)]
struct ReorderProjectsRequest {
    ids: Vec<String>,
}

async fn reorder_projects(
    State(state): State<Arc<AppState>>,
    Json(req): Json<ReorderProjectsRequest>,
) -> impl IntoResponse {
    if req.ids.is_empty() {
        return (StatusCode::BAD_REQUEST, Json(json!({ "success": false, "message": "排序列表不能为空" })));
    }
    let _ = state.config_service.update_config(|cfg| {
        let mut reordered = Vec::with_capacity(cfg.projects.len());
        for id in &req.ids {
            if let Some(pos) = cfg.projects.iter().position(|p| &p.id == id) {
                reordered.push(cfg.projects.remove(pos));
            }
        }
        reordered.append(&mut cfg.projects);
        cfg.projects = reordered;
    });
    (StatusCode::OK, Json(json!({ "success": true })))
}

async fn delete_project(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    let mut removed = false;
    let _ = state.config_service.update_config(|cfg| {
        let before = cfg.projects.len();
        cfg.projects.retain(|p| p.id != id);
        removed = cfg.projects.len() < before;
    });
    if removed {
        (StatusCode::OK, Json(json!({ "success": true })))
    } else {
        (StatusCode::NOT_FOUND, Json(json!({ "message": "项目不存在" })))
    }
}

async fn save_group(
    State(state): State<Arc<AppState>>,
    Json(mut group): Json<DeviceGroup>,
) -> impl IntoResponse {
    if group.name.trim().is_empty() {
        return (StatusCode::BAD_REQUEST, Json(json!({ "message": "分组名称不能为空" })));
    }
    group.name = group.name.trim().to_string();
    group.default_project_id = group.default_project_id.trim().to_string();
    if group.id.trim().is_empty() {
        group.id = format!("group-{}", &Uuid::new_v4().simple().to_string()[..8]);
    }
    let saved = group.clone();
    let _ = state.config_service.update_config(|cfg| {
        if let Some(existing) = cfg.groups.iter_mut().find(|g| g.id == saved.id) {
            *existing = saved.clone();
        } else {
            cfg.groups.push(saved.clone());
        }
    });
    (StatusCode::OK, Json(json!({ "success": true, "group": group })))
}

async fn delete_group(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    let mut removed = false;
    let _ = state.config_service.update_config(|cfg| {
        let before = cfg.groups.len();
        cfg.groups.retain(|g| g.id != id);
        removed = cfg.groups.len() < before;
    });
    if removed {
        (StatusCode::OK, Json(json!({ "success": true })))
    } else {
        (StatusCode::NOT_FOUND, Json(json!({ "message": "分组不存在" })))
    }
}

async fn set_alias(
    State(state): State<Arc<AppState>>,
    Json(req): Json<SetAliasRequest>,
) -> impl IntoResponse {
    if req.sn.trim().is_empty() {
        return (StatusCode::BAD_REQUEST, Json(json!({ "message": "SN 不能为空" })));
    }
    state.device_manager.set_device_alias(req.sn.trim(), req.alias.trim());
    (StatusCode::OK, Json(json!({ "success": true, "sn": req.sn, "alias": req.alias })))
}

async fn bind_headset(
    State(state): State<Arc<AppState>>,
    Json(req): Json<BindHeadsetRequest>,
) -> impl IntoResponse {
    let sn = req.sn.trim().to_string();
    if sn.is_empty() {
        return (StatusCode::BAD_REQUEST, Json(json!({ "message": "SN 序列号不能为空" })));
    }
    let alias = if req.alias.trim().is_empty() {
        sn.clone()
    } else {
        req.alias.trim().to_string()
    };
    state
        .device_manager
        .register_headset(&sn, &alias, req.model.as_deref());
    (StatusCode::OK, Json(json!({ "success": true, "sn": sn, "alias": alias })))
}

async fn unbind_headset(
    State(state): State<Arc<AppState>>,
    Path(sn): Path<String>,
) -> impl IntoResponse {
    if sn.trim().is_empty() {
        return (StatusCode::BAD_REQUEST, Json(json!({ "success": false })));
    }
    state.device_manager.unbind_headset(sn.trim());
    (StatusCode::OK, Json(json!({ "success": true })))
}

async fn set_device_project(
    State(state): State<Arc<AppState>>,
    Json(req): Json<DeviceProjectRequest>,
) -> impl IntoResponse {
    let key = req.device_key.trim().to_string();
    if key.is_empty() {
        return (StatusCode::BAD_REQUEST, Json(json!({ "message": "机号/SN 标识不能为空" })));
    }
    let pkg = req.package_or_project_id.trim().to_string();
    if pkg.is_empty() {
        state.config_service.set_device_default_project(&key, "");
        return (StatusCode::OK, Json(json!({ "success": true, "removed": true })));
    }

    state.config_service.set_device_default_project(&key, &pkg);
    if let Some(dev) = state.device_manager.resolve_devices(Some(&key)).into_iter().next() {
        if !dev.sn.is_empty() && dev.sn != key {
            state.config_service.set_device_default_project(&dev.sn, &pkg);
        }
        if !dev.alias.is_empty() && dev.alias != key {
            state.config_service.set_device_default_project(&dev.alias, &pkg);
        }
        let digits: String = dev.alias.chars().filter(|c| c.is_ascii_digit()).collect();
        if !digits.is_empty() && digits != key {
            state.config_service.set_device_default_project(&digits, &pkg);
        }
    }

    (StatusCode::OK, Json(json!({ "success": true, "deviceKey": key, "package": pkg })))
}

async fn test_udp(
    State(state): State<Arc<AppState>>,
    Json(msg): Json<UdpCommandMessage>,
) -> impl IntoResponse {
    let port = state.udp_port;
    let payload = match serde_json::to_string(&msg) {
        Ok(s) => s,
        Err(e) => return (StatusCode::BAD_REQUEST, Json(json!({ "success": false, "message": e.to_string() }))),
    };

    match UdpSocket::bind("127.0.0.1:0").await {
        Ok(sock) => {
            let target = format!("127.0.0.1:{}", port);
            if let Err(e) = sock.send_to(payload.as_bytes(), &target).await {
                return (StatusCode::BAD_REQUEST, Json(json!({ "success": false, "message": e.to_string() })));
            }
            let mut buf = vec![0u8; 8192];
            match timeout(Duration::from_millis(1500), sock.recv_from(&mut buf)).await {
                Ok(Ok((n, _))) => {
                    let ack = String::from_utf8_lossy(&buf[..n]).to_string();
                    (StatusCode::OK, Json(json!({ "success": true, "ack": ack })))
                }
                _ => (StatusCode::OK, Json(json!({ "success": true, "ack": "已下发 UDP 报文" }))),
            }
        }
        Err(e) => (StatusCode::BAD_REQUEST, Json(json!({ "success": false, "message": e.to_string() }))),
    }
}

async fn update_system_settings(
    State(state): State<Arc<AppState>>,
    Json(req): Json<SystemSettingsRequest>,
) -> impl IntoResponse {
    let _ = state.config_service.update_config(|cfg| {
        cfg.auto_lock_on_find = req.auto_lock_on_find;
        cfg.enable_simulator_fallback = req.enable_simulator_fallback;
        if req.udp_port > 0 {
            cfg.udp_port = req.udp_port;
        }
        cfg.enable_udp_service = req.enable_udp_service;
    });
    Json(json!({ "success": true }))
}

async fn seed_demo() -> impl IntoResponse {
    Json(json!({ "success": true, "count": 0 }))
}

async fn clear_demo(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    state.device_manager.clear_demo_devices();
    Json(json!({ "success": true }))
}

async fn get_pico_service_mode() -> impl IntoResponse {
    let status = tokio::task::spawn_blocking(service_mode::get_status)
        .await
        .unwrap_or_else(|_| service_mode::get_status());
    Json(status)
}

async fn set_pico_service_mode(Json(req): Json<SetServiceModeRequest>) -> impl IntoResponse {
    let res = tokio::task::spawn_blocking(move || service_mode::set_service_mode(req.enable, req.restart_process))
        .await
        .unwrap_or_else(|_| service_mode::set_service_mode(req.enable, false));
    Json(res)
}

async fn restart_pico_service() -> impl IntoResponse {
    let res = tokio::task::spawn_blocking(service_mode::restart_process)
        .await
        .unwrap_or_else(|_| service_mode::restart_process());
    Json(res)
}
