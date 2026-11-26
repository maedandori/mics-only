param(
 [Parameter(Mandatory=$true)][string]$Archive,
 [Parameter(Mandatory=$true)][string]$Target,
 [Parameter(Mandatory=$true)][int]$ParentId,
 [Parameter(Mandatory=$true)][string]$Version,
 [switch]$Migration
)
$ErrorActionPreference='Stop'
$Target=[IO.Path]::GetFullPath($Target)
$data=Join-Path $Target 'data'
$log=Join-Path $data 'update.log'
$stage=Join-Path $env:TEMP ('MICS-Only-Stage-'+$PID)
$backup=Join-Path $data ('update-backup-'+(Get-Date -Format 'yyyyMMdd-HHmmss')+'-'+$PID)
$names=@('mics-only.exe','mics-only-settings.exe')
$web=@('index.html','app.js','style.css','sw.js','manifest.webmanifest','icon.svg','vue-diagnostics.js')
$lock=$null
$changed=$false
$stopping=$false
$backupReady=$false
$oldWasRunning=$false
$process=$null
$exitCode=0
$oldVersion=$null
$vueDest=$null
$vueCreated=$false
$createdCore=$false
$createdState=$false
function Log([string]$message){
 try{Add-Content -LiteralPath $log -Value ("$(Get-Date -Format o) "+$message)}catch{Write-Warning "Cannot write update.log: $_"}
}
function Own([string]$name){
 $path=Join-Path $Target ($name+'.exe')
 @(Get-Process -Name $name -ErrorAction SilentlyContinue | Where-Object {
   try{ $_.Path -and ([IO.Path]::GetFullPath($_.Path) -ieq $path) }catch{$false}
 })
}
function StopOwn([string]$name){
 foreach($p in @(Own $name)){
  Stop-Process -Id $p.Id -Force -ErrorAction Stop
  try{Wait-Process -Id $p.Id -Timeout 20 -ErrorAction Stop}catch{if(Get-Process -Id $p.Id -ErrorAction SilentlyContinue){throw}}
 }
 if(@(Own $name).Count){throw "Process still running: $name"}
}
function GetHealth {
 try{Invoke-RestMethod -Uri 'http://127.0.0.1:8787/api/status' -TimeoutSec 3}catch{$null}
}
function WaitHealth($p,[string]$expected,[bool]$isMigration){
 $deadline=(Get-Date).AddSeconds($(if($isMigration){150}else{100}))
 do{
  $p.Refresh()
  if($p.HasExited){throw "New agent exited: $($p.ExitCode)"}
  $h=GetHealth
  if($h -and $h.ok -eq $true -and $h.version -eq $expected -and $h.mode -in @('ACTIVE','STANDBY','MIRROR_ONLY','OFFLINE')){
   if(!$isMigration){return}
   # A migrated launcher must have a running Core AND a fresh authenticated Worker ACK.
   $receipt=$null
   try{$receipt=Get-Content -LiteralPath (Join-Path $data 'migration-presence.json') -Raw | ConvertFrom-Json}catch{}
   $launcher=$null
   try{$launcher=Get-Content -LiteralPath (Join-Path $data 'launcher-status.json') -Raw | ConvertFrom-Json}catch{}
   if($h.pid -and $receipt -and $receipt.pid -eq $h.pid -and $receipt.version -eq $expected -and $receipt.ack -eq $true -and $receipt.ack_at_ms -ge $script:migrationStarted -and $launcher.phase -eq 'RUNNING' -and $launcher.core_version -eq $expected){return}
  }
  Start-Sleep -Seconds 2
 }while((Get-Date) -lt $deadline)
 throw 'Agent health/version or Worker ACK check failed'
}
function RestorePrevious {
 Log 'UPDATE_RECOVERY_BEGIN'
 if($process -and !$process.HasExited){Stop-Process -Id $process.Id -Force -ErrorAction Stop;try{Wait-Process -Id $process.Id -Timeout 20 -ErrorAction Stop}catch{if(Get-Process -Id $process.Id -ErrorAction SilentlyContinue){throw}}}
 StopOwn 'mics-only'
 StopOwn 'mics-only-settings'
 if($Migration){
  StopOwn 'mics-core-a'
  StopOwn 'mics-core-b'
 }
 if($changed){
  foreach($n in $names){Copy-Item -LiteralPath (Join-Path $backup $n) -Destination (Join-Path $Target $n) -Force}
  foreach($n in $web){
   $src=Join-Path (Join-Path $backup 'www') $n
   $dst=Join-Path (Join-Path $Target 'www') $n
   if(Test-Path -LiteralPath $src){Copy-Item -LiteralPath $src -Destination $dst -Force}
   elseif(Test-Path -LiteralPath $dst){Remove-Item -LiteralPath $dst -Force}
  }
  if($createdCore){Remove-Item -LiteralPath (Join-Path $Target 'mics-core-a.exe') -Force -ErrorAction SilentlyContinue}
  if($createdState){
   foreach($n in @('core-state.json','launcher-status.json','migration-presence.json')){Remove-Item -LiteralPath (Join-Path $data $n) -Force -ErrorAction SilentlyContinue}
  }
  if($vueCreated -and $vueDest){Remove-Item -LiteralPath $vueDest -Recurse -Force -ErrorAction SilentlyContinue}
 }
 if($oldWasRunning -or $changed){
  $old=Start-Process -FilePath (Join-Path $Target 'mics-only.exe') -WorkingDirectory $Target -PassThru
  if($oldVersion){WaitHealth $old $oldVersion $false}
  else{Start-Sleep -Seconds 3;$old.Refresh();if($old.HasExited){throw 'Restored agent failed to start'}}
  Log 'UPDATE_RECOVERY_RESTARTED'
 }
}
try{
 New-Item -ItemType Directory -Path $data -Force | Out-Null
 $lock=[IO.File]::Open((Join-Path $data 'update-apply.lock'),[IO.FileMode]::OpenOrCreate,[IO.FileAccess]::ReadWrite,[IO.FileShare]::None)
 if($Version -notmatch '^(?:mo|v)\d+\.\d+\.\d+$'){throw 'Invalid version'}
 if($Migration -and $ParentId -le 0){throw 'Migration requires Settings'}
 if(!(Test-Path -LiteralPath $Archive)){throw 'Release ZIP missing'}
 if((Get-Item -LiteralPath $Archive).Length -gt 120MB){throw 'ZIP too large'}
 if((Test-Path -LiteralPath (Join-Path $Target 'mics-core-a.exe')) -or (Test-Path -LiteralPath (Join-Path $Target 'mics-core-b.exe')) -or (Test-Path -LiteralPath (Join-Path $data 'core-state.json'))){throw 'Launcher already installed or migration residue; inspect before updating'}
 New-Item -ItemType Directory -Path $stage -Force | Out-Null
 Log 'UPDATE_STAGE 10 Extracting archive'
 Expand-Archive -LiteralPath $Archive -DestinationPath $stage -Force
 $required=@($names)
 if($Migration){$required+='mics-core-a.exe'}
 elseif(Test-Path -LiteralPath (Join-Path $stage 'mics-core-a.exe')){throw 'Unexpected migration archive'}
 foreach($n in $required){
  $f=Join-Path $stage $n
  if(!(Test-Path -LiteralPath $f -PathType Leaf) -or (Get-Item -LiteralPath $f).Length -lt 100KB){throw "Invalid executable: $n"}
 }
 $vueName='vue-'+$Version
 $vueSrc=Join-Path (Join-Path $stage 'www') $vueName
 $vueDest=Join-Path (Join-Path $Target 'www') $vueName
 if(!(Test-Path -LiteralPath (Join-Path $vueSrc 'index.html'))){throw 'Template index missing'}
 foreach($n in $web){if(!(Test-Path -LiteralPath (Join-Path (Join-Path $stage 'www') $n))){throw "Missing web file: $n"}}
 $vueIndex=Get-Content -LiteralPath (Join-Path $vueSrc 'index.html') -Raw
 $refs=[regex]::Matches($vueIndex,'(?:src|href)="/vue-[^"/]+/(assets/[^"?]+)"')
 if(!$refs.Count){throw 'Template asset references missing'}
 foreach($r in $refs){$rel=$r.Groups[1].Value;if($rel.Contains('..') -or !(Test-Path -LiteralPath (Join-Path $vueSrc $rel))){throw "Template asset missing: $rel"}}
 Log 'UPDATE_PREFLIGHT_OK'
 if($ParentId -gt 0){
  try{Wait-Process -Id $ParentId -Timeout 30 -ErrorAction Stop}catch{if(Get-Process -Id $ParentId -ErrorAction SilentlyContinue){throw 'Settings did not close'}}
 }
 $old=@(Own 'mics-only')
 $oldWasRunning=$old.Count -gt 0
 $status=GetHealth
 if($status -and $status.version -and ([version]$status.version -ge [version]($Version -replace '^(mo|v)',''))){throw 'Already updated, update postponed'}
 if($status -and $status.mode -eq 'ACTIVE'){throw 'ACTIVE agent, update postponed'}
 if($oldWasRunning -and (!$status -or !$status.ok -or $status.mode -notin @('STANDBY','MIRROR_ONLY'))){throw 'Agent status unavailable, update postponed'}
 if(!$oldWasRunning -and ($ParentId -eq 0 -or $status)){throw 'Agent status unavailable, update postponed'}
 # Back up BEFORE stopping a healthy process; never touch config, credentials or cached JSON.
 New-Item -ItemType Directory -Path (Join-Path $backup 'www') -Force | Out-Null
 foreach($n in $names){Copy-Item -LiteralPath (Join-Path $Target $n) -Destination (Join-Path $backup $n) -Force}
 foreach($n in $web){$f=Join-Path (Join-Path $Target 'www') $n;if(Test-Path -LiteralPath $f){Copy-Item -LiteralPath $f -Destination (Join-Path (Join-Path $backup 'www') $n) -Force}}
 $backupReady=$true
 if($status){$oldVersion=$status.version}
 # Stage versioned web directory while old agent is still alive. Never overwrite an existing one.
 if(Test-Path -LiteralPath $vueDest){throw 'Target web version already exists; inspect before retry'}
 $vueCreated=$true
 New-Item -ItemType Directory -Path (Split-Path $vueDest) -Force | Out-Null
 Copy-Item -LiteralPath $vueSrc -Destination $vueDest -Recurse
 if($oldWasRunning){
  $status=GetHealth
  if(!$status -or !$status.ok -or $status.mode -notin @('STANDBY','MIRROR_ONLY')){throw 'Agent became active or unavailable, update postponed'}
 }
 $stopping=$true
 StopOwn 'mics-only-settings'
 StopOwn 'mics-only'
 $changed=$true
 Log 'UPDATE_STAGE 45 Previous binaries saved'
 if($Migration){
  $createdCore=$true;$createdState=$true
  Copy-Item -LiteralPath (Join-Path $stage 'mics-core-a.exe') -Destination (Join-Path $Target 'mics-core-a.exe') -Force
  $script:migrationStarted=[DateTimeOffset]::UtcNow.ToUnixTimeMilliseconds()
 }
 foreach($n in $names){Copy-Item -LiteralPath (Join-Path $stage $n) -Destination (Join-Path $Target $n) -Force}
 foreach($n in $web){Copy-Item -LiteralPath (Join-Path (Join-Path $stage 'www') $n) -Destination (Join-Path (Join-Path $Target 'www') $n) -Force}
 Log 'UPDATE_STAGE 75 Starting new agent'
 $process=Start-Process -FilePath (Join-Path $Target 'mics-only.exe') -WorkingDirectory $Target -PassThru
 WaitHealth $process (($Version -replace '^(mo|v)','')) ([bool]$Migration)
 Remove-Item -LiteralPath (Join-Path $data 'update-blocked-version.txt') -Force -ErrorAction SilentlyContinue
 Log "UPDATE_SUCCESS $Version"
}catch{
 $exitCode=1
 if($lock){
  $why=$_.Exception.Message
  Log "UPDATE_FAILED $why"
  if($stopping -and $backupReady){
   try{
    try{Set-Content -LiteralPath (Join-Path $data 'update-blocked-version.txt') -Value $Version}catch{Log 'Could not persist blocked version'}
    RestorePrevious
   }catch{Log "UPDATE_RECOVERY_FAILED $($_.Exception.Message)"}
  }elseif($vueCreated -and $vueDest){Remove-Item -LiteralPath $vueDest -Recurse -Force -ErrorAction SilentlyContinue}
 }else{Write-Error -ErrorAction Continue "Update lock unavailable: $_"}
}finally{
 Remove-Item -LiteralPath $stage -Recurse -Force -ErrorAction SilentlyContinue
 if($lock){$lock.Dispose()}
}
exit $exitCode
