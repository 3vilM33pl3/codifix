# SPDX-License-Identifier: AGPL-3.0-or-later
# Short application-level lab smoke: no provider, installation, SCM or live data.
[CmdletBinding()]
param([Parameter(Mandatory=$true)][guid]$OperationId,
      [Parameter(Mandatory=$true)][guid]$BuildOperationId,
      [Parameter(Mandatory=$true)][string]$SourceRoot,
      [string]$WebRoot)
$ErrorActionPreference='Stop'; $ProgressPreference='SilentlyContinue'
if($env:OS -cne 'Windows_NT' -or $env:COMPUTERNAME -cne 'MEMORY-WINLAB' -or
    $OperationId -eq [guid]::Empty -or $BuildOperationId -eq [guid]::Empty -or
    $SourceRoot -cnotmatch '^C:\\MemoryLab\\source(?:\\[A-Za-z0-9_.-]+)+$') {throw 'Explicit disposable native lab required'}
foreach($name in @('OPENAI_API_KEY','ANTHROPIC_API_KEY','DATABASE_URL','MEMORY_LAYER_TEST_DATABASE_URL')){
    if([Environment]::GetEnvironmentVariable($name)){throw 'Ambient credentials rejected'}
}
$build=Get-Content "C:\MemoryLab\runs\$BuildOperationId\product-build\result.json" -Raw | ConvertFrom-Json
$git='C:\MemoryLab\Tools\git\cmd\git.exe'
$head=& $git -C $SourceRoot rev-parse HEAD
if($LASTEXITCODE -or $build.state -cne 'succeeded' -or $build.action -cne 'Build' -or $build.source_commit -cne $head){throw 'Matching successful native build required'}
if((& $git -C $SourceRoot status --porcelain --untracked-files=no)){throw 'Clean source required'}
$binary='C:\MemoryLab\build\product-msvc\x86_64-pc-windows-msvc\debug\memory.exe'
$pin=@($build.artifacts | Where-Object file -ceq 'memory.exe')
if($pin.Count -ne 1 -or (Get-FileHash $binary).Hash.ToLowerInvariant() -cne $pin[0].sha256){throw 'Native binary changed'}
$root="C:\MemoryLab\runs\$OperationId\core-workflow"
if(Test-Path $root){throw 'Preserve previous evidence; use fresh operation'}
$null=New-Item -ItemType Directory $root
$sid=[Security.Principal.WindowsIdentity]::GetCurrent().User.Value
& "$env:SystemRoot\System32\icacls.exe" $root /inheritance:r /grant:r "*${sid}:(OI)(CI)F" '*S-1-5-18:(OI)(CI)F' '*S-1-5-32-544:(OI)(CI)F' | Out-Null
if($LASTEXITCODE){throw 'Private lab evidence required'}
$code=Join-Path $root 'code';$null=New-Item -ItemType Directory $code
Copy-Item $binary (Join-Path $code 'memory.exe');$binary=Join-Path $code 'memory.exe'
$pg='C:\MemoryLab\NativePostgres';$psql=Join-Path $pg 'runtime\pgsql\bin\psql.exe'
$pgProof=Get-Content (Join-Path $pg 'verified.json') -Raw | ConvertFrom-Json
if($pgProof.host -cne '127.0.0.1' -or $pgProof.port -ne 55471 -or $pgProof.production -ne $false){throw 'Verified disposable PG required'}
$secret=ConvertTo-SecureString (Get-Content (Join-Path $pg 'test-password.dpapi') -Raw).Trim()
$pointer=[Runtime.InteropServices.Marshal]::SecureStringToBSTR($secret)
try{$password=[Runtime.InteropServices.Marshal]::PtrToStringBSTR($pointer)}finally{[Runtime.InteropServices.Marshal]::ZeroFreeBSTR($pointer)}
$database='lab_core_'+$OperationId.ToString('N');$instance=[guid]::NewGuid().ToString()
$token=[guid]::NewGuid().ToString('N')+[guid]::NewGuid().ToString('N')
$listener=[Net.Sockets.TcpListener]::new([Net.IPAddress]::Loopback,0);$listener.Start();$port=$listener.LocalEndpoint.Port;$listener.Stop()
$origin="http://127.0.0.1:$port";$config=Join-Path $root 'memory-layer.toml'
$web=Join-Path $SourceRoot 'web\dist'
if($WebRoot){
    if($WebRoot -cnotmatch '^C:\\MemoryLab\\source(?:\\[A-Za-z0-9_.-]+)+\\web\\dist$'){throw 'Explicit lab Web build required'}
    $web=$WebRoot
}
# A separately built Web tree is explicit, never discovered from live installs.
$hasWeb=Test-Path (Join-Path $web 'index.html')
$webLine=if($hasWeb){"web_root = '"+$web+"'"}else{''}
$text=@"
[area]
name = 'workshop'
instance_id = '$instance'
[service]
bind_addr = '127.0.0.1:$port'
api_token = '$token'
$webLine
[database]
url = 'postgresql://memorylab_test:$([uri]::EscapeDataString($password))@127.0.0.1:55471/$database'
[features]
llm_curation = false
[embeddings]
enabled = false
create_enabled = false
[background_work]
enabled = false
[cluster]
enabled = false
[automation]
enabled = false
[offline]
enabled = false
[reinforcement]
enabled = false
[provenance]
reverify_enabled = false
[consolidation]
enabled = false
[procedural]
enabled = false
[mcp]
enabled = false
http_enabled = false
[telemetry]
enabled = false
"@
[IO.File]::WriteAllText($config,$text,[Text.UTF8Encoding]::new($false));$text=$null
$receipt=[ordered]@{format='memory-native-core-workflow-v1';state='running';source_commit=$head;binary_sha256=$pin[0].sha256
    operation_id=$OperationId.ToString();build_operation_id=$BuildOperationId.ToString();database=$database;instance=$instance
    native_database=$true;real_provider_calls=0;installed_service_verified=$false;browser_interaction_verified=$false;checks=@()}
function Save-Receipt{[IO.File]::WriteAllText((Join-Path $root 'result.json'),($receipt | ConvertTo-Json -Depth 8),[Text.UTF8Encoding]::new($false))}
function Invoke-Owned([string]$Exe,[string[]]$Arguments,[string]$Label,[string]$PgPassword=''){
    $start=[Diagnostics.ProcessStartInfo]::new();$start.FileName=$Exe
    $start.Arguments=($Arguments | ForEach-Object {'"'+($_ -replace '(\\*)"','$1$1\"' -replace '(\\+)$','$1$1')+'"'}) -join ' '
    $start.UseShellExecute=$false;$start.CreateNoWindow=$true;$start.WorkingDirectory=$root
    $start.RedirectStandardOutput=$true;$start.RedirectStandardError=$true;$start.EnvironmentVariables.Clear()
    foreach($key in @('SystemRoot','WINDIR')){$start.EnvironmentVariables[$key]=$env:SystemRoot}
    foreach($key in @('HOME','USERPROFILE','APPDATA','LOCALAPPDATA','TEMP','TMP')){$start.EnvironmentVariables[$key]=$root}
    $start.EnvironmentVariables['PATH']=$env:SystemRoot+'\System32'
    if($PgPassword){$start.EnvironmentVariables['PGPASSWORD']=$PgPassword}
    $process=[Diagnostics.Process]::new();$process.StartInfo=$start;$null=$process.Start()
    $stdout=$process.StandardOutput.ReadToEndAsync();$stderr=$process.StandardError.ReadToEndAsync()
    return @{process=$process;stdout=$stdout;stderr=$stderr;label=$Label}
}
function Finish-Owned($Run){
    $p=$Run.process
    if(-not $p.WaitForExit(60000)){$p.Kill();$p.WaitForExit();throw 'Owned core command deadline'}
    $p.WaitForExit();$out=$Run.stdout.GetAwaiter().GetResult();$err=$Run.stderr.GetAwaiter().GetResult()
    [IO.File]::WriteAllText((Join-Path $root ($Run.label+'.stdout.log')),$out)
    [IO.File]::WriteAllText((Join-Path $root ($Run.label+'.stderr.log')),$err)
    if($p.ExitCode){throw ('Owned core command failed: '+$Run.label)}
    return $out
}
function Stop-Owned($Run){
    if(-not $Run.process.HasExited){$Run.process.Kill();$Run.process.WaitForExit()}
    [IO.File]::WriteAllText((Join-Path $root ($Run.label+'.stdout.log')),$Run.stdout.GetAwaiter().GetResult())
    [IO.File]::WriteAllText((Join-Path $root ($Run.label+'.stderr.log')),$Run.stderr.GetAwaiter().GetResult())
    $Run.process.Dispose()
}
Add-Type -AssemblyName System.Net.Http
$handler=[Net.Http.HttpClientHandler]::new();$handler.UseProxy=$false;$handler.AllowAutoRedirect=$false
$client=[Net.Http.HttpClient]::new($handler);$client.Timeout=[TimeSpan]::FromSeconds(10)
$client.MaxResponseContentBufferSize=1MB;$client.DefaultRequestHeaders.Authorization=[Net.Http.Headers.AuthenticationHeaderValue]::new('Bearer',$token)
function Request([string]$Path,$Body=$null){
    $response=if($null -eq $Body){$client.GetAsync($origin+$Path).GetAwaiter().GetResult()}else{
        $content=[Net.Http.StringContent]::new(($Body | ConvertTo-Json -Depth 12),[Text.Encoding]::UTF8,'application/json')
        try{$client.PostAsync($origin+$Path,$content).GetAwaiter().GetResult()}finally{$content.Dispose()}}
    try{if(-not $response.IsSuccessStatusCode){throw ('Core HTTP failure '+[int]$response.StatusCode+' at '+$Path)}
        return $response.Content.ReadAsStringAsync().GetAwaiter().GetResult()}finally{$response.Dispose()}
}
$service=$null
Save-Receipt
try{
    $null=Finish-Owned (Invoke-Owned $psql @('-h','127.0.0.1','-p','55471','-U','memorylab_test','-d','postgres','-v','ON_ERROR_STOP=1','-c',('CREATE DATABASE '+$database)) 'create-database' $password)
    $password=$null
    for($generation=1;$generation -le 2;$generation++){
        $service=Invoke-Owned $binary @('--config',$config,'service','run') ('service-'+$generation)
        $ready=$false
        for($i=0;$i -lt 60;$i++){
            if($service.process.HasExited){break}
            try{$health=Request '/healthz' | ConvertFrom-Json;if($health.area_instance_id -ceq $instance){$ready=$true;break}}catch{}
            Start-Sleep -Milliseconds 500
        }
        if(-not $ready){throw 'Exact synthetic service readiness failed'}
        if($generation -eq 1){
            $capture=@{project='native-smoke';task_title='Synthetic native workflow';user_prompt='Remember the synthetic backup port';writer_id='native-lab'
                agent_summary='The synthetic backup listener uses port 43117.';idempotency_key=$OperationId.ToString()
                structured_candidates=@(@{canonical_text='The synthetic backup listener uses port 43117.';summary='Synthetic backup port';memory_type='implementation';confidence=0.95;importance=4;tags=@('synthetic','native');sources=@(@{source_kind='note';excerpt='The synthetic backup listener uses port 43117.'})})}
            $first=Request '/v1/capture/task' $capture | ConvertFrom-Json
            $second=Request '/v1/capture/task' $capture | ConvertFrom-Json
            if($first.raw_capture_id -cne $second.raw_capture_id){throw 'Capture replay changed identity'}
            $null=Request '/v1/curate' @{project='native-smoke';raw_capture_id=$first.raw_capture_id}
            $receipt.checks+=@('capture','capture-idempotency','curation')
        }
        $browse=Request '/v1/projects/native-smoke/memories' | ConvertFrom-Json
        $found=@($browse.items | Where-Object {$_.preview -match '43117'})
        if($found.Count -ne 1){throw 'Main memory listing lost or duplicated curated fact'}
        if($generation -eq 1){$memoryId=$found[0].id}else{if($memoryId -cne $found[0].id){throw 'Restart changed memory identity'}}
        $answer=Request '/v1/query' @{project='native-smoke';query='synthetic backup port';retrieval_mode='lexical';answer_mode='deterministic'} | ConvertFrom-Json
        if(@($answer.results).Count -lt 1 -or ($answer.results | ConvertTo-Json -Depth 16) -notmatch '43117'){throw 'Real lexical search omitted curated fact'}
        $null=Request ('/v1/memory/'+$memoryId+'/history')
        if($generation -eq 1){
            $cli=Finish-Owned (Invoke-Owned $binary @('--config',$config,'query','--question','synthetic backup port','--project','native-smoke') 'cli-query')
            if($cli -notmatch '43117'){throw 'CLI search omitted curated fact'}
            $receipt.checks+=@('main-memories-list','lexical-search','history','cli-query')
            if($hasWeb){$page=Request '/';if($page -notmatch '<html|<!doctype'){throw 'Web root did not serve HTML'};$receipt.checks+='web-root'}
        }else{$receipt.checks+='restart-preserves-memory-and-search'}
        Stop-Owned $service;$service=$null
    }
    $receipt.state='succeeded';$receipt.curated_memory_id=$memoryId
}catch{$receipt.state='failed';throw}
finally{
    if($service){Stop-Owned $service}
    $password=$null;$token=$null;$client.Dispose();$handler.Dispose()
    $receipt.observed_at=[DateTime]::UtcNow.ToString('o');Save-Receipt
}
