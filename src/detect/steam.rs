//! Ports `utils/steam_registry.py`: Steam install check through the uninstall registry key.

use windows::Win32::Foundation::ERROR_SUCCESS;
use windows::Win32::System::Registry::{
    HKEY, HKEY_LOCAL_MACHINE, KEY_READ, KEY_WOW64_64KEY, RegCloseKey, RegOpenKeyExW,
};
use windows::core::HSTRING;

const UNINSTALL_KEY: &str = r"SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall";

/// Whether `HKLM\...\Uninstall\Steam App <appid>` can be opened for reading.
pub fn check_steam_game_installed(appid: &str) -> bool {
    check_steam_game_installed_in(HKEY_LOCAL_MACHINE, UNINSTALL_KEY, appid)
}

/// [`check_steam_game_installed`] under any hive and parent key, so tests can
/// use a temporary `HKCU` key. The 64-bit view is explicit: the Python ships
/// as a 64-bit process, whose plain `KEY_READ` never sees `WOW6432Node`.
/// Any failure, access denied included, means not installed.
pub fn check_steam_game_installed_in(hive: HKEY, uninstall_key: &str, appid: &str) -> bool {
    let path = HSTRING::from(format!(r"{uninstall_key}\Steam App {appid}"));
    let mut key = HKEY::default();
    let opened =
        unsafe { RegOpenKeyExW(hive, &path, Some(0), KEY_READ | KEY_WOW64_64KEY, &mut key) };
    if opened != ERROR_SUCCESS {
        return false;
    }
    let _ = unsafe { RegCloseKey(key) };
    true
}
