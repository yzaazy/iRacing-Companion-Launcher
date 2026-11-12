//! Builds fake Start Menu trees with real shortcuts for the detection tests.

#![allow(dead_code)] // each test crate uses a different subset of the helpers
#![allow(clippy::unwrap_used, clippy::expect_used)] // helpers outside #[test] functions; a panic there fails the test

use std::os::windows::ffi::OsStrExt;
use std::path::{Path, PathBuf};

use windows::Win32::System::Com::{
    CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED, CoCreateInstance, CoInitializeEx, IPersistFile,
};
use windows::Win32::UI::Shell::{IShellLinkW, ShellLink};
use windows::core::{Interface, PCWSTR};

/// A temp directory holding `root1` and `root2` (the two Start Menus) and
/// `targets` (the files the shortcuts point at).
pub(crate) struct Tree {
    pub(crate) dir: tempfile::TempDir,
}

impl Tree {
    pub(crate) fn new() -> Self {
        Self::with_prefix("ircl-detect-")
    }

    pub(crate) fn with_prefix(prefix: &str) -> Self {
        let dir = tempfile::Builder::new().prefix(prefix).tempdir().unwrap();
        for sub in ["root1", "root2", "targets"] {
            std::fs::create_dir_all(dir.path().join(sub)).unwrap();
        }
        Self { dir }
    }

    pub(crate) fn root1(&self) -> PathBuf {
        self.dir.path().join("root1")
    }

    pub(crate) fn root2(&self) -> PathBuf {
        self.dir.path().join("root2")
    }

    pub(crate) fn roots(&self) -> Vec<PathBuf> {
        vec![self.root1(), self.root2()]
    }

    /// Creates `targets/<rel>` as a small file and returns its path.
    pub(crate) fn target(&self, rel: &str) -> PathBuf {
        let path = self.dir.path().join("targets").join(rel);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, b"dummy").unwrap();
        path
    }

    /// Creates a shortcut at `<root>/<rel>` pointing at `target`.
    pub(crate) fn lnk(&self, root: &Path, rel: &str, target: &Path) -> PathBuf {
        let path = root.join(rel);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        create_lnk(&path, target);
        path
    }
}

/// Writes a `.lnk` through the same shell interfaces the detection reads.
pub(crate) fn create_lnk(lnk: &Path, target: &Path) {
    // The test thread may or may not have COM yet; either answer is fine.
    // SAFETY: CoInitializeEx takes no pointers (the reserved one is null) and
    // affects only the current test thread.
    let _ = unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) };
    // SAFETY: COM is initialised on this thread; `ShellLink` is a static CLSID,
    // there is no outer object, and the result is an owned interface.
    let link: IShellLinkW =
        unsafe { CoCreateInstance(&ShellLink, None, CLSCTX_INPROC_SERVER) }.unwrap();
    let target = wide(target);
    // SAFETY: `link` is live; `target` is a NUL-terminated UTF-16 string alive
    // across the call, which copies it.
    unsafe { link.SetPath(PCWSTR(target.as_ptr())) }.unwrap();
    let file: IPersistFile = link.cast().unwrap();
    let lnk = wide(lnk);
    // SAFETY: `file` is live; `lnk` is a NUL-terminated UTF-16 string alive
    // across the call.
    unsafe { file.Save(PCWSTR(lnk.as_ptr()), true) }.unwrap();
}

pub(crate) fn wide(path: &Path) -> Vec<u16> {
    path.as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect()
}

pub(crate) fn s(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}
