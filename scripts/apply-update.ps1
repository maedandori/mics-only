param(
 [Parameter(Mandatory=$true)][string]$Archive,
 [Parameter(Mandatory=$true)][string]$Target,
 [Parameter(Mandatory=$true)][int]$ParentId,
 [Parameter(Mandatory=$true)][string]$Version
)
$ErrorActionPreference='Stop'
$log=Join-Path $Target 'data\update.log'
$backup=Join-Path $Target 'data\update-backup'
$stage=Join-Path $env:TEMP ('MICS-Only-Stage-'+$PID)
$names=@('mics-only.exe','mics-only-settings.exe')
function Log([string]$message){
  New-Item -ItemType Directory -Path (Split-Path $log) -Force | Out-Null
  Add-Content -LiteralPath $log -Value ("$(Get-Date -Format o) "+$message)
}
function Restore {
  Log 'ROLLBACK_BEGIN'
  $webBackup=Join-Path $backup 'www'
  if(Test-Path -LiteralPath $webBackup){
    foreach($file in (Get-ChildItem -LiteralPath $webBackup -File)){
      Copy-Item -LiteralPath $file.FullName -Destination (Join-Path (Join-Path $Target 'www') $file.Name) -Force
    }
  }
  foreach($name in $names){
    $old=Join-Path $backup $name
    $dest=Join-Path $Target $name
    if(Test-Path -LiteralPath $old){Copy-Item -LiteralPath $old -Destination $dest -Force}
  }
  Log 'ROLLBACK_RESTORED'
}
$replacementStarted=$false
try {
  if($Version -notmatch '^v\d+\.\d+\.\d+$'){throw 'Invalid version'}
  if(!(Test-Path -LiteralPath $Archive)){throw 'Release ZIP missing'}
  if((Get-Item -LiteralPath $Archive).Length -gt 120MB){throw 'ZIP too large'}
  New-Item -ItemType Directory -Path $stage -Force | Out-Null
  Log 'UPDATE_STAGE 10 Extracting archive'
  Expand-Archive -LiteralPath $Archive -DestinationPath $stage -Force
  Log 'UPDATE_STAGE 25 Archive extracted'
  foreach($name in $names){if(!(Test-Path -LiteralPath (Join-Path $stage $name))){throw "Missing $name"}}
  if($ParentId -gt 0){
    try{Wait-Process -Id $ParentId -Timeout 75 -ErrorAction Stop}catch{
      if(Get-Process -Id $ParentId -ErrorAction SilentlyContinue){throw 'Parent did not exit'}
    }
  }
  # Do not terminate an ACTIVE agent, and never stop another print process.
  $status=$null
  try{$status=Invoke-RestMethod -Uri 'http://127.0.0.1:8787/api/status' -TimeoutSec 3}catch{}
  if($status -and $status.mode -eq 'ACTIVE'){throw 'ACTIVE agent, update postponed'}
  if($ParentId -eq 0 -and (!$status -or !$status.ok)){throw 'Agent status unavailable, update postponed'}
  # Settings invocation may leave agent running. Stop only mics-only by name.
  Log 'UPDATE_STAGE 35 Stopping standby agent'
  Get-Process -Name 'mics-only' -ErrorAction SilentlyContinue | Stop-Process -Force -ErrorAction Stop
  Start-Sleep -Milliseconds 700
  New-Item -ItemType Directory -Path $backup -Force | Out-Null
  foreach($name in $names){
    $src=Join-Path $Target $name
    if(!(Test-Path -LiteralPath $src)){throw "Current $name missing"}
    Copy-Item -LiteralPath $src -Destination (Join-Path $backup $name) -Force
  }
  Log 'UPDATE_STAGE 45 Both executables backed up'
  $replacementStarted=$true
  foreach($name in $names){Copy-Item -LiteralPath (Join-Path $stage $name) -Destination (Join-Path $Target $name) -Force}
  Log 'UPDATE_STAGE 60 New executables installed'
  # Install the versioned Vue files alongside legacy WWW. Existing configurations
  # and custom legacy files are never overwritten.
  $vueName='vue-'+$Version
  $vueSrc=Join-Path (Join-Path $stage 'www') $vueName
  $vueDest=Join-Path (Join-Path $Target 'www') $vueName
  if(!(Test-Path -LiteralPath (Join-Path $vueSrc 'index.html'))){throw 'Vue index.html missing from release'}
  if(Test-Path -LiteralPath $vueDest){Remove-Item -LiteralPath $vueDest -Recurse -Force}
  New-Item -ItemType Directory -Path (Split-Path $vueDest) -Force | Out-Null
  Copy-Item -LiteralPath $vueSrc -Destination $vueDest -Recurse -Force
  $vueIndex=Get-Content -LiteralPath (Join-Path $vueDest 'index.html') -Raw
  if($vueIndex -notmatch 'assets/'){throw 'Vue asset paths missing'}
  # Verify every referenced JS/CSS asset, not just the Vue index.html.
  $assetRefs=[regex]::Matches($vueIndex, '(?:src|href)="/vue-[^"/]+/(assets/[^"?]+)"')
  if($assetRefs.Count -eq 0){throw 'Vue JS/CSS references missing'}
  foreach($assetRef in $assetRefs){
    $relative=$assetRef.Groups[1].Value
    if($relative.Contains('..') -or !(Test-Path -LiteralPath (Join-Path $vueDest $relative))){throw "Vue asset missing: $relative"}
  }
  # Legacy HTML/CSS/JS is the default UI. Ship the exact files used by this
  # release, while retaining a backup for rollback.
  foreach($name in @('index.html','app.js','style.css','sw.js','manifest.webmanifest','icon.svg','vue-diagnostics.js')){
    $source=Join-Path (Join-Path $stage 'www') $name
    if(!(Test-Path -LiteralPath $source)){throw "Legacy web file missing: $name"}
    $destination=Join-Path (Join-Path $Target 'www') $name
    $legacyBackup=Join-Path $backup 'www'
    New-Item -ItemType Directory -Path $legacyBackup -Force | Out-Null
    if(Test-Path -LiteralPath $destination){Copy-Item -LiteralPath $destination -Destination (Join-Path $legacyBackup $name) -Force}
    Copy-Item -LiteralPath $source -Destination $destination -Force
  }
  Log 'UPDATE_STAGE 75 Legacy HTML and Vue resources verified'
  $process=Start-Process -FilePath (Join-Path $Target 'mics-only.exe') -WorkingDirectory $Target -PassThru
  $healthy=$false
  for($i=0;$i -lt 25;$i++){
    Start-Sleep -Seconds 2
    if($process.HasExited){break}
    try{
      $health=Invoke-RestMethod -Uri 'http://127.0.0.1:8787/api/status' -TimeoutSec 2
      if($health.ok -eq $true -and $health.version -eq $Version.TrimStart('v')){$healthy=$true;break}
    }catch{}
  }
  if(!$healthy){throw 'New agent health/version check failed'}
  Log 'UPDATE_STAGE 90 Agent API health verified'
  $page=Invoke-WebRequest -Uri 'http://127.0.0.1:8787/' -UseBasicParsing -TimeoutSec 4
  if($page.StatusCode -ne 200 -or $page.Content -notlike '*id="machines"*' -or $page.Content -notlike '*src="/app.js"*'){
    throw 'Legacy local dashboard health check failed'
  }
  $scriptCheck=Invoke-WebRequest -Uri 'http://127.0.0.1:8787/app.js' -UseBasicParsing -TimeoutSec 4
  if($scriptCheck.StatusCode -ne 200 -or $scriptCheck.Content.Length -lt 100){throw 'Legacy app.js unavailable'}
  Remove-Item -LiteralPath (Join-Path $Target 'data\update-blocked-version.txt') -Force -ErrorAction SilentlyContinue
  Log "UPDATE_STAGE 100 Local web verified"
  Log "UPDATE_SUCCESS $Version"
  # Keep backup until next successful update, for emergency manual recovery.
}catch{
  Log "UPDATE_FAILED $($_.Exception.Message)"
  if($_.Exception.Message -notin @('ACTIVE agent, update postponed','Agent status unavailable, update postponed')){
    New-Item -ItemType Directory -Path (Join-Path $Target 'data') -Force | Out-Null
    Set-Content -LiteralPath (Join-Path $Target 'data\update-blocked-version.txt') -Value $Version
  }
  if($replacementStarted){
    if($vueDest -and (Test-Path -LiteralPath $vueDest)){Remove-Item -LiteralPath $vueDest -Recurse -Force -ErrorAction SilentlyContinue}
    Get-Process -Name 'mics-only' -ErrorAction SilentlyContinue | Stop-Process -Force -ErrorAction SilentlyContinue
    Start-Sleep -Milliseconds 500
    try{
      Restore
      Start-Process -FilePath (Join-Path $Target 'mics-only.exe') -WorkingDirectory $Target | Out-Null
      Log 'ROLLBACK_RESTARTED'
    }catch{Log "ROLLBACK_FAILED $($_.Exception.Message)"}
  }
}finally{Remove-Item -LiteralPath $stage -Recurse -Force -ErrorAction SilentlyContinue}
