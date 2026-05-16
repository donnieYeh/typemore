param(
  [switch]$SkipRustInstall,
  [switch]$SkipBuild
)

$ErrorActionPreference = "Stop"
$ScriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$TargetScript = Join-Path $ScriptDir "setup-dev.ps1"

Write-Host ""
Write-Host "install.ps1 is kept only for compatibility." -ForegroundColor Yellow
Write-Host "It now forwards to scripts\\setup-dev.ps1." -ForegroundColor Yellow
Write-Host ""

& $TargetScript @PSBoundParameters
