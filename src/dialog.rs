//! The Browse dialog: the Windows open dialog as Tk 8.6's `tk_getOpenFile` opens it, which the
//! Python app (up to v2.3.2) used through `filedialog.askopenfilename`.
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
use crate::win::{ComApartment, wide};

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
        // SAFETY: `dialog` is a live IFileOpenDialog on this thread; the owner
        // is this app's own window, alive while the modal dialog runs, or none.
        unsafe { dialog.Show(self.owner()) }.ok()?;
        // SAFETY: `dialog` is live, and Show succeeded, so it holds a result.
        let item = unsafe { dialog.GetResult() }.ok()?;
        // SAFETY: `item` is a live IShellItem; the call takes only a value besides it.
        let name = unsafe { item.GetDisplayName(SIGDN_FILESYSPATH) }.ok()?;
        // SAFETY: GetDisplayName succeeded, so `name` points to a NUL-terminated
        // UTF-16 string that stays valid until it is freed below.
        let path = unsafe { name.to_string() };
        // SAFETY: `name` was allocated by the COM task allocator for this caller,
        // is not read after this line and is freed exactly once.
        unsafe { CoTaskMemFree(Some(name.0.cast_const().cast())) };
        // Tk's ConvertExternalFilename turns backslashes into forward slashes,
        // and Python logs and saves the path as returned.
        path.ok().map(|p| p.replace('\\', "/"))
    }
}

/// `GetFileNameVista` up to `Show`: filters, options, title and folder, in Tk's order.
fn open_dialog(request: &DialogRequest) -> Option<IFileOpenDialog> {
    // SAFETY: `FileOpenDialog` is a static CLSID, there is no outer object, and
    // the result is returned as an owned interface; COM is initialised on this
    // thread, or creation fails and returns an error.
    let dialog: IFileOpenDialog =
        unsafe { CoCreateInstance(&FileOpenDialog, None, CLSCTX_INPROC_SERVER) }.ok()?;
    // SAFETY: `dialog` is a live interface owned here; the call takes nothing else.
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
        // SAFETY: `dialog` is live; each spec points to NUL-terminated strings in
        // `texts`, and `specs` and `texts` outlive the call. They are dropped when
        // this function returns, while the dialog may live on: that is sound
        // because SetFileTypes copies the specs and their strings.
        unsafe { dialog.SetFileTypes(&specs) }.ok()?;
        // No -typevariable: Tk shows the first filter (indices are 1-based).
        // SAFETY: `dialog` is live; the call takes only a value besides it.
        unsafe { dialog.SetFileTypeIndex(1) }.ok()?;
    }

    // SAFETY: `dialog` is live; the call takes only a value besides it.
    unsafe { dialog.SetOptions(tk_open_options(defaults)) }.ok()?;

    let title = wide(&request.title);
    // SAFETY: `dialog` is live; `title` is a NUL-terminated UTF-16 string alive
    // across the call, which copies it.
    unsafe { dialog.SetTitle(PCWSTR(title.as_ptr())) }.ok()?;

    // Tk ignores every failure here and opens wherever the dialog would.
    let dir = wide(&request.initial_dir);
    // SAFETY: `dir` is a NUL-terminated UTF-16 string alive across the call,
    // there is no bind context, and the item is returned as an owned interface.
    let folder: windows::core::Result<IShellItem> =
        unsafe { SHCreateItemFromParsingName(PCWSTR(dir.as_ptr()), None) };
    if let Ok(folder) = folder {
        // SAFETY: `dialog` and `folder` are live interfaces owned here; the
        // dialog takes its own reference to the folder.
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

#[cfg(test)]
mod tests {
    use super::{open_dialog, tk_open_options};
    use crate::app::DialogRequest;
    use crate::win::ComApartment;
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
        // SAFETY: `dialog` is a live interface owned here; the call takes nothing else.
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
        // SAFETY: `dialog` is still live; the call takes nothing else.
        assert_eq!(unsafe { dialog.GetFileTypeIndex() }.unwrap(), 1);
        // SAFETY: `dialog` is still live; the folder comes back as an owned interface.
        let folder = unsafe { dialog.GetFolder() }.unwrap();
        // SAFETY: `folder` is a live IShellItem; the call takes only a value besides it.
        let name = unsafe { folder.GetDisplayName(super::SIGDN_FILESYSPATH) }.unwrap();
        // SAFETY: GetDisplayName succeeded, so `name` points to a NUL-terminated
        // UTF-16 string that stays valid until it is freed below.
        let path = unsafe { name.to_string() }.unwrap();
        // SAFETY: `name` was allocated by the COM task allocator for this caller,
        // is not read after this line and is freed exactly once.
        unsafe { super::CoTaskMemFree(Some(name.0.cast_const().cast())) };
        assert_eq!(path.to_lowercase(), "c:\\program files");
    }
}
