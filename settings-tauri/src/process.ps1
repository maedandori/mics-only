$ErrorActionPreference='Stop'
$root=$env:MICS_SETTINGS_ROOT
$exe=Join-Path $root 'mics-only.exe'
function OwnAgents {
 @(Get-CimInstance Win32_Process -Filter "name='mics-only.exe'" -ErrorAction Stop | Where-Object{$_.ExecutablePath -ieq $exe})
}
function StartOwn {
 if(@(OwnAgents).Count -gt 0){return}
 if(!(Test-Path -LiteralPath $exe)){throw 'Agent missing'}
 Start-Process -FilePath $exe -WorkingDirectory $root
}
switch($env:MICS_SETTINGS_ACTION){
 'start' { StartOwn }
 'stop' { foreach($p in @(OwnAgents)){Stop-Process -Id $p.ProcessId -ErrorAction Stop} }
 'restart' {
  foreach($p in @(OwnAgents)){Stop-Process -Id $p.ProcessId -ErrorAction Stop}
  Start-Sleep -Milliseconds 750
  StartOwn
 }
 'web' { Start-Process 'http://127.0.0.1:8787/' }
 default { throw 'Invalid action' }
}
