param(
  [switch]$SkipSetup
)

$ErrorActionPreference = "Stop"
$ProjectRoot = Split-Path -Parent $PSScriptRoot
$BuildScript = Join-Path $PSScriptRoot "build-installer.ps1"
$BundleDir = Join-Path $ProjectRoot "src-tauri\\target\\release\\bundle\\msi"

function Write-Step($Text) {
  Write-Host ""
  Write-Host "==> $Text" -ForegroundColor Cyan
}

& $BuildScript -SkipSetup:$SkipSetup

$Installer = Get-ChildItem $BundleDir -Filter *.msi | Sort-Object LastWriteTime -Descending | Select-Object -First 1

if (-not $Installer) {
  throw "No MSI installer was found in $BundleDir"
}

Write-Step "Launching installer"
Start-Process -FilePath $Installer.FullName

Write-Step "Done"
Write-Host "Installer launched:" -ForegroundColor Green
Write-Host $Installer.FullName
