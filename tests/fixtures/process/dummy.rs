//! Dummy process for the process-layer tests. `tests/process_layer.rs` compiles
//! it with rustc into a temp directory; it is never part of the shipped binary.
//!
//! It takes no arguments, because the tracker launches exes with none. Its role
//! comes from its own file name: `<prefix>-root.exe` starts `<prefix>-child.exe`
//! from the same directory, `<prefix>-child.exe` starts `<prefix>-grandchild.exe`,
//! anything else just waits. Every instance exits when `<its exe path>.stop`
//! exists, and after two minutes regardless, so a crashed test leaks nothing.
//!
//! Two roles restrict who may open them, like helpers that refuse the launcher:
//! `-locked` allows only terminate and limited query (no SYNCHRONIZE, so
//! waiting on it is denied), `-opaque` allows nothing at all.

#![windows_subsystem = "windows"]

use std::ffi::c_void;
use std::path::PathBuf;
use std::process::Command;
use std::thread::sleep;
use std::time::{Duration, Instant};

#[link(name = "advapi32")]
extern "system" {
    fn ConvertStringSecurityDescriptorToSecurityDescriptorW(
        sddl: *const u16,
        revision: u32,
        sd: *mut *mut c_void,
        len: *mut u32,
    ) -> i32;
    fn GetSecurityDescriptorDacl(
        sd: *mut c_void,
        present: *mut i32,
        dacl: *mut *mut c_void,
        defaulted: *mut i32,
    ) -> i32;
    fn SetSecurityInfo(
        handle: *mut c_void,
        object_type: u32,
        info: u32,
        owner: *mut c_void,
        group: *mut c_void,
        dacl: *mut c_void,
        sacl: *mut c_void,
    ) -> u32;
}

#[link(name = "kernel32")]
extern "system" {
    fn GetCurrentProcess() -> *mut c_void;
}

/// Replaces this process's DACL.
fn restrict_self(sddl: &str) {
    const SE_KERNEL_OBJECT: u32 = 6;
    const DACL_SECURITY_INFORMATION: u32 = 4;
    let wide: Vec<u16> = sddl.encode_utf16().chain(Some(0)).collect();
    let mut sd = std::ptr::null_mut();
    let (mut present, mut defaulted, mut dacl) = (0, 0, std::ptr::null_mut());
    unsafe {
        if ConvertStringSecurityDescriptorToSecurityDescriptorW(
            wide.as_ptr(),
            1,
            &mut sd,
            std::ptr::null_mut(),
        ) == 0
            || GetSecurityDescriptorDacl(sd, &mut present, &mut dacl, &mut defaulted) == 0
        {
            return;
        }
        SetSecurityInfo(
            GetCurrentProcess(),
            SE_KERNEL_OBJECT,
            DACL_SECURITY_INFORMATION,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            dacl,
            std::ptr::null_mut(),
        );
    }
}

fn main() {
    let Ok(exe) = std::env::current_exe() else {
        return;
    };
    let stem = exe
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    if stem.ends_with("-locked") {
        // PROCESS_TERMINATE | PROCESS_QUERY_LIMITED_INFORMATION for everyone.
        restrict_self("D:(A;;0x1001;;;WD)");
    } else if stem.ends_with("-opaque") {
        restrict_self("D:");
    }
    let next = if let Some(prefix) = stem.strip_suffix("-root") {
        Some(format!("{prefix}-child.exe"))
    } else {
        stem.strip_suffix("-child")
            .map(|prefix| format!("{prefix}-grandchild.exe"))
    };
    let _child = next.and_then(|name| Command::new(exe.with_file_name(name)).spawn().ok());

    let mut stop = exe.clone().into_os_string();
    stop.push(".stop");
    let stop = PathBuf::from(stop);
    let started = Instant::now();
    while !stop.exists() && started.elapsed() < Duration::from_secs(120) {
        sleep(Duration::from_millis(50));
    }
}
