//! Builds fake Start Menu trees with real shortcuts for the detection tests.

#![allow(dead_code)] // each test crate uses a different subset of the helpers

use std::os::windows::ffi::OsStrExt;
use std::path::{Path, PathBuf};

use windows::Win32::System::Com::{
    CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED, CoCreateInstance, CoInitializeEx, IPersistFile,
};
use windows::Win32::UI::Shell::{IShellLinkW, ShellLink};
use windows::core::{Interface, PCWSTR};

/// A temp directory holding `root1` and `root2` (the two Start Menus) and
/// `targets` (the files the shortcuts point at).
pub struct Tree {
    pub dir: tempfile::TempDir,
}

impl Tree {
    pub fn new() -> Self {
        Self::with_prefix("ircl-detect-")
    }

    pub fn with_prefix(prefix: &str) -> Self {
        let dir = tempfile::Builder::new().prefix(prefix).tempdir().unwrap();
        for sub in ["root1", "root2", "targets"] {
            std::fs::create_dir_all(dir.path().join(sub)).unwrap();
        }
        Self { dir }
    }

    pub fn root1(&self) -> PathBuf {
        self.dir.path().join("root1")
    }

    pub fn root2(&self) -> PathBuf {
        self.dir.path().join("root2")
    }

    pub fn roots(&self) -> Vec<PathBuf> {
        vec![self.root1(), self.root2()]
    }

    /// Creates `targets/<rel>` as a small file and returns its path.
    pub fn target(&self, rel: &str) -> PathBuf {
        let path = self.dir.path().join("targets").join(rel);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, b"dummy").unwrap();
        path
    }

    /// Creates a shortcut at `<root>/<rel>` pointing at `target`.
    pub fn lnk(&self, root: &Path, rel: &str, target: &Path) -> PathBuf {
        let path = root.join(rel);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        create_lnk(&path, target);
        path
    }
}

/// Writes a `.lnk` through the same shell interfaces the detection reads.
pub fn create_lnk(lnk: &Path, target: &Path) {
    // The test thread may or may not have COM yet; either answer is fine.
    let _ = unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) };
    let link: IShellLinkW =
        unsafe { CoCreateInstance(&ShellLink, None, CLSCTX_INPROC_SERVER) }.unwrap();
    let target = wide(target);
    unsafe { link.SetPath(PCWSTR(target.as_ptr())) }.unwrap();
    let file: IPersistFile = link.cast().unwrap();
    let lnk = wide(lnk);
    unsafe { file.Save(PCWSTR(lnk.as_ptr()), true) }.unwrap();
}

pub fn wide(path: &Path) -> Vec<u16> {
    path.as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect()
}

pub fn s(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}
