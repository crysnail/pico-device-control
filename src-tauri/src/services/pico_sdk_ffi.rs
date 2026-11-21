use libloading::Library;
use std::ffi::{c_char, c_int, c_uint, c_void, CStr, CString};
use std::path::PathBuf;
use std::sync::OnceLock;

#[allow(dead_code)]
pub mod cb_type {
    pub const FRAME_CAPTURE_START: u32 = 1;
    pub const FRAME_CAPTURE_STOP: u32 = 1 << 1;
    pub const SERVER_CONNECT: u32 = 1 << 2;
    pub const SERVER_DISCONNECT: u32 = 1 << 3;
    pub const DEVICE_FIND: u32 = 1 << 4;
    pub const DEVICE_MISSING: u32 = 1 << 5;
    pub const DEVICE_BATTERY: u32 = 1 << 6;
    pub const DEVICE_LOCK: u32 = 1 << 7;
    pub const DEVICE_UNLOCK: u32 = 1 << 8;
    pub const DEVICE_CONNECT: u32 = 1 << 9;
    pub const DEVICE_SENSOR: u32 = 1 << 10;
    pub const DEVICE_SHUTDOWN: u32 = 1 << 11;
    pub const DEVICE_RESUME: u32 = 1 << 12;
    pub const DEVICE_MONITOR_PARAMETER: u32 = 1 << 13;
    pub const DEVICE_MONITOR: u32 = 1 << 14;
    pub const DEVICE_MODEL: u32 = 1 << 15;
    pub const CURRENT_APPLICATION: u32 = 1 << 16;
    pub const CONTROLLER_BATTERY: u32 = 1 << 17;
    pub const SCREEN_STATE: u32 = 1 << 18;
    pub const VIDEO_CONTROL_RESULT: u32 = 1 << 19;
    pub const DEVICE_SCREEN_MONITOR_PARAMETER: u32 = 1 << 20;
    pub const DEVICE_SCREEN_MONITOR: u32 = 1 << 21;
    pub const DEVICE_STREAM_MONITOR_PARAMETER: u32 = 1 << 22;
    pub const DEVICE_STREAM_MONITOR: u32 = 1 << 23;
    pub const DEVICE_ALIAS: u32 = 1 << 24;
    pub const DEVICE_STATE_JSON: u32 = 1 << 25;
    pub const DEVICE_CUSTOM_MESSAGE: u32 = 1 << 26;
    pub const FULL_MASK: u32 = 0xffffffff;
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
#[allow(dead_code)]
pub enum MonitorQuality {
    Quality480P = 0,
    Quality720P = 1,
    Quality1080P = 2,
    Quality2K = 3,
    Quality360P = 4,
    QualityUserConfig = 1000,
}

#[repr(C)]
pub struct PXREADevBattery {
    pub dev_id: [c_char; 32],
    pub battery: c_uint,
}

#[repr(C)]
pub struct PXREACurrentApp {
    pub dev_id: [c_char; 32],
    pub app_name: [c_char; 128],
}

#[repr(C)]
pub struct PXREADevModel {
    pub dev_id: [c_char; 32],
    pub dev_model: [c_char; 32],
}

#[repr(C)]
pub struct PXREACtrllerBattery {
    pub dev_id: [c_char; 32],
    pub controller_id: c_int,
    pub battery: c_uint,
}

#[repr(C)]
pub struct PXREAFrameBlob {
    pub dev_id: [c_char; 32],
    pub addr: *const u8,
    pub size: c_uint,
    pub width: c_uint,
    pub height: c_uint,
    pub bytes_per_line: c_uint,
    pub pix_format: c_int,
}

#[repr(C)]
pub struct PXREAFrameBlobMultiplyMonitor {
    pub dev_id: [c_char; 32],
    pub is_screen: c_int,
    pub addr: *const u8,
    pub size: c_uint,
    pub width: c_uint,
    pub height: c_uint,
    pub bytes_per_line: c_uint,
    pub pix_format: c_int,
}

#[repr(C)]
pub struct PXREAMonitorPara {
    pub dev_id: [c_char; 32],
    pub width: c_uint,
    pub height: c_uint,
}

#[repr(C)]
pub struct PXREADevAlias {
    pub dev_id: [c_char; 32],
    pub alias: [c_char; 128],
}

pub type SdkCallbackFn =
    unsafe extern "C" fn(context: *mut c_void, cb_type: u32, status: c_int, user_data: *mut c_void);

pub fn fixed_cstr_to_string(buf: &[c_char]) -> String {
    let bytes: Vec<u8> = buf
        .iter()
        .take_while(|&&c| c != 0)
        .map(|&c| c as u8)
        .collect();
    String::from_utf8_lossy(&bytes).trim().to_string()
}

pub unsafe fn ptr_to_string(ptr: *const c_void) -> String {
    if ptr.is_null() {
        return String::new();
    }
    CStr::from_ptr(ptr as *const c_char)
        .to_string_lossy()
        .trim()
        .to_string()
}

struct NativeApi {
    _lib: Library,
    init: unsafe extern "C" fn(*mut c_void, SdkCallbackFn, c_uint) -> c_int,
    deinit: unsafe extern "C" fn() -> c_int,
    get_battery: unsafe extern "C" fn(*const c_char) -> c_int,
    get_current_app: unsafe extern "C" fn(*const c_char) -> c_int,
    get_device_model: unsafe extern "C" fn(*const c_char) -> c_int,
    get_controller_battery: unsafe extern "C" fn(*const c_char, c_int) -> c_int,
    set_screen_state: unsafe extern "C" fn(*const c_char, c_int) -> c_int,
    get_device_alias: unsafe extern "C" fn(*const c_char) -> c_int,
    lock_device: unsafe extern "C" fn(*const c_char) -> c_int,
    unlock_device: unsafe extern "C" fn(*const c_char) -> c_int,
    start_play_app: unsafe extern "C" fn(*const c_char, *const c_char) -> c_int,
    stop_play_app: unsafe extern "C" fn(*const c_char, *const c_char) -> c_int,
    pause_video: unsafe extern "C" fn(*const c_char) -> c_int,
    continue_video: unsafe extern "C" fn(*const c_char) -> c_int,
    set_device_volumn: unsafe extern "C" fn(*const c_char, c_int) -> c_int,
    shut_down_device: unsafe extern "C" fn(*const c_char) -> c_int,
    restart_device: unsafe extern "C" fn(*const c_char) -> c_int,
    recenter_view: unsafe extern "C" fn(*const c_char) -> c_int,
    start_monitor: Option<unsafe extern "C" fn(*const c_char) -> c_int>,
    stop_monitor: Option<unsafe extern "C" fn(*const c_char) -> c_int>,
    start_monitor_screen: Option<unsafe extern "C" fn(*const c_char, c_int) -> c_int>,
    stop_monitor_screen: Option<unsafe extern "C" fn(*const c_char) -> c_int>,
    device_control_json: Option<unsafe extern "C" fn(*const c_char, *const c_char) -> c_int>,
}

// Library 函数指针在进程存活期内保持静态有效
unsafe impl Send for NativeApi {}
unsafe impl Sync for NativeApi {}

static NATIVE_API: OnceLock<Option<NativeApi>> = OnceLock::new();
static EMBEDDED_PXREA_DLL: &[u8] = include_bytes!("../../../src/PicoManager.Server/libs/PXREAClientSDK.dll");

fn ensure_runtime_dll_path() -> Option<PathBuf> {
    let base = dirs::data_local_dir()?;
    let runtime_dir = base.join("PicoManager").join("runtime");
    let dll_path = runtime_dir.join("PXREAClientSDK.dll");
    let need_write = match std::fs::metadata(&dll_path) {
        Ok(meta) => meta.len() != EMBEDDED_PXREA_DLL.len() as u64,
        Err(_) => true,
    };
    if need_write {
        if std::fs::create_dir_all(&runtime_dir).is_ok() {
            let _ = std::fs::write(&dll_path, EMBEDDED_PXREA_DLL);
        }
    }
    if dll_path.exists() {
        Some(dll_path)
    } else {
        None
    }
}

fn resolve_dll_candidates() -> Vec<PathBuf> {
    let mut cands = Vec::new();
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            cands.push(dir.join("PXREAClientSDK.dll"));
            cands.push(dir.join("libs").join("PXREAClientSDK.dll"));
        }
    }
    if let Some(runtime_dll) = ensure_runtime_dll_path() {
        cands.push(runtime_dll);
    }
    if let Ok(cwd) = std::env::current_dir() {
        cands.push(cwd.join("PXREAClientSDK.dll"));
        cands.push(cwd.join("src").join("PicoManager.Server").join("libs").join("PXREAClientSDK.dll"));
    }
    cands.push(PathBuf::from("PXREAClientSDK.dll"));
    cands
}

fn load_native_api() -> Option<NativeApi> {
    for path in resolve_dll_candidates() {
        if path.components().count() > 1 && !path.exists() {
            continue;
        }
        unsafe {
            match Library::new(&path) {
                Ok(lib) => {
                    tracing::info!("[PICO FFI] 成功加载原生库: {:?}", path);
                    macro_rules! sym {
                        ($name:literal, $ty:ty) => {
                            match lib.get::<$ty>($name) {
                                Ok(s) => *s,
                                Err(err) => {
                                    tracing::error!("[PICO FFI] 缺少必需符号 {:?}: {}", $name, err);
                                    return None;
                                }
                            }
                        };
                    }

                    let init = sym!(b"Init\0", unsafe extern "C" fn(*mut c_void, SdkCallbackFn, c_uint) -> c_int);
                    let deinit = sym!(b"Deinit\0", unsafe extern "C" fn() -> c_int);
                    let get_battery = sym!(b"GetBattery\0", unsafe extern "C" fn(*const c_char) -> c_int);
                    let get_current_app = sym!(b"GetCurrentApp\0", unsafe extern "C" fn(*const c_char) -> c_int);
                    let get_device_model = sym!(b"GetDeviceModel\0", unsafe extern "C" fn(*const c_char) -> c_int);
                    let get_controller_battery = sym!(b"GetControllerBattery\0", unsafe extern "C" fn(*const c_char, c_int) -> c_int);
                    let set_screen_state = sym!(b"SetScreenState\0", unsafe extern "C" fn(*const c_char, c_int) -> c_int);
                    let get_device_alias = sym!(b"GetDeviceAlias\0", unsafe extern "C" fn(*const c_char) -> c_int);
                    let lock_device = sym!(b"LockDevice\0", unsafe extern "C" fn(*const c_char) -> c_int);
                    let unlock_device = sym!(b"UnlockDevice\0", unsafe extern "C" fn(*const c_char) -> c_int);
                    let start_play_app = sym!(b"StartPlayApp\0", unsafe extern "C" fn(*const c_char, *const c_char) -> c_int);
                    let stop_play_app = sym!(b"StopPlayApp\0", unsafe extern "C" fn(*const c_char, *const c_char) -> c_int);
                    let pause_video = sym!(b"PauseVideo\0", unsafe extern "C" fn(*const c_char) -> c_int);
                    let continue_video = sym!(b"ContinueVideo\0", unsafe extern "C" fn(*const c_char) -> c_int);
                    let set_device_volumn = sym!(b"SetDeviceVolumn\0", unsafe extern "C" fn(*const c_char, c_int) -> c_int);
                    let shut_down_device = sym!(b"ShutDownDevice\0", unsafe extern "C" fn(*const c_char) -> c_int);
                    let restart_device = sym!(b"RestartDevice\0", unsafe extern "C" fn(*const c_char) -> c_int);
                    let recenter_view = sym!(b"RecenterView\0", unsafe extern "C" fn(*const c_char) -> c_int);

                    let start_monitor = lib.get::<unsafe extern "C" fn(*const c_char) -> c_int>(b"StartMonitor\0").ok().map(|s| *s);
                    let stop_monitor = lib.get::<unsafe extern "C" fn(*const c_char) -> c_int>(b"StopMonitor\0").ok().map(|s| *s);
                    let start_monitor_screen = lib.get::<unsafe extern "C" fn(*const c_char, c_int) -> c_int>(b"StartMonitorScreen\0").ok().map(|s| *s);
                    let stop_monitor_screen = lib.get::<unsafe extern "C" fn(*const c_char) -> c_int>(b"StopMonitorScreen\0").ok().map(|s| *s);
                    let device_control_json = lib.get::<unsafe extern "C" fn(*const c_char, *const c_char) -> c_int>(b"DeviceControlJson\0").ok().map(|s| *s);

                    return Some(NativeApi {
                        _lib: lib,
                        init,
                        deinit,
                        get_battery,
                        get_current_app,
                        get_device_model,
                        get_controller_battery,
                        set_screen_state,
                        get_device_alias,
                        lock_device,
                        unlock_device,
                        start_play_app,
                        stop_play_app,
                        pause_video,
                        continue_video,
                        set_device_volumn,
                        shut_down_device,
                        restart_device,
                        recenter_view,
                        start_monitor,
                        stop_monitor,
                        start_monitor_screen,
                        stop_monitor_screen,
                        device_control_json,
                    });
                }
                Err(err) => {
                    tracing::debug!("[PICO FFI] 尝试加载 {:?} 失败: {}", path, err);
                }
            }
        }
    }
    tracing::warn!("[PICO FFI] 未找到 PXREAClientSDK.dll，将在无原生 SDK 模式下运行");
    None
}

fn api() -> Option<&'static NativeApi> {
    NATIVE_API.get_or_init(load_native_api).as_ref()
}

pub fn is_dll_loaded() -> bool {
    api().is_some()
}

fn to_cstring(s: &str) -> Option<CString> {
    CString::new(s).ok()
}

pub fn init(cb: SdkCallbackFn, mask: u32) -> i32 {
    match api() {
        Some(a) => unsafe { (a.init)(std::ptr::null_mut(), cb, mask) },
        None => -1,
    }
}

pub fn deinit() {
    if let Some(a) = api() {
        unsafe {
            let _ = (a.deinit)();
        }
    }
}

pub fn get_battery(sn: &str) -> i32 {
    match (api(), to_cstring(sn)) {
        (Some(a), Some(c)) => unsafe { (a.get_battery)(c.as_ptr()) },
        _ => -1,
    }
}

pub fn get_current_app(sn: &str) -> i32 {
    match (api(), to_cstring(sn)) {
        (Some(a), Some(c)) => unsafe { (a.get_current_app)(c.as_ptr()) },
        _ => -1,
    }
}

pub fn get_device_model(sn: &str) -> i32 {
    match (api(), to_cstring(sn)) {
        (Some(a), Some(c)) => unsafe { (a.get_device_model)(c.as_ptr()) },
        _ => -1,
    }
}

pub fn get_controller_battery(sn: &str, controller_id: i32) -> i32 {
    match (api(), to_cstring(sn)) {
        (Some(a), Some(c)) => unsafe { (a.get_controller_battery)(c.as_ptr(), controller_id) },
        _ => -1,
    }
}

pub fn set_screen_state(sn: &str, state: i32) -> i32 {
    match (api(), to_cstring(sn)) {
        (Some(a), Some(c)) => unsafe { (a.set_screen_state)(c.as_ptr(), state) },
        _ => -1,
    }
}

pub fn get_device_alias(sn: &str) -> i32 {
    match (api(), to_cstring(sn)) {
        (Some(a), Some(c)) => unsafe { (a.get_device_alias)(c.as_ptr()) },
        _ => -1,
    }
}

pub fn lock_device(sn: &str) -> i32 {
    match (api(), to_cstring(sn)) {
        (Some(a), Some(c)) => unsafe { (a.lock_device)(c.as_ptr()) },
        _ => -1,
    }
}

pub fn unlock_device(sn: &str) -> i32 {
    match (api(), to_cstring(sn)) {
        (Some(a), Some(c)) => unsafe { (a.unlock_device)(c.as_ptr()) },
        _ => -1,
    }
}

pub fn start_play_app(sn: &str, app_name: &str) -> i32 {
    match (api(), to_cstring(sn), to_cstring(app_name)) {
        (Some(a), Some(c_sn), Some(c_app)) => unsafe { (a.start_play_app)(c_sn.as_ptr(), c_app.as_ptr()) },
        _ => -1,
    }
}

pub fn stop_play_app(sn: &str, app_name: &str) -> i32 {
    match (api(), to_cstring(sn), to_cstring(app_name)) {
        (Some(a), Some(c_sn), Some(c_app)) => unsafe { (a.stop_play_app)(c_sn.as_ptr(), c_app.as_ptr()) },
        _ => -1,
    }
}

pub fn pause_video(sn: &str) -> i32 {
    match (api(), to_cstring(sn)) {
        (Some(a), Some(c)) => unsafe { (a.pause_video)(c.as_ptr()) },
        _ => -1,
    }
}

pub fn continue_video(sn: &str) -> i32 {
    match (api(), to_cstring(sn)) {
        (Some(a), Some(c)) => unsafe { (a.continue_video)(c.as_ptr()) },
        _ => -1,
    }
}

pub fn set_device_volumn(sn: &str, vol: i32) -> i32 {
    match (api(), to_cstring(sn)) {
        (Some(a), Some(c)) => unsafe { (a.set_device_volumn)(c.as_ptr(), vol.clamp(0, 100)) },
        _ => -1,
    }
}

pub fn shut_down_device(sn: &str) -> i32 {
    match (api(), to_cstring(sn)) {
        (Some(a), Some(c)) => unsafe { (a.shut_down_device)(c.as_ptr()) },
        _ => -1,
    }
}

pub fn restart_device(sn: &str) -> i32 {
    match (api(), to_cstring(sn)) {
        (Some(a), Some(c)) => unsafe { (a.restart_device)(c.as_ptr()) },
        _ => -1,
    }
}

pub fn recenter_view(sn: &str) -> i32 {
    match (api(), to_cstring(sn)) {
        (Some(a), Some(c)) => unsafe { (a.recenter_view)(c.as_ptr()) },
        _ => -1,
    }
}

pub fn start_monitor_screen(sn: &str, quality: MonitorQuality) -> i32 {
    match (api().and_then(|a| a.start_monitor_screen), to_cstring(sn)) {
        (Some(f), Some(c)) => unsafe { f(c.as_ptr(), quality as c_int) },
        _ => -1,
    }
}

pub fn stop_monitor_screen(sn: &str) -> i32 {
    match (api().and_then(|a| a.stop_monitor_screen), to_cstring(sn)) {
        (Some(f), Some(c)) => unsafe { f(c.as_ptr()) },
        _ => -1,
    }
}

pub fn start_monitor(sn: &str) -> i32 {
    match (api().and_then(|a| a.start_monitor), to_cstring(sn)) {
        (Some(f), Some(c)) => unsafe { f(c.as_ptr()) },
        _ => -1,
    }
}

pub fn stop_monitor(sn: &str) -> i32 {
    match (api().and_then(|a| a.stop_monitor), to_cstring(sn)) {
        (Some(f), Some(c)) => unsafe { f(c.as_ptr()) },
        _ => -1,
    }
}

pub fn device_control_json(sn: &str, json_str: &str) -> i32 {
    match (api().and_then(|a| a.device_control_json), to_cstring(sn), to_cstring(json_str)) {
        (Some(f), Some(c_sn), Some(c_json)) => unsafe { f(c_sn.as_ptr(), c_json.as_ptr()) },
        _ => -1,
    }
}
