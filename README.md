# iRacing Companion Launcher

A modern GUI application to launch and manage iRacing companion apps.

## Version Management

The application uses centralized version management. To update the version:

1. Edit `version.py` and change the `__version__` variable
2. Run the update script to propagate the version:
```bash
python update_version.py
```

This will automatically update:
- Windows version info for the executable (`version_info.txt`)
- Inno Setup installer script (`windows installer.iss`)
- The application will display the version in the header

## Building the Executable

To build the application as a standalone executable:

```bash
# First, ensure versions are up to date
python update_version.py

# Build with PyInstaller using the spec file
python -m PyInstaller iracing_companion_launcher.spec
```

The executable will be created in the `dist/` directory.

## Creating Installer

After building the executable, compile the Inno Setup script:

```bash
# Use Inno Setup Compiler (ISCC) to build installer
iscc "windows installer.iss"

# Output installer will be in installer_output/
```

## Development

Run the application in development mode:

```bash
python iracing_launcher.py
```

## Requirements

- Python 3.x
- customtkinter
- psutil
