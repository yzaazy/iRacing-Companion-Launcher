//! The fonts Tk draws with on Windows, and the fonts it falls back to for characters they lack.
//!
//! At startup only Segoe UI (regular and bold) and Consolas are read, about 2 MB. Text can hold
//! any character a user's paths and names do (the log prints paths, and a profile folder can be
//! in Chinese), and Tk draws those from other Windows fonts. Here a [`FontProvider`] finds such a
//! font the first time a character is missing, in the order GDI links fonts to the family
//! (`HKLM\...\FontLink\SystemLink`), then in a list of Windows fonts covering the scripts that
//! list does not, then egui's bundled fonts. Choosing reads only a candidate's character map; the
//! whole file, which can be 20 MB for a CJK font, is read only when it has the character.

use crate::ui::theme;
use egui::epaint::text::{
    FallbackRequest, FontData, FontDefinitions, FontFamily, FontInsert, FontProvider, FontTweak,
    InsertFontFamily,
};
use std::collections::HashMap;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
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
pub fn install(ctx: &egui::Context) {
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

    pub fn load(path: &Path) -> Option<egui::epaint::text::Blob> {
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
    pub fn loaded() -> Vec<PathBuf> {
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

/// The fallback font files this process has read in full, for tests and diagnostics.
pub fn loaded_fallback_files() -> Vec<PathBuf> {
    files::loaded()
}

/// Reading `HKLM\SOFTWARE\Microsoft\Windows NT\CurrentVersion\FontLink\SystemLink`.
mod registry {
    use windows::Win32::System::Registry::{HKEY_LOCAL_MACHINE, RRF_RT_REG_MULTI_SZ, RegGetValueW};
    use windows::core::HSTRING;

    const KEY: &str = r"SOFTWARE\Microsoft\Windows NT\CurrentVersion\FontLink\SystemLink";

    /// The entries of one `REG_MULTI_SZ` value, or none when it is missing.
    pub fn system_link(value: &str) -> Vec<String> {
        let (key, name) = (HSTRING::from(KEY), HSTRING::from(value));
        let mut size = 0u32;
        // SAFETY: a size query with no buffer.
        let status = unsafe {
            RegGetValueW(
                HKEY_LOCAL_MACHINE,
                &key,
                &name,
                RRF_RT_REG_MULTI_SZ,
                None,
                None,
                Some(&mut size),
            )
        };
        if status.is_err() || size == 0 {
            return Vec::new();
        }
        let mut buf = vec![0u16; (size as usize).div_ceil(2)];
        // SAFETY: `buf` holds `size` bytes.
        let status = unsafe {
            RegGetValueW(
                HKEY_LOCAL_MACHINE,
                &key,
                &name,
                RRF_RT_REG_MULTI_SZ,
                None,
                Some(buf.as_mut_ptr().cast()),
                Some(&mut size),
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

/// Just enough of the OpenType format to answer "does this face have this character" from the
/// table directory, `name` and `cmap`, without reading the glyphs.
pub mod cmap {
    use super::*;

    /// Random access to a font file's bytes.
    trait Source {
        fn bytes(&mut self, offset: u64, len: usize) -> Option<Vec<u8>>;
    }

    impl Source for File {
        fn bytes(&mut self, offset: u64, len: usize) -> Option<Vec<u8>> {
            self.seek(SeekFrom::Start(offset)).ok()?;
            let mut buf = vec![0; len];
            self.read_exact(&mut buf).ok()?;
            Some(buf)
        }
    }

    impl Source for &[u8] {
        fn bytes(&mut self, offset: u64, len: usize) -> Option<Vec<u8>> {
            let start = usize::try_from(offset).ok()?;
            self.get(start..start.checked_add(len)?).map(<[u8]>::to_vec)
        }
    }

    fn u16_at(b: &[u8], at: usize) -> Option<u16> {
        Some(u16::from_be_bytes(b.get(at..at + 2)?.try_into().ok()?))
    }

    fn u32_at(b: &[u8], at: usize) -> Option<u32> {
        Some(u32::from_be_bytes(b.get(at..at + 4)?.try_into().ok()?))
    }

    /// Code point ranges (inclusive) that map to a glyph.
    type Coverage = Vec<(u32, u32)>;

    /// What is known about a file: per face, its names and coverage.
    struct Faces {
        faces: Vec<(Vec<String>, Coverage)>,
    }

    static PROBED: OnceLock<Mutex<HashMap<PathBuf, Option<Arc<Faces>>>>> = OnceLock::new();

    /// The face of `path` named `face` (or its first), and whether it has `c`. `None` when the
    /// file is missing or unreadable. Each file is probed once per process.
    pub fn probe_file(path: &Path, face: &str, c: char) -> Option<(u32, bool)> {
        let probed = PROBED.get_or_init(Default::default);
        let faces = {
            let mut probed = probed
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            probed
                .entry(path.to_path_buf())
                .or_insert_with(|| {
                    let mut file = File::open(path).ok()?;
                    read_faces(&mut file).map(Arc::new)
                })
                .clone()?
        };
        let index = faces
            .faces
            .iter()
            .position(|(names, _)| names.iter().any(|n| n.eq_ignore_ascii_case(face)))
            .unwrap_or(0);
        let (_, coverage) = faces.faces.get(index)?;
        Some((index as u32, contains(coverage, c)))
    }

    /// Whether face `index` of the font in `bytes` has `c`.
    pub fn covers(mut bytes: &[u8], index: u32, c: char) -> bool {
        read_faces(&mut bytes)
            .and_then(|f| f.faces.into_iter().nth(index as usize))
            .is_some_and(|(_, coverage)| contains(&coverage, c))
    }

    fn contains(coverage: &Coverage, c: char) -> bool {
        let c = c as u32;
        coverage.iter().any(|&(lo, hi)| lo <= c && c <= hi)
    }

    fn read_faces(src: &mut impl Source) -> Option<Faces> {
        let head = src.bytes(0, 12)?;
        let offsets = if head.get(0..4)? == b"ttcf" {
            let n = u32_at(&head, 8)? as usize;
            let table = src.bytes(12, 4 * n)?;
            (0..n)
                .map(|i| u32_at(&table, 4 * i).map(u64::from))
                .collect::<Option<Vec<_>>>()?
        } else {
            vec![0]
        };
        let faces = offsets
            .into_iter()
            .map(|offset| read_face(src, offset))
            .collect::<Option<Vec<_>>>()?;
        Some(Faces { faces })
    }

    fn read_face(src: &mut impl Source, offset: u64) -> Option<(Vec<String>, Coverage)> {
        let head = src.bytes(offset, 12)?;
        let n = usize::from(u16_at(&head, 4)?);
        let records = src.bytes(offset + 12, 16 * n)?;
        let table = |tag: &[u8; 4]| {
            (0..n).find_map(|i| {
                let r = &records[16 * i..16 * i + 16];
                (&r[0..4] == tag).then(|| (u32_at(r, 8), u32_at(r, 12)))
            })
        };
        let names = match table(b"name") {
            Some((Some(at), Some(len))) => src
                .bytes(u64::from(at), len as usize)
                .map(|b| face_names(&b))
                .unwrap_or_default(),
            _ => Vec::new(),
        };
        let (Some(at), Some(len)) = table(b"cmap")? else {
            return None;
        };
        let cmap = src.bytes(u64::from(at), len as usize)?;
        Some((names, coverage(&cmap).unwrap_or_default()))
    }

    /// The Windows (platform 3) family and full names of a face.
    fn face_names(name: &[u8]) -> Vec<String> {
        let (Some(count), Some(strings)) = (u16_at(name, 2), u16_at(name, 4)) else {
            return Vec::new();
        };
        (0..usize::from(count))
            .filter_map(|i| {
                let r = 6 + 12 * i;
                let (platform, id) = (u16_at(name, r)?, u16_at(name, r + 6)?);
                if platform != 3 || !matches!(id, 1 | 4 | 16) {
                    return None;
                }
                let (len, at) = (u16_at(name, r + 8)?, u16_at(name, r + 10)?);
                let start = usize::from(strings) + usize::from(at);
                let raw = name.get(start..start + usize::from(len))?;
                let units: Vec<u16> = raw
                    .as_chunks::<2>()
                    .0
                    .iter()
                    .map(|p| u16::from_be_bytes(*p))
                    .collect();
                Some(String::from_utf16_lossy(&units))
            })
            .collect()
    }

    /// The code points a `cmap` table maps to a glyph, from its Unicode subtable: format 12
    /// (full repertoire) when there is one, else format 4 (BMP).
    fn coverage(cmap: &[u8]) -> Option<Coverage> {
        let n = usize::from(u16_at(cmap, 2)?);
        let mut best: Option<(u8, usize)> = None;
        for i in 0..n {
            let r = 4 + 8 * i;
            let (platform, encoding) = (u16_at(cmap, r)?, u16_at(cmap, r + 2)?);
            let at = u32_at(cmap, r + 4)? as usize;
            let format = u16_at(cmap, at)?;
            let rank = match (platform, encoding, format) {
                (3, 10, 12) | (0, 4 | 6, 12) => 3,
                (3, 1, 4) | (0, 3, 4) => 2,
                (3, 0, 4) => 1,
                _ => continue,
            };
            if best.is_none_or(|(b, _)| rank > b) {
                best = Some((rank, at));
            }
        }
        let (_, at) = best?;
        let sub = cmap.get(at..)?;
        match u16_at(sub, 0)? {
            12 => format12(sub),
            _ => format4(sub),
        }
    }

    fn format12(sub: &[u8]) -> Option<Coverage> {
        let groups = u32_at(sub, 12)? as usize;
        (0..groups)
            .map(|i| {
                let g = 16 + 12 * i;
                let (start, end, glyph) =
                    (u32_at(sub, g)?, u32_at(sub, g + 4)?, u32_at(sub, g + 8)?);
                // Glyph 0 is .notdef: a group starting there does not cover its first code.
                Some(if glyph == 0 {
                    (start + 1, end)
                } else {
                    (start, end)
                })
            })
            .collect()
    }

    fn format4(sub: &[u8]) -> Option<Coverage> {
        let segs = usize::from(u16_at(sub, 6)? / 2);
        let ends = 14;
        let starts = ends + 2 * segs + 2;
        let deltas = starts + 2 * segs;
        let range_offsets = deltas + 2 * segs;
        let mut out = Vec::new();
        for s in 0..segs {
            let end = u32::from(u16_at(sub, ends + 2 * s)?);
            let start = u32::from(u16_at(sub, starts + 2 * s)?);
            let delta = u16_at(sub, deltas + 2 * s)?;
            let ro_at = range_offsets + 2 * s;
            let ro = usize::from(u16_at(sub, ro_at)?);
            if start > end || start == 0xFFFF {
                continue;
            }
            if ro == 0 {
                // Every code maps to code + delta; only the one that lands on 0 is missing.
                let missing = u32::from(0u16.wrapping_sub(delta));
                if (start..=end).contains(&missing) {
                    if missing > start {
                        out.push((start, missing - 1));
                    }
                    if missing < end {
                        out.push((missing + 1, end));
                    }
                } else {
                    out.push((start, end));
                }
                continue;
            }
            for code in start..=end {
                let at = ro_at + ro + 2 * (code - start) as usize;
                if u16_at(sub, at).is_some_and(|g| g != 0) {
                    match out.last_mut() {
                        Some((_, hi)) if *hi + 1 == code => *hi = code,
                        _ => out.push((code, code)),
                    }
                }
            }
        }
        Some(out)
    }
}
