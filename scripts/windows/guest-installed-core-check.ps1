# SPDX-License-Identifier: AGPL-3.0-or-later
# Verify a completed lab installation, including recovery of a failed driver.
# Does not rewrite its original receipt, repackage, migrate or call providers.
[CmdletBinding()]
param([guid]$InstallOperationId,[string]$SourceRoot)
$ErrorActionPreference='Stop';$ProgressPreference='SilentlyContinue'
if($env:OS -cne 'Windows_NT' -or $env:COMPUTERNAME -cne 'MEMORY-WINLAB' -or
    $InstallOperationId -eq [guid]::Empty -or
    $SourceRoot -cnotmatch '^C:\\MemoryLab\\source(?:\\[A-Za-z0-9_.-]+)+$' -or
    $SourceRoot.Split('\') -contains '..'){throw 'Exact disposable lab installation required'}
. (Join-Path $SourceRoot 'scripts/windows/guest-core-install.ps1') -FunctionsOnly -InstallOperationId $InstallOperationId -SourceRoot $SourceRoot
. (Join-Path $SourceRoot 'packaging/windows/native-instance-functions.ps1')
$root="C:\MemoryLab\runs\$InstallOperationId\core-install"
$oldPath=Join-Path $root 'result.json';Assert-CoreInstallPath $oldPath
$old=Get-Content -LiteralPath $oldPath -Raw | ConvertFrom-Json
if($old.format -cne 'memory-native-core-install-v1' -or $old.operation_id -cne $InstallOperationId.ToString() -or
    $old.lab_only -ne $true -or $old.release_candidate -ne $false -or $old.real_provider_calls -ne 0 -or
    $old.installer_build -lt 60000 -or $old.installer_build -gt 60999){throw 'Original bounded lab installation receipt required'}
$corePath="C:\MemoryLab\runs\$($old.core_operation_id)\core-workflow\result.json"
if((Get-CoreInstallHash $corePath) -cne $old.core_receipt_sha256){throw 'Completed core receipt changed'}
$core=Get-Content -LiteralPath $corePath -Raw | ConvertFrom-Json
$preview=Get-Content -LiteralPath (Join-Path $root 'preview.stdout.log') -Raw | ConvertFrom-Json
$journalPath="C:\ProgramData\Codifix\operations\$InstallOperationId\journal.json"
Assert-CoreInstallPath $journalPath
$journal=Get-Content -LiteralPath $journalPath -Raw | ConvertFrom-Json
if($journal.phase -cne 'completed' -or $journal.input_sha256 -cne $old.preview_input_sha256 -or
    $journal.operation_id -cne $InstallOperationId.ToString()){throw 'Exact completed installation journal required'}
$name='Codifix-lab-core-'+$InstallOperationId.ToString('N').Substring(0,16)
if($old.service_name -cne $name -or $preview.plan.service_name -cne $name){throw 'Exact selected lab service required'}
$release='C:\Program Files\Codifix\releases\windows-v2.1.0-rc.5-build.'+$old.installer_build
$config='C:\ProgramData\Codifix\configurations\'+$preview.plan.name+'\codifix.toml'
if((Get-CoreInstallHash $config) -cne $old.config_sha256 -or
    (Get-CoreInstallHash "$release\bin\codifix.exe") -cne $core.binary_sha256){throw 'Installed configuration or executable differs'}
$binding=Get-CoreInstallBinding ([IO.File]::ReadAllText($config)) ([guid]$old.core_operation_id) ([guid]$old.instance)
$binding.password=$null
$account='NT SERVICE\'+$name
$sid=([Security.Principal.NTAccount]::new($account)).Translate([Security.Principal.SecurityIdentifier]).Value
$git='C:\MemoryLab\Tools\git\cmd\git.exe';$head=(& $git -C $SourceRoot rev-parse HEAD)
if($LASTEXITCODE -or $head -cnotmatch '^[0-9a-f]{40}$'){throw 'Committed verifier source required'}
$proof=[ordered]@{format='memory-native-installed-core-verification-v1';operation_id=$InstallOperationId.ToString();state='running'
    lab_only=$true;release_candidate=$false;original_receipt_sha256=(Get-CoreInstallHash $oldPath)
    helper_source_commit=$head;verifier_sha256=(Get-CoreInstallHash $PSCommandPath);memory_sha256=$core.binary_sha256
    service_name=$name;service_sid=$sid;checks=@();real_provider_calls=0;reboot_verified=$false;upgrade_verified=$false;sync_verified=$false}
$proofPath=Join-Path $root 'installed-verification.json'
if(Test-Path -LiteralPath $proofPath){throw 'Preserve existing installed verification evidence'}
Add-Type -AssemblyName System.Net.Http
$handler=[Net.Http.HttpClientHandler]::new();$handler.UseProxy=$false;$handler.AllowAutoRedirect=$false
$client=[Net.Http.HttpClient]::new($handler);$client.Timeout=[TimeSpan]::FromSeconds(5);$client.MaxResponseContentBufferSize=4MB
$client.DefaultRequestHeaders.Authorization=[Net.Http.Headers.AuthenticationHeaderValue]::new('Bearer',$binding.token)
$origin='http://127.0.0.1:'+$binding.port
function Read-InstalledCore([string]$Path,$Body=$null){
    $response=if($null -eq $Body){$client.GetAsync($origin+$Path).GetAwaiter().GetResult()}else{
        $content=[Net.Http.StringContent]::new(($Body | ConvertTo-Json),[Text.Encoding]::UTF8,'application/json')
        try{$client.PostAsync($origin+$Path,$content).GetAwaiter().GetResult()}finally{$content.Dispose()}}
    try{if(-not $response.IsSuccessStatusCode){throw 'Installed lab HTTP check failed'};return $response.Content.ReadAsStringAsync().GetAwaiter().GetResult()}finally{$response.Dispose()}
}
try{
    for($pass=0;$pass -lt 2;$pass++){
        if($pass -eq 1){Restart-Service -Name $name;(Get-Service $name).WaitForStatus('Running',[TimeSpan]::FromSeconds(30))}
        $ready=$false
        for($attempt=0;$attempt -lt 30;$attempt++){
            try{$health=Read-InstalledCore '/healthz' | ConvertFrom-Json;if(Test-NativeReadinessHealth $health ([guid]$old.instance)){$ready=$true;break}}catch{}
            Start-Sleep -Seconds 1
        }
        if(-not $ready){throw 'Installed instance unavailable'}
        $scm=Get-CimInstance Win32_Service -Filter "Name='$name'"
        if($scm.State -cne 'Running' -or $scm.StartName -ine $account){throw 'Exact running virtual service account required'}
        $wrapper=Get-CimInstance Win32_Process -Filter "ProcessId=$($scm.ProcessId)"
        $owner=Invoke-CimMethod -InputObject $wrapper -MethodName GetOwnerSid
        if($owner.ReturnValue -ne 0 -or $owner.Sid -cne $sid){throw 'Wrapper identity differs'}
        $children=@(Get-CimInstance Win32_Process -Filter "ParentProcessId=$($scm.ProcessId)" | Where-Object ExecutablePath -ieq "$release\bin\codifix.exe")
        if($children.Count -ne 1){throw 'Exact installed Memory child required'}
        $owner=Invoke-CimMethod -InputObject $children[0] -MethodName GetOwnerSid
        if($owner.ReturnValue -ne 0 -or $owner.Sid -cne $sid){throw 'Memory identity differs'}
        $browse=Read-InstalledCore '/v1/projects/native-smoke/memories' | ConvertFrom-Json
        $found=@($browse.items | Where-Object {$_.id -ceq $core.curated_memory_id -and $_.preview -match '43117'})
        if($found.Count -ne 1){throw 'Previously curated memory identity missing'}
        $answer=Read-InstalledCore '/v1/query' @{project='native-smoke';query='synthetic backup port';retrieval_mode='lexical';answer_mode='deterministic'} | ConvertFrom-Json
        if(@($answer.results).Count -lt 1 -or ($answer.results | ConvertTo-Json -Depth 16) -notmatch '43117'){throw 'Existing synthetic fact not searchable'}
        if($pass -eq 0){
            $html=Read-InstalledCore '/';$sha=[Security.Cryptography.SHA256]::Create()
            try{$hash=([BitConverter]::ToString($sha.ComputeHash([Text.Encoding]::UTF8.GetBytes($html)))).Replace('-','').ToLowerInvariant()}finally{$sha.Dispose()}
            if($hash -cne $old.web_index_sha256){throw 'Packaged Web bytes differ'}
            $proof.checks+=@('scm-wrapper-and-child-virtual-identity','exact-instance-health','packaged-web-html','main-memory-identity','lexical-search')
        }else{$proof.checks+='scm-restart-preserves-memory-and-search'}
    }
    $proof.state='succeeded';$proof.curated_memory_id=$core.curated_memory_id
}finally{
    $proof.observed_at=[DateTime]::UtcNow.ToString('o');$client.Dispose();$handler.Dispose();$binding.token=$null
    Write-CoreInstallNew $proofPath ([Text.UTF8Encoding]::new($false).GetBytes(($proof | ConvertTo-Json -Depth 8)))
}
$proof | ConvertTo-Json -Depth 8
