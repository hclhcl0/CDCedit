param([string]$Destination = "C:\Users\Admin\Desktop\CDCedit-windows-x64\google-flow-worker\dist")
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

if (-not (Test-Path $Destination)) {
    New-Item -ItemType Directory -Path $Destination -Force | Out-Null
}

Copy-Item -Recurse -Force "$workerDir\dist\*" $Destination
$sw.Stop()
Write-Host "✅ [Fast Flow Dev] Synced worker to $Destination in $($sw.Elapsed.TotalSeconds.ToString('F1'))s!" -ForegroundColor Green
Write-Host "💡 CDCedit can execute new Flow features immediately WITHOUT restarting the app or rebuilding the .exe!" -ForegroundColor Yellow
