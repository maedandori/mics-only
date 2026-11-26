$ErrorActionPreference='Stop'
$root=Join-Path $env:TEMP ("mics-launcher-test-"+$PID)
New-Item -ItemType Directory -Path $root -Force | Out-Null
$launcher=$null
try {
  Copy-Item target/release/mics-only.exe $root
  Copy-Item target/release/mics-core.exe (Join-Path $root 'mics-core-a.exe')
  $launcher=Start-Process (Join-Path $root 'mics-only.exe') -WorkingDirectory $root -PassThru
  # No INI: core must exit but supervisor must survive and retain its state.
  Start-Sleep -Seconds 8
  $launcher.Refresh()
  if($launcher.HasExited){throw 'Launcher exited after a broken Core start'}
  $state=Get-Content (Join-Path $root 'data/core-state.json') -Raw | ConvertFrom-Json
  if($state.active.name -ne 'a'){throw 'Initial slot A not recorded'}
  # Fix local config without any Firebird or network access; launcher should restart core.
  @'
[mics]
enabled=0
agent_id=ci-test
display_name=CI
host=
port=3050
database=
user=
coordinator_url=
interval_seconds=30
'@ | Set-Content (Join-Path $root 'mics.ini') -Encoding ascii
  $core=$null
  for($i=0;$i -lt 35;$i++){
    try{$core=Invoke-RestMethod 'http://127.0.0.1:8787/api/status' -TimeoutSec 2}catch{}
    if($core -and $core.ok -and $core.mode -eq 'MIRROR_ONLY'){break}
    Start-Sleep -Seconds 2
  }
  if(!$core -or !$core.ok -or $core.mode -ne 'MIRROR_ONLY' -or $core.version -ne $state.active.version){throw 'Core failed to recover without launcher restart'}
  $coreId=[int]$core.pid
  Stop-Process -Id $launcher.Id -Force
  $launcher.WaitForExit()
  for($i=0;$i -lt 10;$i++){
    if(!(Get-Process -Id $coreId -ErrorAction SilentlyContinue)){break}
    Start-Sleep -Seconds 1
  }
  if(Get-Process -Id $coreId -ErrorAction SilentlyContinue){throw 'Core survived launcher termination'}
  Write-Host 'Launcher crash recovery and child job lifetime verified'
}finally{
  if($launcher -and !$launcher.HasExited){Stop-Process -Id $launcher.Id -Force -ErrorAction SilentlyContinue}
  Remove-Item $root -Recurse -Force -ErrorAction SilentlyContinue
}
