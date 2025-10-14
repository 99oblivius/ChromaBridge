# Quick Start Guide

## First Time Setup

1. **Copy the example spectrum file**
   ```
   Copy assets\example-protanopia.json to %APPDATA%\ColorInterlacer\spectrums\
   ```

2. **Launch the application**
   ```
   target\release\color-interlacer.exe
   ```

## Using the Application

### Main Window

- **Start/Stop Button**: Toggles the overlay on/off
- **Monitor Selection**: Choose which display to apply the overlay (shown if multiple monitors detected)
- **Overlay FPS**: Shows current overlay performance
- **Frame Time**: Shows how long each frame takes to render (in milliseconds)
- **Settings Button**: Opens the settings panel

### Settings

1. **Color Blind Type**: Select from available spectrum files in `%APPDATA%\ColorInterlacer\spectrums\`
2. **Correction Strength**: Adjust from 0.0 (no correction) to 1.0 (full correction)
3. **Noise Pattern**: Optional - select a noise texture for dual-spectrum interlacing
4. **Open Asset Folder**: Quickly open the asset folder in Explorer
5. **Refresh Assets**: Reload spectrum and noise files without restarting (useful after adding new files)
6. **Run at Windows startup**: Enable to start the application automatically

## Creating Custom Spectrum Files

1. **Navigate to** `%APPDATA%\ColorInterlacer\spectrums\`

2. **Create a JSON file** (e.g., `my-correction.json`):
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

3. **Click "Refresh Assets"** in settings - Your new spectrum will appear in the dropdown (no restart needed!)

### Dual Spectrum with Noise Interlacing

To use noise-based interlacing:

1. **Create noise texture** - 1920x1080 PNG with black and white patterns
2. **Place in** `%APPDATA%\ColorInterlacer\noise\`
3. **Add second spectrum** to your JSON file:
   ```json
   [
     { /* first spectrum - for black pixels */ },
     { /* second spectrum - for white pixels */ }
   ]
   ```
4. **Select noise pattern** in settings

## Tips

- **Start with low strength** (0.3-0.5) and gradually increase
- **Monitor frame time** - if it exceeds your monitor's refresh rate interval, you'll see lag
- **Use noise interlacing** for more nuanced color correction across different hue ranges
- **Multiple monitors** - Run separate instances for different monitors with different settings

## Troubleshooting

**Overlay not appearing**
- Ensure a spectrum file is selected
- Check that the overlay process started (check Task Manager)
- Try restarting both the GUI and overlay

**Low FPS / High frame time**
- Reduce correction strength
- Disable noise interlacing
- Close other graphics-intensive applications

**Settings not saving**
- Ensure %APPDATA%\ColorInterlacer\ is writable
- Check config.json for syntax errors

**No spectrum files in dropdown**
- Place .json files in %APPDATA%\ColorInterlacer\spectrums\
- Use the "Open Asset Folder" button to navigate there quickly
- Copy the example file from assets\example-protanopia.json

## Advanced Usage

### Command Line (Overlay Process)

The overlay can be run directly with parameters:

```bash
color-interlacer-overlay.exe --monitor 0 --spectrum example-protanopia --strength 0.75 --noise my-noise
```

Parameters:
- `--monitor <index>`: Monitor index (0-based)
- `--spectrum <name>`: Spectrum file name (without .json)
- `--strength <value>`: Correction strength (0.0-1.0)
- `--noise <name>`: Optional noise texture name (without .png)

### Startup Automation

Enable "Run at Windows startup" in settings, or manually create a shortcut in:
```
%APPDATA%\Microsoft\Windows\Start Menu\Programs\Startup\
```

## Next Steps

- Experiment with different correction strengths
- Create custom spectrum files for your specific needs
- Try noise-based interlacing for advanced color correction
- Share your spectrum configurations with others

## Getting Help

- Check README.md for detailed technical information
- See BUILDING.md for compilation instructions
- Review the example spectrum file in assets/
