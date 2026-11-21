use crate::models::{PicoDevice, StreamPhase, WsEvent};
use crate::services::adb_service::AdbService;
use crate::services::config_service::ConfigService;
use crate::services::pico_sdk_ffi::{self, MonitorQuality};
use crate::services::screen_monitor::ScreenMonitorService;
use chrono::Utc;
use dashmap::DashMap;
use std::collections::HashSet;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::sync::broadcast;
use tokio::time::{sleep, Duration};

pub struct PicoDeviceManager {
    devices: DashMap<String, PicoDevice>,
    ws_tx: broadcast::Sender<WsEvent>,
    config_service: Arc<ConfigService>,
    screen_monitor: Arc<ScreenMonitorService>,
    adb_service: Arc<AdbService>,
    sdk_active: AtomicBool,
}

impl PicoDeviceManager {
    pub fn new(
        ws_tx: broadcast::Sender<WsEvent>,
        config_service: Arc<ConfigService>,
        screen_monitor: Arc<ScreenMonitorService>,
        adb_service: Arc<AdbService>,
    ) -> Self {
        Self {
            devices: DashMap::new(),
            ws_tx,
            config_service,
            screen_monitor,
            adb_service,
            sdk_active: AtomicBool::new(false),
        }
    }

    pub fn set_sdk_active(&self, active: bool) {
        self.sdk_active.store(active, Ordering::SeqCst);
    }

    pub fn is_sdk_active(&self) -> bool {
        self.sdk_active.load(Ordering::SeqCst)
    }

    fn emit_ws(&self, event_type: &str, data: serde_json::Value) {
        let _ = self.ws_tx.send(WsEvent {
            event_type: event_type.to_string(),
            data,
        });
    }

    pub fn get_all_devices(&self) -> Vec<PicoDevice> {
        let mut list: Vec<PicoDevice> = self.devices.iter().map(|e| e.value().clone()).collect();
        list.sort_by(|a, b| a.alias.cmp(&b.alias).then(a.sn.cmp(&b.sn)));
        list
    }

    pub fn get_device(&self, sn: &str) -> Option<PicoDevice> {
        self.devices.get(sn).map(|d| d.clone())
    }

    pub fn initialize_configured_devices(&self) {
        let cfg = self.config_service.get_config();
        for (sn, alias) in cfg.headset_aliases {
            let sn = sn.trim().to_string();
            let alias = alias.trim().to_string();
            if sn.is_empty() {
                continue;
            }
            self.devices
                .entry(sn.clone())
                .or_insert_with(|| PicoDevice::new(sn, alias));
        }
    }

    pub fn resolve_devices(&self, device_no: Option<&str>) -> Vec<PicoDevice> {
        let raw = device_no.unwrap_or("").trim();
        if raw.is_empty() || raw.eq_ignore_ascii_case("all") {
            let online: Vec<PicoDevice> = self
                .devices
                .iter()
                .filter(|e| e.value().online)
                .map(|e| e.value().clone())
                .collect();
            return if !online.is_empty() {
                online
            } else {
                self.get_all_devices()
            };
        }

        let parts: Vec<&str> = raw
            .split(|c| matches!(c, ',' | ';' | '，' | '；'))
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .collect();

        if parts.len() > 1 {
            let mut result = Vec::new();
            let mut seen = HashSet::new();
            for p in parts {
                if let Some(d) = self.resolve_single_device(p) {
                    if seen.insert(d.sn.clone()) {
                        result.push(d);
                    }
                }
            }
            return result;
        }

        self.resolve_single_device(raw).into_iter().collect()
    }

    pub fn resolve_single_device(&self, clean_no: &str) -> Option<PicoDevice> {
        let q = clean_no.trim();
        if q.is_empty() {
            return None;
        }

        // 1. 直接匹配 SN
        if let Some(d) = self.devices.get(q) {
            return Some(d.clone());
        }

        // 2. 精确匹配 Alias / SuiteNo / DisplayName
        for entry in self.devices.iter() {
            let d = entry.value();
            if d.alias.eq_ignore_ascii_case(q)
                || d.suite_no.eq_ignore_ascii_case(q)
                || d.display_name.eq_ignore_ascii_case(q)
            {
                return Some(d.clone());
            }
        }

        // 3. 机号数字模糊匹配 (如 "22" 匹配 "022" 或 "022号机")
        let digits_in: String = q.chars().filter(|c| c.is_ascii_digit()).collect();
        if !digits_in.is_empty() {
            let in_num = digits_in.parse::<i64>().ok();
            for entry in self.devices.iter() {
                let d = entry.value();
                let mut dev_digits: String = d.alias.chars().filter(|c| c.is_ascii_digit()).collect();
                if dev_digits.is_empty() {
                    dev_digits = d.suite_no.chars().filter(|c| c.is_ascii_digit()).collect();
                }
                if dev_digits.is_empty() {
                    dev_digits = d.sn.chars().filter(|c| c.is_ascii_digit()).collect();
                }
                if dev_digits == digits_in
                    || (in_num.is_some() && dev_digits.parse::<i64>().ok() == in_num)
                {
                    return Some(d.clone());
                }
            }
        }

        // 4. 从 config.json 别名表查找
        let cfg = self.config_service.get_config();
        for (sn, alias) in cfg.headset_aliases {
            if alias.eq_ignore_ascii_case(q) || sn.eq_ignore_ascii_case(q) {
                if let Some(d) = self.devices.get(&sn) {
                    return Some(d.clone());
                }
            }
        }

        None
    }

    pub fn resolve_package_name(&self, device_no: Option<&str>, input_pkg: Option<&str>) -> String {
        let cfg = self.config_service.get_config();

        if let Some(raw) = input_pkg.map(str::trim).filter(|s| !s.is_empty()) {
            if let Some(p) = cfg.projects.iter().find(|p| {
                p.name.eq_ignore_ascii_case(raw)
                    || p.id.eq_ignore_ascii_case(raw)
                    || p.package_name.eq_ignore_ascii_case(raw)
            }) {
                return p.package_name.clone();
            }
            return raw.to_string();
        }

        if let Some(no) = device_no.map(str::trim).filter(|s| !s.is_empty()) {
            if let Some(dev) = self.resolve_devices(Some(no)).into_iter().next() {
                if let Some(mapped) = self.config_service.get_device_default_project(&dev.sn) {
                    return self.resolve_package_name(None, Some(&mapped));
                }
                if !dev.alias.trim().is_empty() {
                    if let Some(mapped) = self.config_service.get_device_default_project(&dev.alias) {
                        return self.resolve_package_name(None, Some(&mapped));
                    }
                    let digits: String = dev.alias.chars().filter(|c| c.is_ascii_digit()).collect();
                    if !digits.is_empty() {
                        if let Some(mapped) = self.config_service.get_device_default_project(&digits) {
                            return self.resolve_package_name(None, Some(&mapped));
                        }
                    }
                }
            }

            if let Some(mapped) = self.config_service.get_device_default_project(no) {
                return self.resolve_package_name(None, Some(&mapped));
            }
            let digits: String = no.chars().filter(|c| c.is_ascii_digit()).collect();
            if !digits.is_empty() {
                if let Some(mapped) = self.config_service.get_device_default_project(&digits) {
                    return self.resolve_package_name(None, Some(&mapped));
                }
            }
        }

        cfg.projects
            .first()
            .map(|p| p.package_name.clone())
            .unwrap_or_else(|| "com.pico.vrplayer".to_string())
    }

    pub fn add_or_update_device<F>(&self, sn: &str, updater: F) -> PicoDevice
    where
        F: FnOnce(&mut PicoDevice),
    {
        let mut is_new = false;
        let mut entry = self.devices.entry(sn.to_string()).or_insert_with(|| {
            is_new = true;
            let cfg_alias = self.config_service.get_alias(sn);
            let alias = if cfg_alias == sn { String::new() } else { cfg_alias };
            PicoDevice::new(sn.to_string(), alias)
        });

        entry.is_demo = false;
        updater(&mut entry);
        entry.refresh_display_name();
        let snapshot = entry.clone();
        drop(entry);

        if let Ok(val) = serde_json::to_value(&snapshot) {
            self.emit_ws(if is_new { "DeviceAdded" } else { "DeviceUpdated" }, val);
        }
        snapshot
    }

    pub fn register_headset(&self, sn: &str, alias: &str, model: Option<&str>) -> PicoDevice {
        self.config_service.set_alias(sn, alias);
        let model_str = model
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .unwrap_or("PICO 4 Enterprise")
            .to_string();
        self.add_or_update_device(sn, |d| {
            d.alias = alias.to_string();
            d.model = model_str;
        })
    }

    pub fn unbind_headset(&self, sn: &str) {
        let _ = self.config_service.update_config(|cfg| {
            cfg.headset_aliases.remove(sn);
            cfg.device_default_projects.remove(sn);
        });
        if self.devices.remove(sn).is_some() {
            self.emit_ws("DeviceRemoved", serde_json::Value::String(sn.to_string()));
        }
    }

    pub fn set_device_online(&self, sn: &str) {
        self.add_or_update_device(sn, |d| {
            d.online = true;
            d.last_seen = Some(Utc::now());
        });
    }

    pub fn mark_device_offline(&self, sn: &str) {
        if let Some(mut entry) = self.devices.get_mut(sn) {
            entry.online = false;
            entry.is_streaming = false;
            entry.last_seen = Some(Utc::now());
            let snap = entry.clone();
            drop(entry);
            if let Ok(val) = serde_json::to_value(&snap) {
                self.emit_ws("DeviceUpdated", val);
            }
        }
    }

    pub fn update_battery(&self, sn: &str, battery: u32) {
        self.add_or_update_device(sn, |d| {
            d.battery = battery;
            if battery > 0 && d.online {
                d.last_seen = Some(Utc::now());
            }
        });
    }

    pub fn update_controller_battery(&self, sn: &str, controller_id: i32, battery: u32) {
        self.add_or_update_device(sn, |d| {
            if controller_id == 0 {
                d.controller_left_battery = battery;
            } else {
                d.controller_right_battery = battery;
            }
        });
    }

    pub fn update_current_app(&self, sn: &str, app_name: &str) {
        let title = self.get_app_title(app_name);
        self.add_or_update_device(sn, |d| {
            if Self::is_tracking_lost_app(app_name) {
                d.tracking_lost = true;
                d.current_app = app_name.to_string();
                d.app_title = "大空间定位丢失 (透视找回中)".to_string();
            } else if Self::is_desktop_app(app_name) {
                d.tracking_lost = false;
                d.current_app = "系统桌面".to_string();
                d.app_title = "系统桌面".to_string();
            } else {
                d.tracking_lost = false;
                d.current_app = app_name.to_string();
                d.app_title = title;
            }
        });
    }

    pub fn is_tracking_lost_app(pkg: &str) -> bool {
        let c = pkg.trim().to_ascii_lowercase();
        c == "com.pvr.seethrough.setting" || c.contains("seethrough.setting")
    }

    pub fn is_desktop_app(pkg: &str) -> bool {
        let c = pkg.trim();
        c.is_empty()
            || c == "系统桌面"
            || c.eq_ignore_ascii_case("com.picovr.enterpriseassistant")
            || c.eq_ignore_ascii_case("com.pico.launcher")
    }

    pub fn update_lock_state(&self, sn: &str, locked: bool) {
        self.add_or_update_device(sn, |d| d.locked = locked);
    }

    pub fn update_screen_state(&self, sn: &str, screen_on: bool) {
        self.add_or_update_device(sn, |d| d.screen_on = screen_on);
    }

    pub fn update_suite_no(&self, sn: &str, suite_no: &str) {
        self.add_or_update_device(sn, |d| d.suite_no = suite_no.trim().to_string());
    }

    pub fn update_model(&self, sn: &str, model: &str) {
        self.add_or_update_device(sn, |d| d.model = model.trim().to_string());
    }

    pub fn set_device_alias(&self, sn: &str, alias: &str) -> i32 {
        self.config_service.set_alias(sn, alias);
        self.add_or_update_device(sn, |d| d.alias = alias.to_string());
        0
    }

    fn get_app_title(&self, pkg: &str) -> String {
        if Self::is_desktop_app(pkg) {
            return "系统桌面".to_string();
        }
        let cfg = self.config_service.get_config();
        cfg.projects
            .iter()
            .find(|p| p.package_name.eq_ignore_ascii_case(pkg))
            .map(|p| p.name.clone())
            .unwrap_or_else(|| pkg.to_string())
    }

    pub fn clear_demo_devices(&self) {
        let demo_sns: Vec<String> = self
            .devices
            .iter()
            .filter(|e| e.value().is_demo || e.key().starts_with("P4E-88"))
            .map(|e| e.key().clone())
            .collect();
        for sn in demo_sns {
            if self.devices.remove(&sn).is_some() {
                self.emit_ws("DeviceRemoved", serde_json::Value::String(sn));
            }
        }
    }

    // ======================== 控制指令 ========================

    pub fn lock_device(&self, sn: &str) -> i32 {
        let ret = if self.is_sdk_active() {
            pico_sdk_ffi::lock_device(sn)
        } else {
            0
        };
        self.update_lock_state(sn, true);
        ret
    }

    pub fn unlock_device(&self, sn: &str) -> i32 {
        let ret = if self.is_sdk_active() {
            pico_sdk_ffi::unlock_device(sn)
        } else {
            0
        };
        self.update_lock_state(sn, false);
        ret
    }

    pub fn start_app(&self, sn: &str, package_name: &str, extra_json: &str) -> i32 {
        self.lock_device(sn);
        let ret = if self.is_sdk_active() {
            if extra_json.trim().is_empty() {
                pico_sdk_ffi::start_play_app(sn, package_name)
            } else {
                let payload = serde_json::json!({
                    "packageName": package_name,
                    "extra": extra_json
                })
                .to_string();
                pico_sdk_ffi::start_play_app(sn, &payload)
            }
        } else {
            0
        };
        self.update_current_app(sn, package_name);
        ret
    }

    pub fn stop_app(&self, sn: &str, package_name: &str) -> i32 {
        let dev_opt = self.get_device(sn);
        if let Some(ref d) = dev_opt {
            if !d.online {
                return -1;
            }
        }

        self.lock_device(sn);

        let mut target_pkg = package_name.trim().to_string();
        if target_pkg.is_empty() || target_pkg == "系统桌面" {
            if let Some(ref d) = dev_opt {
                if !d.current_app.trim().is_empty() && !Self::is_desktop_app(&d.current_app) {
                    target_pkg = d.current_app.clone();
                } else {
                    target_pkg = self.resolve_package_name(Some(sn), None);
                }
            } else {
                target_pkg = self.resolve_package_name(Some(sn), None);
            }
        }

        let ret = if self.is_sdk_active() {
            if !target_pkg.is_empty() && target_pkg != "系统桌面" {
                pico_sdk_ffi::stop_play_app(sn, &target_pkg)
            } else {
                pico_sdk_ffi::stop_play_app(sn, " ")
            }
        } else {
            0
        };

        // 异步 ADB force-stop 双保险，彻底关闭游戏不弹确认框
        let mut pkgs: HashSet<String> = HashSet::new();
        if !target_pkg.is_empty() && target_pkg != "系统桌面" {
            pkgs.insert(target_pkg);
        }
        if let Some(ref d) = dev_opt {
            if !d.current_app.trim().is_empty() && !Self::is_desktop_app(&d.current_app) {
                pkgs.insert(d.current_app.clone());
            }
        }
        for proj in self.config_service.get_config().projects {
            if !proj.package_name.trim().is_empty() && proj.r#type != "vr_video" {
                pkgs.insert(proj.package_name);
            }
        }

        let adb = self.adb_service.clone();
        let sn_owned = sn.to_string();
        let pkg_list: Vec<String> = pkgs.into_iter().collect();
        tokio::spawn(async move {
            adb.force_stop_all_games(&sn_owned, pkg_list).await;
        });

        self.update_current_app(sn, "系统桌面");
        ret
    }

    pub fn pause_video(&self, sn: &str) -> i32 {
        if let Some(d) = self.get_device(sn) {
            if !d.online {
                return -1;
            }
        }
        self.lock_device(sn);
        if self.is_sdk_active() {
            pico_sdk_ffi::pause_video(sn)
        } else {
            0
        }
    }

    pub fn continue_video(&self, sn: &str) -> i32 {
        if let Some(d) = self.get_device(sn) {
            if !d.online {
                return -1;
            }
        }
        self.lock_device(sn);
        if self.is_sdk_active() {
            pico_sdk_ffi::continue_video(sn)
        } else {
            0
        }
    }

    pub fn set_volume(&self, sn: &str, volume: i32) -> i32 {
        self.lock_device(sn);
        if self.is_sdk_active() {
            pico_sdk_ffi::set_device_volumn(sn, volume.clamp(0, 100))
        } else {
            0
        }
    }

    pub fn recenter(&self, sn: &str) -> i32 {
        if self.is_sdk_active() {
            pico_sdk_ffi::recenter_view(sn)
        } else {
            0
        }
    }

    pub fn reboot(&self, sn: &str) -> i32 {
        let ret = if self.is_sdk_active() {
            pico_sdk_ffi::restart_device(sn)
        } else {
            0
        };
        self.mark_device_offline(sn);
        ret
    }

    pub fn shutdown(&self, sn: &str) -> i32 {
        let ret = if self.is_sdk_active() {
            pico_sdk_ffi::shut_down_device(sn)
        } else {
            0
        };
        self.mark_device_offline(sn);
        ret
    }

    pub async fn set_screen_state(&self, sn: &str, screen_on: bool) -> i32 {
        let ret = if self.is_sdk_active() {
            pico_sdk_ffi::set_screen_state(sn, if screen_on { 1 } else { 0 })
        } else {
            0
        };
        self.adb_service.set_screen_state(sn, screen_on).await;
        self.update_screen_state(sn, screen_on);
        ret
    }

    pub fn start_monitor(&self, sn: &str) -> i32 {
        let ret = if self.is_sdk_active() {
            let mut r = pico_sdk_ffi::start_monitor_screen(sn, MonitorQuality::Quality480P);
            tracing::info!("[PICO SDK] StartMonitorScreen({}, Quality480P) -> ret={}", sn, r);
            if r != 0 {
                r = pico_sdk_ffi::start_monitor_screen(sn, MonitorQuality::QualityUserConfig);
                tracing::info!("[PICO SDK] StartMonitorScreen({}, QualityUserConfig) -> ret={}", sn, r);
            }
            if r != 0 {
                r = pico_sdk_ffi::start_monitor(sn);
                tracing::info!("[PICO SDK] StartMonitor({}) -> ret={}", sn, r);
            }
            if r == 0 {
                self.screen_monitor.record_start_requested(
                    sn,
                    0,
                    "监看指令已下发，等待头显 ByteRTC 推流响应...",
                    StreamPhase::WaitingFirstFrame,
                );
            } else {
                self.screen_monitor.record_start_requested(
                    sn,
                    r,
                    &format!("StartMonitorScreen 返回错误码 {}，官方套件未就绪或设备未响应", r),
                    StreamPhase::Error,
                );
            }
            r
        } else {
            self.screen_monitor.record_start_requested(
                sn,
                -1,
                "官方企业应用套件未连接 (60061 端口不可达，请启动并登录套件)",
                StreamPhase::SuiteOffline,
            );
            -1
        };

        self.add_or_update_device(sn, |d| d.is_streaming = true);
        ret
    }

    pub fn stop_monitor(&self, sn: &str) -> i32 {
        let ret = if self.is_sdk_active() {
            let r = pico_sdk_ffi::stop_monitor_screen(sn);
            let _ = pico_sdk_ffi::stop_monitor(sn);
            r
        } else {
            0
        };
        self.screen_monitor.record_stop_requested(sn);
        self.add_or_update_device(sn, |d| d.is_streaming = false);
        ret
    }

    // ======================== 批量控制 ========================

    pub async fn batch_start_app(&self, sn_list: &[String], pkg: &str, extra: &str) -> usize {
        let mut count = 0;
        for sn in sn_list {
            if let Some(d) = self.get_device(sn) {
                if !d.online {
                    continue;
                }
            }
            self.start_app(sn, pkg, extra);
            count += 1;
            sleep(Duration::from_millis(50)).await;
        }
        count
    }

    pub async fn batch_stop_app(&self, sn_list: &[String]) -> usize {
        let mut count = 0;
        for sn in sn_list {
            if let Some(d) = self.get_device(sn) {
                if !d.online {
                    continue;
                }
            }
            self.stop_app(sn, "");
            count += 1;
            sleep(Duration::from_millis(50)).await;
        }
        count
    }

    pub async fn batch_pause_video(&self, sn_list: &[String]) -> usize {
        let mut count = 0;
        for sn in sn_list {
            if let Some(d) = self.get_device(sn) {
                if !d.online {
                    continue;
                }
            }
            self.pause_video(sn);
            count += 1;
            sleep(Duration::from_millis(30)).await;
        }
        count
    }

    pub async fn batch_continue_video(&self, sn_list: &[String]) -> usize {
        let mut count = 0;
        for sn in sn_list {
            if let Some(d) = self.get_device(sn) {
                if !d.online {
                    continue;
                }
            }
            self.continue_video(sn);
            count += 1;
            sleep(Duration::from_millis(30)).await;
        }
        count
    }

    pub async fn batch_set_volume(&self, sn_list: &[String], vol: i32) -> usize {
        let mut count = 0;
        for sn in sn_list {
            self.set_volume(sn, vol);
            count += 1;
            sleep(Duration::from_millis(30)).await;
        }
        count
    }

    pub async fn batch_set_screen(&self, sn_list: &[String], screen_on: bool) -> usize {
        for sn in sn_list {
            self.set_screen_state(sn, screen_on).await;
        }
        sn_list.len()
    }

    pub async fn batch_reboot(&self, sn_list: &[String]) -> usize {
        let mut count = 0;
        for sn in sn_list {
            self.reboot(sn);
            count += 1;
            sleep(Duration::from_millis(100)).await;
        }
        count
    }

    pub async fn find_device(&self, sn: &str) -> i32 {
        let mut ret = 0;
        if self.is_sdk_active() {
            let _ = pico_sdk_ffi::set_screen_state(sn, 1);
            ret = pico_sdk_ffi::device_control_json(sn, r#"{"action":"findDevice"}"#);
        }
        self.adb_service.set_screen_state(sn, true).await;
        self.update_screen_state(sn, true);
        ret
    }

    pub async fn batch_recenter(&self, sn_list: &[String]) -> usize {
        let mut count = 0;
        for sn in sn_list {
            self.recenter(sn);
            count += 1;
            sleep(Duration::from_millis(30)).await;
        }
        count
    }

    pub async fn batch_find(&self, sn_list: &[String]) -> usize {
        let mut count = 0;
        for sn in sn_list {
            self.find_device(sn).await;
            count += 1;
            sleep(Duration::from_millis(40)).await;
        }
        count
    }

    pub async fn batch_shutdown(&self, sn_list: &[String]) -> usize {
        let mut count = 0;
        for sn in sn_list {
            self.shutdown(sn);
            count += 1;
            sleep(Duration::from_millis(100)).await;
        }
        count
    }
}
