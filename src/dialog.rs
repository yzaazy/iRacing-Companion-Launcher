//! The Browse dialog: the Windows open dialog as Tk 8.6's `tk_getOpenFile` opens it (`filedialog.askopenfilename` in `ui/main_window.py`).
//!
//! The controller asks for a file through [`FileDialog`]; this is the real
//! implementation, kept apart so the controller holds no Win32 code.

use std::ffi::c_void;

use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use windows::Win32::Foundation::HWND;
use windows::Win32::System::Com::{CLSCTX_INPROC_SERVER, CoCreateInstance, CoTaskMemFree};
use windows::Win32::UI::Shell::Common::COMDLG_FILTERSPEC;
use windows::Win32::UI::Shell::{
    FILEOPENDIALOGOPTIONS, FOS_ALLOWMULTISELECT, FOS_FORCEFILESYSTEM, FOS_NOVALIDATE,
    FOS_OVERWRITEPROMPT, FOS_PATHMUSTEXIST, FOS_PICKFOLDERS, FileOpenDialog, IFileOpenDialog,
    IShellItem, SHCreateItemFromParsingName, SIGDN_FILESYSPATH,
};
use windows::core::PCWSTR;

use crate::app::{DialogRequest, FileDialog};
use crate::worker::ComApartment;

/// The Windows open dialog as Tk 8.6's `tk_getOpenFile` opens it
/// (`GetFileNameVista` in `win/tkWinDialog.c`), modal to the window.
///
/// Tk keeps the dialog's default options and adds its own; it never sets
/// `FOS_NODEREFERENCELINKS`, so a picked shortcut comes back as its target.
/// Opening the dialog here, rather than through a crate with its own option
/// set, keeps that and every other option equal to Tk's.
pub struct NativeDialog<'a> {
    parent: &'a eframe::Frame,
}

impl<'a> NativeDialog<'a> {
    pub fn new(parent: &'a eframe::Frame) -> Self {
        Self { parent }
    }

    fn owner(&self) -> Option<HWND> {
        match self.parent.window_handle().ok()?.as_raw() {
            RawWindowHandle::Win32(handle) => Some(HWND(handle.hwnd.get() as *mut c_void)),
            _ => None,
        }
    }
}

impl FileDialog for NativeDialog<'_> {
    fn pick(&mut self, request: &DialogRequest) -> Option<String> {
        // Tk calls CoInitialize on its thread before its first dialog.
        let _com = ComApartment::enter();
        let dialog = open_dialog(request)?;
        // SAFETY: `dialog` is a live IFileOpenDialog; the owner is our window or none.
        unsafe { dialog.Show(self.owner()) }.ok()?;
        // SAFETY: Show succeeded, so the dialog holds a result.
        let item = unsafe { dialog.GetResult() }.ok()?;
        // SAFETY: `item` is a live IShellItem.
        let name = unsafe { item.GetDisplayName(SIGDN_FILESYSPATH) }.ok()?;
        // SAFETY: GetDisplayName returned a NUL-terminated string we now own.
        let path = unsafe { name.to_string() };
        // SAFETY: the string came from the COM task allocator and is freed once.
        unsafe { CoTaskMemFree(Some(name.0.cast_const().cast())) };
        // Tk's ConvertExternalFilename turns backslashes into forward slashes,
        // and Python logs and saves the path as returned.
        path.ok().map(|p| p.replace('\\', "/"))
    }
}

/// `GetFileNameVista` up to `Show`: filters, options, title and folder, in Tk's order.
fn open_dialog(request: &DialogRequest) -> Option<IFileOpenDialog> {
    // SAFETY: plain COM creation on a thread with an apartment.
    let dialog: IFileOpenDialog =
        unsafe { CoCreateInstance(&FileOpenDialog, None, CLSCTX_INPROC_SERVER) }.ok()?;
    // SAFETY: `dialog` is live for every call below.
    let defaults = unsafe { dialog.GetOptions() }.ok()?;

    // Tk passes each pattern list joined with ';', as the request holds it.
    let texts: Vec<(Vec<u16>, Vec<u16>)> = request
        .filters
        .iter()
        .map(|(label, pattern)| (wide(label), wide(pattern)))
        .collect();
    let specs: Vec<COMDLG_FILTERSPEC> = texts
        .iter()
        .map(|(label, pattern)| COMDLG_FILTERSPEC {
            pszName: PCWSTR(label.as_ptr()),
            pszSpec: PCWSTR(pattern.as_ptr()),
        })
        .collect();
    if !specs.is_empty() {
        // SAFETY: `specs` points into `texts`, both alive across the call.
        unsafe { dialog.SetFileTypes(&specs) }.ok()?;
        // No -typevariable: Tk shows the first filter (indices are 1-based).
        unsafe { dialog.SetFileTypeIndex(1) }.ok()?;
    }

    unsafe { dialog.SetOptions(tk_open_options(defaults)) }.ok()?;

    let title = wide(&request.title);
    // SAFETY: `title` is NUL-terminated and alive across the call.
    unsafe { dialog.SetTitle(PCWSTR(title.as_ptr())) }.ok()?;

    // Tk ignores every failure here and opens wherever the dialog would.
    let dir = wide(&request.initial_dir);
    // SAFETY: `dir` is NUL-terminated and alive across the call.
    let folder: windows::core::Result<IShellItem> =
        unsafe { SHCreateItemFromParsingName(PCWSTR(dir.as_ptr()), None) };
    if let Ok(folder) = folder {
        // SAFETY: both interfaces are live.
        let _ = unsafe { dialog.SetFolder(&folder) };
    }
    Some(dialog)
}

/// The options Tk sets on an open dialog, over the dialog's defaults: it adds
/// `FOS_FORCEFILESYSTEM | FOS_NOVALIDATE | FOS_PATHMUSTEXIST`, clears folder
/// picking and multi-select, and adds `FOS_OVERWRITEPROMPT` because
/// `-confirmoverwrite` defaults to on for every file dialog.
fn tk_open_options(defaults: FILEOPENDIALOGOPTIONS) -> FILEOPENDIALOGOPTIONS {
    let mut flags = defaults | FOS_FORCEFILESYSTEM | FOS_NOVALIDATE | FOS_PATHMUSTEXIST;
    flags &= !FOS_PICKFOLDERS;
    flags &= !FOS_ALLOWMULTISELECT;
    flags | FOS_OVERWRITEPROMPT
}

fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(std::iter::once(0)).collect()
}

#[cfg(test)]
mod tests {
    use super::{open_dialog, tk_open_options};
    use crate::app::DialogRequest;
    use crate::worker::ComApartment;
    use windows::Win32::UI::Shell::{
        FOS_FILEMUSTEXIST, FOS_FORCEFILESYSTEM, FOS_NOCHANGEDIR, FOS_NODEREFERENCELINKS,
        FOS_NOVALIDATE, FOS_OVERWRITEPROMPT, FOS_PATHMUSTEXIST, FOS_PICKFOLDERS,
    };

    fn game_request() -> DialogRequest {
        DialogRequest {
            title: "Select iRacing executable or shortcut".to_owned(),
            filters: vec![
                (
                    "Executable and Shortcuts".to_owned(),
                    "*.exe;*.lnk".to_owned(),
                ),
                ("Executable files".to_owned(), "*.exe".to_owned()),
                ("Shortcuts".to_owned(), "*.lnk".to_owned()),
                ("All files".to_owned(), "*.*".to_owned()),
            ],
            initial_dir: "C:\\Program Files".to_owned(),
        }
    }

    #[test]
    fn options_are_tks_and_shortcuts_are_followed_as_in_tk() {
        let flags = tk_open_options(FOS_PICKFOLDERS | FOS_NOCHANGEDIR);
        assert_eq!(
            flags,
            FOS_NOCHANGEDIR
                | FOS_FORCEFILESYSTEM
                | FOS_NOVALIDATE
                | FOS_PATHMUSTEXIST
                | FOS_OVERWRITEPROMPT
        );
        assert_eq!(flags & FOS_NODEREFERENCELINKS, Default::default());
    }

    /// Builds the real dialog up to `Show` (which would wait for a person) and
    /// reads back what it was given.
    #[test]
    fn dialog_is_configured_like_tks() {
        let _com = ComApartment::enter();
        let dialog = open_dialog(&game_request()).expect("dialog configured");
        // SAFETY: `dialog` is live.
        let flags = unsafe { dialog.GetOptions() }.unwrap();
        for wanted in [
            FOS_FORCEFILESYSTEM,
            FOS_NOVALIDATE,
            FOS_PATHMUSTEXIST,
            FOS_FILEMUSTEXIST,
            FOS_OVERWRITEPROMPT,
        ] {
            assert_eq!(flags & wanted, wanted, "{wanted:?} set");
        }
        assert_eq!(flags & FOS_NODEREFERENCELINKS, Default::default());
        // SAFETY: as above.
        assert_eq!(unsafe { dialog.GetFileTypeIndex() }.unwrap(), 1);
        // SAFETY: as above.
        let folder = unsafe { dialog.GetFolder() }.unwrap();
        // SAFETY: `folder` is live; the returned string is freed by the allocator below.
        let name = unsafe { folder.GetDisplayName(super::SIGDN_FILESYSPATH) }.unwrap();
        // SAFETY: NUL-terminated, owned here.
        let path = unsafe { name.to_string() }.unwrap();
        // SAFETY: allocated by the COM task allocator, freed once.
        unsafe { super::CoTaskMemFree(Some(name.0.cast_const().cast())) };
        assert_eq!(path.to_lowercase(), "c:\\program files");
    }
}
