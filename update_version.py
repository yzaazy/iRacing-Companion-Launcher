"""
Update version numbers across all build configuration files.

This script reads the version from version.py and updates:
1. Windows version_info.txt for PyInstaller
2. Inno Setup .iss file
"""

import re
from version import __version__


def parse_version(version_string):
    """
    Parse version string into components.

    Args:
        version_string: Version in format "1.4.0" or "1.4.0-beta"

    Returns:
        tuple: (major, minor, patch, prerelease)
    """
    # Remove any prerelease suffix
    match = re.match(r'(\d+)\.(\d+)\.(\d+)(?:-(.+))?', version_string)
    if not match:
        raise ValueError(f"Invalid version format: {version_string}")

    major, minor, patch, prerelease = match.groups()
    return int(major), int(minor), int(patch), prerelease or ""


def create_version_info_file():
    """Create version_info.txt file for PyInstaller."""
    major, minor, patch, prerelease = parse_version(__version__)

    # Windows expects 4 numbers for version
    build = 0  # Can be incremented for builds of the same version

    version_info = f"""# UTF-8
#
# For more details about fixed file info:
# https://msdn.microsoft.com/en-us/library/ms646997.aspx

VSVersionInfo(
  ffi=FixedFileInfo(
    # filevers and prodvers should be always a tuple with four items: (1, 2, 3, 4)
    # Set not needed items to zero 0.
    filevers=({major}, {minor}, {patch}, {build}),
    prodvers=({major}, {minor}, {patch}, {build}),
    # Contains a bitmask that specifies the valid bits 'flags'r
    mask=0x3f,
    # Contains a bitmask that specifies the Boolean attributes of the file.
    flags=0x0,
    # The operating system for which this file was designed.
    # 0x4 - NT and there is no need to change it.
    OS=0x40004,
    # The general type of file.
    # 0x1 - the file is an application.
    fileType=0x1,
    # The function of the file.
    # 0x0 - the function is not defined for this fileType
    subtype=0x0,
    # Creation date and time stamp.
    date=(0, 0)
    ),
  kids=[
    StringFileInfo(
      [
      StringTable(
        u'040904B0',
        [StringStruct(u'CompanyName', u'Tobias Termeczky'),
        StringStruct(u'FileDescription', u'iRacing Companion Launcher'),
        StringStruct(u'FileVersion', u'{__version__}'),
        StringStruct(u'InternalName', u'iRacing Companion Launcher'),
        StringStruct(u'LegalCopyright', u'© 2025 Tobias Termeczky'),
        StringStruct(u'OriginalFilename', u'iRacing Companion Launcher.exe'),
        StringStruct(u'ProductName', u'iRacing Companion Launcher'),
        StringStruct(u'ProductVersion', u'{__version__}')])
      ]),
    VarFileInfo([VarStruct(u'Translation', [1033, 1200])])
  ]
)
"""

    with open('version_info.txt', 'w', encoding='utf-8') as f:
        f.write(version_info)

    print(f"Created version_info.txt with version {__version__}")


def update_inno_setup():
    """Update the Inno Setup .iss file with the current version."""
    iss_file = 'windows installer.iss'

    # Read the file
    with open(iss_file, 'r', encoding='utf-8') as f:
        content = f.read()

    # Update AppVersion line
    # Convert "1.4.0" to "1.4" for Inno Setup (remove .0 if patch is 0)
    major, minor, patch, prerelease = parse_version(__version__)

    # Format version for Inno Setup
    if patch == 0:
        inno_version = f"{major}.{minor}"
    else:
        inno_version = f"{major}.{minor}.{patch}"

    if prerelease:
        inno_version += f"-{prerelease}"

    # Replace AppVersion line
    content = re.sub(
        r'AppVersion=.*',
        f'AppVersion={inno_version}',
        content
    )

    # Write back
    with open(iss_file, 'w', encoding='utf-8') as f:
        f.write(content)

    print(f"Updated {iss_file} with version {inno_version}")


if __name__ == '__main__':
    print(f"Updating version numbers to {__version__}...")
    create_version_info_file()
    update_inno_setup()
    print("Version update complete!")
