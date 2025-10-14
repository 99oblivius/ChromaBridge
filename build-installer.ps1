#!/usr/bin/env pwsh
# PowerShell build script for ChromaBridge installer

Write-Host "Building ChromaBridge..." -ForegroundColor Cyan

# Build release binary
Write-Host "`nBuilding release binary..." -ForegroundColor Yellow
cargo build --release

if ($LASTEXITCODE -ne 0) {
    Write-Host "Build failed!" -ForegroundColor Red
    exit $LASTEXITCODE
}

Write-Host "Build successful!" -ForegroundColor Green
Write-Host ""

# Find NSIS
$makensis = $null
$nsisPath = @(
    "C:\Program Files (x86)\NSIS\makensis.exe",
    "C:\Program Files\NSIS\makensis.exe"
)

foreach ($path in $nsisPath) {
    if (Test-Path $path) {
        $makensis = $path
        break
    }
}

if (-not $makensis) {
    # Try to find in PATH
    $makensis = (Get-Command makensis -ErrorAction SilentlyContinue).Source
}

if (-not $makensis) {
    Write-Host "NSIS not found. Please install NSIS from:" -ForegroundColor Red
    Write-Host "https://nsis.sourceforge.io/Download" -ForegroundColor Yellow
    Write-Host ""
    Write-Host "After installation, either:" -ForegroundColor Yellow
    Write-Host "1. Add NSIS to your PATH, or" -ForegroundColor Yellow
    Write-Host "2. Install to default location: C:\Program Files (x86)\NSIS\" -ForegroundColor Yellow
    exit 1
}

Write-Host "Creating installer using: $makensis" -ForegroundColor Yellow
& $makensis "installer\installer.nsi"

if ($LASTEXITCODE -ne 0) {
    Write-Host "Installer creation failed!" -ForegroundColor Red
    exit $LASTEXITCODE
}

Write-Host ""
Write-Host "Installer created successfully at: target\ChromaBridge-Setup.exe" -ForegroundColor Green