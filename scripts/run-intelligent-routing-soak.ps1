param([switch]$Smoke)
$ErrorActionPreference = 'Stop'
$repoRoot = Split-Path -Parent $PSScriptRoot
$duration = if ($Smoke) { 1 } else { 60 }
$started = Get-Date
$result = node (Join-Path $repoRoot 'scripts/intelligent-routing-qualification.mjs')
if ($LASTEXITCODE -ne 0) { throw 'qualification failed' }
[pscustomobject]@{
  status = 'ok'
  smoke = [bool]$Smoke
  durationMinutes = $duration
  startedAt = $started.ToUniversalTime().ToString('o')
  qualification = ($result | ConvertFrom-Json)
} | ConvertTo-Json -Depth 6
