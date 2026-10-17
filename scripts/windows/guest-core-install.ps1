# SPDX-License-Identifier: AGPL-3.0-or-later
# LAB ONLY: install a pinned debug Memory service against a completed synthetic
# core-workflow database. Never a published candidate, upgrade or reboot proof.
# Run this script from a separate committed operator source in an elevated
# MEMORY-WINLAB\memorylab task; do not check out or rebuild an active build tree.
# Example inputs (the same completed core operation is deliberately reused):
# -OperationId 399bb47c-2931-46e5-967f-e5c88625eb26
# -InstallOperationId <fresh-UUID-for-this-install-attempt>
# -InstallerBuild 60001 # Fresh LAB-ONLY identity for corrected package inputs.
# -BuildOperationId 19627f7d-bc81-4a6a-ab2d-7db3312447fe
# -SourceRoot C:\MemoryLab\source\<committed-operator-copy>
# -WebRoot C:\MemoryLab\source\native-stack-v6\web\dist
[CmdletBinding()]
param([guid]$OperationId,[guid]$InstallOperationId,[guid]$BuildOperationId,[string]$SourceRoot,[string]$WebRoot,
    [ValidateRange(60000,60999)][int]$InstallerBuild=60000,[switch]$FunctionsOnly)
$ErrorActionPreference='Stop';$ProgressPreference='SilentlyContinue'
function Assert-CoreInstallPath([string]$Path){
    $current=[IO.Path]::GetFullPath($Path)
    while($current){if(Test-Path -LiteralPath $current){if((Get-Item -LiteralPath $current -Force).Attributes -band [IO.FileAttributes]::ReparsePoint){throw 'Core installation rejects reparse points'}}
        $parent=[IO.Path]::GetDirectoryName($current);if($parent -eq $current){break};$current=$parent}
}
function Get-CoreInstallHash([string]$Path){Assert-CoreInstallPath $Path;return (Get-FileHash -LiteralPath $Path -Algorithm SHA256).Hash.ToLowerInvariant()}
function Write-CoreInstallNew([string]$Path,[byte[]]$Bytes){
    Assert-CoreInstallPath $Path;$file=[IO.File]::Open($Path,[IO.FileMode]::CreateNew,[IO.FileAccess]::Write,[IO.FileShare]::None)
    try{$file.Write($Bytes,0,$Bytes.Length);$file.Flush($true)}finally{$file.Dispose()}
}
function Get-CoreInstallExecutableSource([ValidateSet('memory.exe','memory-windows-service.exe')][string]$File,[string]$CoreRoot,[string]$DebugRoot){
    # Cargo may rewrite normal debug binaries during unrelated native tests.
    # The completed core proof already retained its exact private Memory bytes.
    if($File -ceq 'memory.exe'){return Join-Path $CoreRoot 'code\memory.exe'}
    return Join-Path $DebugRoot $File
}
function Copy-CoreInstallFile([string]$From,[string]$To,[string]$Expected,
    [ValidateSet('asset','memory.exe','memory-windows-service.exe')][string]$Component='asset'){
    try{
        if($Expected -cnotmatch '^[0-9a-f]{64}$' -or (Get-CoreInstallHash $From) -cne $Expected){throw 'Input pin differs'}
        Assert-CoreInstallPath $To;$null=New-Item -ItemType Directory -Path (Split-Path -Parent $To) -Force
        $inputFile=[IO.File]::Open($From,[IO.FileMode]::Open,[IO.FileAccess]::Read,[IO.FileShare]::Read)
        try{$outputFile=[IO.File]::Open($To,[IO.FileMode]::CreateNew,[IO.FileAccess]::Write,[IO.FileShare]::None)
            try{$inputFile.CopyTo($outputFile);$outputFile.Flush($true)}finally{$outputFile.Dispose()}}finally{$inputFile.Dispose()}
        if((Get-CoreInstallHash $To) -cne $Expected){throw 'Private payload pin differs'}
    }catch{throw ('Pinned core installation input unavailable or changed: '+$Component)}
}
function New-CoreInstallArchive([string]$Payload,[string]$Package,$PayloadFiles){
    New-NativeCanonicalArchive $Payload $Package $PayloadFiles
}
function Get-CoreInstallBinding([string]$Text,[guid]$Operation,[guid]$Instance){
    $patterns=@{instance="(?m)^instance_id = '([0-9a-f-]{36})'\r?$";port="(?m)^bind_addr = '127\.0\.0\.1:([0-9]{1,5})'\r?$"
        token="(?m)^api_token = '([0-9a-f]{64})'\r?$";database="(?m)^url = 'postgresql://memorylab_test:([^'\r\n]+)@127\.0\.0\.1:55471/(lab_core_[0-9a-f]{32})'\r?$"}
    $selected=@{}
    foreach($key in $patterns.Keys){$matches=[regex]::Matches($Text,$patterns[$key]);if($matches.Count -ne 1){throw 'Exact completed synthetic configuration required'};$selected[$key]=$matches[0]}
    # Only the exact credential-free, provider-disabled core recipe is accepted.
    # Do not silently carry new TOML sections or enabled background work into SCM.
    $shape=$Text.Replace("`r`n","`n").TrimEnd()
    foreach($key in $patterns.Keys){$shape=[regex]::Replace($shape,$patterns[$key],$key)}
    $shape=[regex]::Replace($shape,"(?m)^web_root = '[^'\r\n]*'\r?$",'web')
    $expected=@('[area]',"name = 'workshop'",'instance','[service]','port','token','web','[database]','database',
        '[features]','llm_curation = false','[embeddings]','enabled = false','create_enabled = false',
        '[background_work]','enabled = false','[cluster]','enabled = false','[automation]','enabled = false',
        '[offline]','enabled = false','[reinforcement]','enabled = false','[provenance]','reverify_enabled = false',
        '[consolidation]','enabled = false','[procedural]','enabled = false','[mcp]','enabled = false',
        'http_enabled = false','[telemetry]','enabled = false') -join "`n"
    if($shape -cne $expected){throw 'Exact provider-disabled completed core configuration required'}
    $port=[int]$selected.port.Groups[1].Value
    if($selected.instance.Groups[1].Value -cne $Instance.ToString() -or $port -lt 1 -or $port -gt 65535 -or
        $selected.database.Groups[2].Value -cne ('lab_core_'+$Operation.ToString('N'))){throw 'Core configuration differs from completed operation'}
    return @{port=$port;token=$selected.token.Groups[1].Value;password=[uri]::UnescapeDataString($selected.database.Groups[1].Value);database=$selected.database.Groups[2].Value}
}
function Set-CoreInstalledWeb([string]$Text,[string]$InstalledWeb){
    $pattern="(?m)^web_root = '[^'\r\n]*'\r?$"
    if([regex]::Matches($Text,$pattern).Count -ne 1 -or $InstalledWeb.Contains("'") -or $InstalledWeb.Contains("`n")){throw 'One explicit Web root required'}
    return [regex]::Replace($Text,$pattern,[Text.RegularExpressions.MatchEvaluator]{param($m)"web_root = '"+$InstalledWeb+"'"})
}
function Get-CoreInstallKnownFolderEnvironment{
    $folders=@{ProgramData='CommonApplicationData';ALLUSERSPROFILE='CommonApplicationData';ProgramFiles='ProgramFiles';'ProgramFiles(x86)'='ProgramFilesX86'
        CommonProgramFiles='CommonProgramFiles';'CommonProgramFiles(x86)'='CommonProgramFilesX86'}
    $result=@{}
    foreach($key in $folders.Keys){
        $folder=[Enum]::Parse([Environment+SpecialFolder],$folders[$key]);$path=[Environment]::GetFolderPath($folder)
        if(-not $path -or -not [IO.Path]::IsPathRooted($path) -or $path -cnotmatch '^[A-Za-z]:\\' -or
            $path.Contains([char]0) -or $path.Contains("`r") -or $path.Contains("`n")){throw ('Required Windows known folder unavailable: '+$key)}
        $result[$key]=$path
    }
    # Shell known-folder expansion needs SystemDrive even when ProgramData is
    # explicit. Without it .NET returns an empty CommonApplicationData path.
    $drive=[IO.Path]::GetPathRoot([Environment]::GetFolderPath('Windows')).TrimEnd('\')
    if($drive -cnotmatch '^[A-Za-z]:$'){throw 'Required Windows system drive unavailable'}
    $result.SystemDrive=$drive
    return $result
}
function New-CoreInstallerDiagnosticCommand([string[]]$Arguments,[string]$Diagnostic){
    if($Arguments.Count -lt 6 -or ($Arguments[0..4] -join ' ') -cne '-NoProfile -NonInteractive -ExecutionPolicy Bypass -File'){
        throw 'Fixed installer PowerShell invocation required'
    }
    # Only these driver-owned switches become syntax. Every argument value is a
    # single-quoted literal, including paths/recovery references containing '.
    $valueParameters=@('-Name','-OperationId','-PackagePath','-ManifestPath','-ManifestSha256','-ConfigPath',
        '-HealthUri','-ExpectedInstanceId','-RecoveryReference','-Action','-PreviewAction','-ExpectedInputSha256')
    $command="& '"+$Arguments[5].Replace("'","''")+"'"
    for($i=6;$i -lt $Arguments.Count;$i++){
        $parameter=$Arguments[$i]
        if($parameter -ceq '-DatabaseCompatibilityConfirmed'){$command+=' '+$parameter;continue}
        if($parameter -cnotin $valueParameters -or $i+1 -ge $Arguments.Count){throw 'Unexpected installer PowerShell argument'}
        $i++;$command+=' '+$parameter+" '"+$Arguments[$i].Replace("'","''")+"'"
    }
    $catch=@'
} catch {
    $diagnostic=@($_.InvocationInfo.PositionMessage,$_.ScriptStackTrace) -join [Environment]::NewLine
    $bytes=[Text.UTF8Encoding]::new($false).GetBytes($diagnostic)
    $file=[IO.File]::Open(DIAGNOSTIC_LITERAL,[IO.FileMode]::CreateNew,[IO.FileAccess]::Write,[IO.FileShare]::None)
    try{$file.Write($bytes,0,$bytes.Length);$file.Flush($true)}finally{$file.Dispose()}
    throw 'Exact installer child failed; inspect private failure diagnostic'
}
'@
    $catch=$catch.Replace('DIAGNOSTIC_LITERAL',("'"+$Diagnostic.Replace("'","''")+"'"))
    return '$ErrorActionPreference=''Stop''; try { '+$command+"`n"+$catch
}
if($FunctionsOnly){return}
if($env:OS -cne 'Windows_NT' -or $env:COMPUTERNAME -cne 'MEMORY-WINLAB' -or
    $OperationId -eq [guid]::Empty -or $InstallOperationId -eq [guid]::Empty -or $InstallOperationId -eq $OperationId -or $BuildOperationId -eq [guid]::Empty -or
    $SourceRoot -cnotmatch '^C:\\MemoryLab\\source(?:\\[A-Za-z0-9_.-]+)+$' -or
    $WebRoot -cnotmatch '^C:\\MemoryLab\\source(?:\\[A-Za-z0-9_.-]+)+\\web\\dist$' -or
    $env:PROCESSOR_ARCHITECTURE -cne 'AMD64' -or
    $SourceRoot.Split('\') -contains '..' -or $WebRoot.Split('\') -contains '..'){throw 'Exact disposable lab source, Web and operation required'}
$identity=[Security.Principal.WindowsIdentity]::GetCurrent();$principal=[Security.Principal.WindowsPrincipal]::new($identity)
if($identity.Name -ine 'MEMORY-WINLAB\memorylab' -or -not $principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)){
    throw 'Exact elevated MEMORY-WINLAB memorylab task required'
}
foreach($key in @('OPENAI_API_KEY','ANTHROPIC_API_KEY','GEMINI_API_KEY','DATABASE_URL','MEMORY_LAYER_TEST_DATABASE_URL')){
    if([Environment]::GetEnvironmentVariable($key)){throw 'Ambient provider/database credentials rejected'}
}
Assert-CoreInstallPath $SourceRoot;Assert-CoreInstallPath $WebRoot
$git='C:\MemoryLab\Tools\git\cmd\git.exe';$head=& $git -C $SourceRoot rev-parse HEAD
if($LASTEXITCODE -or $head -cnotmatch '^[0-9a-f]{40}$'){throw 'Committed installer source required'}
$changes=& $git -C $SourceRoot status --porcelain --untracked-files=no
if($LASTEXITCODE -or $changes){throw 'Clean installer source required; guest source is never edited'}
$operators=Join-Path $SourceRoot 'packaging\windows'
. (Join-Path $operators 'native-instance-functions.ps1');. (Join-Path $operators 'release-identity.ps1')
$coreRoot="C:\MemoryLab\runs\$OperationId\core-workflow";$corePath=Join-Path $coreRoot 'result.json'
Assert-CoreInstallPath $corePath
$core=Get-Content -LiteralPath $corePath -Raw | ConvertFrom-Json
$buildPath="C:\MemoryLab\runs\$BuildOperationId\product-build\result.json";Assert-CoreInstallPath $buildPath
$build=Get-Content -LiteralPath $buildPath -Raw | ConvertFrom-Json
if($core.format -cne 'memory-native-core-workflow-v1' -or $core.state -cne 'succeeded' -or $core.real_provider_calls -ne 0 -or
    $core.operation_id -cne $OperationId.ToString() -or $core.build_operation_id -cne $BuildOperationId.ToString() -or
    $core.checks -notcontains 'web-root' -or $core.checks -notcontains 'restart-preserves-memory-and-search' -or
    $build.format -cne 'memory-native-product-check-v1' -or $build.state -cne 'succeeded' -or $build.action -cne 'Build' -or
    $build.compilation_verified -ne $true -or $build.release_candidate -ne $false -or
    $build.source_commit -cnotmatch '^[0-9a-f]{40}$'){throw 'Completed matching core workflow and native ProductBuild required'}
$instance=[guid]$core.instance;if($instance -eq [guid]::Empty){throw 'Exact core instance identity required'}
$name='lab-core-'+$InstallOperationId.ToString('N').Substring(0,16);$serviceName='MemoryLayer-'+$name
if(Get-Service -Name $serviceName -ErrorAction SilentlyContinue){throw 'Existing lab service retained; no automatic adoption or reinstall'}
$root="C:\MemoryLab\runs\$InstallOperationId\core-install"
Assert-CoreInstallPath $root
if(Test-Path -LiteralPath $root){throw 'Retain installation evidence; no automatic repeat'}
$null=New-Item -ItemType Directory -Path $root
$sid=$identity.User.Value
& "$env:SystemRoot\System32\icacls.exe" $root /inheritance:r /grant:r "*${sid}:(OI)(CI)F" '*S-1-5-18:(OI)(CI)F' '*S-1-5-32-544:(OI)(CI)F' | Out-Null
if($LASTEXITCODE){throw 'Private installation rehearsal directory required'}
$releaseIdentity=Get-WindowsReleaseIdentity '2.1.0-rc.5' $InstallerBuild
$release=Join-Path ([Environment]::GetFolderPath('ProgramFiles')) ('Memory Layer\releases\'+$releaseIdentity.release_identity)
if(Test-Path -LiteralPath $release){throw 'Selected LAB-ONLY build already exists; preserve it and select a fresh build identity'}
$receipt=[ordered]@{format='memory-native-core-install-v1';state='running';operation_id=$InstallOperationId.ToString();core_operation_id=$OperationId.ToString();lab_only=$true;release_candidate=$false
    installer_build=$InstallerBuild;installer_source_commit=$head;binary_source_commit=$build.source_commit;core_source_commit=$core.source_commit
    build_operation_id=$BuildOperationId.ToString();build_receipt_sha256=(Get-CoreInstallHash $buildPath);core_receipt_sha256=(Get-CoreInstallHash $corePath)
    script_sha256=(Get-CoreInstallHash $PSCommandPath);service_name=$serviceName;instance=$instance.ToString();database_reused=$true
    real_provider_calls=0;installation_verified=$false;scm_virtual_identity_verified=$false;health_verified=$false;web_verified=$false;search_verified=$false
    reboot_verified=$false;upgrade_verified=$false;sync_verified=$false;failure_data_runtime_preserved=$true}
function Save-CoreInstall{
    $partial=Join-Path $root ('result-'+[guid]::NewGuid().ToString()+'.partial.json');$target=Join-Path $root 'result.json'
    Write-CoreInstallNew $partial ([Text.UTF8Encoding]::new($false).GetBytes(($receipt | ConvertTo-Json -Depth 12)))
    if([IO.File]::Exists($target)){[IO.File]::Replace($partial,$target,[NullString]::Value)}else{[IO.File]::Move($partial,$target)}
}
function Invoke-CoreInstallerProcess([string]$Exe,[string[]]$Arguments,[string]$Label,[string]$PgPassword=''){
    if($Label -in @('preview','install')){
        $command=New-CoreInstallerDiagnosticCommand $Arguments (Join-Path $root ($Label+'.failure.log'))
        $Arguments=@('-NoProfile','-NonInteractive','-ExecutionPolicy','Bypass','-Command',$command)
    }
    $info=[Diagnostics.ProcessStartInfo]::new();$info.FileName=$Exe;$info.Arguments=($Arguments | ForEach-Object {ConvertTo-NativeCommandArgument $_}) -join ' '
    $info.UseShellExecute=$false;$info.CreateNoWindow=$true;$info.WorkingDirectory=$root;$info.RedirectStandardOutput=$true;$info.RedirectStandardError=$true
    $info.EnvironmentVariables.Clear();$info.EnvironmentVariables['SystemRoot']=$env:SystemRoot;$info.EnvironmentVariables['WINDIR']=$env:SystemRoot
    $info.EnvironmentVariables['OS']='Windows_NT';$info.EnvironmentVariables['COMPUTERNAME']='MEMORY-WINLAB'
    $info.EnvironmentVariables['PROCESSOR_ARCHITECTURE']='AMD64'
    $knownFolders=Get-CoreInstallKnownFolderEnvironment
    foreach($key in $knownFolders.Keys){$info.EnvironmentVariables[$key]=$knownFolders[$key]}
    $info.EnvironmentVariables['PATH']=$env:SystemRoot+'\System32'
    foreach($key in @('HOME','USERPROFILE','APPDATA','LOCALAPPDATA','TEMP','TMP')){$info.EnvironmentVariables[$key]=$root}
    if($PgPassword){$info.EnvironmentVariables['PGPASSWORD']=$PgPassword}
    $process=[Diagnostics.Process]::new();$process.StartInfo=$info;$null=$process.Start()
    try{$stdout=$process.StandardOutput.ReadToEndAsync();$stderr=$process.StandardError.ReadToEndAsync()
        if(-not $process.WaitForExit(180000)){$process.Kill();$null=$process.WaitForExit(10000);throw 'Exact installer child exceeded three minutes; retain operation'}
        $process.WaitForExit();$out=$stdout.GetAwaiter().GetResult();$err=$stderr.GetAwaiter().GetResult()
        Write-CoreInstallNew (Join-Path $root ($Label+'.stdout.log')) ([Text.UTF8Encoding]::new($false).GetBytes($out))
        Write-CoreInstallNew (Join-Path $root ($Label+'.stderr.log')) ([Text.UTF8Encoding]::new($false).GetBytes($err))
        if($process.ExitCode -ne 0){throw ('Exact lab step failed: '+$Label)};return $out
    }finally{$process.Dispose()}
}
Save-CoreInstall
$client=$null;$handler=$null;$token=$null;$binding=$null
try{
    $payload=Join-Path $root 'LAB-ONLY-payload';$null=New-Item -ItemType Directory -Path $payload
    $debug='C:\MemoryLab\build\product-msvc\x86_64-pc-windows-msvc\debug'
    foreach($file in @('memory.exe','memory-windows-service.exe')){
        $pin=@($build.artifacts | Where-Object file -ceq $file)
        if($pin.Count -ne 1){throw ('Required native executable absent from ProductBuild receipt: '+$file)}
        if($file -eq 'memory.exe' -and $pin[0].sha256 -cne $core.binary_sha256){throw 'Completed core workflow used different native bytes'}
        $from=Get-CoreInstallExecutableSource $file $coreRoot $debug
        Copy-CoreInstallFile $from (Join-Path $payload ('bin\'+$file)) $pin[0].sha256 $file
    }
    $receipt.memory_input='completed-core-private-copy';$receipt.service_wrapper_input='original-build-pinned-cache'
    $webItems=@(Get-ChildItem -LiteralPath $WebRoot -Recurse -Force)
    if($webItems | Where-Object {$_.Attributes -band [IO.FileAttributes]::ReparsePoint}){throw 'Explicit Web tree contains reparse points'}
    foreach($file in @($webItems | Where-Object {-not $_.PSIsContainer})){
        $relative=$file.FullName.Substring($WebRoot.Length+1);Assert-NativeArchiveName $relative.Replace('\','/')
        Copy-CoreInstallFile $file.FullName (Join-Path $payload ('web\'+$relative)) (Get-CoreInstallHash $file.FullName)
    }
    if(-not (Test-Path (Join-Path $payload 'web\index.html'))){throw 'Explicit passed Web assets must include index.html'}
    $payloadFiles=@(Get-ChildItem -LiteralPath $payload -Recurse -File | Sort-Object FullName | ForEach-Object {
        @{name=$_.FullName.Substring($payload.Length+1).Replace('\','/');sha256=(Get-CoreInstallHash $_.FullName)}})
    Add-Type -AssemblyName System.IO.Compression.FileSystem
    $package=Join-Path $root ('LAB-ONLY-memory-rc5-build'+$InstallerBuild+'.zip')
    New-CoreInstallArchive $payload $package $payloadFiles
    $manifest=[ordered]@{schema_version=1;platform='windows-x86_64';application_version='2.1.0-rc.5';installer_build=$InstallerBuild
        release_identity=$releaseIdentity.release_identity;installer_version=$releaseIdentity.installer_version;lab_only=$true;release_candidate=$false
        signing_status='unsigned-lab-debug';native_stack_bundled=$false;payload_files=$payloadFiles
        artifacts=@(@{name=[IO.Path]::GetFileName($package);sha256=(Get-CoreInstallHash $package)})}
    $manifestPath=Join-Path $root 'LAB-ONLY-manifest.json'
    Write-CoreInstallNew $manifestPath ([Text.UTF8Encoding]::new($false).GetBytes(($manifest | ConvertTo-Json -Depth 10)))
    $receipt.package_sha256=Get-CoreInstallHash $package;$receipt.manifest_sha256=Get-CoreInstallHash $manifestPath;$receipt.web_index_sha256=Get-CoreInstallHash (Join-Path $payload 'web\index.html')
    $coreConfig=Join-Path $coreRoot 'memory-layer.toml';Assert-CoreInstallPath $coreConfig;$text=[IO.File]::ReadAllText($coreConfig)
    $binding=Get-CoreInstallBinding $text $OperationId $instance;$token=$binding.token
    $origin='http://127.0.0.1:'+$binding.port
    if(Get-NetTCPConnection -LocalPort $binding.port -State Listen -ErrorAction SilentlyContinue){throw 'Completed foreground core listener must be stopped before installation'}
    $config=Join-Path $root 'memory-layer.toml'
    $text=Set-CoreInstalledWeb $text (Join-Path $release 'web')
    Write-CoreInstallNew $config ([Text.UTF8Encoding]::new($false).GetBytes($text));$text=$null
    $receipt.config_sha256=Get-CoreInstallHash $config
    $pg='C:\MemoryLab\NativePostgres';$pgProofPath=Join-Path $pg 'verified.json';$pgProof=Get-Content -LiteralPath $pgProofPath -Raw | ConvertFrom-Json
    if($pgProof.host -cne '127.0.0.1' -or $pgProof.port -ne 55471 -or $pgProof.production -ne $false){throw 'Exact existing disposable native PostgreSQL required'}
    $psql=Join-Path $pg 'runtime\pgsql\bin\psql.exe'
    $schema=Invoke-CoreInstallerProcess $psql @('-X','-h','127.0.0.1','-p','55471','-U','memorylab_test','-d',$binding.database,'-A','-t','-v','ON_ERROR_STOP=1','-c',"SELECT coalesce(max(version),0)::text || ':' || (count(*) FILTER (WHERE NOT success))::text FROM _sqlx_migrations") 'schema-read' $binding.password
    $binding.password=$null
    if($schema.Trim() -cne '84:0'){throw 'Already prepared native schema84 required; installer never initializes or migrates the lab database'}
    $receipt.schema_version=84;$receipt.postgres_proof_sha256=Get-CoreInstallHash $pgProofPath
    $operator=Join-Path $operators 'native-instance.ps1';$receipt.operator_sha256=Get-CoreInstallHash $operator
    $receipt.operator_helpers=@(@('native-instance-functions.ps1','native-prerequisites-functions.ps1','release-identity.ps1') | ForEach-Object {
        @{name=$_;sha256=(Get-CoreInstallHash (Join-Path $operators $_))}})
    $recovery='LAB-ONLY: retain core '+$OperationId.ToString()+' database/config and install '+$InstallOperationId.ToString()+' artifacts'
    $arguments=@('-NoProfile','-NonInteractive','-ExecutionPolicy','Bypass','-File',$operator,'-Name',$name,'-OperationId',$InstallOperationId.ToString()
        '-PackagePath',$package,'-ManifestPath',$manifestPath,'-ManifestSha256',$receipt.manifest_sha256,'-ConfigPath',$config
        '-HealthUri',($origin+'/healthz'),'-ExpectedInstanceId',$instance.ToString(),'-RecoveryReference',$recovery)
    $powershell=Join-Path $env:SystemRoot 'System32\WindowsPowerShell\v1.0\powershell.exe'
    $preview=Invoke-CoreInstallerProcess $powershell ($arguments+@('-Action','Preview','-PreviewAction','Install')) 'preview' | ConvertFrom-Json
    if($preview.action -cne 'Install' -or $preview.plan.service_name -cne $serviceName -or $preview.input_sha256 -cnotmatch '^[0-9a-f]{64}$'){throw 'Exact actual installer Preview required'}
    $receipt.preview_input_sha256=$preview.input_sha256;Save-CoreInstall
    $null=Invoke-CoreInstallerProcess $powershell ($arguments+@('-Action','Install','-ExpectedInputSha256',$preview.input_sha256,'-DatabaseCompatibilityConfirmed')) 'install'
    $scm=Get-CimInstance Win32_Service -Filter "Name='$serviceName'"
    $account='NT SERVICE\'+$serviceName;$serviceSid=([Security.Principal.NTAccount]::new($account)).Translate([Security.Principal.SecurityIdentifier]).Value
    if($scm.State -cne 'Running' -or $scm.StartName -ine $account -or $scm.ProcessId -le 0){throw 'SCM virtual service identity is not running'}
    $process=Get-CimInstance Win32_Process -Filter "ProcessId=$($scm.ProcessId)"
    $owner=Invoke-CimMethod -InputObject $process -MethodName GetOwnerSid
    if($owner.ReturnValue -ne 0 -or $owner.Sid -cne $serviceSid){throw 'SCM wrapper process did not use its exact virtual account'}
    $children=@(Get-CimInstance Win32_Process -Filter "ParentProcessId=$($scm.ProcessId)" | Where-Object ExecutablePath -ieq (Join-Path $release 'bin\memory.exe'))
    if($children.Count -ne 1){throw 'Exact installed Memory child process required'}
    $childOwner=Invoke-CimMethod -InputObject $children[0] -MethodName GetOwnerSid
    if($childOwner.ReturnValue -ne 0 -or $childOwner.Sid -cne $serviceSid){throw 'Installed Memory child did not use its exact virtual account'}
    $receipt.scm_virtual_identity_verified=$true;$receipt.service_sid=$serviceSid;$receipt.wrapper_pid=$scm.ProcessId;$receipt.memory_pid=$children[0].ProcessId
    Add-Type -AssemblyName System.Net.Http
    $handler=[Net.Http.HttpClientHandler]::new();$handler.UseProxy=$false;$handler.AllowAutoRedirect=$false
    $client=[Net.Http.HttpClient]::new($handler);$client.Timeout=[TimeSpan]::FromSeconds(10);$client.MaxResponseContentBufferSize=4MB
    $client.DefaultRequestHeaders.Authorization=[Net.Http.Headers.AuthenticationHeaderValue]::new('Bearer',$token)
    function Read-CoreHttp([string]$Path,$Body=$null){
        $response=if($null -eq $Body){$client.GetAsync($origin+$Path).GetAwaiter().GetResult()}else{
            $content=[Net.Http.StringContent]::new(($Body | ConvertTo-Json -Depth 10),[Text.Encoding]::UTF8,'application/json')
            try{$client.PostAsync($origin+$Path,$content).GetAwaiter().GetResult()}finally{$content.Dispose()}}
        try{if(-not $response.IsSuccessStatusCode){throw 'Installed lab HTTP check failed'};return $response.Content.ReadAsStringAsync().GetAwaiter().GetResult()}finally{$response.Dispose()}
    }
    $health=Read-CoreHttp '/healthz' | ConvertFrom-Json
    if(-not (Test-NativeReadinessHealth $health $instance)){throw 'Installed loopback health has another instance'}
    $receipt.health_verified=$true
    $html=Read-CoreHttp '/';$sha=[Security.Cryptography.SHA256]::Create()
    try{$htmlHash=([BitConverter]::ToString($sha.ComputeHash([Text.Encoding]::UTF8.GetBytes($html)))).Replace('-','').ToLowerInvariant()}finally{$sha.Dispose()}
    if($html -notmatch '<html|<!doctype' -or $htmlHash -cne $receipt.web_index_sha256){throw 'Installed Web HTML differs from packaged passed assets'}
    $receipt.web_verified=$true
    $answer=Read-CoreHttp '/v1/query' @{project='native-smoke';query='synthetic backup port';retrieval_mode='lexical';answer_mode='deterministic'} | ConvertFrom-Json
    if(@($answer.results).Count -lt 1 -or ($answer.results | ConvertTo-Json -Depth 16) -notmatch '43117'){throw 'Installed service search omitted the existing curated synthetic fact'}
    $receipt.search_verified=$true;$receipt.curated_memory_id=$core.curated_memory_id
    $browse=Read-CoreHttp '/v1/projects/native-smoke/memories' | ConvertFrom-Json
    $found=@($browse.items | Where-Object {$_.id -ceq $core.curated_memory_id -and $_.preview -match '43117'})
    if($found.Count -ne 1){throw 'Installed service did not retain the exact previously curated memory identity'}
    if((Get-CoreInstallHash $corePath) -cne $receipt.core_receipt_sha256){throw 'Completed core proof changed during installation'}
    $receipt.installation_verified=$true;$receipt.state='succeeded'
}catch{$receipt.state='failed';throw}finally{
    $token=$null;if($binding){$binding.password=$null;$binding.token=$null};if($client){$client.Dispose()};if($handler){$handler.Dispose()}
    $receipt.observed_at=[DateTime]::UtcNow.ToString('o');Save-CoreInstall
    # No uninstall, rollback, service stop, runtime deletion or database drop.
}
