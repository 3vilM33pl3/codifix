# SPDX-License-Identifier: AGPL-3.0-or-later
# Native parser fixture only: no SCM/service/elevation/credential operations.
$ErrorActionPreference='Stop'
if($env:OS -ne 'Windows_NT'){throw 'Native Windows argv parser required'}
. (Join-Path $PSScriptRoot 'native-instance-functions.ps1')
if(-not ('MemoryLayer.NativeScmArgvFixture' -as [type])){
    Add-Type -TypeDefinition @'
using System;
using System.ComponentModel;
using System.Runtime.InteropServices;
namespace MemoryLayer {
    public static class NativeScmArgvFixture {
        [DllImport("shell32.dll", CharSet=CharSet.Unicode, SetLastError=true)]
        private static extern IntPtr CommandLineToArgvW(string command, out int count);
        [DllImport("kernel32.dll")]
        private static extern IntPtr LocalFree(IntPtr memory);
        public static string[] Parse(string command) {
            int count;
            IntPtr memory=CommandLineToArgvW(command, out count);
            if(memory==IntPtr.Zero) throw new Win32Exception(Marshal.GetLastWin32Error());
            try {
                var values=new string[count];
                for(int index=0;index<count;index++)
                    values[index]=Marshal.PtrToStringUni(Marshal.ReadIntPtr(memory,index*IntPtr.Size));
                return values;
            } finally { LocalFree(memory); }
        }
    }
}
'@
}
$serviceValues=@('C:\Program Files\Memory Layer\bin\memory-windows-service.exe','--name','MemoryLayer-synthetic',
    '--directory','C:\ProgramData\MemoryLayer\instances\synthetic','--','C:\Program Files\Memory Layer\bin\memory.exe',
    '--config','C:\ProgramData\MemoryLayer\configurations\synthetic\memory-layer.toml','service','run')
$binaryPath=($serviceValues | ForEach-Object {ConvertTo-NativeCommandArgument $_}) -join ' '
$scValues=@('create','MemoryLayer-synthetic','binPath=',$binaryPath,'start=','demand','obj=','NT SERVICE\MemoryLayer-synthetic')
$encoded=($scValues | ForEach-Object {ConvertTo-NativeCommandArgument $_}) -join ' '
$observed=[MemoryLayer.NativeScmArgvFixture]::Parse('sc.exe '+$encoded)
if($observed.Count -ne ($scValues.Count+1)){throw 'SCM argv boundary count changed'}
for($index=0;$index -lt $scValues.Count;$index++){
    if($observed[$index+1] -cne $scValues[$index]){throw 'SCM argument boundary or embedded binaryPath quotes changed'}
}
$child=[MemoryLayer.NativeScmArgvFixture]::Parse($observed[4])
if($child.Count -ne $serviceValues.Count){throw 'Stored service command argument count changed'}
for($index=0;$index -lt $serviceValues.Count;$index++){
    if($child[$index] -cne $serviceValues[$index]){throw 'Stored service command argument changed'}
}
Write-Output 'Native Windows SCM/service argv roundtrip passed; no SCM operation was invoked.'
