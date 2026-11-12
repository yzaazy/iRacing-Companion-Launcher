//! The fonts Tk draws with on Windows, and the fonts it falls back to for characters they lack.
//!
//! At startup only Segoe UI (regular and bold) and Consolas are read, about 2 MB. Text can hold
//! any character a user's paths and names do (the log prints paths, and a profile folder can be
//! in Chinese), and Tk draws those from other Windows fonts. Here a [`FontProvider`] finds such a
//! font the first time a character is missing, in the order GDI links fonts to the family
//! (`HKLM\...\FontLink\SystemLink`), then in a list of Windows fonts covering the scripts that
//! list does not, then egui's bundled fonts. Choosing reads only a candidate's character map; the
//! whole file, which can be 20 MB for a CJK font, is read only when it has the character.

pub mod cmap;

use crate::ui::theme;
use egui::epaint::text::{
    FallbackRequest, FontData, FontDefinitions, FontFamily, FontInsert, FontProvider, FontTweak,
    InsertFontFamily,
};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};

/// The Windows fonts Tk uses, keyed by the egui family each one heads.
const SYSTEM_FONTS: [(&str, &str); 3] = [
    (theme::SEGOE, "segoeui.ttf"),
    (theme::SEGOE_BOLD, "segoeuib.ttf"),
    (theme::CONSOLAS, "consola.ttf"),
];

/// Fonts for scripts the SystemLink lists leave out, in the order they are tried after those
/// lists: the fonts Windows' own fallback (Uniscribe, DirectWrite) assigns to those scripts.
const SCRIPT_FONTS: &[(&str, &str)] = &[
    ("seguisym.ttf", "Segoe UI Symbol"),
    ("Nirmala.ttc", "Nirmala UI"),
    ("nirmala.ttf", "Nirmala UI"),
    ("LeelawUI.ttf", "Leelawadee UI"),
    ("ebrima.ttf", "Ebrima"),
    ("gadugi.ttf", "Gadugi"),
    ("mmrtext.ttf", "Myanmar Text"),
    ("javatext.ttf", "Javanese Text"),
    ("monbaiti.ttf", "Mongolian Baiti"),
    ("himalaya.ttf", "Microsoft Himalaya"),
    ("msyi.ttf", "Microsoft Yi Baiti"),
    ("taile.ttf", "Microsoft Tai Le"),
    ("ntailu.ttf", "Microsoft New Tai Lue"),
    ("phagspa.ttf", "Microsoft PhagsPa"),
    ("seguihis.ttf", "Segoe UI Historic"),
    ("sylfaen.ttf", "Sylfaen"),
    ("simsunb.ttf", "SimSun-ExtB"),
    ("SimsunExtG.ttf", "SimSun-ExtG"),
    ("seguiemj.ttf", "Segoe UI Emoji"),
    ("arial.ttf", "Arial"),
];

/// GDI places glyphs on whole pixels; so do we, so sub-pixel offsets stay off.
fn gdi_tweak() -> FontTweak {
    FontTweak {
        subpixel_binning: Some(false),
        ..Default::default()
    }
}

fn fonts_dir() -> PathBuf {
    let windir = std::env::var_os("WINDIR").unwrap_or_else(|| "C:\\Windows".into());
    Path::new(&windir).join("Fonts")
}

/// Load Segoe UI (regular and bold) and Consolas and register the families the theme refers to,
/// then install the fallback provider. Without the Windows files the families fall back to
/// egui's bundled fonts, so a missing file never fails.
pub(super) fn install(ctx: &egui::Context) {
    let mut defs = FontDefinitions::default();
    let builtin = |family: FontFamily| defs.families.get(&family).cloned().unwrap_or_default();
    let builtin_proportional = builtin(FontFamily::Proportional);
    let builtin_monospace = builtin(FontFamily::Monospace);

    let dir = fonts_dir();
    let mut loaded = Vec::new();
    for (name, file) in SYSTEM_FONTS {
        if let Ok(bytes) = std::fs::read(dir.join(file)) {
            let data = FontData::from_owned(bytes).tweak(gdi_tweak());
            defs.font_data.insert(name.to_owned(), Arc::new(data));
            loaded.push(name);
        }
    }

    // egui's bundled fonts head a family only when its Windows font is missing; otherwise they
    // come last, from the provider, after every Windows font GDI would use.
    let chain = |primary: &[&str], builtin: &[String]| -> Vec<String> {
        let names: Vec<String> = primary
            .iter()
            .filter(|n| loaded.contains(n))
            .map(|n| (*n).to_owned())
            .collect();
        if names.is_empty() {
            builtin.to_vec()
        } else {
            names
        }
    };
    let segoe = chain(&[theme::SEGOE, theme::CONSOLAS], &builtin_proportional);
    let segoe_bold = chain(
        &[theme::SEGOE_BOLD, theme::SEGOE, theme::CONSOLAS],
        &builtin_proportional,
    );
    let consolas = chain(&[theme::CONSOLAS, theme::SEGOE], &builtin_monospace);

    let bundled: Vec<(String, Arc<FontData>)> = defs
        .font_data
        .iter()
        .filter(|(name, _)| !loaded.contains(&name.as_str()))
        .map(|(name, data)| (name.clone(), data.clone()))
        .collect();

    defs.families
        .insert(FontFamily::Name(theme::SEGOE.into()), segoe.clone());
    defs.families
        .insert(FontFamily::Name(theme::SEGOE_BOLD.into()), segoe_bold);
    defs.families
        .insert(FontFamily::Name(theme::CONSOLAS.into()), consolas.clone());
    defs.families.insert(FontFamily::Proportional, segoe);
    defs.families.insert(FontFamily::Monospace, consolas);
    ctx.set_fonts(defs);
    ctx.add_font_provider(Arc::new(Fallbacks {
        dir,
        bundled,
        links: OnceLock::new(),
    }));
}

/// One fallback candidate: a font file and the face in it GDI names.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct Candidate {
    file: PathBuf,
    face: String,
}

/// The fallback provider. Asked by egui at most once per (family, character).
struct Fallbacks {
    dir: PathBuf,
    /// egui's bundled fonts, the last resort.
    bundled: Vec<(String, Arc<FontData>)>,
    /// The candidates per link list, read from the registry on first use.
    links: OnceLock<HashMap<&'static str, Vec<Candidate>>>,
}

/// The SystemLink entry each family follows. Consolas has none on Windows; Tk then looks among
/// other fonts, and Segoe UI's list is what the rest of the window falls back to.
fn link_key(family: &FontFamily) -> &'static str {
    match family {
        FontFamily::Name(name) if &**name == theme::SEGOE_BOLD => "Segoe UI Bold",
        _ => "Segoe UI",
    }
}

impl Fallbacks {
    fn candidates(&self, family: &FontFamily) -> Vec<Candidate> {
        let links = self.links.get_or_init(|| {
            ["Segoe UI", "Segoe UI Bold"]
                .into_iter()
                .map(|key| (key, self.system_link(key)))
                .collect()
        });
        let mut list = links.get(link_key(family)).cloned().unwrap_or_default();
        for (file, face) in SCRIPT_FONTS {
            let c = Candidate {
                file: self.dir.join(file),
                face: (*face).to_owned(),
            };
            if !list.contains(&c) {
                list.push(c);
            }
        }
        list
    }

    /// The fonts GDI links to `key`, in order, without the duplicate scaled entries.
    fn system_link(&self, key: &str) -> Vec<Candidate> {
        let mut list: Vec<Candidate> = Vec::new();
        for entry in registry::system_link(key) {
            let mut parts = entry.split(',');
            let (Some(file), Some(face)) = (parts.next(), parts.next()) else {
                continue;
            };
            let path = Path::new(file.trim());
            let file = if path.is_absolute() {
                path.to_path_buf()
            } else {
                self.dir.join(path)
            };
            let c = Candidate {
                file,
                face: face.trim().to_owned(),
            };
            if !list.contains(&c) {
                list.push(c);
            }
        }
        list
    }
}

impl FontProvider for Fallbacks {
    fn font_for(&self, request: &FallbackRequest<'_>) -> Option<FontInsert> {
        let c = request.cluster.chars().next()?;
        let insert = |name: String, data: FontData| {
            FontInsert::new(
                &name,
                data,
                vec![InsertFontFamily {
                    family: request.family.clone(),
                    priority: egui::epaint::text::FontPriority::Lowest,
                }],
            )
        };
        for candidate in self.candidates(request.family) {
            let Some((index, has)) = cmap::probe_file(&candidate.file, &candidate.face, c) else {
                continue;
            };
            if !has {
                continue;
            }
            let blob = files::load(&candidate.file)?;
            let data = FontData::from_blob(blob, index).tweak(gdi_tweak());
            let name = format!("{}#{index}", candidate.file.display());
            return Some(insert(name, data));
        }
        self.bundled
            .iter()
            .find(|(_, data)| cmap::covers(data.bytes(), data.index, c))
            .map(|(name, data)| insert(name.clone(), (**data).clone()))
    }
}

/// Fallback font files read so far, shared by every context in the process.
mod files {
    use super::*;

    static FILES: OnceLock<Mutex<HashMap<PathBuf, egui::epaint::text::Blob>>> = OnceLock::new();

    pub(super) fn load(path: &Path) -> Option<egui::epaint::text::Blob> {
        let files = FILES.get_or_init(Default::default);
        let mut files = files
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if let Some(blob) = files.get(path) {
            return Some(blob.clone());
        }
        let blob: egui::epaint::text::Blob = Arc::new(std::fs::read(path).ok()?);
        files.insert(path.to_path_buf(), blob.clone());
        Some(blob)
    }

    /// The fallback files read from disk so far.
    pub(super) fn loaded() -> Vec<PathBuf> {
        FILES.get().map_or_else(Vec::new, |files| {
            files
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .keys()
                .cloned()
                .collect()
        })
    }
}

/// The fallback font files this process has read in full. Public for tests.
pub fn loaded_fallback_files() -> Vec<PathBuf> {
    files::loaded()
}

/// Reading `HKLM\SOFTWARE\Microsoft\Windows NT\CurrentVersion\FontLink\SystemLink`.
mod registry {
    use windows::Win32::System::Registry::{HKEY_LOCAL_MACHINE, RRF_RT_REG_MULTI_SZ, RegGetValueW};
    use windows::core::HSTRING;

    const KEY: &str = r"SOFTWARE\Microsoft\Windows NT\CurrentVersion\FontLink\SystemLink";

    /// The entries of one `REG_MULTI_SZ` value, or none when it is missing.
    pub(super) fn system_link(value: &str) -> Vec<String> {
        let (key, name) = (HSTRING::from(KEY), HSTRING::from(value));
        let mut size = 0u32;
        // SAFETY: `key` and `name` are NUL-terminated UTF-16 strings that live until the call
        // returns. With no type or data buffer the call writes only the value's size in bytes,
        // through a pointer to the local `size`, which outlives the call.
        let status = unsafe {
            RegGetValueW(
                HKEY_LOCAL_MACHINE,
                &key,
                &name,
                RRF_RT_REG_MULTI_SZ,
                None,
                None,
                Some(&raw mut size),
            )
        };
        if status.is_err() || size == 0 {
            return Vec::new();
        }
        let mut buf = vec![0u16; (size as usize).div_ceil(2)];
        // SAFETY: `key` and `name` are NUL-terminated UTF-16 strings that live until the call
        // returns. `buf` is a live allocation of at least `size` bytes (`size.div_ceil(2)` u16s)
        // and `size` tells the call so: it writes at most that many bytes into `buf`, then the
        // number written through a pointer to the local `size`, which outlives the call.
        let status = unsafe {
            RegGetValueW(
                HKEY_LOCAL_MACHINE,
                &key,
                &name,
                RRF_RT_REG_MULTI_SZ,
                None,
                Some(buf.as_mut_ptr().cast()),
                Some(&raw mut size),
            )
        };
        if status.is_err() {
            return Vec::new();
        }
        buf.truncate(size as usize / 2);
        String::from_utf16_lossy(&buf)
            .split('\0')
            .filter(|s| !s.is_empty())
            .map(str::to_owned)
            .collect()
    }
}
