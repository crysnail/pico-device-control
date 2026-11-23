use crate::models::AdbDeviceInfo;
use crate::services::config_service::ConfigService;
use crate::services::network_service;
use regex::Regex;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::net::TcpStream;
use tokio::process::Command;
use tokio::sync::Semaphore;
use tokio::time::timeout;
use uuid::Uuid;

#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x08000000;

pub struct AdbService {
    config_service: Arc<ConfigService>,
}

impl AdbService {
    pub fn new(config_service: Arc<ConfigService>) -> Self {
        Self { config_service }
    }

    pub fn resolve_adb_path(&self) -> String {
        if let Some(custom) = self.config_service.get_config().adb_path {
            let trimmed = custom.trim();
            if !trimmed.is_empty() && Path::new(trimmed).exists() {
                return trimmed.to_string();
            }
        }

        let mut candidates: Vec<PathBuf> = Vec::new();
        if let Ok(exe) = std::env::current_exe() {
            if let Some(dir) = exe.parent() {
                candidates.push(dir.join("adb.exe"));
                candidates.push(dir.join("platform-tools").join("adb.exe"));
                candidates.push(dir.join("tools").join("adb.exe"));
            }
        }
        if let Some(local_app) = dirs::data_local_dir() {
            candidates.push(local_app.join("Android").join("Sdk").join("platform-tools").join("adb.exe"));
        }
        candidates.push(PathBuf::from(r"C:\platform-tools\adb.exe"));
        candidates.push(PathBuf::from(r"C:\Program Files\platform-tools\adb.exe"));

        for c in candidates {
            if c.exists() {
                return c.to_string_lossy().to_string();
            }
        }
        "adb".to_string()
    }

    /// 过滤危险 Shell 元字符，防止命令注入
    pub fn sanitize_shell_arg(input: &str) -> String {
        const DANGEROUS: &[char] = &[';', '&', '|', '$', '`', '\n', '\r', '"', '\'', '<', '>'];
        input
            .chars()
            .filter(|c| !DANGEROUS.contains(c))
            .collect::<String>()
            .trim()
            .to_string()
    }

    /// 严格校验 Android 包名格式
    pub fn is_valid_package_name(pkg: &str) -> bool {
        !pkg.is_empty()
            && pkg.len() <= 200
            && pkg
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'))
    }

    pub async fn run_adb(&self, args: &[&str], timeout_ms: u64) -> (i32, String, String) {
        let adb_path = self.resolve_adb_path();
        let mut cmd = Command::new(&adb_path);
        cmd.args(args)
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped());

        #[cfg(windows)]
        {
            cmd.creation_flags(CREATE_NO_WINDOW);
        }

        let fut = cmd.output();
        match timeout(Duration::from_millis(timeout_ms), fut).await {
            Ok(Ok(output)) => {
                let code = output.status.code().unwrap_or(-1);
                let out = String::from_utf8_lossy(&output.stdout).trim().to_string();
                let err = String::from_utf8_lossy(&output.stderr).trim().to_string();
                (code, out, err)
            }
            Ok(Err(e)) => (-2, String::new(), format!("执行 ADB 出错 ({}): {}", adb_path, e)),
            Err(_) => (-1, String::new(), "操作超时".to_string()),
        }
    }

    pub async fn get_connected_devices(&self) -> Vec<AdbDeviceInfo> {
        let (_, out, _) = self.run_adb(&["devices", "-l"], 8000).await;
        if out.is_empty() {
            return Vec::new();
        }

        let model_re = Regex::new(r"model:(\S+)").unwrap();
        let mut list = Vec::new();

        for raw_line in out.lines() {
            let line = raw_line.trim();
            if line.is_empty() || line.to_ascii_lowercase().starts_with("list of devices") {
                continue;
            }
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() < 2 {
                continue;
            }

            let target = parts[0].to_string();
            let status = parts[1].to_string();
            let is_connected = status.eq_ignore_ascii_case("device");

            let (is_wireless, ip, port, mut sn) = if let Some((h, p)) = target.split_once(':') {
                (true, h.to_string(), p.parse::<u16>().unwrap_or(5555), String::new())
            } else {
                (false, String::new(), 5555, target.clone())
            };

            let model = model_re
                .captures(line)
                .and_then(|c| c.get(1))
                .map(|m| m.as_str().replace('_', " "))
                .unwrap_or_else(|| "PICO VR".to_string());

            let mut battery = -1;
            if is_connected {
                if sn.is_empty() {
                    sn = self.read_device_sn(&target).await;
                }
                battery = self.read_device_battery(&target).await;
            }

            list.push(AdbDeviceInfo {
                target,
                ip,
                port,
                status,
                sn,
                model,
                battery,
                is_wireless,
                is_connected,
            });
        }

        list
    }

    pub async fn resolve_target(&self, sn_or_target: &str) -> String {
        let devs = self.get_connected_devices().await;
        if let Some(m) = devs.iter().find(|d| {
            d.sn.eq_ignore_ascii_case(sn_or_target)
                || d.target.eq_ignore_ascii_case(sn_or_target)
                || d.ip.eq_ignore_ascii_case(sn_or_target)
        }) {
            return Self::sanitize_shell_arg(&m.target);
        }
        // 若传入的是 SN 但当前无线 ADB 列表里尚未记录该 SN，则主动查 ro.serialno 匹配
        for d in &devs {
            if d.status == "device" {
                let (code, out, _) = self
                    .run_adb(&["-s", &d.target, "shell", "getprop", "ro.serialno"], 2000)
                    .await;
                if code == 0 && out.trim().eq_ignore_ascii_case(sn_or_target) {
                    return Self::sanitize_shell_arg(&d.target);
                }
            }
        }
        Self::sanitize_shell_arg(sn_or_target)
    }

    pub async fn connect(&self, ip: &str, port: u16) -> bool {
        let clean_ip = Self::sanitize_shell_arg(ip);
        if clean_ip.is_empty() {
            return false;
        }
        let target = if clean_ip.contains(':') {
            clean_ip
        } else {
            format!("{}:{}", clean_ip, if port == 0 { 5555 } else { port })
        };
        let (_, out, _) = self.run_adb(&["connect", &target], 8000).await;
        let low = out.to_ascii_lowercase();
        low.contains("connected to") || low.contains("already connected")
    }

    pub async fn disconnect(&self, target: &str) -> bool {
        let clean = Self::sanitize_shell_arg(target);
        if clean.is_empty() {
            return false;
        }
        let (code, _, _) = self.run_adb(&["disconnect", &clean], 5000).await;
        code == 0
    }

    pub async fn read_device_sn(&self, target: &str) -> String {
        let clean = Self::sanitize_shell_arg(target);
        if clean.is_empty() {
            return String::new();
        }
        let (_, out, _) = self
            .run_adb(&["-s", &clean, "shell", "getprop", "ro.serialno"], 4000)
            .await;
        if !out.is_empty() && !out.to_ascii_lowercase().contains("error") {
            return out;
        }
        let (_, out2, _) = self
            .run_adb(&["-s", &clean, "shell", "getprop", "ro.boot.serialno"], 4000)
            .await;
        if !out2.is_empty() && !out2.to_ascii_lowercase().contains("error") {
            return out2;
        }
        String::new()
    }

    pub async fn read_device_battery(&self, target: &str) -> i32 {
        let clean = Self::sanitize_shell_arg(target);
        let (_, out, _) = self
            .run_adb(&["-s", &clean, "shell", "dumpsys", "battery"], 4000)
            .await;
        for line in out.lines() {
            let trimmed = line.trim();
            if let Some(rest) = trimmed.strip_prefix("level:") {
                if let Ok(v) = rest.trim().parse::<i32>() {
                    return v;
                }
            }
        }
        -1
    }

    pub async fn reboot(&self, target: &str) -> bool {
        let clean = Self::sanitize_shell_arg(target);
        if clean.is_empty() {
            return false;
        }
        let (code, _, _) = self.run_adb(&["-s", &clean, "reboot"], 8000).await;
        code == 0
    }

    pub async fn set_screen_state(&self, sn_or_target: &str, screen_on: bool) {
        let target = self.resolve_target(sn_or_target).await;
        if target.is_empty() {
            return;
        }
        if screen_on {
            let _ = self
                .run_adb(&["-s", &target, "shell", "input", "keyevent", "224"], 3000)
                .await;
            let _ = self
                .run_adb(&["-s", &target, "shell", "input", "keyevent", "82"], 2000)
                .await;
        } else {
            let _ = self
                .run_adb(&["-s", &target, "shell", "input", "keyevent", "223"], 3000)
                .await;
        }
    }

    pub async fn ensure_connected_by_ip(&self, ip: &str, port: u16) -> String {
        let clean_ip = Self::sanitize_shell_arg(ip);
        if clean_ip.is_empty() || clean_ip == "127.0.0.1" {
            return String::new();
        }
        let target = format!("{}:{}", clean_ip, port);
        let existing = self.get_connected_devices().await;
        if existing.iter().any(|d| d.target == target && d.is_connected) {
            return target;
        }
        let ok = self.connect(&clean_ip, port).await;
        if ok { target } else { String::new() }
    }

    pub async fn set_volume(&self, sn_or_target: &str, vol_0_100: i32) {
        let target = self.resolve_target(sn_or_target).await;
        if target.is_empty() {
            return;
        }
        // Android 媒体音量通常为 0..15
        let android_vol = ((vol_0_100.clamp(0, 100) as f32) * 0.15).round() as i32;
        let vol_str = android_vol.to_string();
        let _ = self
            .run_adb(
                &["-s", &target, "shell", "cmd", "media_session", "volume", "--stream", "3", "--set", &vol_str],
                2500,
            )
            .await;
    }

    pub async fn get_volume(&self, sn_or_target: &str) -> Option<u32> {
        let target = self.resolve_target(sn_or_target).await;
        if target.is_empty() {
            return None;
        }
        let (code, stdout, _) = self
            .run_adb(
                &["-s", &target, "shell", "cmd", "media_session", "volume", "--stream", "3", "--get"],
                2500,
            )
            .await;
        if code != 0 {
            return None;
        }
        for line in stdout.lines() {
            if let Some(v_pos) = line.find("volume is ") {
                let tail = &line[v_pos + 10..];
                let cur: u32 = tail
                    .split_whitespace()
                    .next()
                    .and_then(|s| s.parse().ok())?;
                let max_v: u32 = line
                    .find("[0..")
                    .and_then(|p| {
                        let t = &line[p + 4..];
                        t.split(']').next()?.parse().ok()
                    })
                    .unwrap_or(15)
                    .max(1);
                return Some(((cur * 100) / max_v).min(100));
            }
        }
        None
    }

    async fn ensure_find_assets_pushed(&self, target: &str, with_mp3: bool) {
        static EMBEDDED_FIND_AUDIO_DEX: &[u8] = include_bytes!("../../assets/PicoFindAudio.dex");
        static EMBEDDED_FIND_RINGTONE_MP3: &[u8] = include_bytes!("../../assets/find_ringtone.mp3");

        let cache_dir = std::env::temp_dir().join("PicoVR-Manager");
        let _ = std::fs::create_dir_all(&cache_dir);
        let local_dex = cache_dir.join("PicoFindAudio.dex");
        let _ = std::fs::write(&local_dex, EMBEDDED_FIND_AUDIO_DEX);

        let check_cmd = if with_mp3 {
            "wc -c /data/local/tmp/PicoFindAudio.dex /data/local/tmp/pico_find_ringtone.mp3 2>/dev/null"
        } else {
            "wc -c /data/local/tmp/PicoFindAudio.dex 2>/dev/null"
        };
        let (code_wc, wc_out, _) = self.run_adb(&["-s", target, "shell", check_cmd], 2500).await;
        let dex_ok = code_wc == 0 && wc_out.contains(&EMBEDDED_FIND_AUDIO_DEX.len().to_string());
        if !dex_ok {
            let dex_str = local_dex.to_string_lossy().to_string();
            let _ = self
                .run_adb(&["-s", target, "push", &dex_str, "/data/local/tmp/PicoFindAudio.dex"], 6000)
                .await;
        }
        if with_mp3 {
            let local_mp3 = cache_dir.join("find_ringtone.mp3");
            let _ = std::fs::write(&local_mp3, EMBEDDED_FIND_RINGTONE_MP3);
            let mp3_ok = code_wc == 0 && wc_out.contains(&EMBEDDED_FIND_RINGTONE_MP3.len().to_string());
            if !mp3_ok {
                let mp3_str = local_mp3.to_string_lossy().to_string();
                let _ = self
                    .run_adb(&["-s", target, "push", &mp3_str, "/data/local/tmp/pico_find_ringtone.mp3"], 8000)
                    .await;
            }
        }
    }

    pub async fn trigger_find_sound(&self, sn_or_target: &str, active: bool) {
        let target = self.resolve_target(sn_or_target).await;
        if target.is_empty() {
            return;
        }
        if active {
            self.ensure_find_assets_pushed(&target, true).await;
            let _ = self
                .run_adb(&["-s", &target, "shell", "input", "keyevent", "224"], 2500)
                .await;
            let start_cmd = "kill $(cat /data/local/tmp/pico_find.pid 2>/dev/null) 2>/dev/null; pkill -x PicoFindAudio 2>/dev/null; cmd media_session volume --stream 3 --set 15; cmd notification post -t 'PicoVR-Manager' 'find_tag' '正在查找此头显'; nohup env CLASSPATH=/data/local/tmp/PicoFindAudio.dex app_process /data/local/tmp --nice-name=PicoFindAudio PicoFindAudio /data/local/tmp/pico_find_ringtone.mp3 >/dev/null 2>&1 </dev/null & echo $! > /data/local/tmp/pico_find.pid";
            let _ = self
                .run_adb(&["-s", &target, "shell", start_cmd], 4000)
                .await;
        } else {
            let _ = self
                .run_adb(
                    &["-s", &target, "shell", "kill $(cat /data/local/tmp/pico_find.pid 2>/dev/null) 2>/dev/null; pkill -x PicoFindAudio 2>/dev/null; rm -f /data/local/tmp/pico_find.pid; cmd notification cancel 'find_tag' 2>/dev/null || true"],
                    3000,
                )
                .await;
        }
    }

    pub async fn start_voice_receiver(&self, sn_or_target: &str) -> Option<u16> {
        let target = self.resolve_target(sn_or_target).await;
        if target.is_empty() {
            return None;
        }
        self.ensure_find_assets_pushed(&target, false).await;
        let start_cmd = "kill $(cat /data/local/tmp/pico_voice.pid 2>/dev/null) 2>/dev/null; pkill -x PicoVoicePlay 2>/dev/null; cmd media_session volume --stream 3 --set 14; nohup env CLASSPATH=/data/local/tmp/PicoFindAudio.dex app_process /data/local/tmp --nice-name=PicoVoicePlay PicoVoicePlay 18997 >/dev/null 2>&1 </dev/null & echo $! > /data/local/tmp/pico_voice.pid";
        let _ = self.run_adb(&["-s", &target, "shell", start_cmd], 4000).await;
        let (code, _, _) = self
            .run_adb(&["-s", &target, "forward", "tcp:18997", "tcp:18997"], 3000)
            .await;
        if code == 0 {
            Some(18997)
        } else {
            None
        }
    }

    pub async fn stop_voice_receiver(&self, sn_or_target: &str) {
        let target = self.resolve_target(sn_or_target).await;
        if target.is_empty() {
            return;
        }
        let _ = self
            .run_adb(
                &["-s", &target, "shell", "kill $(cat /data/local/tmp/pico_voice.pid 2>/dev/null) 2>/dev/null; pkill -x PicoVoicePlay 2>/dev/null; rm -f /data/local/tmp/pico_voice.pid"],
                3000,
            )
            .await;
        let _ = self
            .run_adb(&["-s", &target, "forward", "--remove", "tcp:18997"], 2000)
            .await;
    }

    pub async fn force_stop_all_games(&self, sn_or_target: &str, pkgs: Vec<String>) {
        let target = self.resolve_target(sn_or_target).await;
        if target.is_empty() {
            return;
        }
        for pkg in pkgs {
            let clean = Self::sanitize_shell_arg(&pkg);
            if Self::is_valid_package_name(&clean) {
                let _ = self
                    .run_adb(&["-s", &target, "shell", "am", "force-stop", &clean], 3000)
                    .await;
            }
        }
    }

    pub fn detect_lan_subnets(&self) -> Vec<String> {
        network_service::detect_lan_subnets()
    }

    /// 受控并发（32 并发）探测局域网 TCP 5555 并自动连接无线 ADB
    pub async fn scan_and_auto_connect(&self, custom_subnet: Option<String>, timeout_ms: u64) -> Vec<AdbDeviceInfo> {
        let subnets = match custom_subnet.filter(|s| !s.trim().is_empty()) {
            Some(s) => vec![s.trim().trim_end_matches('.').to_string()],
            None => self.detect_lan_subnets(),
        };

        let sem = Arc::new(Semaphore::new(32));
        let mut handles = Vec::new();

        for subnet in subnets {
            for i in 1..=254u8 {
                let ip = format!("{}.{}", subnet, i);
                let sem_c = sem.clone();
                handles.push(tokio::spawn(async move {
                    let _permit = sem_c.acquire().await.ok()?;
                    let addr = format!("{}:5555", ip);
                    match timeout(Duration::from_millis(timeout_ms), TcpStream::connect(&addr)).await {
                        Ok(Ok(_)) => Some(ip),
                        _ => None,
                    }
                }));
            }
        }

        let mut discovered = Vec::new();
        for h in handles {
            if let Ok(Some(ip)) = h.await {
                discovered.push(ip);
            }
        }

        for ip in discovered {
            let _ = self.connect(&ip, 5555).await;
        }

        self.get_connected_devices().await
    }

    pub async fn list_installed_packages(&self, sn_or_target: &str) -> Vec<String> {
        let target = self.resolve_target(sn_or_target).await;
        if target.is_empty() {
            return Vec::new();
        }
        let (_, out, _) = self
            .run_adb(&["-s", &target, "shell", "pm", "list", "packages", "-3"], 10000)
            .await;
        let mut pkgs: Vec<String> = out
            .lines()
            .map(|l| l.trim().trim_start_matches("package:").trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();
        pkgs.sort();
        pkgs
    }

    pub async fn uninstall_app(&self, sn_or_target: &str, package_name: &str) -> (bool, String) {
        let clean_pkg = Self::sanitize_shell_arg(package_name);
        if !Self::is_valid_package_name(&clean_pkg) {
            return (false, "包名格式不合法".to_string());
        }
        let target = self.resolve_target(sn_or_target).await;
        if target.is_empty() {
            return (false, "未指定有效的目标设备".to_string());
        }
        let (_, out, err) = self
            .run_adb(&["-s", &target, "uninstall", &clean_pkg], 30000)
            .await;
        if out.to_ascii_lowercase().contains("success") {
            (true, format!("卸载成功 ({})", clean_pkg))
        } else {
            let msg = if !err.is_empty() { err } else { out };
            (false, msg)
        }
    }

    pub async fn create_folder(&self, sn_or_target: &str, remote_path: &str) -> (bool, String) {
        let target = self.resolve_target(sn_or_target).await;
        if target.is_empty() {
            return (false, "未指定有效的目标设备".to_string());
        }
        let mut normalized = Self::sanitize_shell_arg(remote_path).replace('\\', "/");
        if normalized.is_empty() {
            return (false, "远程路径不能为空或包含非法字符".to_string());
        }
        if !normalized.starts_with('/') {
            normalized.insert(0, '/');
        }

        let (code, out, err) = self
            .run_adb(&["-s", &target, "shell", "mkdir", "-p", &normalized], 10000)
            .await;
        if code == 0 && err.is_empty() {
            (true, format!("已成功在 [{}] 创建远程文件夹: {}", target, normalized))
        } else {
            let detail = if !err.is_empty() { err } else { out };
            (false, format!("创建文件夹失败: {}", detail))
        }
    }

    pub async fn install_apk(&self, sn_or_target: &str, apk_path: &Path) -> (bool, String) {
        if !apk_path.exists() {
            return (false, format!("本地 APK 文件不存在: {}", apk_path.display()));
        }
        let target = self.resolve_target(sn_or_target).await;
        if target.is_empty() {
            return (false, "未指定有效的目标设备".to_string());
        }

        let apk_str = apk_path.to_string_lossy().to_string();
        let start = Instant::now();

        // 1. 标准安装 (-r -d -t -g)
        let (_, mut out, mut err) = self
            .run_adb(&["-s", &target, "install", "-r", "-d", "-t", "-g", &apk_str], 600_000)
            .await;
        let mut combined = format!("{}\n{}", out, err);
        if combined.to_ascii_lowercase().contains("success") {
            return (true, format!("安装成功 (耗时 {:.1}s)", start.elapsed().as_secs_f64()));
        }

        // 2. 兼容降级（部分系统不支持 -g）
        if combined.to_ascii_lowercase().contains("unknown option") && combined.contains("-g") {
            let r2 = self
                .run_adb(&["-s", &target, "install", "-r", "-d", "-t", &apk_str], 600_000)
                .await;
            out = r2.1;
            err = r2.2;
            combined = format!("{}\n{}", out, err);
            if combined.to_ascii_lowercase().contains("success") {
                return (true, format!("兼容安装成功 (耗时 {:.1}s)", start.elapsed().as_secs_f64()));
            }
        }

        // 3. 大包流式中断两阶段兜底 (push -> pm install -> rm)
        let low = combined.to_ascii_lowercase();
        let is_net_break = low.contains("closed") || low.contains("reset") || low.contains("broken pipe");
        let file_len = std::fs::metadata(apk_path).map(|m| m.len()).unwrap_or(0);
        if is_net_break && file_len > 100 * 1024 * 1024 {
            let tmp_remote = format!("/data/local/tmp/pico_install_{}.apk", Uuid::new_v4().simple());
            let (push_code, _, _) = self
                .run_adb(&["-s", &target, "push", &apk_str, &tmp_remote], 600_000)
                .await;
            if push_code == 0 {
                let (_, pm_out, pm_err) = self
                    .run_adb(&["-s", &target, "shell", "pm", "install", "-r", "-d", "-t", "-g", &tmp_remote], 300_000)
                    .await;
                let _ = self
                    .run_adb(&["-s", &target, "shell", "rm", "-f", &tmp_remote], 10_000)
                    .await;
                let pm_comb = format!("{}\n{}", pm_out, pm_err);
                if pm_comb.to_ascii_lowercase().contains("success") {
                    return (
                        true,
                        format!("两阶段稳定安装成功 (耗时 {:.1}s)", start.elapsed().as_secs_f64()),
                    );
                }
                out = pm_out;
                err = pm_err;
            }
        }

        (false, parse_adb_install_error(&out, &err))
    }

    async fn trigger_media_scanner(&self, target: &str, remote_file_path: &str) {
        let ext = Path::new(remote_file_path)
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();
        if matches!(
            ext.as_str(),
            "mp4" | "mkv" | "avi" | "mov" | "wmv" | "jpg" | "jpeg" | "png" | "gif" | "webp" | "mp3" | "wav" | "ogg" | "apk"
        ) {
            let uri = format!("file://{}", remote_file_path);
            let _ = self
                .run_adb(
                    &[
                        "-s",
                        target,
                        "shell",
                        "am",
                        "broadcast",
                        "-a",
                        "android.intent.action.MEDIA_SCANNER_SCAN_FILE",
                        "-d",
                        &uri,
                    ],
                    3000,
                )
                .await;
        }
    }

    pub async fn push_file_or_folder(
        &self,
        sn_or_target: &str,
        local_path: &Path,
        remote_target_path: &str,
        overwrite: bool,
        flatten: bool,
    ) -> (bool, String) {
        if !local_path.exists() {
            return (false, format!("本地路径不存在: {}", local_path.display()));
        }
        let target = self.resolve_target(sn_or_target).await;
        if target.is_empty() {
            return (false, "未指定有效的目标设备".to_string());
        }

        let mut normalized_remote = Self::sanitize_shell_arg(remote_target_path).replace('\\', "/");
        if normalized_remote.is_empty() {
            return (false, "远程目标路径包含非法控制字符".to_string());
        }
        if !normalized_remote.starts_with('/') {
            normalized_remote.insert(0, '/');
        }

        let start = Instant::now();

        if local_path.is_file() {
            let file_name = local_path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("file");
            let has_ext = Path::new(&normalized_remote).extension().is_some();
            let (target_dir, dest_remote) = if normalized_remote.ends_with('/') || !has_ext {
                let d = normalized_remote.trim_end_matches('/').to_string();
                let f = format!("{}/{}", d, file_name);
                (d, f)
            } else {
                let d = Path::new(&normalized_remote)
                    .parent()
                    .map(|p| p.to_string_lossy().replace('\\', "/"))
                    .unwrap_or_else(|| "/sdcard".to_string());
                (d, normalized_remote.clone())
            };

            let _ = self
                .run_adb(&["-s", &target, "shell", "mkdir", "-p", &target_dir], 8000)
                .await;
            if overwrite {
                let _ = self
                    .run_adb(&["-s", &target, "shell", "rm", "-f", &dest_remote], 8000)
                    .await;
            }

            let local_str = local_path.to_string_lossy().to_string();
            let (code, out, err) = self
                .run_adb(&["-s", &target, "push", &local_str, &dest_remote], 600_000)
                .await;
            let combined = format!("{}\n{}", out, err);
            if code == 0 && (combined.to_ascii_lowercase().contains("pushed") || err.is_empty()) {
                self.trigger_media_scanner(&target, &dest_remote).await;
                return (
                    true,
                    format!("传输成功 (耗时 {:.1}s): {}", start.elapsed().as_secs_f64(), file_name),
                );
            }
            let detail = if !err.is_empty() { err } else { out };
            return (false, detail);
        }

        // 目录推送
        let target_dir = normalized_remote.trim_end_matches('/').to_string();
        let _ = self
            .run_adb(&["-s", &target, "shell", "mkdir", "-p", &target_dir], 8000)
            .await;

        if flatten {
            let entries: Vec<PathBuf> = std::fs::read_dir(local_path)
                .map(|rd| rd.flatten().map(|e| e.path()).collect())
                .unwrap_or_default();
            if entries.is_empty() {
                return (true, "所选目录为空，已在头显同步确保目标目录存在".to_string());
            }

            let mut ok_count = 0usize;
            let mut fails = Vec::new();
            for entry in &entries {
                let name = entry.file_name().and_then(|n| n.to_str()).unwrap_or("");
                let entry_str = entry.to_string_lossy().to_string();
                if entry.is_file() {
                    let dest_file = format!("{}/{}", target_dir, name);
                    if overwrite {
                        let _ = self
                            .run_adb(&["-s", &target, "shell", "rm", "-f", &dest_file], 5000)
                            .await;
                    }
                    let (code, _, _) = self
                        .run_adb(&["-s", &target, "push", &entry_str, &dest_file], 300_000)
                        .await;
                    if code == 0 {
                        ok_count += 1;
                        self.trigger_media_scanner(&target, &dest_file).await;
                    } else {
                        fails.push(name.to_string());
                    }
                } else if entry.is_dir() {
                    let dest_dir = format!("{}/", target_dir);
                    let (code, _, _) = self
                        .run_adb(&["-s", &target, "push", &entry_str, &dest_dir], 300_000)
                        .await;
                    if code == 0 {
                        ok_count += 1;
                    } else {
                        fails.push(name.to_string());
                    }
                }
            }

            if fails.is_empty() {
                (
                    true,
                    format!(
                        "展开传输成功 (共 {} 项，耗时 {:.1}s)",
                        entries.len(),
                        start.elapsed().as_secs_f64()
                    ),
                )
            } else {
                (
                    false,
                    format!(
                        "展开传输部分成功: {}/{}，失败: {}",
                        ok_count,
                        entries.len(),
                        fails.into_iter().take(3).collect::<Vec<_>>().join(", ")
                    ),
                )
            }
        } else {
            let local_str = local_path.to_string_lossy().to_string();
            let dest_dir = format!("{}/", target_dir);
            let (code, out, err) = self
                .run_adb(&["-s", &target, "push", &local_str, &dest_dir], 600_000)
                .await;
            let combined = format!("{}\n{}", out, err);
            if code == 0 && (combined.to_ascii_lowercase().contains("pushed") || err.is_empty()) {
                (true, format!("目录传输成功 (耗时 {:.1}s)", start.elapsed().as_secs_f64()))
            } else {
                let detail = if !err.is_empty() { err } else { out };
                (false, detail)
            }
        }
    }
}

fn parse_adb_install_error(output: &str, error: &str) -> String {
    let combined = format!("{}\n{}", output, error);
    let upper = combined.to_ascii_uppercase();

    if upper.contains("INSTALL_FAILED_VERSION_DOWNGRADE") {
        return "设备已安装更高版本的应用，系统禁止降级覆盖安装（请先卸载旧版后再试）".into();
    }
    if upper.contains("INSTALL_FAILED_ALREADY_EXISTS") || upper.contains("INSTALL_FAILED_UPDATE_INCOMPATIBLE") {
        return "签名冲突或应用已存在（请先卸载旧版后重装）".into();
    }
    if upper.contains("INSTALL_FAILED_TEST_ONLY") {
        return "该应用为开发调试测试包（testOnly），需使用 -t 参数安装".into();
    }
    if upper.contains("INSTALL_FAILED_INSUFFICIENT_STORAGE") {
        return "头显剩余存储空间不足，安装已终止".into();
    }
    if upper.contains("INSTALL_FAILED_INVALID_APK") {
        return "APK 安装包解析失败或文件损坏".into();
    }
    if upper.contains("INSTALL_PARSE_FAILED_NOT_APK") {
        return "指定的文件并非有效的 Android APK 安装包".into();
    }
    if upper.contains("INSTALL_FAILED_NO_MATCHING_ABIS") {
        return "CPU 芯片架构不匹配（未包含 ARM64-v8a 库）".into();
    }
    if upper.contains("DEVICE OFFLINE") {
        return "目标设备处于离线状态 (offline)".into();
    }
    if upper.contains("DEVICE NOT FOUND") {
        return "未找到目标设备，请检查无线 ADB 连接状态".into();
    }

    if let Ok(re) = Regex::new(r"(?i)Failure\s*\[([^\]]+)\]") {
        if let Some(c) = re.captures(&combined) {
            if let Some(m) = c.get(1) {
                return format!("Android系统报错: {}", m.as_str());
            }
        }
    }

    combined
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.to_ascii_lowercase().contains("performing streamed install"))
        .last()
        .map(str::to_string)
        .unwrap_or_else(|| if !error.is_empty() { error.to_string() } else { output.to_string() })
}
