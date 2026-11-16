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
$replacementStarted=$false
$replacementCompleted=$false
try {
  if($Version -notmatch '^v\d+\.\d+\.\d+$'){throw 'Invalid version'}
  if(!(Test-Path -LiteralPath $Archive)){throw 'Release ZIP missing'}
  if((Get-Item -LiteralPath $Archive).Length -gt 120MB){throw 'ZIP too large'}
  New-Item -ItemType Directory -Path $stage -Force | Out-Null
  Log 'UPDATE_STAGE 10 Extracting archive'
  Expand-Archive -LiteralPath $Archive -DestinationPath $stage -Force
  Log 'UPDATE_STAGE 25 Archive extracted'
  foreach($name in $names){
    $binary=Join-Path $stage $name
    if(!(Test-Path -LiteralPath $binary)){throw "Missing $name"}
    if((Get-Item -LiteralPath $binary).Length -lt 100KB){throw "Suspiciously small executable: $name"}
  }
  # Validate all mandatory release files before touching a working installation.
  $vueName='vue-'+$Version
  $vueSrc=Join-Path (Join-Path $stage 'www') $vueName
  if(!(Test-Path -LiteralPath (Join-Path $vueSrc 'index.html'))){throw 'Template index missing in ZIP'}
  foreach($name in @('index.html','app.js','style.css','sw.js','manifest.webmanifest','icon.svg','vue-diagnostics.js')){
    if(!(Test-Path -LiteralPath (Join-Path (Join-Path $stage 'www') $name))){
      throw "Static web file missing in ZIP: $name"
    }
  }
  Log 'UPDATE_PREFLIGHT_OK All binaries and web entry files present'
  if($ParentId -gt 0){
    try{Wait-Process -Id $ParentId -Timeout 15 -ErrorAction Stop}catch{
      if(Get-Process -Id $ParentId -ErrorAction SilentlyContinue){
        Stop-Process -Id $ParentId -Force -ErrorAction SilentlyContinue
        try{Wait-Process -Id $ParentId -Timeout 10 -ErrorAction SilentlyContinue}catch{}
      }
    }
  }
  # Do not terminate an ACTIVE agent, and never stop another print process.
  $status=$null
  try{$status=Invoke-RestMethod -Uri 'http://127.0.0.1:8787/api/status' -TimeoutSec 3}catch{}
  if($status -and $status.mode -eq 'ACTIVE'){throw 'ACTIVE agent, update postponed'}
  if($ParentId -eq 0 -and (!$status -or !$status.ok)){throw 'Agent status unavailable, update postponed'}
  # Stop both mics-only and any open mics-only-settings window so neither EXE is locked.
  Log 'UPDATE_STAGE 35 Stopping standby agent and settings UI'
  $oldSettings=@(Get-Process -Name 'mics-only-settings' -ErrorAction SilentlyContinue)
  foreach($s in $oldSettings){Stop-Process -Id $s.Id -Force -ErrorAction SilentlyContinue}
  foreach($s in $oldSettings){try{Wait-Process -Id $s.Id -Timeout 10 -ErrorAction SilentlyContinue}catch{}}
  $oldAgents=@(Get-Process -Name 'mics-only' -ErrorAction SilentlyContinue)
  foreach($oldAgent in $oldAgents){Stop-Process -Id $oldAgent.Id -Force -ErrorAction Stop}
  foreach($oldAgent in $oldAgents){
    try{Wait-Process -Id $oldAgent.Id -Timeout 20 -ErrorAction Stop}catch{
      if(Get-Process -Id $oldAgent.Id -ErrorAction SilentlyContinue){throw "Old agent PID $($oldAgent.Id) still running"}
    }
  }
  # Confirm that no old agent remains before replacing files.
  if(Get-Process -Name 'mics-only' -ErrorAction SilentlyContinue){throw 'Agent still running after stop'}
  Log 'UPDATE_STAGE 38 Old agent fully exited'
  New-Item -ItemType Directory -Path $backup -Force | Out-Null
  # Never restore stale web files from an earlier update attempt.
  $webBackup=Join-Path $backup 'www'
  if(Test-Path -LiteralPath $webBackup){Remove-Item -LiteralPath $webBackup -Recurse -Force}
  foreach($name in $names){
    $src=Join-Path $Target $name
    if(!(Test-Path -LiteralPath $src)){throw "Current $name missing"}
    Copy-Item -LiteralPath $src -Destination (Join-Path $backup $name) -Force
  }
  Log 'UPDATE_STAGE 45 Both executables backed up'
  $replacementStarted=$true
  foreach($name in $names){
    $copied=$false
    $lastErr=$null
    for($attempt=1;$attempt -le 10;$attempt++){
      try{
        Copy-Item -LiteralPath (Join-Path $stage $name) -Destination (Join-Path $Target $name) -Force -ErrorAction Stop
        $copied=$true
        break
      }catch{
        $lastErr=$_
        Get-Process -Name ([System.IO.Path]::GetFileNameWithoutExtension($name)) -ErrorAction SilentlyContinue | Stop-Process -Force -ErrorAction SilentlyContinue
        Start-Sleep -Milliseconds 500
      }
    }
    if(!$copied){throw "Cannot replace ${name}: $($lastErr.Exception.Message)"}
  }
  $replacementCompleted=$true
  Log 'UPDATE_STAGE 60 New executables installed'
  # Install the versioned Vue files alongside legacy WWW. Existing configurations
  # and custom legacy files are never overwritten.
  $vueDest=Join-Path (Join-Path $Target 'www') $vueName
  if(!(Test-Path -LiteralPath (Join-Path $vueSrc 'index.html'))){throw 'Vue index.html missing from release'}
  if(Test-Path -LiteralPath $vueDest){Remove-Item -LiteralPath $vueDest -Recurse -Force}
  New-Item -ItemType Directory -Path (Split-Path $vueDest) -Force | Out-Null
  Copy-Item -LiteralPath $vueSrc -Destination $vueDest -Recurse -Force
  # Missing template assets are an interface issue, NOT a reason to
  # downgrade a healthy executable. Rust serves the stable fallback HTML.
  $templateWarnings=@()
  $assetRefs=@()
  try{
    $vueIndex=Get-Content -LiteralPath (Join-Path $vueDest 'index.html') -Raw
    if($vueIndex -notmatch 'assets/'){throw 'Template index has no assets'}
    $assetRefs=[regex]::Matches($vueIndex, '(?:src|href)="/vue-[^"/]+/(assets/[^"?]+)"')
    if($assetRefs.Count -eq 0){throw 'Template JS/CSS references missing'}
    foreach($assetRef in $assetRefs){
      $relative=$assetRef.Groups[1].Value
      if($relative.Contains('..') -or !(Test-Path -LiteralPath (Join-Path $vueDest $relative))){throw "Template file missing: $relative"}
    }
  }catch{
    $templateWarnings+= $_.Exception.Message
    Log "WEB_WARNING $($_.Exception.Message)"
  }
  # Keep existing legacy files as a manual emergency backup only.
  # No automatic downgrade or rollback is performed.
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
  for($i=0;$i -lt 40;$i++){
    Start-Sleep -Seconds 2
    if($process.HasExited){break}
    try{
      $health=Invoke-RestMethod -Uri 'http://127.0.0.1:8787/api/status' -TimeoutSec 2
      if($health.ok -eq $true -and $health.version -eq $Version.TrimStart('v')){$healthy=$true;break}
    }catch{}
  }
  if(!$healthy){
    $detail=if($process.HasExited){"agent exited code=$($process.ExitCode)"}else{"agent stayed alive but /api/status not healthy"}
    throw "New agent health/version check failed: $detail"
  }
  Log 'UPDATE_STAGE 90 Agent API health verified'
  # Web problems should not rollback a working agent. Probe the actual HTML
  # and assets, retain the EXE, and make any UI failure visible in update.log.
  try{
    $page=Invoke-WebRequest -Uri 'http://127.0.0.1:8787/' -UseBasicParsing -TimeoutSec 4
    if($page.StatusCode -ne 200){throw 'Default page not HTTP 200'}
    if($page.Content -notlike ('*'+$vueName+'/*')){throw 'Template not selected as default; stable fallback is active'}
    foreach($assetRef in $assetRefs){
      $url='http://127.0.0.1:8787/'+$vueName+'/'+$assetRef.Groups[1].Value
      $assetCheck=Invoke-WebRequest -Uri $url -UseBasicParsing -TimeoutSec 4
      if($assetCheck.StatusCode -ne 200 -or $assetCheck.Content.Length -lt 20){throw "Template asset unavailable: $url"}
    }
  }catch{
    $templateWarnings+= $_.Exception.Message
    Log "WEB_WARNING $($_.Exception.Message)"
  }
  Remove-Item -LiteralPath (Join-Path $Target 'data\update-blocked-version.txt') -Force -ErrorAction SilentlyContinue
  Log "UPDATE_STAGE 100 Local web verified"
  if($templateWarnings.Count -gt 0){Log "UPDATE_SUCCESS_WITH_WEB_WARNING $Version details=$($templateWarnings -join '; ')"}
  else{Log "UPDATE_SUCCESS $Version"}
  # Previous binaries are retained for manual recovery only.
}catch{
  Log "UPDATE_FAILED $($_.Exception.Message) at $($_.InvocationInfo.ScriptLineNumber)"
  if($_.Exception.Message -notin @('ACTIVE agent, update postponed','Agent status unavailable, update postponed')){
    New-Item -ItemType Directory -Path (Join-Path $Target 'data') -Force | Out-Null
    Set-Content -LiteralPath (Join-Path $Target 'data\update-blocked-version.txt') -Value $Version
  }
  # Never copy old binaries back or delete newly installed web resources.
  if($replacementStarted){
    Log "UPDATE_FAILED_NO_ROLLBACK version=$Version replaced=$replacementCompleted"
    # Start new EXE only if no agent remains. Do not terminate a healthy agent.
    if(!(Get-Process -Name 'mics-only' -ErrorAction SilentlyContinue)){
      try{
        Start-Process -FilePath (Join-Path $Target 'mics-only.exe') -WorkingDirectory $Target | Out-Null
        Log 'UPDATE_RESTART_ATTEMPTED'
      }catch{Log "UPDATE_RESTART_FAILED $($_.Exception.Message)"}
    }
  }
}finally{Remove-Item -LiteralPath $stage -Recurse -Force -ErrorAction SilentlyContinue}
