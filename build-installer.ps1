#!/usr/bin/env pwsh
# Build ChromaBridge Installer
# Extracts version from Cargo.toml and passes it to NSIS

$ErrorActionPreference = "Stop"

# Extract version from Cargo.toml
$cargoToml = Get-Content "Cargo.toml" -Raw
if ($cargoToml -match 'version\s*=\s*"([^"]+)"') {
    $cargoVersion = $matches[1]
    # Strip "0." prefix for display (0.2025.15 -> 2025.15)
    $version = $cargoVersion -replace '^0\.', ''
    Write-Host "Building installer for version: $version (Cargo: $cargoVersion)" -ForegroundColor Cyan
} else {
    Write-Error "Could not find version in Cargo.toml"
    exit 1
}

# Build release binary first
Write-Host "`nBuilding release binary..." -ForegroundColor Yellow
cargo build --release
if ($LASTEXITCODE -ne 0) {
    Write-Host "Build failed!" -ForegroundColor Red
    exit $LASTEXITCODE
}

Write-Host "Build successful!" -ForegroundColor Green

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

# Build installer with version
Write-Host "`nCreating installer using: $makensis" -ForegroundColor Yellow
Push-Location installer
& $makensis "/DVERSION=$version" installer.nsi
Pop-Location

if ($LASTEXITCODE -eq 0) {
    Write-Host ""
    Write-Host "Installer created successfully: target\ChromaBridge-Setup-$version.exe" -ForegroundColor Green
} else {
    Write-Host "Installer creation failed!" -ForegroundColor Red
    exit $LASTEXITCODE
}
