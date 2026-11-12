//! Steam uninstall-key check under a temporary HKCU key.

use iracing_companion_launcher::detect::steam::{
    check_steam_game_installed, check_steam_game_installed_in,
};
use windows::Win32::Foundation::ERROR_SUCCESS;
use windows::Win32::System::Registry::{
    HKEY, HKEY_CURRENT_USER, KEY_READ, KEY_WRITE, REG_OPTION_VOLATILE, RegCloseKey,
    RegCreateKeyExW, RegDeleteTreeW, RegOpenKeyExW,
};
use windows::core::HSTRING;

/// `HKCU\Software\ircl-detect-test-<pid>-<tag>`, volatile and deleted on drop.
struct TempKey(String);

impl TempKey {
    fn new(tag: &str) -> Self {
        Self(format!(
            r"Software\ircl-detect-test-{}-{tag}",
            std::process::id()
        ))
    }

    fn create(&self, sub: &str) {
        let path = HSTRING::from(format!(r"{}\{sub}", self.0));
        let mut key = HKEY::default();
        // SAFETY: `path` is a NUL-terminated HSTRING alive across the call, the
        // optional pointers are null, and `key` is a live HKEY for the result,
        // closed once below.
        let made = unsafe {
            RegCreateKeyExW(
                HKEY_CURRENT_USER,
                &path,
                None,
                None,
                REG_OPTION_VOLATILE,
                KEY_WRITE,
                None,
                &raw mut key,
                None,
            )
        };
        assert_eq!(made, ERROR_SUCCESS);
        // SAFETY: `key` was opened by the successful call above and is closed once.
        let _ = unsafe { RegCloseKey(key) };
    }
}

impl Drop for TempKey {
    fn drop(&mut self) {
        // SAFETY: the subkey name is a NUL-terminated HSTRING alive across the
        // call; it names only this test's own volatile key.
        let _ = unsafe { RegDeleteTreeW(HKEY_CURRENT_USER, &HSTRING::from(&self.0)) };
    }
}

#[test]
fn present_key_means_installed() {
    let path = {
        let key = TempKey::new("present");
        key.create("Steam App 266410");

        assert!(check_steam_game_installed_in(
            HKEY_CURRENT_USER,
            &key.0,
            "266410"
        ));
        key.0.clone()
    };
    // No test leaves a key behind in the developer's registry.
    assert!(!check_steam_game_installed_in(
        HKEY_CURRENT_USER,
        &path,
        "266410"
    ));
    let mut handle = HKEY::default();
    // SAFETY: the subkey name is a NUL-terminated HSTRING alive across the call
    // and `handle` a live HKEY for the result; the call is expected to fail.
    let opened = unsafe {
        RegOpenKeyExW(
            HKEY_CURRENT_USER,
            &HSTRING::from(path),
            Some(0),
            KEY_READ,
            &raw mut handle,
        )
    };
    assert_ne!(opened, ERROR_SUCCESS);
}

#[test]
fn absent_or_different_key_means_not_installed() {
    let key = TempKey::new("absent");
    key.create("Steam App 805550");

    assert!(!check_steam_game_installed_in(
        HKEY_CURRENT_USER,
        &key.0,
        "266410"
    ));
    assert!(!check_steam_game_installed_in(
        HKEY_CURRENT_USER,
        &key.0,
        "80555"
    ));
    assert!(!check_steam_game_installed_in(
        HKEY_CURRENT_USER,
        &key.0,
        ""
    ));
    assert!(!check_steam_game_installed_in(
        HKEY_CURRENT_USER,
        r"Software\ircl-no-such-key-7f3a",
        "805550"
    ));
}

#[test]
fn real_registry_check_does_not_fail() {
    // Read-only against HKLM: no Steam app has this id.
    assert!(!check_steam_game_installed("0000000000"));
}
