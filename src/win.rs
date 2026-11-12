//! Small Windows helpers shared by the modules that call Win32: UTF-16
//! strings, `ntpath.basename`, the COM apartment guard and RAII wrappers for
//! handles and registry keys.

use std::ffi::OsStr;
use std::marker::PhantomData;
use std::os::windows::ffi::OsStrExt;

use windows::Win32::Foundation::{CloseHandle, HANDLE, RPC_E_CHANGED_MODE};
use windows::Win32::Storage::FileSystem::FindClose;
use windows::Win32::System::Com::{COINIT_APARTMENTTHREADED, CoInitializeEx, CoUninitialize};
use windows::Win32::System::Registry::{HKEY, RegCloseKey};

/// A NUL-terminated UTF-16 copy of `text`, for `PCWSTR` and `PWSTR` arguments.
pub(crate) fn wide(text: impl AsRef<OsStr>) -> Vec<u16> {
    text.as_ref()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect()
}

/// `ntpath.basename`: after the drive, after the last `\` or `/`.
pub(crate) fn basename(path: &str) -> &str {
    let bytes = path.as_bytes();
    let tail = if bytes.len() >= 2 && bytes[1] == b':' {
        &path[2..]
    } else {
        path
    };
    tail.rsplit(['\\', '/']).next().unwrap_or(tail)
}

/// A single-threaded COM apartment for the current thread while it lives: the
/// worker thread from its start to its end, the UI thread around the file
/// dialog, and each shortcut read in `detect::shortcuts`.
///
/// `shell_open` calls `ShellExecuteW` on the worker thread, which may hand the
/// target to shell extensions that need COM. Python made those calls on the Tk
/// thread, which importing `winshell` (through `pythoncom`) had already made
/// an apartment. A guard entered on a thread that already has one nests: its
/// `CoInitializeEx` returns `S_FALSE` and its drop balances it.
pub(crate) struct ComApartment {
    owns_init: bool,
    /// `!Send` and `!Sync`: the drop must run on the thread that entered.
    _thread_bound: PhantomData<*const ()>,
}

impl ComApartment {
    /// Initialises COM with the flags pythoncom uses. `S_OK` and `S_FALSE`
    /// (already an apartment) count and are balanced on drop;
    /// `RPC_E_CHANGED_MODE` (already multithreaded) leaves the thread as it
    /// was and is accepted as is, with nothing to balance. Any other failure
    /// is `None`: a shortcut read then gives up, while the worker thread and
    /// the dialog carry on without COM of their own.
    pub(crate) fn enter() -> Option<Self> {
        // SAFETY: CoInitializeEx takes no pointers (the reserved one is null) and
        // affects only the current thread. A success is balanced by
        // CoUninitialize in Drop, which runs on the thread that entered: the
        // guard is neither Send nor Sync.
        let hr = unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) };
        if hr == RPC_E_CHANGED_MODE {
            Some(Self {
                owns_init: false,
                _thread_bound: PhantomData,
            })
        } else if hr.is_ok() {
            Some(Self {
                owns_init: true,
                _thread_bound: PhantomData,
            })
        } else {
            None
        }
    }
}

impl Drop for ComApartment {
    fn drop(&mut self) {
        if self.owns_init {
            // SAFETY: balances the successful CoInitializeEx in `enter`, on the same thread.
            unsafe { CoUninitialize() };
        }
    }
}

/// Owns a Win32 handle and closes it on drop.
pub(crate) struct Handle(pub(crate) HANDLE);

impl Drop for Handle {
    fn drop(&mut self) {
        // SAFETY: the handle was returned open by Win32 and is closed exactly once.
        let _ = unsafe { CloseHandle(self.0) };
    }
}

/// Owns a `FindFirstFileW` search handle and closes it on drop.
pub(crate) struct FindHandle(pub(crate) HANDLE);

impl Drop for FindHandle {
    fn drop(&mut self) {
        // SAFETY: the handle came from a successful FindFirstFileW and is closed exactly once.
        let _ = unsafe { FindClose(self.0) };
    }
}

/// Owns an open registry key and closes it on drop.
pub(crate) struct RegKey(pub(crate) HKEY);

impl Drop for RegKey {
    fn drop(&mut self) {
        // SAFETY: the key came from a successful RegOpenKeyExW and is closed exactly once.
        let _ = unsafe { RegCloseKey(self.0) };
    }
}

#[cfg(test)]
mod tests {
    use super::{basename, wide};

    #[test]
    fn basename_splits_like_ntpath() {
        assert_eq!(basename("C:/Program Files/A/Fanatec.exe"), "Fanatec.exe");
        assert_eq!(
            basename(r"C:\Program Files\Elgato\StreamDeck.exe"),
            "StreamDeck.exe"
        );
        assert_eq!(basename(r"C:\A\b.exe"), "b.exe");
        assert_eq!(basename("C:/a/b.exe"), "b.exe");
        assert_eq!(basename("C:b.exe"), "b.exe");
        assert_eq!(basename("b.exe"), "b.exe");
        assert_eq!(basename("C:/A/"), "");
        // Only the drive's colon splits, as ntpath.splitdrive takes "a:" alone.
        assert_eq!(basename("a:b:c"), "b:c");
    }

    #[test]
    fn wide_is_utf16_with_one_terminator() {
        assert_eq!(wide("NUL"), [b'N' as u16, b'U' as u16, b'L' as u16, 0]);
        assert_eq!(wide("é𝄞"), "é𝄞\0".encode_utf16().collect::<Vec<_>>());
        assert_eq!(wide(std::path::Path::new("a")), [b'a' as u16, 0]);
    }
}
