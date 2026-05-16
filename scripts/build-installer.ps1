param(
  [switch]$SkipSetup
)

$ErrorActionPreference = "Stop"
$ProjectRoot = Split-Path -Parent $PSScriptRoot
$SetupScript = Join-Path $PSScriptRoot "setup-dev.ps1"

function Write-Step($Text) {
  Write-Host ""
  Write-Host "==> $Text" -ForegroundColor Cyan
}

Push-Location $ProjectRoot

if (-not $SkipSetup) {
  & $SetupScript
}

Write-Step "Building MSI installer"
npm run tauri build

Write-Step "Done"
Write-Host "Installer output directory:" -ForegroundColor Green
Write-Host "src-tauri\\target\\release\\bundle\\msi\\"

Pop-Location
