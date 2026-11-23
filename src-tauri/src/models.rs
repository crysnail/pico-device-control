use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

fn default_vr_app() -> String {
    "vr_app".to_string()
}

fn default_duration() -> i32 {
    15
}

fn default_icon() -> String {
    "🎮".to_string()
}

fn default_web_port() -> u16 {
    8999
}

fn default_udp_port() -> u16 {
    8998
}

fn default_true() -> bool {
    true
}

fn default_adb_port() -> u16 {
    5555
}

fn default_volume() -> i32 {
    70
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppProject {
    #[serde(default, alias = "Id")]
    pub id: String,
    #[serde(default, alias = "Name")]
    pub name: String,
    #[serde(default = "default_vr_app", alias = "Type")]
    pub r#type: String,
    #[serde(default, alias = "PackageName")]
    pub package_name: String,
    #[serde(default = "default_duration", alias = "DurationMinutes")]
    pub duration_minutes: i32,
    #[serde(default = "default_icon", alias = "Icon")]
    pub icon: String,
    #[serde(default, alias = "BannerUrl")]
    pub banner_url: String,
    #[serde(default, alias = "ExtraJson")]
    pub extra_json: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceGroup {
    #[serde(default, alias = "Id")]
    pub id: String,
    #[serde(default, alias = "Name")]
    pub name: String,
    #[serde(default, alias = "DefaultProjectId")]
    pub default_project_id: String,
    #[serde(default, alias = "DeviceIdentifiers")]
    pub device_identifiers: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SystemConfig {
    #[serde(default = "default_web_port", alias = "WebPort")]
    pub web_port: u16,
    #[serde(default = "default_udp_port", alias = "UdpPort")]
    pub udp_port: u16,
    #[serde(default = "default_true", alias = "EnableUdpService")]
    pub enable_udp_service: bool,
    #[serde(default = "default_true", alias = "AutoLockOnFind")]
    pub auto_lock_on_find: bool,
    #[serde(default, alias = "EnableSimulatorFallback")]
    pub enable_simulator_fallback: bool,
    #[serde(default, alias = "AdbPath")]
    pub adb_path: Option<String>,
    #[serde(default, alias = "Projects")]
    pub projects: Vec<AppProject>,
    #[serde(default, alias = "Groups")]
    pub groups: Vec<DeviceGroup>,
    #[serde(default, alias = "HeadsetAliases")]
    pub headset_aliases: HashMap<String, String>,
    #[serde(default, alias = "DeviceDefaultProjects")]
    pub device_default_projects: HashMap<String, String>,
}

impl Default for SystemConfig {
    fn default() -> Self {
        Self {
            web_port: 8999,
            udp_port: 8998,
            enable_udp_service: true,
            auto_lock_on_find: true,
            enable_simulator_fallback: false,
            adb_path: None,
            projects: vec![
                AppProject {
                    id: "proj-1".into(),
                    name: "深海探秘大冒险".into(),
                    r#type: "vr_app".into(),
                    package_name: "com.vr.ocean".into(),
                    duration_minutes: 12,
                    icon: "🐋".into(),
                    banner_url: String::new(),
                    extra_json: String::new(),
                },
                AppProject {
                    id: "proj-2".into(),
                    name: "重返恐龙纪元".into(),
                    r#type: "vr_app".into(),
                    package_name: "com.vr.dino".into(),
                    duration_minutes: 10,
                    icon: "🦖".into(),
                    banner_url: String::new(),
                    extra_json: String::new(),
                },
                AppProject {
                    id: "proj-3".into(),
                    name: "PICO 官方全景影院".into(),
                    r#type: "vr_app".into(),
                    package_name: "com.pico.vrplayer".into(),
                    duration_minutes: 20,
                    icon: "🎬".into(),
                    banner_url: String::new(),
                    extra_json: String::new(),
                },
            ],
            groups: Vec::new(),
            headset_aliases: HashMap::new(),
            device_default_projects: HashMap::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PicoDevice {
    pub sn: String,
    pub alias: String,
    pub suite_no: String,
    pub model: String,
    pub online: bool,
    pub locked: bool,
    pub battery: u32,
    pub is_charging: bool,
    pub screen_on: bool,
    pub is_worn: bool,
    pub controller_left_battery: u32,
    pub controller_right_battery: u32,
    pub current_app: String,
    pub app_title: String,
    pub tracking_lost: bool,
    pub is_streaming: bool,
    pub volume: u32,
    pub is_finding: bool,
    pub is_in_voice_call: bool,
    pub ip_address: String,
    pub last_seen: Option<DateTime<Utc>>,
    pub is_demo: bool,
    pub display_name: String,
}

impl PicoDevice {
    pub fn new(sn: String, alias: String) -> Self {
        let mut dev = Self {
            sn,
            alias,
            suite_no: String::new(),
            model: "PICO VR".to_string(),
            online: false,
            locked: false,
            battery: 0,
            is_charging: false,
            screen_on: true,
            is_worn: false,
            controller_left_battery: 0,
            controller_right_battery: 0,
            current_app: "系统桌面".to_string(),
            app_title: "系统桌面".to_string(),
            tracking_lost: false,
            is_streaming: false,
            volume: 70,
            is_finding: false,
            is_in_voice_call: false,
            ip_address: "127.0.0.1".to_string(),
            last_seen: None,
            is_demo: false,
            display_name: String::new(),
        };
        dev.refresh_display_name();
        dev
    }

    pub fn refresh_display_name(&mut self) {
        if !self.alias.trim().is_empty() {
            self.display_name = self.alias.clone();
        } else if !self.suite_no.trim().is_empty() {
            self.display_name = self.suite_no.clone();
        } else {
            self.display_name = self.sn.clone();
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum StreamPhase {
    Standby,
    SuiteOffline,
    Requesting,
    WaitingFirstFrame,
    Streaming,
    Error,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceStreamStatus {
    pub sn: String,
    pub is_requested: bool,
    pub phase: StreamPhase,
    pub last_start_result: i32,
    pub diagnostic_message: String,
    pub requested_time: Option<DateTime<Utc>>,
    pub first_frame_time: Option<DateTime<Utc>>,
    pub last_frame_time: Option<DateTime<Utc>>,
    pub total_frames: u64,
    pub last_width: u32,
    pub last_height: u32,
    pub pix_format: i32,
    pub fps: f64,
}

impl DeviceStreamStatus {
    pub fn new(sn: String) -> Self {
        Self {
            sn,
            is_requested: false,
            phase: StreamPhase::Standby,
            last_start_result: 999,
            diagnostic_message: "待命中 (点击开启实时监看)".to_string(),
            requested_time: None,
            first_frame_time: None,
            last_frame_time: None,
            total_frames: 0,
            last_width: 0,
            last_height: 0,
            pix_format: 0,
            fps: 0.0,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SuiteHealthStatus {
    pub is_process_running: bool,
    pub is_port_open: bool,
    pub is_sdk_active: bool,
    pub is_server_connected: bool,
    pub listen_port: u16,
    pub status_summary: String,
    pub advice: String,
    pub check_time: DateTime<Utc>,
    pub streams: HashMap<String, DeviceStreamStatus>,
}

impl Default for SuiteHealthStatus {
    fn default() -> Self {
        Self {
            is_process_running: false,
            is_port_open: false,
            is_sdk_active: false,
            is_server_connected: false,
            listen_port: 60061,
            status_summary: String::new(),
            advice: String::new(),
            check_time: Utc::now(),
            streams: HashMap::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AdbDeviceInfo {
    pub target: String,
    pub ip: String,
    pub port: u16,
    pub status: String,
    pub sn: String,
    pub model: String,
    pub battery: i32,
    pub is_wireless: bool,
    pub is_connected: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PicoServiceStatus {
    pub found_ini: bool,
    pub ini_path: String,
    pub run_as_service: bool,
    pub is_process_running: bool,
    pub process_id: u32,
    pub process_path: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PicoServiceResult {
    pub success: bool,
    pub message: String,
    pub status: PicoServiceStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LanIpInfo {
    pub ip: String,
    pub interface_name: String,
    pub description: String,
    pub web_url: String,
    pub is_preferred: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OtaVersionInfo {
    pub version: String,
    pub build_time: String,
    pub channel: String,
    pub os_platform: String,
    pub runtime: String,
}

// ======================== HTTP Request DTOs ========================

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VolumeRequest {
    #[serde(default = "default_volume")]
    pub volume: i32,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StartAppRequest {
    #[serde(default)]
    pub package_name: String,
    #[serde(default)]
    pub extra_json: Option<String>,
}

#[derive(Debug, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct StopAppRequest {
    #[serde(default)]
    pub package_name: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SetAliasRequest {
    #[serde(default, alias = "SN")]
    pub sn: String,
    #[serde(default)]
    pub alias: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScreenStateRequest {
    #[serde(default = "default_true")]
    pub screen_on: bool,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BatchCommandRequest {
    #[serde(default)]
    pub sn_list: Vec<String>,
    #[serde(default)]
    pub package_name: String,
    #[serde(default)]
    pub extra_json: String,
    #[serde(default = "default_volume")]
    pub volume: i32,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BatchScreenRequest {
    #[serde(default)]
    pub sn_list: Vec<String>,
    #[serde(default = "default_true")]
    pub screen_on: bool,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BindHeadsetRequest {
    #[serde(default, alias = "SN")]
    pub sn: String,
    #[serde(default)]
    pub alias: String,
    #[serde(default)]
    pub model: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceProjectRequest {
    #[serde(default)]
    pub device_key: String,
    #[serde(default)]
    pub package_or_project_id: String,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UdpCommandMessage {
    #[serde(default)]
    pub device_no: Option<String>,
    #[serde(default)]
    pub action: Option<String>,
    #[serde(default)]
    pub package_name: Option<String>,
    #[serde(default)]
    pub extra_json: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SystemSettingsRequest {
    #[serde(default = "default_true")]
    pub auto_lock_on_find: bool,
    #[serde(default)]
    pub enable_simulator_fallback: bool,
    #[serde(default = "default_udp_port")]
    pub udp_port: u16,
    #[serde(default = "default_true")]
    pub enable_udp_service: bool,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SetServiceModeRequest {
    #[serde(default = "default_true")]
    pub enable: bool,
    #[serde(default)]
    pub restart_process: bool,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AdbConnectRequest {
    #[serde(default)]
    pub ip: String,
    #[serde(default = "default_adb_port")]
    pub port: u16,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AdbTargetRequest {
    #[serde(default)]
    pub target: String,
}

#[derive(Debug, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct AdbScanRequest {
    #[serde(default)]
    pub subnet: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AdbLocalInstallRequest {
    #[serde(default)]
    pub local_path: String,
    #[serde(default)]
    pub targets: Option<Vec<String>>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AdbUninstallRequest {
    #[serde(default)]
    pub package_name: String,
    #[serde(default)]
    pub targets: Option<Vec<String>>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AdbLocalPushRequest {
    #[serde(default)]
    pub local_path: String,
    #[serde(default)]
    pub remote_path: Option<String>,
    #[serde(default)]
    pub targets: Option<Vec<String>>,
    #[serde(default = "default_true")]
    pub overwrite: bool,
    #[serde(default)]
    pub flatten: bool,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AdbMkdirRequest {
    #[serde(default)]
    pub remote_path: String,
    #[serde(default)]
    pub targets: Option<Vec<String>>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AdbScreenRequest {
    #[serde(default)]
    pub target: String,
    #[serde(default = "default_true")]
    pub screen_on: bool,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OtaDownloadRequest {
    #[serde(default)]
    pub url: String,
}

/// 实时 WebSocket 广播事件信封
#[derive(Debug, Clone, Serialize)]
pub struct WsEvent {
    #[serde(rename = "type")]
    pub event_type: String,
    pub data: serde_json::Value,
}

/// 桌面端运行时状态信息（供 Tauri 前端动态获取端口与地址）
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeInfo {
    pub version: String,
    pub web_port: u16,
    pub udp_port: u16,
    pub config_path: String,
    pub lan_urls: Vec<String>,
}
