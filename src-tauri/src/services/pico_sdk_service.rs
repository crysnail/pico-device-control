use crate::models::SuiteHealthStatus;
use crate::services::config_service::ConfigService;
use crate::services::device_manager::PicoDeviceManager;
use crate::services::pico_sdk_ffi::{
    self, cb_type, fixed_cstr_to_string, ptr_to_string, PXREACtrllerBattery, PXREACurrentApp,
    PXREADevAlias, PXREADevBattery, PXREADevModel, PXREAFrameBlob, PXREAFrameBlobMultiplyMonitor,
    PXREAMonitorPara,
};
use crate::services::screen_monitor::{resolve_volume_from_suite_log, ScreenMonitorService};
use crate::services::service_mode;
use chrono::Utc;
use std::ffi::{c_int, c_void};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, OnceLock};
use tokio::net::TcpStream;
use tokio::sync::Mutex;
use tokio::time::{sleep, timeout, Duration};

struct CallbackContext {
    device_manager: Arc<PicoDeviceManager>,
    config_service: Arc<ConfigService>,
    screen_monitor: Arc<ScreenMonitorService>,
    is_sdk_initialized: Arc<AtomicBool>,
    is_server_connected: Arc<AtomicBool>,
}

static CB_CTX: OnceLock<CallbackContext> = OnceLock::new();

unsafe extern "C" fn on_sdk_callback(
    _context: *mut c_void,
    event_type: u32,
    _status: c_int,
    user_data: *mut c_void,
) {
    let Some(ctx) = CB_CTX.get() else {
        return;
    };

    let res = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| match event_type {
        cb_type::SERVER_CONNECT => {
            tracing::info!("[PICO SDK] 官方套件 gRPC 服务已连接");
            ctx.is_server_connected.store(true, Ordering::SeqCst);
            ctx.is_sdk_initialized.store(true, Ordering::SeqCst);
            ctx.device_manager.set_sdk_active(true);
        }
        cb_type::SERVER_DISCONNECT => {
            tracing::warn!("[PICO SDK] 官方套件 gRPC 服务已断开");
            ctx.is_server_connected.store(false, Ordering::SeqCst);
            ctx.is_sdk_initialized.store(false, Ordering::SeqCst);
            ctx.device_manager.set_sdk_active(false);
        }
        cb_type::DEVICE_FIND | cb_type::DEVICE_CONNECT | cb_type::DEVICE_RESUME => {
            let sn = ptr_to_string(user_data);
            if !sn.is_empty() {
                tracing::info!("[PICO SDK] 硬件设备上线/唤醒 SN: {}", sn);
                ctx.device_manager.clear_demo_devices();
                ctx.device_manager.add_or_update_device(&sn, |d| {
                    d.online = true;
                    d.is_demo = false;
                    d.last_seen = Some(Utc::now());
                });
                if ctx.config_service.get_config().auto_lock_on_find {
                    ctx.device_manager.lock_device(&sn);
                }
                let _ = pico_sdk_ffi::get_battery(&sn);
                let _ = pico_sdk_ffi::get_controller_battery(&sn, 0);
                let _ = pico_sdk_ffi::get_controller_battery(&sn, 1);
                let _ = pico_sdk_ffi::get_current_app(&sn);
                let _ = pico_sdk_ffi::get_device_model(&sn);
                let _ = pico_sdk_ffi::get_device_alias(&sn);
            }
        }
        cb_type::DEVICE_MISSING => {
            let sn = ptr_to_string(user_data);
            if !sn.is_empty() {
                tracing::info!("[PICO SDK] 设备离线 SN: {}", sn);
                ctx.device_manager.mark_device_offline(&sn);
            }
        }
        cb_type::DEVICE_BATTERY => {
            if !user_data.is_null() {
                let bat = &*(user_data as *const PXREADevBattery);
                let sn = fixed_cstr_to_string(&bat.dev_id);
                if !sn.is_empty() {
                    ctx.device_manager.update_battery(&sn, bat.battery);
                    if bat.battery > 0 {
                        ctx.device_manager.set_device_online(&sn);
                    }
                }
            }
        }
        cb_type::CONTROLLER_BATTERY => {
            if !user_data.is_null() {
                let cb = &*(user_data as *const PXREACtrllerBattery);
                let sn = fixed_cstr_to_string(&cb.dev_id);
                if !sn.is_empty() {
                    ctx.device_manager
                        .update_controller_battery(&sn, cb.controller_id, cb.battery);
                }
            }
        }
        cb_type::CURRENT_APPLICATION => {
            if !user_data.is_null() {
                let app = &*(user_data as *const PXREACurrentApp);
                let sn = fixed_cstr_to_string(&app.dev_id);
                let app_name = fixed_cstr_to_string(&app.app_name);
                if !sn.is_empty() {
                    ctx.device_manager.update_current_app(&sn, &app_name);
                    if !app_name.is_empty() {
                        ctx.device_manager.set_device_online(&sn);
                    }
                }
            }
        }
        cb_type::DEVICE_MODEL => {
            if !user_data.is_null() {
                let m = &*(user_data as *const PXREADevModel);
                let sn = fixed_cstr_to_string(&m.dev_id);
                let model = fixed_cstr_to_string(&m.dev_model);
                if !sn.is_empty() {
                    ctx.device_manager.update_model(&sn, &model);
                }
            }
        }
        cb_type::DEVICE_ALIAS => {
            if !user_data.is_null() {
                let a = &*(user_data as *const PXREADevAlias);
                let sn = fixed_cstr_to_string(&a.dev_id);
                let alias = fixed_cstr_to_string(&a.alias);
                if !sn.is_empty() {
                    ctx.device_manager.update_suite_no(&sn, &alias);
                }
            }
        }
        cb_type::DEVICE_LOCK => {
            let sn = ptr_to_string(user_data);
            if !sn.is_empty() {
                ctx.device_manager.update_lock_state(&sn, true);
            }
        }
        cb_type::DEVICE_UNLOCK => {
            let sn = ptr_to_string(user_data);
            if !sn.is_empty() {
                ctx.device_manager.update_lock_state(&sn, false);
            }
        }
        cb_type::DEVICE_SENSOR => {
            let sn = ptr_to_string(user_data);
            if !sn.is_empty() {
                ctx.device_manager.update_wear_state(&sn, _status != 0);
            }
        }
        cb_type::DEVICE_SCREEN_MONITOR_PARAMETER
        | cb_type::DEVICE_MONITOR_PARAMETER
        | cb_type::DEVICE_STREAM_MONITOR_PARAMETER => {
            if !user_data.is_null() {
                let p = &*(user_data as *const PXREAMonitorPara);
                let sn = fixed_cstr_to_string(&p.dev_id);
                tracing::info!(
                    "[PICO SDK] 监看参数就绪 DevID={} {}x{}",
                    sn,
                    p.width,
                    p.height
                );
            }
        }
        cb_type::DEVICE_MONITOR => {
            if !user_data.is_null() {
                let f = &*(user_data as *const PXREAFrameBlob);
                let sn = fixed_cstr_to_string(&f.dev_id);
                ctx.screen_monitor.enqueue_raw_frame(
                    sn,
                    f.addr,
                    f.size,
                    f.width,
                    f.height,
                    f.bytes_per_line,
                    f.pix_format,
                );
            }
        }
        cb_type::DEVICE_SCREEN_MONITOR | cb_type::DEVICE_STREAM_MONITOR => {
            if !user_data.is_null() {
                let f = &*(user_data as *const PXREAFrameBlobMultiplyMonitor);
                let sn = fixed_cstr_to_string(&f.dev_id);
                ctx.screen_monitor.enqueue_raw_frame(
                    sn,
                    f.addr,
                    f.size,
                    f.width,
                    f.height,
                    f.bytes_per_line,
                    f.pix_format,
                );
            }
        }
        _ => {}
    }));

    if res.is_err() {
        tracing::error!("[PICO SDK] 回调处理捕获到 panic (type={})", event_type);
    }
}

pub struct PicoSdkService {
    device_manager: Arc<PicoDeviceManager>,
    screen_monitor: Arc<ScreenMonitorService>,
    is_sdk_initialized: Arc<AtomicBool>,
    is_server_connected: Arc<AtomicBool>,
    init_lock: Mutex<()>,
    shutdown: Arc<AtomicBool>,
}

impl PicoSdkService {
    pub fn new(
        device_manager: Arc<PicoDeviceManager>,
        config_service: Arc<ConfigService>,
        screen_monitor: Arc<ScreenMonitorService>,
    ) -> Arc<Self> {
        let is_sdk_initialized = Arc::new(AtomicBool::new(false));
        let is_server_connected = Arc::new(AtomicBool::new(false));

        let _ = CB_CTX.set(CallbackContext {
            device_manager: device_manager.clone(),
            config_service,
            screen_monitor: screen_monitor.clone(),
            is_sdk_initialized: is_sdk_initialized.clone(),
            is_server_connected: is_server_connected.clone(),
        });

        Arc::new(Self {
            device_manager,
            screen_monitor,
            is_sdk_initialized,
            is_server_connected,
            init_lock: Mutex::new(()),
            shutdown: Arc::new(AtomicBool::new(false)),
        })
    }

    pub fn start(self: &Arc<Self>) {
        self.device_manager.initialize_configured_devices();

        // 首次连接尝试
        let svc_init = self.clone();
        tokio::spawn(async move {
            let _ = svc_init.reconnect_sdk().await;
        });

        // 2 秒遥测轮询循环
        let svc_tel = self.clone();
        tokio::spawn(async move {
            while !svc_tel.shutdown.load(Ordering::Relaxed) {
                sleep(Duration::from_secs(2)).await;
                if svc_tel.shutdown.load(Ordering::Relaxed) {
                    break;
                }
                if !svc_tel.is_sdk_initialized.load(Ordering::Relaxed) {
                    continue;
                }
                let now = Utc::now();
                for dev in svc_tel.device_manager.get_all_devices() {
                    if dev.is_demo {
                        continue;
                    }
                    if dev.online {
                        let _ = pico_sdk_ffi::get_battery(&dev.sn);
                        let _ = pico_sdk_ffi::get_controller_battery(&dev.sn, 0);
                        let _ = pico_sdk_ffi::get_controller_battery(&dev.sn, 1);
                        let _ = pico_sdk_ffi::get_current_app(&dev.sn);
                        let adb_vol = svc_tel.device_manager.adb_service().get_volume(&dev.sn).await;
                        let real_vol = adb_vol.or_else(|| resolve_volume_from_suite_log(&dev.sn));
                        if let Some(v) = real_vol {
                            if v != dev.volume {
                                svc_tel.device_manager.add_or_update_device(&dev.sn, |d| {
                                    d.volume = v;
                                });
                            }
                        }
                        if let Some(last) = dev.last_seen {
                            if (now - last).num_seconds() > 30 {
                                tracing::info!("[PICO SDK] 设备失联超过30s，置为离线 SN: {}", dev.sn);
                                svc_tel.device_manager.mark_device_offline(&dev.sn);
                            }
                        }
                    } else {
                        let _ = pico_sdk_ffi::get_battery(&dev.sn);
                        let _ = pico_sdk_ffi::get_current_app(&dev.sn);
                    }
                }
            }
        });

        // 5 秒官方套件保活巡检
        let svc_ka = self.clone();
        tokio::spawn(async move {
            while !svc_ka.shutdown.load(Ordering::Relaxed) {
                sleep(Duration::from_secs(5)).await;
                if svc_ka.shutdown.load(Ordering::Relaxed) {
                    break;
                }
                if !svc_ka.is_sdk_initialized.load(Ordering::Relaxed) {
                    if pico_sdk_ffi::is_dll_loaded() {
                        let health = svc_ka.check_suite_health().await;
                        if health.is_port_open {
                            tracing::info!("[PICO SDK 保活] 检测到套件 60061 端口开放，自动连接 SDK...");
                            let _ = svc_ka.reconnect_sdk().await;
                        }
                    }
                } else if !svc_ka.is_server_connected.load(Ordering::Relaxed)
                    && service_mode::find_pico_suite_process().is_none()
                {
                    svc_ka.is_sdk_initialized.store(false, Ordering::SeqCst);
                    svc_ka.device_manager.set_sdk_active(false);
                }
            }
        });
    }

    pub async fn reconnect_sdk(&self) -> bool {
        let _guard = self.init_lock.lock().await;
        if !pico_sdk_ffi::is_dll_loaded() {
            self.is_sdk_initialized.store(false, Ordering::SeqCst);
            self.is_server_connected.store(false, Ordering::SeqCst);
            self.device_manager.set_sdk_active(false);
            return false;
        }

        // 先探测本地 60061 端口，套件未开启时不触发原生 C++ Init/Deinit，彻底避免异常或闪退
        let port_open = matches!(
            timeout(Duration::from_millis(600), TcpStream::connect("127.0.0.1:60061")).await,
            Ok(Ok(_))
        );
        if !port_open {
            self.is_server_connected.store(false, Ordering::SeqCst);
            self.device_manager.set_sdk_active(false);
            tracing::debug!("[PICO SDK] 127.0.0.1:60061 未开放，跳过原生 Init");
            return false;
        }

        // 已经初始化过时不再重复 Deinit()+Init()，防止 PXREAClientSDK.dll C++ 单例重建引发 0xC0000005 闪退
        if self.is_sdk_initialized.load(Ordering::SeqCst) {
            self.is_server_connected.store(true, Ordering::SeqCst);
            self.device_manager.set_sdk_active(true);
            return true;
        }

        let ret = pico_sdk_ffi::init(on_sdk_callback, cb_type::FULL_MASK);
        if ret == 0 {
            self.is_sdk_initialized.store(true, Ordering::SeqCst);
            self.is_server_connected.store(true, Ordering::SeqCst);
            self.device_manager.set_sdk_active(true);
            tracing::info!("★ PICO Enterprise Suite SDK 原生 FFI 初始化成功 (ret=0)");
            true
        } else {
            self.is_sdk_initialized.store(false, Ordering::SeqCst);
            self.is_server_connected.store(false, Ordering::SeqCst);
            self.device_manager.set_sdk_active(false);
            tracing::warn!("PICO SDK Init 未连接 (ret={})，等待官方套件就绪", ret);
            false
        }
    }

    pub async fn check_suite_health(&self) -> SuiteHealthStatus {
        let mut status = SuiteHealthStatus {
            is_sdk_active: self.is_sdk_initialized.load(Ordering::Relaxed),
            is_server_connected: self.is_server_connected.load(Ordering::Relaxed),
            listen_port: 60061,
            check_time: Utc::now(),
            streams: self.screen_monitor.get_all_stream_statuses(),
            ..Default::default()
        };

        if status.is_sdk_active && status.is_server_connected {
            status.is_process_running = true;
            status.is_port_open = true;
            status.status_summary = "官方企业套件 gRPC 通道握手正常，具备完整视频流传输能力".into();
            status.advice = "通信链路就绪，可随时开启头显屏幕画面实时监控".into();
            return status;
        }

        status.is_process_running = service_mode::find_pico_suite_process().is_some();
        status.is_port_open =
            matches!(timeout(Duration::from_millis(800), TcpStream::connect("127.0.0.1:60061")).await, Ok(Ok(_)));

        if !pico_sdk_ffi::is_dll_loaded() {
            status.status_summary = "未检测到 PXREAClientSDK.dll 动态库".into();
            status.advice = "请确认 PXREAClientSDK.dll 与 PicoVR-Manager.exe 位于同一目录".into();
        } else if !status.is_process_running {
            status.status_summary = "PICO 官方企业应用套件未运行".into();
            status.advice = "请在电脑上启动并登录 PICO Business Suite 官方软件".into();
        } else if !status.is_port_open {
            status.status_summary = "已检测到官方套件进程运行中，但本地 60061 gRPC 端口尚未开放".into();
            status.advice = "请在 PICO Business Suite 窗口登录企业账号并进入主界面，端口开放后控制中心将自动握手连接".into();
        } else {
            status.status_summary = "官方套件 60061 服务端口正常，但 SDK 尚未建立连接".into();
            status.advice = "系统正在自动握手重连中，或点击下方「一键重连」立刻激活".into();
        }

        status
    }

    pub fn shutdown(&self) {
        self.shutdown.store(true, Ordering::SeqCst);
        if self.is_sdk_initialized.swap(false, Ordering::SeqCst) {
            pico_sdk_ffi::deinit();
            tracing::info!("[PICO SDK] 已安全释放原生资源 (Deinit)");
        }
    }
}
