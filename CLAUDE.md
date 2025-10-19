# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

This is a Windows desktop GUI application that launches and manages iRacing companion applications (Fanatec, Crew Chief V4, Trading Paints, and Garage61). Built with Python and Tkinter, it provides a modern dark-themed interface for batch launching/closing these applications with real-time status monitoring.

## Architecture

### Main Application (`iracing_launcher.py`)
- **iRacingLauncher**: Main application class that orchestrates the GUI and app management
  - Manages application definitions with auto-detection across common install paths
  - Uses `psutil` for process monitoring and management
  - Implements custom UI components for a modern appearance

- **ModernButton**: Custom Tkinter Canvas-based widget that creates rounded buttons with hover effects
  - Implements manual state management and event handling
  - Uses polygon drawing with smoothing for rounded corners

- **StatusCard**: Frame-based widget displaying app status with colored indicators
  - Fixed height (50px) for consistent layout
  - Color-coded status states: idle, starting, running, failed, stopped

### Application Detection System
The app uses a path-based auto-detection system for installed applications:
- Checks multiple common installation directories (Program Files, Program Files (x86), AppData)
- Each app has a defined list of potential paths checked in order
- Falls back gracefully with error logging when apps aren't found

### Build System
- **PyInstaller**: Used to create standalone Windows executables
  - Spec file: `iRacing Companion Launcher v2.spec`
  - Bundles the `iRCL.png` icon with the executable
  - Creates a windowed (non-console) application
  - Uses UPX compression

- **Inno Setup**: Creates Windows installer
  - Script: `windows installer.iss`
  - Outputs to `installer_output/` directory
  - Creates 64-bit installer with desktop icon option

## Development Commands

### Running the Application
```bash
python iracing_launcher.py
```

### Converting Icon (if needed)
```bash
python convert_icon.py
```

### Version Management
The application uses centralized version management with semantic versioning (MAJOR.MINOR.PATCH).

#### Automated Version Bump Workflow
When the user says "bump version", follow this workflow:

1. **Determine Version Type** (if not specified, ask the user):
   - **bug/bugfix/patch** → Increment patch version (x.x.Y) - for bug fixes
   - **feature/minor** → Increment minor version (x.Y.0) - for new features
   - **major/breaking** → Increment major version (Y.0.0) - for breaking changes

2. **Update version.py** with the new version number

3. **Run the update script** to propagate the version:
   ```bash
   python update_version.py
   ```

4. **Build the executable automatically**:
   ```bash
   python -m PyInstaller iracing_companion_launcher.spec
   ```

5. **Create a git tag** with the version number:
   ```bash
   git tag v{version}
   ```

This will automatically update:
- `version.py` - Source of truth for version
- `version_info.txt` - Windows version info for the executable
- `windows installer.iss` - Inno Setup installer script AppVersion
- The application will display the version in the header
- A fresh executable will be built in `dist/` directory
- A git tag will be created for the version

#### Manual Version Update
To manually update the version:

1. Edit `version.py` and change the `__version__` variable
2. Run the update script to propagate the version:
```bash
python update_version.py
```

### Building Executable
```bash
# First, ensure versions are up to date
python update_version.py

# Build with PyInstaller using the spec file
python -m PyInstaller iracing_companion_launcher.spec

# Output will be in dist/ directory
```

### Creating Installer
After building the executable, compile the Inno Setup script:
```bash
# Use Inno Setup Compiler (ISCC) to build installer
iscc "windows installer.iss"

# Output installer will be in installer_output/
```

### Creating a Release
After committing your changes, follow these steps to publish a release:

1. **Create and push a git tag**:
   ```bash
   # Create an annotated tag with the version number
   git tag -a v{version} -m "Release v{version}"

   # Push the tag to remote
   git push origin v{version}
   ```

2. **Create a GitHub release**:
   ```bash
   # Using GitHub CLI (recommended)
   gh release create v{version} "installer_output/iRacingCompanionLauncher-Setup-v{version}.exe" --title "v{version}" --notes "Release notes describing changes"

   # Or create manually via GitHub web interface at:
   # https://github.com/{username}/{repo}/releases/new
   ```

## Dependencies
Required Python packages:
- `tkinter` - GUI framework (usually included with Python on Windows)
- `Pillow` (PIL) - Image handling for icons
- `psutil` - Process management and monitoring

## Key Implementation Details

### Process Management
- Uses `psutil.process_iter()` to check if processes are running by name
- Launches apps with `subprocess.Popen()` with `shell=False` for security
- Implements 2-second wait after launching to verify process started
- Kill operations use `proc.kill()` for immediate termination

### UI Layout
- Log text widget height is dynamically calculated based on number of status cards
- Each StatusCard has fixed 50px height to maintain alignment with log
- Uses dark theme colors consistent across all components
- Activity log uses timestamp tags and color-coded message levels

### Special Cases
- Garage61 has both a launcher (`garage61-launcher.exe`) and an agent (`garage61-agent.exe`)
- The agent process is included in the close operation but not in status cards
- All paths use raw strings (r"") to properly handle Windows backslashes

## File Structure
- `iracing_launcher.py` - Main application entry point
- `iracing_launcher_app/` - Application package directory
  - `gui.py` - Main GUI class
  - `app_manager.py` - Application management logic
  - `config_manager.py` - Configuration handling
  - `widgets.py` - Custom UI widgets
  - `constants.py` - UI constants and colors
- `version.py` - **Single source of truth for version number**
- `update_version.py` - Script to update version across all files
- `version_info.txt` - Windows version info (auto-generated)
- `iRCL.png` / `iRCL.ico` - Application icons
- `convert_icon.py` - Utility to convert PNG to ICO format
- `iracing_companion_launcher.spec` - PyInstaller build specification
- `windows installer.iss` - Inno Setup installer script
- `dist/` - PyInstaller output directory
- `build/` - PyInstaller temporary build files
- `installer_output/` - Final installer output directory
