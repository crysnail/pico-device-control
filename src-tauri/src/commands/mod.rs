use crate::models::RuntimeInfo;
use crate::services::network_service;
use crate::state::AppState;
use std::sync::Arc;
use tauri::{command, AppHandle, Manager, State};

#[command]
pub async fn get_runtime_info(state: State<'_, Arc<AppState>>) -> Result<RuntimeInfo, String> {
    let lan_urls = network_service::get_lan_ips(state.web_port)
        .into_iter()
        .map(|i| i.web_url)
        .collect();

    Ok(RuntimeInfo {
        version: format!("v{}", env!("CARGO_PKG_VERSION")),
        web_port: state.web_port,
        udp_port: state.udp_port,
        config_path: state.config_service.config_path().to_string_lossy().to_string(),
        lan_urls,
    })
}

#[command]
pub async fn reconnect_sdk(state: State<'_, Arc<AppState>>) -> Result<bool, String> {
    Ok(state.sdk_service.reconnect_sdk().await)
}

#[command]
pub fn hide_main_window(app: AppHandle) -> Result<(), String> {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.hide();
    }
    Ok(())
}

#[command]
pub fn exit_application(app: AppHandle, state: State<'_, Arc<AppState>>) -> Result<(), String> {
    state.sdk_service.shutdown();
    app.exit(0);
    std::process::exit(0);
}

#[command]
pub fn minimize_window(app: AppHandle) -> Result<(), String> {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.minimize();
    }
    Ok(())
}

#[command]
pub fn toggle_maximize_window(app: AppHandle) -> Result<(), String> {
    if let Some(window) = app.get_webview_window("main") {
        if window.is_maximized().unwrap_or(false) {
            let _ = window.unmaximize();
        } else {
            let _ = window.maximize();
        }
    }
    Ok(())
}

#[command]
pub fn start_dragging_window(app: AppHandle) -> Result<(), String> {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.start_dragging();
    }
    Ok(())
}
