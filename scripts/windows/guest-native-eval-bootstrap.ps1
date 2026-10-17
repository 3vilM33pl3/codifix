# SPDX-License-Identifier: AGPL-3.0-or-later
# Fixed prepare-only acceptance fixture. No database/provider/session activation.
[CmdletBinding()]
param(
    [ValidateSet('PrepareOnly')][string]$Action='PrepareOnly',
    [string]$SourceRoot,
    [guid]$OperationId,
    [guid]$RustProofOperationId,
    [switch]$FunctionsOnly
)
$ErrorActionPreference='Stop'
function Assert-EvalPath([string]$Path) {
    $current=[IO.Path]::GetFullPath($Path)
    while($current){
        if(Test-Path -LiteralPath $current){
            if((Get-Item -LiteralPath $current -Force).Attributes -band [IO.FileAttributes]::ReparsePoint){throw 'Evaluation fixture rejects reparse points'}
        }
        $parent=[IO.Path]::GetDirectoryName($current);if($parent -eq $current){break};$current=$parent
    }
}
function Get-EvalHash([string]$Path){Assert-EvalPath $Path;return (Get-FileHash -LiteralPath $Path -Algorithm SHA256).Hash.ToLowerInvariant()}
function Write-EvalNew([string]$Path,[byte[]]$Bytes){
    Assert-EvalPath $Path
    $file=[IO.File]::Open($Path,[IO.FileMode]::CreateNew,[IO.FileAccess]::Write,[IO.FileShare]::None)
    try{$file.Write($Bytes,0,$Bytes.Length);$file.Flush($true)}finally{$file.Dispose()}
}
function Write-EvalJson([string]$Path,$Value){Write-EvalNew $Path ([Text.UTF8Encoding]::new($false).GetBytes(($Value | ConvertTo-Json -Depth 20)))}
function Copy-EvalFile([string]$From,[string]$To,[string]$Expected){
    if($Expected -cnotmatch '^[0-9a-f]{64}$' -or (Get-EvalHash $From) -cne $Expected){throw 'Evaluation source bytes changed'}
    $null=New-Item -ItemType Directory -Path (Split-Path -Parent $To) -Force
    $inputFile=[IO.File]::Open($From,[IO.FileMode]::Open,[IO.FileAccess]::Read,[IO.FileShare]::Read)
    try{$outputFile=[IO.File]::Open($To,[IO.FileMode]::CreateNew,[IO.FileAccess]::Write,[IO.FileShare]::None)
        try{$inputFile.CopyTo($outputFile);$outputFile.Flush($true)}finally{$outputFile.Dispose()}
    }finally{$inputFile.Dispose()}
    if((Get-EvalHash $To) -cne $Expected){throw 'Evaluation private copy changed'}
}
function Get-EvalInventory([string]$Root){
    Assert-EvalPath $Root
    $entries=@(Get-ChildItem -LiteralPath $Root -Recurse -Force)
    if($entries | Where-Object {$_.Attributes -band [IO.FileAttributes]::ReparsePoint}){throw 'Evaluation inventory contains a reparse point'}
    $files=@($entries | Where-Object {-not $_.PSIsContainer} | Sort-Object FullName)
    if(-not $files.Count -or $files.Count -gt 65536){throw 'Bounded nonempty evaluation inventory required'}
    return @($files | ForEach-Object {@{path=$_.FullName;sha256=(Get-EvalHash $_.FullName)}})
}
function Assert-EvalInventory([string]$Root,$Expected){
    $actual=@(Get-EvalInventory $Root);$pins=@{}
    foreach($pin in @($Expected)){
        if($pins.ContainsKey($pin.path) -or $pin.sha256 -cnotmatch '^[0-9a-f]{64}$'){throw 'Invalid reviewed evaluation inventory'}
        $pins[$pin.path]=$pin.sha256
    }
    if($actual.Count -ne $pins.Count){throw 'Reviewed evaluation inventory changed'}
    foreach($pin in $actual){if(-not $pins.ContainsKey($pin.path) -or $pins[$pin.path] -cne $pin.sha256){throw 'Reviewed evaluation bytes changed'}}
}
function Join-EvalArguments([string[]]$Arguments){
    return (@($Arguments | ForEach-Object {
        if($_ -match '["\x00\r\n]' -or $_.EndsWith('\')){throw 'Unsupported evaluation process argument'}
        '"'+$_+'"'
    }) -join ' ')
}
if($FunctionsOnly){return}
if($env:OS -ne 'Windows_NT' -or $env:COMPUTERNAME -ne 'MEMORY-WINLAB' -or
    $env:PROCESSOR_ARCHITECTURE -ne 'AMD64' -or $OperationId -eq [guid]::Empty -or
    $RustProofOperationId -eq [guid]::Empty -or $OperationId -eq $RustProofOperationId -or
    $SourceRoot -cnotmatch '^C:\\MemoryLab\\source(?:\\[A-Za-z0-9_.-]+)+$' -or $SourceRoot.Split('\') -contains '..'){
    throw 'Explicit disposable AMD64 native evaluation source and distinct operations required'
}
$identity=[Security.Principal.WindowsIdentity]::GetCurrent()
if($identity.Name -ine 'MEMORY-WINLAB\memorylab' -or [Diagnostics.Process]::GetCurrentProcess().SessionId -eq 0){throw 'Ordinary interactive memorylab identity required'}
foreach($name in @('OPENAI_API_KEY','ANTHROPIC_API_KEY','GEMINI_API_KEY','AZURE_OPENAI_API_KEY','DATABASE_URL','MEMORY_LAYER_TEST_DATABASE_URL')){
    if([Environment]::GetEnvironmentVariable($name)){throw 'Prepare-only fixture rejects ambient provider/database credentials'}
}
Assert-EvalPath $SourceRoot
$git='C:\MemoryLab\Tools\git\cmd\git.exe'
$head=(& $git -C $SourceRoot rev-parse HEAD)
if($LASTEXITCODE -or $head -cnotmatch '^[0-9a-f]{40}$'){throw 'Committed evaluation source required'}
$changes=(& $git -C $SourceRoot status --porcelain --untracked-files=no)
if($LASTEXITCODE -or $changes){throw 'Clean committed evaluation source required'}
$oldRoot=Join-Path 'C:\MemoryLab\runs' ($RustProofOperationId.ToString()+'\native-rust-eval')
$oldProfilePath=Join-Path $oldRoot 'profile.json';$oldResultPath=Join-Path $oldRoot 'result.json'
Assert-EvalPath $oldRoot
$oldProfile=Get-Content -LiteralPath $oldProfilePath -Raw | ConvertFrom-Json
$oldResult=Get-Content -LiteralPath $oldResultPath -Raw | ConvertFrom-Json
if($oldResult.format -cne 'memory-native-rust-evaluation-proof-v1' -or $oldResult.state -cne 'succeeded' -or
    $oldResult.positive_compilation_verified -ne $true -or $oldResult.wrong_answer_rejected -ne $true -or
    $oldResult.broken_source_rejected -ne $true -or $oldResult.real_provider_requests -ne 0 -or
    $oldResult.official_evidence -ne $false -or @($oldProfile.toolchains).Count -ne 1 -or
    $oldProfile.toolchains[0].root -cne (Join-Path $oldRoot 'code')){throw 'Successful exact prior native Rust fixture required'}
Assert-EvalInventory $oldProfile.toolchains[0].root $oldProfile.toolchains[0].files
if((Get-EvalHash $oldResult.compiler.path) -cne $oldResult.compiler.sha256){throw 'Prior reviewed compiler changed'}
$root=Join-Path 'C:\MemoryLab\runs' ($OperationId.ToString()+'\native-eval-bootstrap')
if(Test-Path -LiteralPath $root){throw 'Retain prior bootstrap evidence; use a new explicit operation'}
Assert-EvalPath $root
$null=New-Item -ItemType Directory -Path $root
$sid=$identity.User.Value
& "$env:SystemRoot\System32\icacls.exe" $root /inheritance:r /grant:r "*${sid}:(OI)(CI)F" '*S-1-5-18:(OI)(CI)F' '*S-1-5-32-544:(OI)(CI)F' | Out-Null
if($LASTEXITCODE){throw 'Private bootstrap operation ACL required'}
$receipt=[ordered]@{format='memory-native-eval-bootstrap-proof-v1';action=$Action;operation_id=$OperationId.ToString();source_commit=$head
    state='running';old_rust_operation=$RustProofOperationId.ToString();old_profile_sha256=(Get-EvalHash $oldProfilePath)
    old_result_sha256=(Get-EvalHash $oldResultPath);script_sha256=(Get-EvalHash $PSCommandPath)
    provider_requests=0;database_contacted=$false;campaign_verified=$false;official_evidence=$false;execution_started=$false;prepare_verified=$false;steps=@()}
function Save-EvalResult{
    $partial=Join-Path $root ('result-'+[guid]::NewGuid().ToString()+'.partial.json');$target=Join-Path $root 'result.json'
    Write-EvalJson $partial $receipt
    if([IO.File]::Exists($target)){[IO.File]::Replace($partial,$target,[NullString]::Value)}else{[IO.File]::Move($partial,$target)}
}
$system=$env:SystemRoot;$userProfile=$env:USERPROFILE
$buildEnv=@{SystemRoot=$system;WINDIR=$system;USERPROFILE=$userProfile;HOME=$userProfile;TEMP=(Join-Path $root 'temp');TMP=(Join-Path $root 'temp')
    PATH=((Join-Path $userProfile '.cargo\bin')+';C:\MemoryLab\Tools\git\cmd;'+(Join-Path $system 'System32'))
    CARGO_TARGET_DIR='C:\MemoryLab\build\product-msvc';CARGO_BUILD_TARGET='x86_64-pc-windows-msvc';CARGO_BUILD_JOBS='2'
    CARGO_HOME=(Join-Path $userProfile '.cargo');RUSTUP_HOME=(Join-Path $userProfile '.rustup');RUSTUP_TOOLCHAIN='1.99.1-x86_64-pc-windows-msvc'
    GIT_TERMINAL_PROMPT='0';GIT_CONFIG_NOSYSTEM='1';GIT_CONFIG_GLOBAL='NUL';ComSpec=(Join-Path $system 'System32\cmd.exe')
    ProgramFiles=[Environment]::GetFolderPath('ProgramFiles');'ProgramFiles(x86)'=[Environment]::GetFolderPath('ProgramFilesX86')}
$null=New-Item -ItemType Directory -Path $buildEnv.TEMP
function Invoke-EvalProcess([string]$Exe,[string[]]$Arguments,[string]$Label,[int]$Seconds,[hashtable]$Environment,[switch]$OuterJobOwner){
    $stdout=Join-Path $root ($Label+'.stdout.log');$stderr=Join-Path $root ($Label+'.stderr.log')
    $info=[Diagnostics.ProcessStartInfo]::new();$info.FileName=$Exe;$info.Arguments=Join-EvalArguments $Arguments
    $info.WorkingDirectory=$SourceRoot;$info.UseShellExecute=$false;$info.CreateNoWindow=$true
    $info.RedirectStandardOutput=$true;$info.RedirectStandardError=$true;$info.EnvironmentVariables.Clear()
    foreach($entry in $Environment.GetEnumerator()){$info.EnvironmentVariables[$entry.Key]=[string]$entry.Value}
    $outFile=[IO.File]::Open($stdout,[IO.FileMode]::CreateNew,[IO.FileAccess]::Write,[IO.FileShare]::Read)
    $errFile=[IO.File]::Open($stderr,[IO.FileMode]::CreateNew,[IO.FileAccess]::Write,[IO.FileShare]::Read)
    $process=[Diagnostics.Process]::new();$process.StartInfo=$info
    try{
        if(-not $process.Start()){throw 'Cannot start exact native evaluation child'}
        $outCopy=$process.StandardOutput.BaseStream.CopyToAsync($outFile);$errCopy=$process.StandardError.BaseStream.CopyToAsync($errFile)
        if(-not $process.WaitForExit($Seconds*1000)){
            if($OuterJobOwner){$process.Kill()}else{& "$system\System32\taskkill.exe" /PID $process.Id /T /F | Out-Null}
            $null=$process.WaitForExit(10000);throw 'Owned evaluation step exceeded its fixed deadline; retain evidence'
        }
        $process.WaitForExit();$outCopy.GetAwaiter().GetResult();$errCopy.GetAwaiter().GetResult()
        if($process.ExitCode -ne 0){throw 'Native evaluation step failed; inspect retained private logs'}
    }finally{$outFile.Dispose();$errFile.Dispose();$process.Dispose()}
    $receipt.steps+=@{label=$Label;stdout_sha256=(Get-EvalHash $stdout);stderr_sha256=(Get-EvalHash $stderr)};Save-EvalResult
}
$mutex=[Threading.Mutex]::new($false,'Local\MemoryLab-NativeProductBuild');$held=$false
$lock=Join-Path $SourceRoot 'Cargo.lock';$lockHash=Get-EvalHash $lock;$receipt.lock_sha256=$lockHash
Save-EvalResult
try{
    try{$held=$mutex.WaitOne(0)}catch [Threading.AbandonedMutexException]{$held=$true}
    if(-not $held){throw 'Another native product build owns the shared cache; no concurrent evaluation build'}
    $cargo=Join-Path $userProfile '.cargo\bin\cargo.exe'
    $receipt.cargo_sha256=Get-EvalHash $cargo;$receipt.rust_toolchain=$buildEnv.RUSTUP_TOOLCHAIN
    $specs=@(
        @{label='helper-build';args=@('-p','codifix-platform','--bin','codifix-windows-evaluation-helper','--features','windows-evaluation-helper')},
        @{label='memory-build';args=@('-p','codifix-cli','--bin','codifix')},
        @{label='snapshot-build';args=@('-p','codifix-test-support','--example','r3_snapshot')},
        @{label='score-retriever-build';args=@('-p','codifix-eval','--example','r3_score','--example','r3_retriever')})
    $buildClock=[Diagnostics.Stopwatch]::StartNew()
    foreach($spec in $specs){
        $remaining=7200-[int]$buildClock.Elapsed.TotalSeconds
        if($remaining -lt 1){throw 'Combined native evaluation build deadline exceeded'}
        Invoke-EvalProcess $cargo (@('build','--locked','--offline','--target','x86_64-pc-windows-msvc')+$spec.args) $spec.label $remaining $buildEnv
    }
    if((Get-EvalHash $lock) -cne $lockHash){throw 'Native build changed frozen Cargo.lock'}
    $code=Join-Path $root 'code';$io=Join-Path $root 'io';$source=Join-Path $root 'source\r3'
    $null=New-Item -ItemType Directory -Path $code,$io,$source
    $debug=Join-Path $buildEnv.CARGO_TARGET_DIR 'x86_64-pc-windows-msvc\debug'
    $selected=@{helper=(Join-Path $debug 'codifix-windows-evaluation-helper.exe');memory=(Join-Path $debug 'codifix.exe')
        snapshot=(Join-Path $debug 'examples\r3_snapshot.exe');score=(Join-Path $debug 'examples\r3_score.exe');retriever=(Join-Path $debug 'examples\r3_retriever.exe')}
    foreach($name in @($selected.Keys)){$target=if($name -eq 'helper'){Join-Path $root 'evaluation-helper.exe'}else{Join-Path $code ([IO.Path]::GetFileName($selected[$name]))}
        Copy-EvalFile $selected[$name] $target (Get-EvalHash $selected[$name]);$selected[$name]=$target}
    Copy-EvalFile $oldProfile.node.path (Join-Path $code 'node.exe') $oldProfile.node.sha256
    Copy-EvalFile (Join-Path $SourceRoot 'evals\r3\isolation\native-guest.mjs') (Join-Path $code 'native-guest.mjs') (Get-EvalHash (Join-Path $SourceRoot 'evals\r3\isolation\native-guest.mjs'))
    $sourceOriginal=Join-Path $SourceRoot 'evals\r3';$sourceFiles=@(Get-EvalInventory $sourceOriginal)
    foreach($pin in $sourceFiles){$relative=$pin.path.Substring($sourceOriginal.Length+1);Copy-EvalFile $pin.path (Join-Path $source $relative) $pin.sha256}
    $bound={param($File)@{path=$File;sha256=(Get-EvalHash $File)}}
    $profile=[ordered]@{schema_version=1;helper=(&$bound $selected.helper);io_root=$io;node=(&$bound (Join-Path $code 'node.exe'));guest=(&$bound (Join-Path $code 'native-guest.mjs'))
        toolchains=@($oldProfile.toolchains)+@(@{root=$code;files=@(Get-EvalInventory $code)})
        search_path=@($code)+@($oldProfile.search_path);library_path=@($oldProfile.library_path)}
    $profilePath=Join-Path $root 'profile.json';$executionPath=Join-Path $root 'execution.json';Write-EvalJson $profilePath $profile
    $hashDriver=Join-Path $root 'hash-profile.mjs'
    Write-EvalNew $hashDriver ([Text.UTF8Encoding]::new($false).GetBytes(@'
import fs from 'node:fs';import path from 'node:path';import {pathToFileURL} from 'node:url';
const [source,oldProfile,oldResult,profile,output]=process.argv.slice(2);
const {nativeProfileHash}=await import(pathToFileURL(path.join(source,'isolation/native-windows.mjs')));
const read=p=>JSON.parse(fs.readFileSync(p,'utf8').replace(/^\uFEFF/,''));
if(nativeProfileHash(read(oldProfile))!==read(oldResult).profile_sha256)throw Error('Prior native profile hash changed');
const value=read(profile);fs.writeFileSync(output,JSON.stringify({kind:'windows-native-v1',profile:value,profile_sha256:nativeProfileHash(value)}),{flag:'wx'});
'@))
    $runEnv=@{SystemRoot=$system;WINDIR=$system;PATH='';HOME=$root;USERPROFILE=$root;TEMP=$buildEnv.TEMP;TMP=$buildEnv.TEMP}
    Invoke-EvalProcess $profile.node.path @($hashDriver,$source,$oldProfilePath,$oldResultPath,$profilePath,$executionPath) 'profile-hash' 30 $runEnv
    $destination=Join-Path (Join-Path $io ([guid]::NewGuid().ToString())) 'pack';$output=Join-Path $io ([guid]::NewGuid().ToString())
    $descriptor=[ordered]@{schema_version=1;profile='memory-r3-native-bootstrap-prepare-v1';node=$profile.node;source=@{root=$source;files=@(Get-EvalInventory $source)}
        execution=(&$bound $executionPath);destination=$destination;compiler=$oldResult.compiler.path
        binaries=@{memory=$selected.memory;snapshot=$selected.snapshot;score=$selected.score;retriever=$selected.retriever}
        database_url='postgresql://synthetic:unused@127.0.0.1:9/memory_r3_prepare';one_family_per_track=$true;deadline_ms=120000}
    $descriptorPath=Join-Path $root 'bootstrap.json';Write-EvalJson $descriptorPath $descriptor
    $receipt.profile_file_sha256=Get-EvalHash $profilePath;$receipt.execution_sha256=Get-EvalHash $executionPath;$receipt.descriptor_sha256=Get-EvalHash $descriptorPath
    $receipt.native_profile_sha256=(Get-Content -LiteralPath $executionPath -Raw | ConvertFrom-Json).profile_sha256
    $receipt.source_inventory=$descriptor.source.files;$receipt.first_party=@($selected.GetEnumerator() | ForEach-Object {@{kind=$_.Key;sha256=(Get-EvalHash $_.Value)}})
    Assert-EvalInventory $sourceOriginal $sourceFiles
    $currentHead=(& $git -C $SourceRoot rev-parse HEAD);$changes=(& $git -C $SourceRoot status --porcelain --untracked-files=no)
    if($LASTEXITCODE -or $currentHead -cne $head -or $changes){throw 'Reviewed source changed before native preparation'}
    Save-EvalResult
    Invoke-EvalProcess $selected.helper @('--bootstrap-prepare',$descriptorPath,'--sha256',$receipt.descriptor_sha256,'--output',$output) 'launcher' 150 $runEnv -OuterJobOwner
    $launcher=Get-Content -LiteralPath (Join-Path $root 'launcher.stdout.log') -Raw | ConvertFrom-Json
    if($launcher.format -cne 'memory-r3-native-bootstrap-prepare-result-v1' -or $launcher.state -cne 'prepared' -or
        $launcher.descriptor_sha256 -cne $receipt.descriptor_sha256 -or $launcher.node_sha256 -cne $profile.node.sha256 -or
        $launcher.execution_sha256 -cne $receipt.execution_sha256 -or $launcher.outer_job_reaped -ne $true -or
        $launcher.execution_started -ne $false -or $launcher.official_evidence -ne $false -or $launcher.destination -cne $destination){throw 'Exact prepare-only launcher receipt required'}
    if((Get-EvalHash (Join-Path $output 'stdout.bin')) -cne $launcher.stdout_sha256 -or
        (Get-EvalHash (Join-Path $output 'stderr.bin')) -cne $launcher.stderr_sha256){throw 'Private launcher output hashes changed'}
    $receipt.launcher=$launcher;$receipt.pack_manifest_sha256=Get-EvalHash (Join-Path $destination 'campaign.json')
    $receipt.pack_runtime_sha256=Get-EvalHash (Join-Path $destination 'private\runtime.json')
    if((Get-EvalHash $oldProfilePath) -cne $receipt.old_profile_sha256 -or (Get-EvalHash $oldResultPath) -cne $receipt.old_result_sha256){throw 'Previous proof was changed'}
    Assert-EvalInventory $oldProfile.toolchains[0].root $oldProfile.toolchains[0].files
    if((Get-EvalHash $lock) -cne $lockHash){throw 'Preparation changed frozen Cargo.lock'}
    $receipt.state='succeeded';$receipt.prepare_verified=$true
}catch{$receipt.state='failed';throw}finally{
    try{$receipt.completed_at=[DateTime]::UtcNow.ToString('o');Save-EvalResult}
    finally{if($held){$mutex.ReleaseMutex()};$mutex.Dispose()}
}
