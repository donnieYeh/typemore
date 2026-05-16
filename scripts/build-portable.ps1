param(
  [switch]$SkipSetup,
  [switch]$Run
)

$ErrorActionPreference = "Stop"
$ProjectRoot = Split-Path -Parent $PSScriptRoot
$SetupScript = Join-Path $PSScriptRoot "setup-dev.ps1"
$ExePath = Join-Path $ProjectRoot "src-tauri\\target\\release\\typemore.exe"

function Write-Step($Text) {
  Write-Host ""
  Write-Host "==> $Text" -ForegroundColor Cyan
}

function Invoke-Native($Command, $Arguments) {
  & $Command @Arguments
  if ($LASTEXITCODE -ne 0) {
    throw "Command failed: $Command $($Arguments -join ' ')"
  }
}

Push-Location $ProjectRoot

try {
  if (-not $SkipSetup) {
    & $SetupScript
    if ($LASTEXITCODE -ne 0) {
      throw "setup-dev.ps1 failed"
    }
  }

  Write-Step "Building portable executable"
  Invoke-Native "npm" @("run", "tauri", "build", "--", "--no-bundle")

  if (-not (Test-Path $ExePath)) {
    throw "Portable executable was not found: $ExePath"
  }

  Write-Step "Done"
  Write-Host "Portable executable:" -ForegroundColor Green
  Write-Host $ExePath

  if ($Run) {
    Write-Step "Launching portable executable"
    Start-Process -FilePath $ExePath
  }
}
finally {
  Pop-Location
}
