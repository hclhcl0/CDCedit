param([Parameter(Mandatory=$true)][string]$Destination)
$ErrorActionPreference = 'Stop'
$taskRoot = Split-Path $PSScriptRoot -Parent
$taskWorker = Join-Path $taskRoot 'tools\google-flow-worker'
Push-Location $taskWorker
try {
    if (-not (Test-Path node_modules)) { & npm ci --ignore-scripts; if ($LASTEXITCODE -ne 0) {throw 'npm ci failed'} }
    & node node_modules/typescript/bin/tsc -p tsconfig.build.json
    if ($LASTEXITCODE -ne 0) {throw 'Flow worker build failed'}
    $taskOut = [System.IO.Path]::GetFullPath($Destination)
    New-Item -ItemType Directory -Path $taskOut -Force | Out-Null
    foreach ($taskName in @('dist','node_modules','bridge.mjs','character-protocol.mjs','character-protocol.d.mts','LICENSE','LICENSE.characters','package.json','package-lock.json')) {
        $taskSourcePath=Join-Path $taskWorker $taskName
        if((Test-Path -LiteralPath $taskSourcePath -PathType Container) -and (Get-Command robocopy -ErrorAction SilentlyContinue)){
            $taskTargetPath=Join-Path $taskOut $taskName
            New-Item -ItemType Directory -Path $taskTargetPath -Force | Out-Null
            & robocopy $taskSourcePath $taskTargetPath /E /XO /MT:16 /R:1 /W:1 /NFL /NDL /NJH /NJS /NP
            if($LASTEXITCODE -gt 7){throw "Worker directory copy failed: $taskName ($LASTEXITCODE)"}
            $global:LASTEXITCODE=0
        }else{Copy-Item -LiteralPath $taskSourcePath -Destination $taskOut -Recurse -Force}
    }
    $taskNodeSource=(Get-Command node).Source
    $taskNodeTarget=Join-Path $taskOut 'node.exe'
    $taskRuntimeChanged= -not (Test-Path -LiteralPath $taskNodeTarget)
    if(-not $taskRuntimeChanged){$taskRuntimeChanged=(Get-FileHash -LiteralPath $taskNodeSource).Hash -ne (Get-FileHash -LiteralPath $taskNodeTarget).Hash}
    if($taskRuntimeChanged){Copy-Item -LiteralPath $taskNodeSource -Destination $taskNodeTarget -Force}
    $taskNodeVersion=(& node --version).Trim()
    if($taskRuntimeChanged -or -not (Test-Path (Join-Path $taskOut 'LICENSE.node.txt'))){Invoke-WebRequest -Uri "https://raw.githubusercontent.com/nodejs/node/$taskNodeVersion/LICENSE" -OutFile (Join-Path $taskOut 'LICENSE.node.txt')}
    Write-Host "Packaged Google Flow worker: $taskOut"
} finally {Pop-Location}
