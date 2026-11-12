//! Ports `core/config_manager.py`: `config.ini` in Python `configparser` format.
//!
//! The reader, interpolation and writer follow `configparser.ConfigParser` with
//! its defaults (Python 3.13): `=` and `:` delimiters, `#` and `;` full-line
//! comments, lowercased keys, indented continuation lines, a `[DEFAULT]`
//! section, strict duplicate checks and `BasicInterpolation`.

use std::collections::HashSet;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use windows::Win32::Globalization::{GetACP, MULTI_BYTE_TO_WIDE_CHAR_FLAGS, MultiByteToWideChar};

const CONFIG_FILENAME: &str = "config.ini";
const APP_PATHS: &str = "AppPaths";
const SETTINGS: &str = "Settings";
const DEFAULT_SECTION: &str = "DEFAULT";
const MAX_INTERPOLATION_DEPTH: usize = 10;
const APP_DIR_NAME: &str = "iRacingCompanionLauncher";

/// Key/value pairs in file order, values raw (not interpolated).
type Items = Vec<(String, String)>;

/// `ConfigManager`: the parsed `config.ini` plus the directory it lives in.
#[derive(Debug, Clone, PartialEq)]
pub struct ConfigStore {
    dir: PathBuf,
    defaults: Items,
    sections: Vec<(String, Items)>,
}

/// `ConfigManager` is one object shared by the window and both managers in
/// Python; here the controller (UI thread) and the managers (worker thread)
/// share one store. Clones share it too.
#[derive(Debug, Clone)]
pub struct SharedConfig(Arc<Mutex<ConfigStore>>);

impl SharedConfig {
    pub fn new(store: ConfigStore) -> Self {
        Self(Arc::new(Mutex::new(store)))
    }

    /// The store, for one call. A panic elsewhere must not take config access
    /// down with it, so a poisoned lock is taken over as it is.
    pub fn lock(&self) -> MutexGuard<'_, ConfigStore> {
        self.0.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

/// Where `config.ini` and `process_state.json` live (contracts.md, "Config and state location").
pub fn resolve_config_dir() -> PathBuf {
    let dir = if let Some(dir) = non_empty_env("IRCL_CONFIG_DIR") {
        PathBuf::from(dir)
    } else if cfg!(debug_assertions) {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("target")
            .join("dev-config")
    } else if let Some(appdata) = non_empty_env("APPDATA") {
        PathBuf::from(appdata).join(APP_DIR_NAME)
    } else {
        // Python falls back to the executable's directory when APPDATA is unset.
        std::env::current_exe()
            .ok()
            .and_then(|exe| exe.parent().map(Path::to_path_buf))
            .unwrap_or_default()
    };
    // A failure here surfaces later as save() returning false, as in Python.
    let _ = fs::create_dir_all(&dir);
    dir
}

fn non_empty_env(name: &str) -> Option<std::ffi::OsString> {
    std::env::var_os(name).filter(|v| !v.is_empty())
}

/// `ConfigManager.get_config_key`: lowercase, spaces to underscores.
pub fn config_key(name: &str) -> String {
    name.to_lowercase().replace(' ', "_")
}

impl ConfigStore {
    pub fn load(dir: &Path) -> Self {
        // configparser.read skips a file it cannot open; ConfigManager swallows
        // parse errors and keeps whatever was read.
        let (defaults, mut sections) = match fs::read(dir.join(CONFIG_FILENAME)) {
            Ok(bytes) => parse(&decode(&bytes)),
            Err(_) => (Vec::new(), Vec::new()),
        };
        for name in [APP_PATHS, SETTINGS] {
            if !sections.iter().any(|(n, _)| n == name) {
                sections.push((name.to_string(), Vec::new()));
            }
        }
        Self {
            dir: dir.to_path_buf(),
            defaults,
            sections,
        }
    }

    pub fn config_dir(&self) -> &Path {
        &self.dir
    }

    /// Only if it exists on disk.
    pub fn app_path(&self, key: &str) -> Option<String> {
        // Python raises on a broken interpolation here; the port treats it as unsaved.
        let saved = self.get(APP_PATHS, key).ok()??;
        Path::new(&saved).exists().then_some(saved)
    }

    /// Saves.
    pub fn set_app_path(&mut self, key: &str, path: &str) -> bool {
        // Python raises ValueError before saving; nothing is stored or written.
        if !self.set(APP_PATHS, key, path) {
            return false;
        }
        self.save()
    }

    pub fn setting(&self, key: &str) -> Option<String> {
        self.get(SETTINGS, key).ok().flatten()
    }

    /// `getboolean('Settings', key, fallback=...)`. Python raises on a value
    /// that is not a boolean word; the port returns the fallback instead.
    pub fn bool_setting(&self, key: &str, fallback: bool) -> bool {
        match self.get_boolean(SETTINGS, key) {
            Ok(Some(value)) => value,
            _ => fallback,
        }
    }

    /// Does not save. A value configparser would reject (a lone `%`) is dropped.
    pub fn set_setting(&mut self, key: &str, value: &str) {
        self.set(SETTINGS, key, value);
    }

    pub fn save(&self) -> bool {
        // Python's text mode writes CRLF line endings. UTF-8 is deliberate
        // difference 6 in behaviour.md (Python used the ANSI code page).
        let text = self.render().replace('\n', "\r\n");
        replace_atomically(&self.dir.join(CONFIG_FILENAME), text.as_bytes())
    }

    /// `ConfigParser.get(section, key)` with `BasicInterpolation`. `Ok(None)`
    /// when the key is absent from the section and from `[DEFAULT]`, `Err` where
    /// Python raises an interpolation error.
    fn get(&self, section: &str, key: &str) -> Result<Option<String>, ()> {
        let Some(items) = self.section(section) else {
            return Ok(None);
        };
        let key = key.to_lowercase();
        let Some(raw) = self.lookup(items, &key) else {
            return Ok(None);
        };
        let mut out = String::new();
        self.interpolate(items, raw, 1, &mut out)?;
        Ok(Some(out))
    }

    /// `ConfigParser.getboolean`; `Err` where Python raises.
    fn get_boolean(&self, section: &str, key: &str) -> Result<Option<bool>, ()> {
        let Some(value) = self.get(section, key)? else {
            return Ok(None);
        };
        match value.to_lowercase().as_str() {
            "1" | "yes" | "true" | "on" => Ok(Some(true)),
            "0" | "no" | "false" | "off" => Ok(Some(false)),
            _ => Err(()),
        }
    }

    /// `ConfigParser.set`: false where `BasicInterpolation.before_set` raises.
    fn set(&mut self, section: &str, key: &str, value: &str) -> bool {
        if !configparser_accepts(value) {
            return false;
        }
        let key = key.to_lowercase();
        let Some((_, items)) = self.sections.iter_mut().find(|(n, _)| n == section) else {
            return false;
        };
        insert(items, key, value.to_string());
        true
    }

    fn section(&self, name: &str) -> Option<&Items> {
        self.sections
            .iter()
            .find(|(n, _)| n == name)
            .map(|(_, items)| items)
    }

    /// Section first, then `[DEFAULT]`, like configparser's ChainMap.
    fn lookup<'a>(&'a self, items: &'a Items, key: &str) -> Option<&'a str> {
        items
            .iter()
            .chain(&self.defaults)
            .find(|(k, _)| k == key)
            .map(|(_, v)| v.as_str())
    }

    /// `BasicInterpolation._interpolate_some`.
    fn interpolate(
        &self,
        items: &Items,
        mut rest: &str,
        depth: usize,
        out: &mut String,
    ) -> Result<(), ()> {
        if depth > MAX_INTERPOLATION_DEPTH {
            return Err(());
        }
        while !rest.is_empty() {
            let Some(p) = rest.find('%') else {
                out.push_str(rest);
                return Ok(());
            };
            out.push_str(&rest[..p]);
            rest = &rest[p..];
            match rest[1..].chars().next() {
                Some('%') => {
                    out.push('%');
                    rest = &rest[2..];
                }
                Some('(') => {
                    let len = key_ref_len(rest).ok_or(())?;
                    let var = rest[2..len - 2].to_lowercase();
                    rest = &rest[len..];
                    let value = self.lookup(items, &var).ok_or(())?;
                    if value.contains('%') {
                        self.interpolate(items, value, depth + 1, out)?;
                    } else {
                        out.push_str(value);
                    }
                }
                _ => return Err(()),
            }
        }
        Ok(())
    }

    /// `RawConfigParser.write` with `space_around_delimiters=True`, `\n` line ends.
    fn render(&self) -> String {
        let mut out = String::new();
        if !self.defaults.is_empty() {
            render_section(&mut out, DEFAULT_SECTION, &self.defaults);
        }
        for (name, items) in &self.sections {
            render_section(&mut out, name, items);
        }
        out
    }
}

fn render_section(out: &mut String, name: &str, items: &Items) {
    out.push('[');
    out.push_str(name);
    out.push_str("]\n");
    for (key, value) in items {
        out.push_str(key);
        out.push_str(" = ");
        out.push_str(&value.replace('\n', "\n\t"));
        out.push('\n');
    }
    out.push('\n');
}

fn insert<V>(items: &mut Vec<(String, V)>, key: String, value: V) {
    // A dict assignment: an existing key keeps its position.
    match items.iter_mut().find(|(k, _)| *k == key) {
        Some((_, slot)) => *slot = value,
        None => items.push((key, value)),
    }
}

/// Length of a `%(name)s` reference at the start of `s` (`_KEYCRE`), if any.
fn key_ref_len(s: &str) -> Option<usize> {
    let after = s.strip_prefix("%(")?;
    let close = after.find(')')?;
    (close > 0 && after[close + 1..].starts_with('s')).then_some(2 + close + 2)
}

/// Writes `bytes` to a temporary file beside `path`, then moves it over `path`,
/// so a process that ends mid-save leaves the old file whole. Python's
/// in-place write could not be interrupted there: the window blocked until it
/// was done. False on any failure, with the temporary file removed.
fn replace_atomically(path: &Path, bytes: &[u8]) -> bool {
    let mut tmp = path.as_os_str().to_owned();
    tmp.push(".tmp");
    let tmp = PathBuf::from(tmp);
    let written = fs::File::create(&tmp).and_then(|mut file| {
        file.write_all(bytes)?;
        file.sync_all()
    });
    // `rename` replaces an existing file (MoveFileExW with MOVEFILE_REPLACE_EXISTING).
    if written.and_then(|()| fs::rename(&tmp, path)).is_ok() {
        return true;
    }
    let _ = fs::remove_file(&tmp);
    false
}

/// `BasicInterpolation.before_set`: after `%%` and `%(name)s` references are
/// removed, a remaining `%` makes `configparser.set` raise `ValueError`.
/// [`ConfigStore::set_app_path`] returns false both for such a value and for a
/// failed save; this tells the two apart.
pub fn configparser_accepts(value: &str) -> bool {
    let mut rest = value.replace("%%", "");
    let mut remaining = String::new();
    while let Some(p) = rest.find('%') {
        remaining.push_str(&rest[..p]);
        let tail = &rest[p..];
        match key_ref_len(tail) {
            Some(len) => rest = tail[len..].to_string(),
            None => {
                remaining.push('%');
                rest = tail[1..].to_string();
            }
        }
    }
    !remaining.contains('%')
}

/// UTF-8 first; Python wrote the file in the ANSI code page, so fall back to it.
fn decode(bytes: &[u8]) -> String {
    match std::str::from_utf8(bytes) {
        // Python never read a BOM file (cp1252 turns it into a broken header);
        // stripping it is part of reading UTF-8 properly.
        Ok(text) => text.strip_prefix('\u{feff}').unwrap_or(text).to_string(),
        // SAFETY: GetACP takes no arguments and only returns a number.
        Err(_) => decode_code_page(bytes, unsafe { GetACP() }),
    }
}

fn decode_code_page(bytes: &[u8], code_page: u32) -> String {
    if i32::try_from(bytes.len()).is_err() {
        return String::from_utf8_lossy(bytes).into_owned();
    }
    let flags = MULTI_BYTE_TO_WIDE_CHAR_FLAGS::default();
    // SAFETY: the input slice is valid; no output buffer asks for the length.
    let len = unsafe { MultiByteToWideChar(code_page, flags, bytes, None) };
    let Ok(len) = usize::try_from(len) else {
        return String::from_utf8_lossy(bytes).into_owned();
    };
    let mut wide = vec![0u16; len];
    // SAFETY: both slices are valid and the output is the length just asked for.
    let written = unsafe { MultiByteToWideChar(code_page, flags, bytes, Some(&mut wide)) };
    wide.truncate(usize::try_from(written).unwrap_or(0));
    String::from_utf16_lossy(&wide)
}

#[derive(Clone, Copy)]
enum Cursor {
    None,
    Defaults,
    Section(usize),
}

type RawItems = Vec<(String, Vec<String>)>;

/// `RawConfigParser._read` followed by `_join_multiline_values`.
///
/// Python raises `MissingSectionHeaderError`, `DuplicateSectionError` and
/// `DuplicateOptionError` mid-read, keeping what it read before that line;
/// ConfigManager swallows the error, so the port stops at that line too.
/// A line that is neither header nor option is skipped and reading goes on,
/// as Python does before raising `ParsingError` at the end.
fn parse(text: &str) -> (Items, Vec<(String, Items)>) {
    let mut defaults: RawItems = Vec::new();
    let mut sections: Vec<(String, RawItems)> = Vec::new();
    let mut cursor = Cursor::None;
    let mut sectname = String::new();
    let mut optname: Option<String> = None;
    let mut indent_level = 0usize;
    let mut added_sections: HashSet<String> = HashSet::new();
    let mut added_options: HashSet<(String, String)> = HashSet::new();

    let normalised = text.replace("\r\n", "\n").replace('\r', "\n");
    for line in normalised.split_terminator('\n') {
        let has_comment = line.trim().starts_with(['#', ';']);
        let clean = if has_comment { "" } else { line.trim() };

        // The current option, if one is open (an empty name counts as none).
        let current = |cursor: Cursor, optname: &Option<String>| -> Option<(Cursor, String)> {
            match (cursor, optname) {
                (Cursor::None, _) | (_, None) => None,
                (_, Some(name)) if name.is_empty() => None,
                (c, Some(name)) => Some((c, name.clone())),
            }
        };

        if clean.is_empty() {
            // Blank lines belong to a multi-line value; trailing ones are
            // stripped at the join. Comment lines are not added.
            if !has_comment && let Some((c, name)) = current(cursor, &optname) {
                append(&mut defaults, &mut sections, c, &name, String::new());
            }
            continue;
        }

        let cur_indent = line.chars().position(|c| !c.is_whitespace()).unwrap_or(0);
        if cur_indent > indent_level
            && let Some((c, name)) = current(cursor, &optname)
        {
            append(&mut defaults, &mut sections, c, &name, clean.to_string());
            continue;
        }

        indent_level = cur_indent;
        if let Some(header) = section_header(clean) {
            sectname = header.to_string();
            if let Some(i) = sections.iter().position(|(n, _)| n == header) {
                if added_sections.contains(header) {
                    break; // DuplicateSectionError
                }
                cursor = Cursor::Section(i);
                added_sections.insert(sectname.clone());
            } else if header == DEFAULT_SECTION {
                cursor = Cursor::Defaults;
            } else {
                sections.push((sectname.clone(), Vec::new()));
                cursor = Cursor::Section(sections.len() - 1);
                added_sections.insert(sectname.clone());
            }
            optname = None;
            continue;
        }
        if matches!(cursor, Cursor::None) {
            break; // MissingSectionHeaderError
        }

        // OPTCRE: the first `=` or `:` splits name from value.
        let Some(pos) = clean.find(['=', ':']) else {
            continue; // ParsingError, collected; optname stays as it was
        };
        let name = clean[..pos].trim_end().to_lowercase();
        let value = clean[pos + 1..].trim().to_string();
        if !added_options.insert((sectname.clone(), name.clone())) {
            break; // DuplicateOptionError
        }
        let items = match cursor {
            Cursor::Defaults => &mut defaults,
            Cursor::Section(i) => &mut sections[i].1,
            Cursor::None => unreachable!("checked above"),
        };
        insert(items, name.clone(), vec![value]);
        optname = Some(name);
    }

    let join = |items: RawItems| -> Items {
        items
            .into_iter()
            .map(|(k, lines)| (k, lines.join("\n").trim_end().to_string()))
            .collect()
    };
    (
        join(defaults),
        sections
            .into_iter()
            .map(|(name, items)| (name, join(items)))
            .collect(),
    )
}

fn append(
    defaults: &mut RawItems,
    sections: &mut [(String, RawItems)],
    cursor: Cursor,
    name: &str,
    line: String,
) {
    let items = match cursor {
        Cursor::Defaults => defaults,
        Cursor::Section(i) => &mut sections[i].1,
        Cursor::None => return,
    };
    if let Some((_, lines)) = items.iter_mut().find(|(k, _)| k == name) {
        lines.push(line);
    }
}

/// `SECTCRE` (`\[(?P<header>.+)\]`, matched at the start): up to the last `]`.
fn section_header(clean: &str) -> Option<&str> {
    let inner = clean.strip_prefix('[')?;
    let close = inner.rfind(']')?;
    (close > 0).then(|| &inner[..close])
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;

    fn fixture_dir() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/core")
    }

    /// Every fixture in tests/fixtures/core, named by its `.expected.json`, so a new
    /// file cannot go untested. Each has a `.saved.ini`, every `.ini` input has an
    /// expectation, and only `missing` has no input.
    fn fixtures() -> Vec<String> {
        let (mut names, mut saved, mut inputs) = (Vec::new(), Vec::new(), Vec::new());
        for entry in fs::read_dir(fixture_dir()).unwrap() {
            let file = entry.unwrap().file_name().into_string().unwrap();
            if let Some(name) = file.strip_suffix(".expected.json") {
                names.push(name.to_owned());
            } else if let Some(name) = file.strip_suffix(".saved.ini") {
                saved.push(name.to_owned());
            } else if let Some(name) = file.strip_suffix(".ini") {
                inputs.push(name.to_owned());
            }
        }
        names.sort();
        saved.sort();
        assert!(names.len() >= 8, "fixtures not found: {names:?}");
        assert_eq!(saved, names, "a .saved.ini per fixture");
        for input in &inputs {
            assert!(names.contains(input), "{input}.ini has no expectation");
        }
        for name in &names {
            assert!(
                inputs.contains(name) == (name != "missing"),
                "{name}: only `missing` has no .ini"
            );
        }
        names
    }

    fn load_fixture(name: &str) -> (ConfigStore, tempfile::TempDir) {
        let dir = tempfile::tempdir().unwrap();
        let src = fixture_dir().join(format!("{name}.ini"));
        if src.exists() {
            fs::copy(&src, dir.path().join(CONFIG_FILENAME)).unwrap();
        }
        (ConfigStore::load(dir.path()), dir)
    }

    fn expected(name: &str) -> Value {
        let path = fixture_dir().join(format!("{name}.expected.json"));
        serde_json::from_str(&fs::read_to_string(path).unwrap()).unwrap()
    }

    fn pairs(value: &Value) -> Items {
        value
            .as_array()
            .unwrap()
            .iter()
            .map(|p| {
                let p = p.as_array().unwrap();
                (
                    p[0].as_str().unwrap().to_string(),
                    p[1].as_str().unwrap().to_string(),
                )
            })
            .collect()
    }

    /// The parsed state equals what configparser itself parsed, section by section.
    #[test]
    fn parse_matches_configparser() {
        for name in &fixtures() {
            let (store, _dir) = load_fixture(name);
            let exp = expected(name);
            assert_eq!(store.defaults, pairs(&exp["defaults"]), "{name}: defaults");
            let sections: Vec<(String, Items)> = exp["sections"]
                .as_array()
                .unwrap()
                .iter()
                .map(|s| (s[0].as_str().unwrap().to_string(), pairs(&s[1])))
                .collect();
            assert_eq!(store.sections, sections, "{name}: sections");
        }
    }

    /// `get` and `getboolean` give configparser's result, or fail where it raises.
    #[test]
    fn get_and_getboolean_match_configparser() {
        for name in &fixtures() {
            let (store, _dir) = load_fixture(name);
            let exp = expected(name);
            for (ident, want) in exp["get"].as_object().unwrap() {
                let (section, key) = ident.split_once('/').unwrap();
                let got = store.get(section, key);
                match want {
                    Value::Null => assert_eq!(got, Ok(None), "{name}: get {ident}"),
                    Value::String(s) => {
                        assert_eq!(got, Ok(Some(s.clone())), "{name}: get {ident}")
                    }
                    _ => assert_eq!(got, Err(()), "{name}: get {ident}"),
                }
            }
            for (ident, want) in exp["getboolean"].as_object().unwrap() {
                let (section, key) = ident.split_once('/').unwrap();
                let got = store.get_boolean(section, key);
                match want {
                    Value::Null => assert_eq!(got, Ok(None), "{name}: getboolean {ident}"),
                    Value::Bool(b) => assert_eq!(got, Ok(Some(*b)), "{name}: getboolean {ident}"),
                    _ => assert_eq!(got, Err(()), "{name}: getboolean {ident}"),
                }
            }
        }
    }

    #[test]
    fn windows_1252_bytes_decode_with_code_page_1252() {
        // "Équipe – Müller" in Windows-1252; 0x96 is the en dash.
        let bytes = b"\xc9quipe \x96 M\xfcller";
        assert_eq!(decode_code_page(bytes, 1252), "Équipe – Müller");
    }

    #[test]
    fn utf8_bom_is_dropped() {
        assert_eq!(decode(b"\xef\xbb\xbf[AppPaths]"), "[AppPaths]");
    }

    #[test]
    fn fatal_errors_keep_what_came_before() {
        // Missing section header on the first line: nothing is read.
        let (d, s) = parse("key = value\n[AppPaths]\nx = y\n");
        assert!(d.is_empty() && s.is_empty());

        // Duplicate option: reading stops at the duplicate.
        let (_, s) = parse("[AppPaths]\na = 1\nb = 2\na = 3\nc = 4\n");
        assert_eq!(
            s,
            vec![(
                "AppPaths".into(),
                vec![("a".into(), "1".into()), ("b".into(), "2".into())]
            )]
        );

        // Duplicate section: reading stops at the second header.
        let (_, s) = parse("[AppPaths]\na = 1\n[Settings]\n[AppPaths]\nb = 2\n");
        assert_eq!(
            s,
            vec![
                ("AppPaths".into(), vec![("a".into(), "1".into())]),
                ("Settings".into(), vec![]),
            ]
        );
    }

    #[test]
    fn before_set_rules() {
        for ok in [
            "",
            "C:\\plain",
            "100%% sure",
            "%(base)s\\x",
            "a%%b%(c)sd",
            "C:/100%%/a.exe",
            "%(home)s/a.exe",
        ] {
            assert!(configparser_accepts(ok), "{ok:?} should be accepted");
        }
        for bad in [
            "50% off",
            "%",
            "%()s",
            "%(x)",
            "%%%",
            "C:/100%/a.exe",
            "%(home)/a.exe",
        ] {
            assert!(!configparser_accepts(bad), "{bad:?} should be rejected");
        }
    }
}
