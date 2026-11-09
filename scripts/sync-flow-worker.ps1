param([string]$DestinationDir = "C:\Users\Admin\Desktop\CDCedit-windows-x64\google-flow-worker")
$ErrorActionPreference = 'Stop'
$repo = Split-Path $PSScriptRoot -Parent
$workerDir = Join-Path $repo 'tools\google-flow-worker'

$sw = [System.Diagnostics.Stopwatch]::StartNew()
Write-Host "⚡ [Fast Flow Dev] Compiling Google Flow Worker TypeScript..." -ForegroundColor Cyan
Push-Location $workerDir
try {
    & npm run build
    if ($LASTEXITCODE -ne 0) { throw "npm run build failed ($LASTEXITCODE)" }
} finally {
    Pop-Location
}

$distDir = Join-Path $DestinationDir "dist"
if (-not (Test-Path $distDir)) {
    New-Item -ItemType Directory -Path $distDir -Force | Out-Null
}

Copy-Item -Recurse -Force "$workerDir\dist\*" $distDir
Copy-Item -Force "$workerDir\bridge.mjs" $DestinationDir
if (Test-Path "$repo\src\target\quick\google-flow-worker") {
    Copy-Item -Recurse -Force "$workerDir\dist\*" "$repo\src\target\quick\google-flow-worker\dist"
    Copy-Item -Force "$workerDir\bridge.mjs" "$repo\src\target\quick\google-flow-worker"
}

# Stop any running node bridge process so CDCedit automatically spawns the fresh worker
Get-CimInstance Win32_Process -Filter "name = 'node.exe'" -ErrorAction SilentlyContinue | Where-Object { $_.CommandLine -like "*bridge.mjs*" } | ForEach-Object { Stop-Process -Id $_.ProcessId -Force -ErrorAction SilentlyContinue }

$sw.Stop()
Write-Host "✅ [Fast Flow Dev] Synced worker and bridge to $DestinationDir in $($sw.Elapsed.TotalSeconds.ToString('F1'))s!" -ForegroundColor Green
Write-Host "💡 CDCedit can execute new Flow features immediately WITHOUT restarting the app or rebuilding the .exe!" -ForegroundColor Yellow
