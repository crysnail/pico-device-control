#Requires -Version 5.1
Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

$RootDir = Split-Path -Parent $PSScriptRoot
$TauriDir = Join-Path $RootDir "src-tauri"
$BuildDir = Join-Path $RootDir "Build"
$LibsDir = Join-Path $RootDir "src\PicoManager.Server\libs"

Write-Host "============================================================" -ForegroundColor Cyan
Write-Host "  PicoManager v2.0.0 - Rust 一体化桌面端 Release 构建脚本" -ForegroundColor Cyan
Write-Host "============================================================" -ForegroundColor Cyan

Push-Location $TauriDir
try {
    cargo build --release
    if ($LASTEXITCODE -ne 0) {
        throw "cargo build --release failed with exit code $LASTEXITCODE"
    }
}
finally {
    Pop-Location
}

$ReleaseExe = Join-Path $TauriDir "target\release\PicoVR-Manager.exe"
if (-not (Test-Path $ReleaseExe)) {
    throw "未找到编译产物: $ReleaseExe"
}

# 若旧版 PicoVR-Manager.exe / PicoManager.exe 正在运行，先安全结束进程以释放文件锁
$running = Get-Process -Name "PicoVR-Manager", "PicoManager" -ErrorAction SilentlyContinue
if ($running) {
    Write-Host "检测到运行中的 PicoVR-Manager，正在停止以更新构建产物..." -ForegroundColor Yellow
    $running | Stop-Process -Force
    Start-Sleep -Milliseconds 600
}

if (-not (Test-Path $BuildDir)) {
    New-Item -ItemType Directory -Path $BuildDir -Force | Out-Null
} else {
    # 清理旧版残留 DLL 与旧名主程序（保留用户已有的 config.json）
    Get-ChildItem -Path $BuildDir -Filter "*.dll" -ErrorAction SilentlyContinue | Remove-Item -Force -ErrorAction SilentlyContinue
    Remove-Item -Force (Join-Path $BuildDir "PicoManager.exe") -ErrorAction SilentlyContinue
}

# 1. 一体化主程序（已内置前端 UI、HTTP/WS/UDP 后台与按需释放的原生 SDK 库，无多余 DLL）
Copy-Item -Force $ReleaseExe (Join-Path $BuildDir "PicoVR-Manager.exe")

# 2. 精简使用说明（不在构建期生成 config.json，仅在软件内实际配置变更后自动在同目录生成）
$readme = @"
PICO VR 场馆统一设备管理控制中心 (PicoVR-Manager v2.0.0 - Rust 原生一体化版本)

【启动方式】
直接双击运行 PicoVR-Manager.exe 即可（无需任何 .bat 脚本、无需多余 DLL、也无需安装 .NET 运行时）：
  - 启动后自动打开原生桌面控制中心界面；
  - 同进程自动运行 HTTP/WebSocket 后台服务（默认端口 8999，供本端与局域网平板访问）与 UDP 播控服务（默认端口 8998）；
  - 若局域网内其他主机已开启服务模式，可在客户端右上角切换「连接远程服务模式」并输入对方 IP 与端口即可直接使用；
  - 关闭窗口将最小化至系统托盘继续在后台运行，右键托盘图标可选择「彻底退出 PicoVR-Manager」。

【目录文件说明】
  - PicoVR-Manager.exe       一体化桌面端 + 内嵌后台 + 原生 SDK 集成主程序
  - config.json              运行配置文件（端口、体验项目、设备别名与默认项目映射）

【前置要求】
  - Windows 10/11 x64（系统已自带 Edge WebView2）
  - 本机直连模式需运行并登录 PICO Business Suite；远程服务模式仅需输入局域网主机 IP 与端口即可。
"@
Set-Content -Path (Join-Path $BuildDir "README.txt") -Value $readme -Encoding UTF8

Write-Host ""
Write-Host "[成功] 一体化构建产物已整理至: $BuildDir" -ForegroundColor Green
Get-ChildItem $BuildDir | Format-Table Name, Length, LastWriteTime -AutoSize
