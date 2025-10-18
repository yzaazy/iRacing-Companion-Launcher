# -*- mode: python ; coding: utf-8 -*-

# Import version from version.py
import sys
sys.path.insert(0, '.')
from version import __version__

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
    a.binaries,
    a.datas,
    [],
    name='iRacing Companion Launcher',
    debug=False,
    bootloader_ignore_signals=False,
    strip=False,
    upx=True,
    upx_exclude=[],
    runtime_tmpdir=None,
    console=False,
    disable_windowed_traceback=False,
    argv_emulation=False,
    target_arch=None,
    codesign_identity=None,
    entitlements_file=None,
    icon=['iRCL.ico'],
    version='version_info.txt',
)
