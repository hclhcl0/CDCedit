param([ValidateSet('Check','Build','Test','ContentTest','Clippy')][string]$Mode='Build', [switch]$NoAppIncremental)
$ErrorActionPreference='Stop'
$taskRepo=Split-Path $PSScriptRoot -Parent
$taskTools=Join-Path $taskRepo '.local-tools'
if(Test-Path (Join-Path $taskTools 'cargo/bin/cargo.exe')){
    $env:CARGO_HOME=Join-Path $taskTools 'cargo'
    $env:RUSTUP_HOME=Join-Path $taskTools 'rustup'
    $env:PATH=(Join-Path $env:CARGO_HOME 'bin')+';'+$env:PATH
}
$taskFF=Get-ChildItem (Join-Path $taskTools 'ffmpeg') -Directory -ErrorAction SilentlyContinue|Select-Object -First 1
if($taskFF){$env:FFMPEG_DIR=$taskFF.FullName;$env:PATH=(Join-Path $taskFF.FullName 'bin')+';'+$env:PATH}
$taskSherpa=Join-Path $taskTools 'sherpa/sherpa-onnx-v1.13.7-win-x64-shared-MT-Release-lib/lib'
if(Test-Path $taskSherpa){
    # sherpa-onnx-sys 1.13.7's DLL copier recognizes only debug/release.
    # Stage import libraries alone for the quick profile; runtime DLLs stay on PATH.
    $taskSherpaLinks=Join-Path $taskTools 'sherpa-link'
    New-Item -ItemType Directory -Path $taskSherpaLinks -Force|Out-Null
    Get-ChildItem $taskSherpa -Filter '*.lib'|Copy-Item -Destination $taskSherpaLinks -Force
    $env:SHERPA_ONNX_LIB_DIR=$taskSherpaLinks;$env:PATH=$taskSherpa+';'+$env:PATH
}
$taskOrt=Join-Path $taskTools 'onnxruntime/onnxruntime-win-x64-1.28.0/lib'
if(Test-Path $taskOrt){$env:ORT_LIB_LOCATION=$taskOrt;$env:ORT_PREFER_DYNAMIC_LINK='1';$env:PATH=$taskOrt+';'+$env:PATH}
$taskLLVM=Join-Path $taskTools 'llvm'
if(Test-Path $taskLLVM){$env:LLVM_HOME=$taskLLVM;$env:LIBCLANG_PATH=Join-Path $taskLLVM 'bin'}
$taskVsWhere=Join-Path ${env:ProgramFiles(x86)} 'Microsoft Visual Studio/Installer/vswhere.exe'
if(Test-Path $taskVsWhere){
    $taskVS=& $taskVsWhere -latest -products '*' -property installationPath
    $taskCmake=Join-Path $taskVS 'Common7/IDE/CommonExtensions/Microsoft/CMake/CMake/bin'
    if(Test-Path $taskCmake){$env:PATH=$taskCmake+';'+$env:PATH}
}
$taskLog=Join-Path $taskRepo "scratch/concat-flow-$($Mode.ToLower()).log"
New-Item -ItemType Directory -Path (Split-Path $taskLog -Parent) -Force|Out-Null
Push-Location (Join-Path $taskRepo 'src')
try {
    $taskArgs=@('-p','concat','--profile','quick','--offline','--config','profile.quick.incremental=false')
    # The generated desktop UI (136 MB app.rs) produces a 14+ GB incremental query graph on Windows
    # that causes severe pagefile thrashing; keeping profile.quick.incremental=false avoids it.
    if($NoAppIncremental){}
    Write-Host "Concat $Mode - log: $taskLog"
    $prevEAP = $ErrorActionPreference
    $ErrorActionPreference = 'Continue'
    switch($Mode){
        'Check' {& cargo check @taskArgs *> $taskLog}
        'Build' {& cargo build @taskArgs *> $taskLog}
        'Test' {& cargo test @taskArgs --lib flow -- --nocapture *> $taskLog}
        'ContentTest' {& cargo test -p concat-speech --profile quick --offline --lib flow_ -- --nocapture *> $taskLog}
        'Clippy' {& cargo clippy @taskArgs *> $taskLog}
    }
    $taskCode=$LASTEXITCODE
    $ErrorActionPreference = $prevEAP
    Get-Content $taskLog -Tail 35
    if($taskCode -ne 0){throw "Concat $Mode failed ($taskCode). See $taskLog"}
    if($Mode -eq 'Build'){
        $taskBin=Join-Path $taskRepo 'src/target/quick'
        $taskLicenses=Join-Path $taskBin 'licenses'
        New-Item -ItemType Directory -Path $taskLicenses -Force|Out-Null
        Copy-Item -LiteralPath (Join-Path $taskRepo 'docs/licenses/autopunk-media-skills-MIT.txt') -Destination $taskLicenses -Force
        # Also applies to an existing linked artifact without recompiling the UI.
        if($taskVS){
            $taskMsvc=Get-ChildItem (Join-Path $taskVS 'VC/Tools/MSVC') -Directory|Sort-Object Name|Select-Object -Last 1
            $taskEditbin=Join-Path $taskMsvc.FullName 'bin/Hostx64/x64/editbin.exe'
            if(Test-Path $taskEditbin){& $taskEditbin /NOLOGO /STACK:33554432 (Join-Path $taskBin 'cdcedit.exe');if($LASTEXITCODE -ne 0){throw 'Could not set native UI stack size'}}
        }
        # Preserve the final overlay order, but do not rewrite identical DLLs
        # held by an older running editor. ORT wins shared sherpa filenames.
        $taskRuntime=@{}
        $taskRuntimeDirs=@()
        if($taskFF){
            $taskRuntimeDirs+=(Join-Path $taskFF.FullName 'bin')
            Copy-Item -LiteralPath (Join-Path $taskFF.FullName 'bin\ffmpeg.exe') -Destination $taskBin -Force -ErrorAction SilentlyContinue
        }
        if(Test-Path $taskSherpa){$taskRuntimeDirs+=$taskSherpa}
        if(Test-Path $taskOrt){$taskRuntimeDirs+=$taskOrt}
        foreach($taskDir in $taskRuntimeDirs){
            Get-ChildItem -LiteralPath $taskDir -Filter '*.dll'|ForEach-Object {$taskRuntime[$_.Name]=$_.FullName}
        }
        foreach($taskName in $taskRuntime.Keys){
            $taskTarget=Join-Path $taskBin $taskName
            $taskSrcItem=Get-Item -LiteralPath $taskRuntime[$taskName]
            $taskDstItem=Get-Item -LiteralPath $taskTarget -ErrorAction SilentlyContinue
            if(-not $taskDstItem -or $taskSrcItem.Length -ne $taskDstItem.Length){
                Copy-Item -LiteralPath $taskRuntime[$taskName] -Destination $taskTarget -Force
            }
        }
        # Keep packaged MCP tools in sync with source when an EXE build is requested.
        & (Join-Path $taskRepo 'scripts/package-flow-worker.ps1') -Destination (Join-Path $taskBin 'google-flow-worker')
    }
} finally {Pop-Location}
