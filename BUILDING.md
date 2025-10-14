# Building Color Interlacer

## Quick Start

### 1. Install Rust

Download and install Rust from: https://rustup.rs/

### 2. Build the Project

```bash
# Navigate to project directory
cd "C:\Users\Livia\Desktop\projects\Color Interlacer"

# Build release binaries
cargo build --release
```

The compiled binaries will be in `target/release/`:
- `color-interlacer.exe` - Main GUI application
- `color-interlacer-overlay.exe` - Overlay process

### 3. Run the Application

```bash
# Run the GUI
.\target\release\color-interlacer.exe
```

## Creating the Installer

### Prerequisites

1. **NSIS** (Nullsoft Scriptable Install System)
   - Download from: https://nsis.sourceforge.io/Download
   - Install and add to PATH

### Build Installer

```bash
# Run the installer build script
.\build-installer.bat
```

The installer will be created at `target/ColorInterlacer-Setup.exe`

## Development Build

For faster compilation during development:

```bash
# Build debug version (faster compile, slower runtime)
cargo build

# Run directly
cargo run --bin color-interlacer

# Run overlay directly (requires arguments)
cargo run --bin color-interlacer-overlay -- --monitor 0 --spectrum example-protanopia --strength 1.0
```

## Troubleshooting

### Compilation Errors

**Error: "linker not found"**
- Install Visual Studio Build Tools: https://visualstudio.microsoft.com/downloads/
- Select "Desktop development with C++"

**Error: "windows crate features"**
- The project requires specific Windows API features
- Ensure you have the latest stable Rust version: `rustup update`

### Runtime Errors

**Error: "Failed to initialize config"**
- The application will automatically create `%APPDATA%/ColorInterlacer/`
- Ensure you have write permissions to AppData

**Error: "Monitor index out of bounds"**
- Check available monitors with the monitor dropdown in GUI
- Config file may reference a disconnected monitor

## Project Structure

```
Color Interlacer/
├── Cargo.toml                 # Workspace configuration
├── crates/
│   ├── core/                  # Core library
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs         # Module exports
│   │       ├── config.rs      # Configuration management
│   │       ├── spectrum.rs    # Spectrum file loading
│   │       ├── hue_mapper.rs  # HSV color mapping
│   │       ├── noise.rs       # Noise texture handling
│   │       └── ipc.rs         # Inter-process communication types
│   ├── gui/                   # Main GUI application
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── main.rs        # Entry point
│   │       ├── app.rs         # Main application logic
│   │       └── monitors.rs    # Monitor enumeration
│   └── overlay/               # Overlay process
│       ├── Cargo.toml
│       └── src/
│           ├── main.rs        # Overlay entry point
│           ├── capture.rs     # Screen capture (placeholder)
│           └── renderer.rs    # Overlay rendering
├── installer/
│   └── installer.nsi          # NSIS installer script
├── assets/                    # Example assets
│   └── example-protanopia.json
└── README.md
```

## Dependencies

Key dependencies:
- **egui / eframe** - Immediate mode GUI
- **egui_overlay** - Transparent overlay support
- **image** - Image loading for noise textures
- **serde / serde_json** - Configuration and spectrum parsing
- **windows** - Windows API bindings
- **anyhow** - Error handling

## Platform Support

Currently **Windows only** due to:
- Overlay implementation uses Windows-specific APIs
- Monitor enumeration via Win32 API
- Screen capture requires Windows Graphics Capture or Desktop Duplication API

Future platform support would require:
- X11 or Wayland overlay support for Linux
- NSWindow/Metal for macOS
