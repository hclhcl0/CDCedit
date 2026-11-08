# CDCedit Android Local APK Build Script
# 100% Portable - No global machine changes required
$ErrorActionPreference = 'Stop'
$repoRoot = $PSScriptRoot
$localTools = Join-Path $repoRoot '.local-tools'

$env:JAVA_HOME = Join-Path $localTools 'jdk-21\jdk-21.0.6+7'
$env:ANDROID_HOME = Join-Path $localTools 'android-sdk'
$env:ANDROID_NDK_HOME = Join-Path $localTools 'android-sdk\ndk\28.2.13676358'
$env:ANDROID_NDK_ROOT = $env:ANDROID_NDK_HOME
$env:ANDROID_NDK = $env:ANDROID_NDK_HOME
$env:ANDROID_JAR = Join-Path $env:ANDROID_HOME 'platforms\android-35\android.jar'
$env:CARGO_HOME = Join-Path $localTools 'cargo'
$env:RUSTUP_HOME = Join-Path $localTools 'rustup'

$env:FFMPEG_DIR = Join-Path $repoRoot 'src\vendor\ffmpeg\aarch64-linux-android'
$env:SHERPA_ONNX_LIB_DIR = Join-Path $repoRoot 'src\vendor\sherpa-onnx\jniLibs\arm64-v8a'
$env:LIBCLANG_PATH = Join-Path $localTools 'llvm\bin'

$vswhere = 'C:\Program Files (x86)\Microsoft Visual Studio\Installer\vswhere.exe'
$vs = & $vswhere -latest -products '*' -property installationPath
$ninjaDir = Join-Path $vs 'Common7\IDE\CommonExtensions\Microsoft\CMake\Ninja'
$cmakeDir = Join-Path $vs 'Common7\IDE\CommonExtensions\Microsoft\CMake\CMake\bin'

$env:CMAKE_GENERATOR = "Ninja"
$env:CMAKE_MAKE_PROGRAM = Join-Path $ninjaDir 'ninja.exe'
Remove-Item Env:VSINSTALLDIR -ErrorAction SilentlyContinue
Remove-Item Env:CMAKE_GENERATOR_INSTANCE -ErrorAction SilentlyContinue

$ndkUnix = ($env:ANDROID_NDK_HOME -replace '\\', '/')
$sysrootUnix = "$ndkUnix/toolchains/llvm/prebuilt/windows-x86_64/sysroot"
$bindgenArgs = "--target=aarch64-linux-android26 --sysroot=$sysrootUnix -isystem$sysrootUnix/usr/include -isystem$sysrootUnix/usr/include/aarch64-linux-android"

$env:RUSTFLAGS = "-C link-arg=-Wl,-z,max-page-size=16384"
$env:BINDGEN_EXTRA_CLANG_ARGS = $bindgenArgs
$env:BINDGEN_EXTRA_CLANG_ARGS_aarch64_linux_android = $bindgenArgs

# Bound LLVM peak memory during Slint codegen (same strategy as [profile.quick.package.concat])
$env:CARGO_PROFILE_RELEASE_LTO = "false"
$env:CARGO_PROFILE_RELEASE_CODEGEN_UNITS = "4"
$env:CARGO_PROFILE_RELEASE_PACKAGE_CONCAT_OPT_LEVEL = "0"
$env:CARGO_PROFILE_RELEASE_PACKAGE_CONCAT_CODEGEN_UNITS = "16"
$env:CARGO_BUILD_JOBS = "2"
$env:CARGO_PROFILE_DEV_DEBUG = "false"

$env:PATH = "$env:CARGO_HOME\bin;$env:JAVA_HOME\bin;$env:ANDROID_HOME\cmdline-tools\latest\bin;$env:ANDROID_HOME\build-tools\35.0.0;$env:ANDROID_HOME\platform-tools;$env:ANDROID_NDK_HOME\toolchains\llvm\prebuilt\windows-x86_64\bin;$ninjaDir;$cmakeDir;$env:LIBCLANG_PATH;$env:PATH"

$keystore = Join-Path $localTools 'android-release.keystore'
if (-not (Test-Path $keystore)) {
    & "$env:JAVA_HOME\bin\keytool.exe" -genkeypair -v -keystore $keystore -storepass concat -keypass concat -alias concat -keyalg RSA -keysize 2048 -validity 10000 -dname "CN=CDCedit, O=CDCedit" | Out-Null
}
$env:CARGO_APK_RELEASE_KEYSTORE = $keystore
$env:CARGO_APK_RELEASE_KEYSTORE_PASSWORD = "concat"

Set-Location (Join-Path $repoRoot 'src')
Write-Host "Building CDCedit Android APK (Release)..." -ForegroundColor Cyan
$prevEap = $ErrorActionPreference
$ErrorActionPreference = 'Continue'
& "$env:CARGO_HOME\bin\cargo-apk.exe" apk build --release -p concat-android --target aarch64-linux-android
$exitCode = $LASTEXITCODE
$ErrorActionPreference = $prevEap
if ($exitCode -ne 0) {
    Write-Error "Build failed with exit code $exitCode"
    exit $exitCode
}

$apkPath = Join-Path $repoRoot 'src\target\aarch64-linux-android\release\apk\CDCedit.apk'
if (Test-Path $apkPath) {
    $outApk = "C:\Users\Admin\Desktop\CDCedit-android-arm64.apk"
    Copy-Item $apkPath $outApk -Force
    Write-Host "Build complete! APK saved to: $outApk" -ForegroundColor Green
}
