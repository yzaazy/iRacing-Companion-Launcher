# -*- mode: python ; coding: utf-8 -*-

# Import version from version.py
import sys
sys.path.insert(0, '.')
from version import __version__

# Note: Application has been refactored into a modular structure:
# - iracing_launcher_app/core/      - Core business logic
# - iracing_launcher_app/managers/  - App and game managers
# - iracing_launcher_app/ui/        - User interface components
# - iracing_launcher_app/utils/     - Utility functions
# PyInstaller will automatically detect and include all modules via import analysis

a = Analysis(
    ['iracing_launcher.py'],
    pathex=[],
    binaries=[],
    datas=[('iRCL.ico', '.'), ('iRCL.png', '.'), ('version.py', '.')],
    hiddenimports=[],
    hookspath=[],
    hooksconfig={},
    runtime_hooks=[],
    excludes=[],
    noarchive=False,
    optimize=0,
)

pyz = PYZ(a.pure)

exe = EXE(
    pyz,
    a.scripts,
    [],
    exclude_binaries=True,
    name='iRacing Companion Launcher',
    debug=False,
    bootloader_ignore_signals=False,
    strip=False,
    upx=True,
    console=False,
    disable_windowed_traceback=False,
    argv_emulation=False,
    target_arch=None,
    codesign_identity=None,
    entitlements_file=None,
    icon=['iRCL.ico'],
    version='version_info.txt',
)

coll = COLLECT(
    exe,
    a.binaries,
    a.datas,
    strip=False,
    upx=True,
    upx_exclude=[],
    name='iRacing Companion Launcher',
)
