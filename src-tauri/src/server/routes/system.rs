use crate::services::network_service;
use crate::state::AppState;
use axum::extract::State;
use axum::response::IntoResponse;
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::process::Command;
use std::sync::Arc;

#[cfg(windows)]
use std::os::windows::process::CommandExt;

#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x08000000;

pub fn router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/api/system/network", get(get_network_info))
        .route("/api/system/health", get(health_check))
        .route("/api/system/firewall-status", get(get_firewall_status))
        .route("/api/system/firewall-setup", post(setup_firewall_and_network))
}

async fn get_network_info(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    let port = state.web_port;
    let ips = network_service::get_lan_ips(port);
    let primary_url = ips
        .first()
        .map(|i| i.web_url.clone())
        .unwrap_or_else(|| format!("http://localhost:{}", port));

    Json(json!({
        "webPort": port,
        "udpPort": state.udp_port,
        "primaryUrl": primary_url,
        "ips": ips
    }))
}

async fn health_check(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    Json(json!({
        "status": "ok",
        "version": env!("CARGO_PKG_VERSION"),
        "webPort": state.web_port,
        "udpPort": state.udp_port,
        "sdkActive": state.device_manager.is_sdk_active()
    }))
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct NetProfileItem {
    name: String,
    interface_alias: String,
    network_category: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct FirewallStatusResponse {
    is_private_network: bool,
    has_public_network: bool,
    tcp_8999_allowed: bool,
    udp_8998_allowed: bool,
    profiles: Vec<NetProfileItem>,
    summary: String,
}

fn run_hidden_powershell(script: &str) -> Result<String, String> {
    let mut cmd = Command::new("powershell");
    cmd.args(["-NoProfile", "-ExecutionPolicy", "Bypass", "-Command", script]);
    #[cfg(windows)]
    cmd.creation_flags(CREATE_NO_WINDOW);
    let out = cmd.output().map_err(|e| format!("执行 PowerShell 失败: {}", e))?;
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

fn inspect_firewall_sync(web_port: u16, udp_port: u16) -> FirewallStatusResponse {
    let script = format!(
        r#"
$ErrorActionPreference = 'SilentlyContinue'
$profiles = @(Get-NetConnectionProfile | Select-Object @{{n='name';e={{$_.Name}}}}, @{{n='interfaceAlias';e={{$_.InterfaceAlias}}}}, @{{n='networkCategory';e={{$_.NetworkCategory.ToString()}}}})
$tcpRule = Get-NetFirewallRule -DisplayName 'PicoManager TCP {0}' | Where-Object {{ $_.Enabled -eq 'True' -and $_.Action -eq 'Allow' -and $_.Direction -eq 'Inbound' }}
if (-not $tcpRule) {{
    $tcpRule = Get-NetFirewallPortFilter -Protocol TCP | Where-Object {{ $_.LocalPort -eq '{0}' }} | ForEach-Object {{ Get-NetFirewallRule -AssociatedNetFirewallPortFilter $_ }} | Where-Object {{ $_.Enabled -eq 'True' -and $_.Action -eq 'Allow' -and $_.Direction -eq 'Inbound' }}
}}
$udpRule = Get-NetFirewallRule -DisplayName 'PicoManager UDP {1}' | Where-Object {{ $_.Enabled -eq 'True' -and $_.Action -eq 'Allow' -and $_.Direction -eq 'Inbound' }}
if (-not $udpRule) {{
    $udpRule = Get-NetFirewallPortFilter -Protocol UDP | Where-Object {{ $_.LocalPort -eq '{1}' }} | ForEach-Object {{ Get-NetFirewallRule -AssociatedNetFirewallPortFilter $_ }} | Where-Object {{ $_.Enabled -eq 'True' -and $_.Action -eq 'Allow' -and $_.Direction -eq 'Inbound' }}
}}
[pscustomobject]@{{
    profiles = $profiles
    tcpAllowed = [bool]($tcpRule)
    udpAllowed = [bool]($udpRule)
}} | ConvertTo-Json -Depth 4 -Compress
"#,
        web_port, udp_port
    );

    let raw = run_hidden_powershell(&script).unwrap_or_default();
    let parsed: serde_json::Value = serde_json::from_str(&raw).unwrap_or_else(|_| json!({}));
    let profiles: Vec<NetProfileItem> = serde_json::from_value(
        parsed.get("profiles").cloned().unwrap_or_else(|| json!([])),
    )
    .unwrap_or_default();

    let has_public = profiles
        .iter()
        .any(|p| p.network_category.eq_ignore_ascii_case("Public"));
    let is_private = !profiles.is_empty() && !has_public;
    let tcp_ok = parsed.get("tcpAllowed").and_then(|v| v.as_bool()).unwrap_or(false);
    let udp_ok = parsed.get("udpAllowed").and_then(|v| v.as_bool()).unwrap_or(false);

    let summary = if is_private && tcp_ok && udp_ok {
        "专用网络与防火墙端口 (8999/TCP, 8998/UDP) 已全部就绪".to_string()
    } else {
        let mut issues = Vec::new();
        if has_public {
            issues.push("当前存在公用网络");
        }
        if !tcp_ok {
            issues.push("8999/TCP 未放行");
        }
        if !udp_ok {
            issues.push("8998/UDP 未放行");
        }
        format!("检测到待配置项: {}", issues.join("、"))
    };

    FirewallStatusResponse {
        is_private_network: is_private,
        has_public_network: has_public,
        tcp_8999_allowed: tcp_ok,
        udp_8998_allowed: udp_ok,
        profiles,
        summary,
    }
}

async fn get_firewall_status(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    let web_port = state.web_port;
    let udp_port = state.udp_port;
    let res = tokio::task::spawn_blocking(move || inspect_firewall_sync(web_port, udp_port))
        .await
        .unwrap_or_default();
    Json(res)
}

async fn setup_firewall_and_network(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    let web_port = state.web_port;
    let udp_port = state.udp_port;
    let result = tokio::task::spawn_blocking(move || {
        let tmp_ps1 = std::env::temp_dir().join(format!("pico_fw_setup_{}.ps1", std::process::id()));
        let script_body = format!(
            "$ErrorActionPreference = 'SilentlyContinue'\r\n\
             Get-NetConnectionProfile | Where-Object {{ $_.NetworkCategory -ne 'DomainAuthenticated' -and $_.NetworkCategory -ne 'Private' }} | Set-NetConnectionProfile -NetworkCategory Private\r\n\
             Remove-NetFirewallRule -DisplayName 'PicoManager TCP {0}' -ErrorAction SilentlyContinue\r\n\
             Remove-NetFirewallRule -DisplayName 'PicoManager UDP {1}' -ErrorAction SilentlyContinue\r\n\
             New-NetFirewallRule -DisplayName 'PicoManager TCP {0}' -Direction Inbound -Protocol TCP -LocalPort {0} -Action Allow -Profile Any -Enabled True | Out-Null\r\n\
             New-NetFirewallRule -DisplayName 'PicoManager UDP {1}' -Direction Inbound -Protocol UDP -LocalPort {1} -Action Allow -Profile Any -Enabled True | Out-Null\r\n",
            web_port, udp_port
        );
        if let Err(e) = std::fs::write(&tmp_ps1, script_body) {
            let status = inspect_firewall_sync(web_port, udp_port);
            return (false, status, Some(format!("写入临时配置脚本失败: {}", e)));
        }

        let tmp_path_str = tmp_ps1.to_string_lossy().replace('\'', "''");
        let launcher = format!(
            "$p = Start-Process powershell -Verb RunAs -WindowStyle Hidden -Wait -PassThru -ArgumentList @('-NoProfile','-ExecutionPolicy','Bypass','-File','{}'); if ($p -and $p.ExitCode -ne 0) {{ exit $p.ExitCode }}",
            tmp_path_str
        );
        let run_res = run_hidden_powershell(&launcher);
        let _ = std::fs::remove_file(&tmp_ps1);

        let status = inspect_firewall_sync(web_port, udp_port);
        let ok = run_res.is_ok() && status.tcp_8999_allowed && status.udp_8998_allowed;
        (ok, status, run_res.err())
    })
    .await;

    match result {
        Ok((ok, status, err)) => Json(json!({
            "success": ok,
            "status": status,
            "message": if ok {
                "已成功设为专用网络并放行 8999/TCP 与 8998/UDP 端口".to_string()
            } else {
                err.unwrap_or_else(|| "请确认在系统 UAC 弹窗中点击「是」授权管理员权限".to_string())
            }
        })),
        Err(e) => Json(json!({
            "success": false,
            "message": format!("执行异常: {}", e)
        })),
    }
}
