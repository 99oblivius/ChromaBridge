# Build ChromaBridge Installer
# Extracts version from Cargo.toml and passes it to NSIS

$ErrorActionPreference = "Stop"

# Extract version from Cargo.toml
$cargoToml = Get-Content "Cargo.toml" -Raw
if ($cargoToml -match 'version\s*=\s*"([^"]+)"') {
    $version = $matches[1]
    Write-Host "Building installer for version: $version"
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
