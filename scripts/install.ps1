param(
  [switch]$SkipRustInstall,
  [switch]$SkipBuild
)

$ErrorActionPreference = "Stop"
$ProjectRoot = Split-Path -Parent $PSScriptRoot

function Write-Step($Text) {
  Write-Host ""
  Write-Host "==> $Text" -ForegroundColor Cyan
}

function Ensure-Command($Name) {
  return [bool](Get-Command $Name -ErrorAction SilentlyContinue)
}

Write-Step "Checking Node.js and npm"
if (-not (Ensure-Command "node")) {
  throw "Node.js is required. Please install Node.js 20+ first."
}
if (-not (Ensure-Command "npm")) {
  throw "npm is required. Please install Node.js and npm first."
}

if (-not $SkipRustInstall) {
  Write-Step "Checking Rust toolchain"
  if (-not (Ensure-Command "cargo")) {
    if (Ensure-Command "winget") {
      Write-Host "Rust not found. Installing rustup with winget..." -ForegroundColor Yellow
      winget install --id Rustlang.Rustup --source winget --accept-package-agreements --accept-source-agreements
      $env:PATH += ";$env:USERPROFILE\.cargo\bin"
    } else {
      throw "Rust toolchain is missing and winget is unavailable. Install rustup manually first."
    }
  }
}

if (-not (Ensure-Command "cargo")) {
  $env:PATH += ";$env:USERPROFILE\.cargo\bin"
}

Write-Step "Installing frontend dependencies"
Push-Location $ProjectRoot
npm install

Write-Step "Fetching Rust dependencies"
cargo fetch --manifest-path .\src-tauri\Cargo.toml

if (-not $SkipBuild) {
  Write-Step "Building frontend"
  npm run build

  Write-Step "Checking Rust backend"
  cargo check --manifest-path .\src-tauri\Cargo.toml
}

Pop-Location

Write-Step "Done"
Write-Host "Next steps:" -ForegroundColor Green
Write-Host "1. Run: npm run tauri dev"
Write-Host "2. Open settings and fill in your DeepSeek API key"
Write-Host "3. Click '自动下载 Whisper 资源'"
