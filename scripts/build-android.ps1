#Requires -Version 5.1
Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

$RootDir = Split-Path -Parent $PSScriptRoot
$AndroidDir = Join-Path $RootDir "android-client"
$BuildDir = Join-Path $RootDir "Build"
$WorkDir = Join-Path $AndroidDir "build"

$SdkDir = Join-Path $env:LOCALAPPDATA "Android\Sdk"
$BuildTools = Get-ChildItem (Join-Path $SdkDir "build-tools") | Sort-Object Name -Descending | Select-Object -First 1 -ExpandProperty FullName
$PlatformJar = (Get-ChildItem (Join-Path $SdkDir "platforms") -Recurse -Filter "android.jar" | Sort-Object FullName -Descending | Select-Object -First 1).FullName

$Aapt2 = Join-Path $BuildTools "aapt2.exe"
$D8 = Join-Path $BuildTools "d8.bat"
$ZipAlign = Join-Path $BuildTools "zipalign.exe"
$ApkSigner = Join-Path $BuildTools "apksigner.bat"

if (Test-Path $WorkDir) { Remove-Item -Recurse -Force $WorkDir }
New-Item -ItemType Directory -Path "$WorkDir\compiled" -Force | Out-Null
New-Item -ItemType Directory -Path "$WorkDir\classes" -Force | Out-Null
New-Item -ItemType Directory -Path "$WorkDir\gen" -Force | Out-Null
New-Item -ItemType Directory -Path "$AndroidDir\res\mipmap-xxhdpi" -Force | Out-Null

Copy-Item -Force (Join-Path $RootDir "src-tauri\icons\128x128.png") (Join-Path $AndroidDir "res\mipmap-xxhdpi\ic_launcher.png")

Write-Host "[1/5] aapt2 compile resources..." -ForegroundColor Cyan
& $Aapt2 compile --dir (Join-Path $AndroidDir "res") -o (Join-Path $WorkDir "res.zip")

Write-Host "[2/5] aapt2 link manifest & resources..." -ForegroundColor Cyan
$UnalignedApk = Join-Path $WorkDir "unaligned.apk"
& $Aapt2 link `
    -o $UnalignedApk `
    --manifest (Join-Path $AndroidDir "AndroidManifest.xml") `
    -I $PlatformJar `
    --java (Join-Path $WorkDir "gen") `
    (Join-Path $WorkDir "res.zip")

Write-Host "[3/5] javac & d8 compile Java..." -ForegroundColor Cyan
$JavaFiles = @(Get-ChildItem -Path (Join-Path $AndroidDir "src") -Recurse -Filter "*.java" | ForEach-Object { $_.FullName })
$GenFiles = @(Get-ChildItem -Path (Join-Path $WorkDir "gen") -Recurse -Filter "*.java" | ForEach-Object { $_.FullName })
& javac --release 8 -cp $PlatformJar -d (Join-Path $WorkDir "classes") ($JavaFiles + $GenFiles)

$ClassFiles = @(Get-ChildItem -Path (Join-Path $WorkDir "classes") -Recurse -Filter "*.class" | ForEach-Object { $_.FullName })
& $D8 --min-api 24 --output $WorkDir $ClassFiles

Write-Host "[4/5] pack classes.dex & zipalign..." -ForegroundColor Cyan
& jar uf $UnalignedApk -C $WorkDir classes.dex

$AlignedApk = Join-Path $WorkDir "aligned.apk"
& $ZipAlign -f 4 $UnalignedApk $AlignedApk

Write-Host "[5/5] sign APK..." -ForegroundColor Cyan
$Keystore = Join-Path $AndroidDir "debug.keystore"
if (-not (Test-Path $Keystore)) {
    & keytool -genkey -v -keystore $Keystore -storepass android -alias androiddebugkey -keypass android -keyalg RSA -keysize 2048 -validity 10000 -dname "CN=PicoVR-Manager,O=PicoVR,C=CN" | Out-Null
}

if (-not (Test-Path $BuildDir)) { New-Item -ItemType Directory -Path $BuildDir -Force | Out-Null }
$FinalApk = Join-Path $BuildDir "PicoVR-Manager-Android.apk"
& $ApkSigner sign --ks $Keystore --ks-pass pass:android --key-pass pass:android --out $FinalApk $AlignedApk

Write-Host "[成功] Android APK 已生成: $FinalApk" -ForegroundColor Green
Get-Item $FinalApk | Select-Object Name, Length, LastWriteTime
