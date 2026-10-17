# SPDX-License-Identifier: AGPL-3.0-or-later
# Portable helpers only; never starts a guest/native build/provider/database.
$ErrorActionPreference='Stop'
$scriptPath=Join-Path $PSScriptRoot 'guest-native-eval-bootstrap.ps1'
$tokens=$null;$errors=$null
$null=[Management.Automation.Language.Parser]::ParseFile($scriptPath,[ref]$tokens,[ref]$errors)
if($errors.Count){throw 'Native evaluation bootstrap PowerShell parser errors'}
. $scriptPath -FunctionsOnly
function Reject-Eval([scriptblock]$Action){$failed=$false;try{&$Action}catch{$failed=$true};if(-not $failed){throw 'Unsafe evaluation input accepted'}}
$root=Join-Path ([IO.Path]::GetTempPath()) ('memory-native-eval-test-'+[guid]::NewGuid())
$null=New-Item -ItemType Directory -Path $root
try{
    $source=Join-Path $root 'source.bin';$copy=Join-Path $root 'copy.bin'
    Write-EvalNew $source ([Text.UTF8Encoding]::new($false).GetBytes('original bytes'))
    $hash=Get-EvalHash $source
    Copy-EvalFile $source $copy $hash
    if((Get-EvalHash $copy) -cne $hash){throw 'Evaluation copy hash changed'}
    Reject-Eval {Copy-EvalFile $source $copy $hash}
    Reject-Eval {Copy-EvalFile $source (Join-Path $root 'wrong.bin') ('0'*64)}
    if(Test-Path (Join-Path $root 'wrong.bin')){throw 'Changed source copied before validation'}
    $inventory=@(Get-EvalInventory $root);Assert-EvalInventory $root $inventory
    [IO.File]::AppendAllText($copy,'changed')
    Reject-Eval {Assert-EvalInventory $root $inventory}
    $json=Join-Path $root 'receipt.json';Write-EvalJson $json @{prepared=$false}
    Reject-Eval {Write-EvalJson $json @{prepared=$true}}
    if((Get-Content -LiteralPath $json -Raw | ConvertFrom-Json).prepared -ne $false){throw 'Existing evidence overwritten'}
    if((Join-EvalArguments @('one','C:\private\two')) -cne '"one" "C:\private\two"'){throw 'Fixed process arguments changed'}
    Reject-Eval {Join-EvalArguments @('contains"quote')}
}finally{Remove-Item -LiteralPath $root -Recurse -Force}
Write-Output 'Native evaluation bootstrap portable helpers passed; native execution was not attempted.'
