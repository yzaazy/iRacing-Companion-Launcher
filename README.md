# iRacing Companion Launcher

A modern GUI application to launch and manage iRacing companion apps.

## Building the Executable

To build the application as a standalone executable:

```bash
python -m PyInstaller --onefile --windowed --icon=iRCL.ico --add-data "iRCL.ico;." --add-data "iRCL.png;." --name "iRacing Companion Launcher" iracing_launcher.py
```

The executable will be created in the `dist/` directory.

## Development

Run the application in development mode:

```bash
python iracing_launcher.py
```

## Requirements

- Python 3.x
- customtkinter
- Pillow
- psutil
