<div align="center">

# PICO VR 场馆运营与多设备统一管理控制中心 · PicoManager

**基于 Rust + Tauri v2 + Axum 构建的单文件一体化桌面中控平台**

[![Version](https://img.shields.io/badge/Version-v2.0.0-0ea5e9?style=for-the-badge)](Build/PicoManager.exe)
[![Rust](https://img.shields.io/badge/Rust-Tauri_v2_+_Axum-f97316?style=for-the-badge&logo=rust)](src-tauri/Cargo.toml)
[![Platform](https://img.shields.io/badge/Platform-Windows_x64-2563eb?style=for-the-badge&logo=windows)](scripts/build-release.ps1)
[![ZeroDLL](https://img.shields.io/badge/Build-Zero_Extra_DLLs-10b981?style=for-the-badge)](Build/)

*无需 `.bat` 启动脚本 · 无需安装 .NET 运行时 · 无额外 DLL 散落目录 · 双击即用*

</div>

---

## 📖 项目简介

**PicoManager** 专为 PICO VR 线下体验馆、大空间项目与多头显运营场景打造。软件已全面完成 **Rust 原生全栈重构**，将桌面端窗口（WebView2 + 系统托盘）、HTTP/WebSocket 后台服务、UDP 外部中控监听、无线 ADB 运维引擎以及 PICO Enterprise Suite 原生 SDK 通信层全部封装进单个 `PicoManager.exe` 中，同时内嵌高清壁纸与玻璃拟态控制台界面，支持局域网平板与远程服务协同接管。

---

## ✨ 核心特性

| 模块 | 核心能力说明 |
| :--- | :--- |
| **🚀 单文件极简部署** | 原生依赖 `PXREAClientSDK.dll` 与 4 张高清壁纸全部编译内嵌于 `PicoManager.exe`，`Build/` 目录零多余 `.dll`，运行时自动按需释放依赖库。 |
| **🌐 本地 / 远程双模式** | 支持**本地直连**（对接本机 PICO 套件）与**远程服务模式**一键切换，输入局域网内其他主机的 `IP:端口` 即可直接接管远端主机的套件与头显控制。 |
| **👓 硬件遥测与实时监看** | 通过 `libloading` FFI 接入 `PXREAClientSDK.dll`，实现设备秒级发现、自动锁定、电量/手柄/前台应用实时遥测；内置无阻塞转码线程（支持 `YUV420P / BGRA / RGBA / RGB24`），提供 MJPEG + 快照双通道实时画面监看。 |
| **🩺 套件链路自检与防闪退** | 自动巡检 `PICOBusinessSuite.exe` 进程、本地 `127.0.0.1:60061` gRPC 端口与 FFI 握手状态，支持端口探测前置保护与一键安全重连。 |
| **🎮 集群播控与设备管理** | 支持分组筛选、批量一键启动/暂停/回桌面、批量亮屏/息屏、全局音量同步、**查找设备（亮屏/提示）**、**单机与批量视角复位**、锁定解锁及远程重启/关机。 |
| **⚡ 无线 ADB 运维中心** | 全网段并发探测 TCP `5555` 端口、自定义勾选扫描设备同步至机号列表、读 SN 自动复制剪贴板、大包 APK 两阶段安装、文件/ZIP 安全推送（含 Zip-Slip 校验）与指定头显第三方应用读取卸载。 |
| **🎨 现代玻璃拟态 UI** | 对齐现代桌面视觉规范，内置深色 / 浅色 / 跟随系统三色外观、**UI 布局预缩放（75%–135%）** 与 **字体大小缩放（80%–140%）**，自适应不同分辨率屏幕。 |
| **📱 局域网平板联动** | 启动后默认在 `0.0.0.0:8999` 开放与桌面端一致的控制台，场馆内平板或手机浏览器可直接扫码/输址接入控制。 |

---

## 🏗️ 系统架构

```text
+-----------------------------------------------------------------------+
|                        PicoManager.exe (Single Binary)                |
|                                                                       |
|  +-----------------------+       +---------------------------------+  |
|  |   Tauri v2 Desktop    |       |    Axum HTTP / WebSocket Server |  |
|  |  WebView2 + TrayIcon  |<----->|    0.0.0.0:8999  (/ws/pico)     |  |
|  +-----------------------+       +----------------+----------------+  |
|                                                   |                   |
|  +-----------------------+       +----------------v----------------+  |
|  |   UDP Control Server  |       |      PicoDeviceManager          |  |
|  |    0.0.0.0:8998       |------>|   设备状态 · 分组 · 播控调度     |  |
|  +-----------------------+       +--------+---------------+--------+  |
|                                           |               |           |
|                          +----------------v--+         +--v--------+  |
|                          | PXREAClientSDK FFI|         |  ADB CLI  |  |
|                          | (60061 gRPC / SHM)|         | (TCP 5555)|  |
|                          +-------------------+         +-----------+  |
+-----------------------------------------------------------------------+
```

---

## 🚀 快速上手

发布产物位于工作区 [`Build/`](Build/) 目录：

1. **启动并登录官方套件**：在主控电脑打开 **PICO Business Suite**，登录企业账号并进入主界面（确保 `60061` 本地服务端口已开放）。
2. **运行中控客户端**：直接双击 [`Build/PicoManager.exe`](Build/PicoManager.exe)（无需任何 `.bat` 脚本）。
3. **局域网协同 / 远程接管**：
   - 平板或手机浏览器访问顶部栏显示的 `http://<本机局域网IP>:8999` 即可协同操作。
   - 若另一台电脑仅作为前台操作端，可点击顶部 **`当前模式: 本地直连`**，输入运行着套件的主控机 `IP:8999` 切换为远程服务模式。
4. **后台驻留与退出**：点击窗口关闭按钮将最小化至系统托盘保持后台服务与 UDP 监听；右键托盘图标可选择 **「彻底退出 PicoManager」**。

> [!TIP]
> 首次运行请确保 Windows 防火墙允许 `PicoManager.exe` 访问专用网络，且 `8999/TCP` 与 `8998/UDP` 端口未被其他程序占用。

---

## 📡 外部 UDP 中控协议（默认端口 `8998`）

支持外部中控系统向 `0.0.0.0:8998` 发送 JSON 或文本指令触发播控：

- **JSON 格式示例**：
  ```json
  { "deviceNo": "022", "action": "start", "packageName": "com.vr.game" }
  ```
- **支持的 `action`**：`start`（启动应用）、`stop`（停止回桌面）、`pause`（暂停）、`resume`（继续）、`recenter`（视角复位）、`screen_on`（亮屏）、`screen_off`（息屏）。
- 可在软件右上角 **`⚙️ 配置中心 -> UDP 联动与测试`** 中实时测试报文回执。

---

## 🛠️ 从源码构建

### 环境要求

- **Rust 工具链**（`rustc` / `cargo`，建议最新 Stable）
- **Windows MSVC 构建环境**（Visual Studio Build Tools with C++ Desktop Development）

### 一键打包至 `Build/` 目录

```powershell
powershell -ExecutionPolicy Bypass -File scripts\build-release.ps1
```

构建完成后 [`Build/`](Build/) 目录仅包含精简产物：
- `PicoManager.exe` — 桌面主控端 + 内嵌后台 + 静态前端 + 内嵌原生库与壁纸
- `config.json` — 场馆设备、分组与体验项目配置文件
- `README.txt` — 现场快速操作说明

---

## 📁 目录结构

```text
pico-device-control/
├── Build/                      # 整理后的单文件绿色发布产物
├── scripts/
│   └── build-release.ps1       # Release 一键编译与 Build 目录整理脚本
├── src-tauri/
│   ├── build.rs                # 资源变更触发配置 (ui / tauri.conf.json)
│   ├── tauri.conf.json         # Tauri v2 窗口与安全策略配置
│   ├── native/                 # 编译期嵌入的 PXREAClientSDK.dll
│   ├── ui/                     # 单一事实来源前端 (HTML / CSS / JS + 内嵌壁纸)
│   └── src/
│       ├── main.rs / lib.rs    # 桌面端入口、系统托盘与运行时拉起
│       ├── commands.rs         # Tauri IPC 命令
│       ├── models/             # 设备、配置与诊断数据模型
│       ├── server/             # Axum HTTP / WS 路由与静态资源嵌入
│       └── services/           # PICO SDK FFI、画面转码、ADB、UDP 与配置落盘
├── wallpaper/                  # 场馆高清壁纸源文件
└── README.md                   # 项目说明文档
```
