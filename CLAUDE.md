# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

This is a Windows desktop GUI application that launches and manages iRacing companion applications (Fanatec, Crew Chief V4, Trading Paints, and Garage61). Built with Python and Tkinter, it provides a modern dark-themed interface for batch launching/closing these applications with real-time status monitoring.

## Architecture

The application follows a clean, modular architecture with clear separation of concerns:

### Package Structure
```
iracing_launcher_app/
├── core/                    # Core business logic
│   ├── activity_logger.py   # Logging abstraction
│   ├── app_definitions.py   # App/game configuration data
│   └── config_manager.py    # Configuration file management
│
├── managers/                # Application managers
│   ├── process_manager.py   # Shared process operations
│   ├── app_manager.py       # Companion app management
│   └── game_manager.py      # Racing game management
│
├── ui/                      # User interface
│   ├── constants.py         # UI constants (colors, sizes)
│   ├── main_window.py       # Main application window
│   ├── sections/            # UI section components
│   │   ├── header.py
│   │   ├── apps_section.py
│   │   ├── games_section.py
│   │   ├── log_section.py
│   │   ├── buttons_section.py
│   │   └── footer.py
│   └── widgets/             # Custom widgets
│       ├── status_card.py
│       └── game_card.py
│
└── utils/                   # Utilities
    ├── path_finder.py       # Path detection logic
    └── steam_registry.py    # Steam registry utilities
```

### Key Components

**Main Window (`ui/main_window.py`)**
- `iRacingLauncherGUI`: Main application orchestrator
  - Coordinates UI sections and business logic
  - Manages application and game state
  - Handles user interactions and callbacks

**Managers (`managers/`)**
- `ProcessManager`: Low-level process operations (launch, kill, check)
- `AppManager`: Companion application management and path detection
- `GameManager`: Racing game management with Steam integration

**UI Components (`ui/`)**
- `StatusCard`: Widget displaying companion app status with colored indicators
- `GameCard`: Widget displaying racing game with radio selection
- Section classes: Modular UI components for different screen areas

**Core Logic (`core/`)**
- `ActivityLogger`: Centralized logging with color-coded messages
- `ConfigManager`: INI file management for storing paths and settings
- `app_definitions.py`: Configuration data for all apps and games

### Application Detection System
The app uses a multi-tier path detection system:
1. **Saved paths**: Checks config.ini for previously configured paths
2. **Start Menu shortcuts**: Searches Windows Start Menu for application shortcuts
3. **Common paths**: Falls back to hardcoded installation directories
4. **Steam Registry**: For games, checks Windows Registry for Steam installations
5. **Manual browse**: Users can manually select executables if auto-detection fails

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
When the user says "release", follow these steps to publish a release:

1. **Ask if the Inno Setup installer has been compiled**:
   - Before proceeding, confirm with the user that they have compiled the installer using Inno Setup Compiler (ISCC)
   - The installer should be available in the `installer_output/` directory

2. **Push the git tag to remote**:
   ```bash
   git push origin v{version}
   ```

3. **Create a GitHub release**:
   ```bash
   # Using GitHub CLI (recommended)
   gh release create v{version} "installer_output/iRacing_Companion_Launcher_Setup.exe" --title "v{version}" --generate-notes
   ```

Note: The installer filename in `installer_output/` is always `iRacing_Companion_Launcher_Setup.exe` (not versioned).

### Discord Release Notifications

To automatically post release notifications to Discord, use **GitTrack.me** - a free service that integrates GitHub with Discord.

**Setup Steps:**

1. **Invite GitTrack Bot**:
   - Visit https://gittrack.me/
   - Click "Add to Discord" and select your server
   - Grant necessary permissions

2. **Connect Your Repository**:
   - In your Discord server, run: `/setup`
   - Follow the prompts to connect your GitHub repository
   - Choose the channel where release notifications should be posted

3. **Configure Notifications**:
   - GitTrack will provide a webhook URL and secret
   - Add the webhook to your GitHub repository settings:
     - Go to: Repository Settings → Webhooks → Add webhook
     - Paste the webhook URL from GitTrack
     - Add the secret token
     - Select events to trigger (at minimum: "Releases")

4. **Test It**:
   - Publish a new release
   - GitTrack will automatically post a formatted notification to your Discord channel

**Features:**
- Automatic, beautifully formatted release notifications
- Real-time updates with rich embeds
- No coding or GitHub Actions required
- Free and easy to maintain
- Can also track commits, PRs, and issues if desired

## Dependencies
Required Python packages:
- `customtkinter` - Modern GUI framework (extends tkinter)
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
```
Scripts/
├── iracing_launcher.py              # Main application entry point
├── version.py                       # Single source of truth for version number
├── iRCL.png / iRCL.ico             # Application icons
├── iracing_companion_launcher.spec # PyInstaller build specification
├── windows installer.iss            # Inno Setup installer script
├── version_info.txt                 # Windows version info (auto-generated)
│
├── iracing_launcher_app/           # Main application package
│   ├── core/                       # Core business logic
│   │   ├── activity_logger.py      # Logging abstraction
│   │   ├── app_definitions.py      # App/game configuration data
│   │   └── config_manager.py       # Configuration file management
│   │
│   ├── managers/                   # Application managers
│   │   ├── process_manager.py      # Shared process operations
│   │   ├── app_manager.py          # Companion app management
│   │   └── game_manager.py         # Racing game management
│   │
│   ├── ui/                         # User interface
│   │   ├── constants.py            # UI constants (colors, sizes)
│   │   ├── main_window.py          # Main application window
│   │   ├── sections/               # UI section components
│   │   │   ├── header.py
│   │   │   ├── apps_section.py
│   │   │   ├── games_section.py
│   │   │   ├── log_section.py
│   │   │   ├── buttons_section.py
│   │   │   └── footer.py
│   │   └── widgets/                # Custom widgets
│   │       ├── status_card.py
│   │       └── game_card.py
│   │
│   └── utils/                      # Utilities
│       ├── path_finder.py          # Path detection logic
│       └── steam_registry.py       # Steam registry utilities
│
├── tools/                           # Development tools
│   ├── convert_icon.py             # Icon conversion utility
│   └── update_version.py           # Version update script
│
├── dist/                            # PyInstaller output directory
├── build/                           # PyInstaller temporary build files
└── installer_output/                # Final installer output directory
```
