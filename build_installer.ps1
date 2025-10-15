# Build ChromaBridge Installer
# Extracts version from Cargo.toml and passes it to NSIS

$ErrorActionPreference = "Stop"

# Extract version from Cargo.toml
$cargoToml = Get-Content "Cargo.toml" -Raw
if ($cargoToml -match 'version\s*=\s*"([^"]+)"') {
    $cargoVersion = $matches[1]
    # Strip "0." prefix for display (0.2025.15 -> 2025.15)
    $version = $cargoVersion -replace '^0\.', ''
    Write-Host "Building installer for version: $version (Cargo: $cargoVersion)"
} else {
    Write-Error "Could not find version in Cargo.toml"
    exit 1
}

# Build release binary first
Write-Host "Building release binary..."
cargo build --release
if ($LASTEXITCODE -ne 0) {
    Write-Error "Cargo build failed"
    exit 1
}

# Build installer with version
Write-Host "Creating installer..."
Push-Location installer
makensis /DVERSION=$version installer.nsi
Pop-Location

if ($LASTEXITCODE -eq 0) {
    Write-Host "Installer created successfully: target\ChromaBridge-Setup.exe"
} else {
    Write-Error "Installer build failed"
    exit 1
}
