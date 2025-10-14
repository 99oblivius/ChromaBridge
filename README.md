# ChromaBridge

An ultra-fast color blind assistance overlay built with Rust.

## Project Structure

This is a **self-contained project** with all necessary files:

```
chromabridge/
├── src/                    # Source code
│   ├── main.rs            # Tray icon + Windows message pump
│   ├── gui.rs             # egui settings window
│   ├── overlay.rs         # DirectComposition + D3D11 rendering
│   ├── state.rs           # State manager (Arc<RwLock> + SQLite)
│   ├── spectrum.rs        # Color mapping and spectrum loading
│   ├── hue_mapper.rs      # HSV ↔ RGB conversion
│   ├── noise.rs           # Noise texture loading
│   ├── logger.rs          # Session-based logging
│   ├── shaders.hlsl       # GPU pixel shaders
│   └── lib.rs             # Module exports
├── assets/                # Runtime assets
│   ├── icons/            # Application icons
│   │   ├── icon.ico
│   │   └── icon-2048.png
│   ├── spectrums/        # Example spectrum files
│   └── example-protanopia.json
├── dev_assets/            # Development files
│   ├── spectrums/        # Example/test spectrum files
│   ├── noise/            # Test noise textures
│   ├── visualize_spectrum.py  # Visualization tool
│   └── README.md         # Dev assets documentation
├── installer/             # NSIS installer
│   └── installer.nsi
├── Cargo.toml            # Rust project manifest
├── build.rs              # Build script (Windows resources)
├── build-installer.ps1   # Installer build script
├── LICENSE.txt           # MIT License
└── README.md             # This file
```

## Building

### Prerequisites

- **Rust** toolchain (MSRV 2021 edition)
- **Windows** 10/11 (required for DirectComposition + D3D11)
- **NSIS** (optional, for creating installer)

### Build Binary

```powershell
# From the chromabridge directory
cargo build --release
```

Binary output: `target/release/chromabridge.exe` (approx. 14MB)

### Build Installer

```powershell
# From the chromabridge directory
.\build-installer.ps1
```

This will:
1. Build the release binary
2. Create the NSIS installer at `target/ChromaBridge-Setup.exe`

## Installation

### From Installer

Run `ChromaBridge-Setup.exe` to install:
- **Install Location**: `%LOCALAPPDATA%\ChromaBridge\`
- **No Admin Required**: User-level installation
- **Optional Shortcuts**: Start Menu and Desktop
- **Uninstaller**: Available in Windows Settings → Apps

### From Binary

Copy `chromabridge.exe` anywhere and run it. The application will:
- Create `%APPDATA%\ChromaBridge\` for settings
- Store state in `state.db` (SQLite)
- Create `assets/spectrums/` and `assets/noise/` subdirectories
- Generate session logs in `logs/`

## Usage

1. **Launch** ChromaBridge (appears as system tray icon)
2. **Click tray icon** or select "Open Settings" to open the settings window
3. **Configure**:
   - Select monitor (if multiple displays)
   - Choose color blind type (spectrum file)
   - Adjust correction strength (0.0–1.0)
   - Optional: Select noise pattern for interlacing
4. **Enable Overlay** via "Start Overlay" button or tray menu

### System Tray Menu

- **Open Settings** - Launch settings window
- **Enable Overlay** ✓ - Toggle overlay on/off
- **Exit** - Close application

### Advanced Settings

- **Run at Windows startup** - Launch ChromaBridge when Windows starts
- **Start overlay on launch** - Auto-enable overlay
- **Keep running in Tray** - Minimize to tray instead of closing
- **Open Asset Folder** - Quick access to `%APPDATA%\ChromaBridge\assets\`
- **Refresh Assets** - Reload spectrum/noise file lists

## Architecture

ChromaBridge uses a **monolithic threaded architecture**:

- **Main Thread** - System tray + Windows message pump
- **GUI Thread** - egui settings window (spawned on demand)
- **Overlay Thread** - DirectComposition rendering (spawned when enabled)

All threads share state via `Arc<RwLock<AppState>>` with async SQLite persistence.

### State Management

- **In-memory**: `Arc<RwLock<AppState>>` for fast concurrent reads
- **Persistence**: Background thread + crossbeam-channel for async writes
- **SQLite**: WAL mode, single JSON blob (KISS approach)

### Rendering Pipeline

1. **Screen Capture** (TODO: Desktop Duplication API - currently test pattern)
2. **GPU Upload** - Screen texture → t0
3. **Pixel Shader** (HLSL):
   - Convert RGB → HSV
   - Sample noise texture (t3) if enabled
   - Select spectrum based on noise (t1 or t2)
   - Lookup corrected hue from 360-element spectrum texture
   - Interpolate using strength parameter
   - Preserve saturation/value with brightness compensation
   - Convert HSV → RGB
4. **Present** - DirectComposition swap chain (FLIP_SEQUENTIAL)

## Development

### Dev Assets

See `dev_assets/README.md` for:
- Example spectrum files
- Visualization tools
- Testing guidance

### Adding Spectrum Files

1. Create JSON file in `dev_assets/spectrums/`
2. Test with `visualize_spectrum.py`
3. Copy to `%APPDATA%\ChromaBridge\assets\spectrums\`
4. Refresh assets in ChromaBridge settings

### Spectrum Format

ChromaBridge supports two spectrum formats:

**Legacy (Hue Mapping)**
```json
[
  {
    "0": 50,
    "27": 120,
    "57": 220,
    ...
  }
]
```

**Node-Based (Color Nodes)**
```json
{
  "spectra": [
    {
      "nodes": [
        {"position": 0.0, "color": "#FF0000"},
        {"position": 0.5, "color": "#00FF00"},
        {"position": 1.0, "color": "#0000FF"}
      ]
    }
  ]
}
```

## Known Limitations

- **Windows Only** - Uses DirectComposition and D3D11
- **Desktop Duplication TODO** - Currently uses test pattern gradient
  - Planned: Windows Graphics Capture API or Desktop Duplication API
  - Note: WDA_EXCLUDEFROMCAPTURE prevents recursive capture
- **Single Overlay** - One overlay instance at a time

## Future Enhancements

- [ ] Implement Desktop Duplication API for real screen capture
- [ ] Add preset spectrums (protanopia, deuteranopia, tritanopia)
- [ ] Provide sample noise textures (Bayer, blue noise, checkerboard)
- [ ] Global hotkey support for quick overlay toggle
- [ ] Per-application targeting (capture specific windows)
- [ ] FPS/frame time overlay display
- [ ] GPU vendor optimizations

## License

MIT License - See LICENSE.txt for details

## Contributing

This project is currently in development. Contributions for Desktop Duplication API implementation and preset spectrum files are welcome!
