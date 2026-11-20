param([ValidateSet('test','build')][string]$Stage = 'build')
$ErrorActionPreference = 'Stop'
$root = Split-Path $PSScriptRoot -Parent
$cargo = Join-Path $env:USERPROFILE '.cargo\bin\cargo.exe'
$vcvars = 'C:\Program Files (x86)\Microsoft Visual Studio\2022\BuildTools\VC\Auxiliary\Build\vcvars64.bat'
$args = if ($Stage -eq 'test') { 'test --release -p eco-audio --lib' } else { 'build --release --bin recorder_mic' }
$timeout = if ($Stage -eq 'test') { 300000 } else { 480000 }
$env:CARGO_NET_RETRY = '0'
$env:CARGO_HTTP_TIMEOUT = '20'
$log = Join-Path $root "$Stage.log"
$command = 'call "{0}" >nul && "{1}" {2} > "{3}" 2>&1' -f $vcvars,$cargo,$args,$log
$start = [Diagnostics.ProcessStartInfo]::new('cmd.exe', '/d /s /c "' + $command + '"')
$start.WorkingDirectory = Join-Path $root 'src-tauri'
$start.UseShellExecute = $false
$start.CreateNoWindow = $true
$process = [Diagnostics.Process]::Start($start)
if (-not $process.WaitForExit($timeout)) {
    & taskkill.exe /PID $process.Id /T /F | Out-Null
    Write-Output "$Stage timed out after $timeout ms; process tree terminated"
    exit 124
}
Get-Content $log -Tail 35
exit $process.ExitCode
