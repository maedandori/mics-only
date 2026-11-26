$ErrorActionPreference='Stop'
$root=Join-Path $env:TEMP ("mics-update-test-"+$PID)
$target=Join-Path $root 'target'
$pkg=Join-Path $root 'pkg'
$zip=Join-Path $root 'broken.zip'
$old=$null
try{
 New-Item -ItemType Directory -Path $target,$pkg -Force | Out-Null
 Copy-Item target/release/mics-only.exe $target
 Copy-Item settings-tauri/target/release/mics-only-settings.exe $target
 Copy-Item www $target -Recurse
 @'
[mics]
enabled=0
agent_id=ci-update
display_name=CI
host=
port=3050
database=
user=
coordinator_url=
interval_seconds=30
'@ | Set-Content (Join-Path $target 'mics.ini') -Encoding ascii
 $old=Start-Process (Join-Path $target 'mics-only.exe') -WorkingDirectory $target -PassThru
 $healthy=$null
 for($i=0;$i -lt 20;$i++){
  try{$healthy=Invoke-RestMethod 'http://127.0.0.1:8787/api/status' -TimeoutSec 2}catch{}
  if($healthy -and $healthy.mode -eq 'MIRROR_ONLY'){break}
  Start-Sleep -Seconds 1
 }
 if(!$healthy -or $healthy.mode -ne 'MIRROR_ONLY'){throw 'Old agent did not start'}
 $version=[version]$healthy.version
 $next="v$($version.Major).$($version.Minor).$($version.Build+1)"
 Copy-Item (Join-Path $target 'mics-only.exe') $pkg
 Copy-Item (Join-Path $target 'mics-only-settings.exe') $pkg
 # Invalid archive: preflight must not stop a working agent.
 Compress-Archive -Path "$pkg/*" -DestinationPath $zip
 & powershell.exe -NoProfile -ExecutionPolicy Bypass -File scripts/apply-update.ps1 -Archive $zip -Target $target -ParentId 0 -Version $next
 $preflightExit=$LASTEXITCODE
 if($preflightExit -ne 1){throw "Invalid archive returned unexpected exit code: $preflightExit"}
 $global:LASTEXITCODE=0
 $old.Refresh();if($old.HasExited){throw 'Preflight killed old agent'}
 # Make a complete package with an intentionally wrong version: same executable
 # must fail version verification and restore/start the old one.
 Copy-Item www $pkg -Recurse
 $source=Join-Path (Join-Path $pkg 'www') ("vue-v"+$healthy.version)
 $dest=Join-Path (Join-Path $pkg 'www') ("vue-"+$next)
 Copy-Item $source $dest -Recurse
 $iniHash=(Get-FileHash (Join-Path $target 'mics.ini')).Hash
 $exeHash=(Get-FileHash (Join-Path $target 'mics-only.exe')).Hash
 Compress-Archive -Path "$pkg/*" -DestinationPath $zip -Force
 & powershell.exe -NoProfile -ExecutionPolicy Bypass -File scripts/apply-update.ps1 -Archive $zip -Target $target -ParentId 0 -Version $next
 $failedUpdateExit=$LASTEXITCODE
 if($failedUpdateExit -ne 1){throw "Wrong version returned unexpected exit code: $failedUpdateExit"}
 $global:LASTEXITCODE=0
 $log=Get-Content (Join-Path $target 'data/update.log') -Raw
 if($log -notmatch 'UPDATE_FAILED Agent health/version or Worker ACK check failed'){throw "Expected failed-version health check was not exercised: $log"}
 if($log -notmatch 'UPDATE_RECOVERY_RESTARTED' -or $log -match 'UPDATE_RECOVERY_FAILED'){throw "Recovery did not complete: $log"}
 $after=Invoke-RestMethod 'http://127.0.0.1:8787/api/status' -TimeoutSec 3
 if($after.version -ne $healthy.version){throw 'Recovered version mismatch'}
 if((Get-FileHash (Join-Path $target 'mics.ini')).Hash -ne $iniHash -or (Get-FileHash (Join-Path $target 'mics-only.exe')).Hash -ne $exeHash){throw 'Recovery modified original files'}
 Write-Host 'Preflight and old-agent recovery verified'
}finally{
 $owned=@(Get-Process -Name 'mics-only' -ErrorAction SilentlyContinue | Where-Object {$_.Path -eq (Join-Path $target 'mics-only.exe')})
 foreach($proc in $owned){Stop-Process -Id $proc.Id -Force -ErrorAction SilentlyContinue;Wait-Process -Id $proc.Id -Timeout 10 -ErrorAction SilentlyContinue}
 Remove-Item $root -Recurse -Force -ErrorAction SilentlyContinue
}
# The negative-case child process intentionally exits 1. Only reach this
# success exit after all assertions and cleanup; real failures still throw.
exit 0
