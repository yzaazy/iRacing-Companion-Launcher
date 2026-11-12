//! The Start Menu search: an app's or game's exe found through its Start Menu shortcut.

use std::ffi::OsString;
use std::os::windows::ffi::OsStringExt;
use std::path::{Path, PathBuf};

use windows::Win32::Storage::FileSystem::{
    FILE_ATTRIBUTE_DIRECTORY, FILE_ATTRIBUTE_REPARSE_POINT, FindFirstFileW, FindNextFileW,
    WIN32_FIND_DATAW,
};
use windows::Win32::System::Com::{
    CLSCTX_INPROC_SERVER, CoCreateInstance, IPersistFile, STGM_READ,
};
use windows::Win32::UI::Shell::{IShellLinkW, SLGP_UNCPRIORITY, ShellLink};
use windows::core::{Interface, PCWSTR};

use crate::win::{ComApartment, FindHandle, wide};

/// `IO_REPARSE_TAG_SYMLINK` (winnt.h); the constant lives behind a `windows`
/// feature this crate does not enable.
const IO_REPARSE_TAG_SYMLINK: u32 = 0xA000_000C;

/// pywin32's `IShellLink.GetPath` passes a `MAX_PATH` buffer, so a longer
/// target comes back truncated and then fails the existence check.
const MAX_PATH: usize = 260;

/// What a Start Menu hit returns. Python's `is_game` flag.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShortcutLookup {
    /// The shortcut's stored target, which must exist (apps, `is_game=False`).
    Target,
    /// The `.lnk` path itself (games, `is_game=True`).
    Shortcut,
}

/// Searches the user's and then the all-users Start Menu.
///
/// Games get the `.lnk` path itself, apps the shortcut's target, which must
/// exist. Python's `Path(os.getenv(...))` raises when a variable is unset; a
/// missing variable skips that root here instead.
pub fn find_shortcut_target(shortcut_names: &[&str], lookup: ShortcutLookup) -> Option<String> {
    let roots: Vec<PathBuf> = ["APPDATA", "ProgramData"]
        .iter()
        .filter_map(std::env::var_os)
        .map(|base| {
            PathBuf::from(base)
                .join("Microsoft")
                .join("Windows")
                .join("Start Menu")
                .join("Programs")
        })
        .collect();
    find_shortcut_target_in(&roots, shortcut_names, lookup)
}

/// [`find_shortcut_target`] over explicit roots, so tests can use a fake tree.
pub fn find_shortcut_target_in(
    roots: &[PathBuf],
    shortcut_names: &[&str],
    lookup: ShortcutLookup,
) -> Option<String> {
    for root in roots {
        if !root.exists() {
            continue;
        }
        for name in shortcut_names {
            let hit = rglob_literal(root, name, |candidate| {
                if lookup == ShortcutLookup::Shortcut {
                    candidate
                        .exists()
                        .then(|| candidate.to_string_lossy().into_owned())
                } else {
                    shortcut_path(candidate).filter(|target| Path::new(target).exists())
                }
            });
            if hit.is_some() {
                return hit;
            }
        }
    }
    None
}

/// Python 3.13's `root.rglob(name)` for a name without glob characters, with
/// `accept` applied to each hit in yield order until it returns `Some`.
///
/// `rglob` checks `<dir>\<name>` with `lstat` (case-insensitive on NTFS, and
/// the yielded path keeps the pattern's casing) for the root, then walks a
/// stack: it lists a directory, checks each child directory in listing order
/// as it meets it, and descends into the most recently pushed one first.
/// Directory symlinks are not followed; junctions are, because `DirEntry`
/// only reports `IO_REPARSE_TAG_SYMLINK` as a link.
fn rglob_literal<T>(
    root: &Path,
    name: &str,
    mut accept: impl FnMut(&Path) -> Option<T>,
) -> Option<T> {
    let mut check = |dir: &Path| {
        let candidate = dir.join(name);
        std::fs::symlink_metadata(&candidate)
            .ok()
            .and_then(|_| accept(&candidate))
    };
    if let Some(hit) = check(root) {
        return Some(hit);
    }
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        for child in subdirectories(&dir) {
            if let Some(hit) = check(&child) {
                return Some(hit);
            }
            stack.push(child);
        }
    }
    None
}

/// The entries of `dir` that `os.scandir` reports as directories without
/// following symlinks, in `FindFirstFileW` order. An unreadable directory
/// yields nothing, as `rglob` swallows the `OSError`.
fn subdirectories(dir: &Path) -> Vec<PathBuf> {
    let pattern = wide(dir.join("*"));
    let mut data = WIN32_FIND_DATAW::default();
    // SAFETY: `pattern` is a NUL-terminated UTF-16 string and `data` a live,
    // writable WIN32_FIND_DATAW, both alive across the call; a returned handle
    // is owned by `FindHandle` right after, which closes it once.
    let Ok(handle) = (unsafe { FindFirstFileW(PCWSTR(pattern.as_ptr()), &raw mut data) }) else {
        return Vec::new();
    };
    let find = FindHandle(handle);
    let mut dirs = Vec::new();
    loop {
        let len = data
            .cFileName
            .iter()
            .position(|&c| c == 0)
            .unwrap_or(MAX_PATH);
        let name = OsString::from_wide(&data.cFileName[..len]);
        let attributes = data.dwFileAttributes;
        let is_symlink = attributes & FILE_ATTRIBUTE_REPARSE_POINT.0 != 0
            && data.dwReserved0 == IO_REPARSE_TAG_SYMLINK;
        if name != "."
            && name != ".."
            && attributes & FILE_ATTRIBUTE_DIRECTORY.0 != 0
            && !is_symlink
        {
            dirs.push(dir.join(name));
        }
        // SAFETY: `find` holds the open search handle; `data` is the same live,
        // writable WIN32_FIND_DATAW.
        if unsafe { FindNextFileW(find.0, &raw mut data) }.is_err() {
            break;
        }
    }
    dirs
}

/// `winshell.shortcut(path).path`: a path ending in `.lnk` (case-sensitive,
/// as winshell checks) is loaded through `IShellLinkW` and its stored target
/// read without resolving it; any other path becomes the target itself.
/// `None` when COM or the load fails, which Python's `except Exception`
/// turns into a skip.
fn shortcut_path(lnk: &Path) -> Option<String> {
    let text = lnk.to_string_lossy();
    if !text.ends_with(".lnk") {
        return Some(text.into_owned());
    }
    let _com = ComApartment::enter()?;
    // SAFETY: COM is initialised on this thread while `_com` lives; `ShellLink`
    // is a static CLSID, there is no outer object, and the result is returned
    // as an owned interface.
    let link: IShellLinkW =
        unsafe { CoCreateInstance(&ShellLink, None, CLSCTX_INPROC_SERVER) }.ok()?;
    // winshell only loads when the file exists; an unloaded link has an
    // empty target, which then fails the existence check.
    if lnk.exists() {
        let file: IPersistFile = link.cast().ok()?;
        let path = wide(lnk);
        // SAFETY: `file` is a live interface; `path` is a NUL-terminated UTF-16
        // string alive across the call.
        unsafe { file.Load(PCWSTR(path.as_ptr()), STGM_READ) }.ok()?;
    }
    let mut buffer = [0u16; MAX_PATH];
    // SAFETY: `link` is a live interface; the buffer is passed as a slice with
    // its length, and a null find-data pointer is allowed.
    unsafe { link.GetPath(&mut buffer, std::ptr::null_mut(), SLGP_UNCPRIORITY.0 as u32) }.ok()?;
    let len = buffer.iter().position(|&c| c == 0).unwrap_or(MAX_PATH);
    Some(String::from_utf16_lossy(&buffer[..len]))
}
