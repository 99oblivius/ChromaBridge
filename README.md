# Color Interlacer

An ultra-fast color blind assistance overlay built with Rust.

## Features

- **Real-time color correction overlay** for multiple types of color blindness
- **Noise-based interlacing** for advanced color mapping using dual spectrums
- **Multi-monitor support** with per-monitor configuration
- **Customizable correction strength** to match individual needs
- **Portable asset management** with custom spectrum and noise texture support
- **Low latency rendering** optimized for minimal performance impact

## Architecture

The application consists of two main components:

1. **GUI Application** (`color-interlacer.exe`) - Control panel for managing settings
2. **Overlay Process** (`color-interlacer-overlay.exe`) - Transparent fullscreen overlay that applies color corrections

### Workspace Structure

```
Color Interlacer/
├── crates/
│   ├── core/           # Shared library (config, spectrum, hue mapping, noise)
│   ├── gui/            # Main GUI application
│   └── overlay/        # Overlay rendering process
├── installer/          # NSIS installer script
└── target/             # Build output
```

## Building

### Prerequisites

- Rust toolchain (stable)
- Windows 10/11 (required for overlay features)
- NSIS (optional, for installer)

### Build Commands

```bash
# Build release binaries
cargo build --release

# Build installer (requires NSIS)
# Note: Open a regular CMD window (not VSCode terminal) to run this
build-installer.bat
```

**Note for VSCode users**: The `build-installer.bat` script works best in a standard Windows CMD prompt. If running from VSCode's integrated terminal, you may need to open an external CMD window.

Binaries will be located in `target/release/`:
- `color-interlacer.exe` - Main GUI
- `color-interlacer-overlay.exe` - Overlay process

### Installation

The NSIS installer (`target/ColorInterlacer-Setup.exe`) provides a user-level installation:

- **Install Location**: `%LOCALAPPDATA%\Color Interlacer\` (typically `C:\Users\YourName\AppData\Local\Color Interlacer\`)
- **No Admin Required**: Per-user installation, no UAC prompt
- **Registry**: User-level registry keys (HKCU)
- **Uninstaller**: Appears in user's "Apps & Features" / "Add or Remove Programs"
- **Shortcuts**: Optional Start Menu and Desktop shortcuts

Run the installer to complete the setup - it will guide you through the process.

## Configuration

Configuration and assets are stored in `%APPDATA%/ColorInterlacer/`:

```
ColorInterlacer/
├── config.json       # User settings (auto-generated)
├── spectrums/        # Color spectrum mappings (JSON files)
└── noise/            # Interlacing noise textures (PNG files)
```

### Spectrum File Format

Spectrum files are JSON arrays containing 1 or 2 spectrum mappings:

```json
[
  {
    "0": 50,
    "27": 120,
    "57": 220,
    "85": 280,
    "117": 388,
    "149": 438,
    "180": 567,
    "208": 634,
    "239": 713,
    "271": 809,
    "302": 880,
    "335": 960
  }
]
```

- **Keys**: Input hue values (0-360 degrees)
- **Values**: Output hue values (0-360 degrees)
- **Interpolation**: Linear interpolation between defined points
- **Dual Spectrum**: If 2 spectrums are provided, they're used for noise-based interlacing

### Noise Textures

- Format: 1920x1080 PNG images (black and white)
- Black pixels → Use first spectrum
- White pixels → Use second spectrum (if available)
- Scaling: Nearest neighbor with aspect ratio preservation

## Usage

1. **Launch** the Color Interlacer GUI
2. **Configure** in Settings:
   - Select color blindness type (loads corresponding spectrum file)
   - Adjust correction strength (0.0 = off, 1.0 = full correction)
   - Choose noise pattern for interlacing (optional)
3. **Start** the overlay - it will appear on the selected monitor
4. **Monitor** FPS and frame time to ensure performance is acceptable

## Implementation Details

### Color Processing Pipeline

1. Capture display pixels
2. Convert RGB → HSV
3. Sample noise texture at pixel position (if enabled)
4. Select spectrum based on noise value (black vs white)
5. Map input hue → output hue using selected spectrum with strength interpolation
6. Preserve saturation and value
7. Convert HSV → RGB
8. Render corrected pixel

### Hue Mapping with Strength

The strength parameter interpolates between original and corrected hues:

```
output_hue = input_hue + strength * (corrected_hue - input_hue)
```

- `strength = 0.0` → No correction (input_hue)
- `strength = 1.0` → Full correction (corrected_hue from spectrum)
- `strength = 0.5` → 50% correction (halfway between)

## Limitations

- **Windows only** - Uses Windows-specific APIs for overlay and capture
- **Screen capture** - Current implementation uses placeholder capture logic
  - TODO: Implement full Windows Graphics Capture API or Desktop Duplication API
  - Fallback to BitBlt for compatibility

## Future Enhancements

- [ ] Implement actual screen capture (currently placeholder)
- [ ] Add preset spectrum files for common color blindness types
- [ ] Provide sample noise textures
- [ ] Optimize rendering performance
- [ ] Add hotkey support for quick enable/disable
- [ ] Per-application overlay targeting
- [ ] Real-time IPC for FPS reporting from overlay to GUI

## License

MIT License - See LICENSE.txt for details
