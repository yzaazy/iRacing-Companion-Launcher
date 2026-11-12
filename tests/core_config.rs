//! `ConfigStore` against files written by Python's configparser (tests/fixtures/core).

use iracing_companion_launcher::core::config::{ConfigStore, config_key};
use serde_json::Value;
use std::fs;
use std::path::{Path, PathBuf};

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/core")
        .join(name)
}

/// Every fixture in tests/fixtures/core, named by its `.expected.json`, so a new
/// file cannot go untested. Each has a `.saved.ini`, every `.ini` input has an
/// expectation, and only `missing` has no input.
fn fixtures() -> Vec<String> {
    let (mut names, mut saved, mut inputs) = (Vec::new(), Vec::new(), Vec::new());
    for entry in fs::read_dir(fixture("")).unwrap() {
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

/// A temp dir holding the fixture as `config.ini` (nothing for `missing`).
fn dir_with(name: &str) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    let src = fixture(&format!("{name}.ini"));
    if src.exists() {
        fs::copy(src, dir.path().join("config.ini")).unwrap();
    }
    dir
}

fn config_bytes(dir: &Path) -> Vec<u8> {
    fs::read(dir.join("config.ini")).unwrap()
}

#[test]
fn load_then_save_writes_what_configmanager_writes() {
    for name in &fixtures() {
        let dir = dir_with(name);
        let store = ConfigStore::load(dir.path());
        assert!(store.save(), "{name}: save");
        assert_eq!(
            String::from_utf8(config_bytes(dir.path())).unwrap(),
            fs::read_to_string(fixture(&format!("{name}.saved.ini"))).unwrap(),
            "{name}"
        );
    }
}

#[test]
fn settings_read_like_configparser() {
    for name in &fixtures() {
        let dir = dir_with(name);
        let store = ConfigStore::load(dir.path());
        let exp: Value = serde_json::from_str(
            &fs::read_to_string(fixture(&format!("{name}.expected.json"))).unwrap(),
        )
        .unwrap();
        for (ident, want) in exp["get"].as_object().unwrap() {
            let Some(key) = ident.strip_prefix("Settings/") else {
                continue;
            };
            assert_eq!(
                store.setting(key).as_deref(),
                want.as_str(),
                "{name}: {ident}"
            );
        }
        for (ident, want) in exp["getboolean"].as_object().unwrap() {
            let Some(key) = ident.strip_prefix("Settings/") else {
                continue;
            };
            for fallback in [true, false] {
                let expected = want.as_bool().unwrap_or(fallback);
                assert_eq!(
                    store.bool_setting(key, fallback),
                    expected,
                    "{name}: {ident}"
                );
            }
        }
    }
}

#[test]
fn file_as_the_app_writes_it() {
    let dir = dir_with("app_written");
    let store = ConfigStore::load(dir.path());
    assert_eq!(store.config_dir(), dir.path());
    assert!(store.bool_setting("fanatec_enabled", false));
    assert!(!store.bool_setting("simhub_enabled", true));
    assert!(!store.bool_setting("garage61_enabled", true));
    // Missing *_enabled keys use the caller's fallback (true at startup).
    assert!(store.bool_setting("crew_chief_v4_enabled", true));
    assert_eq!(store.setting("selected_game").as_deref(), Some(""));
    assert_eq!(store.setting("nope"), None);
    // A saved steam:// URL never exists on disk.
    assert_eq!(store.app_path("game_iracing"), None);
}

#[test]
fn comments_colons_and_mixed_case_keys() {
    let dir = dir_with("comments");
    let store = ConfigStore::load(dir.path());
    assert_eq!(store.setting("SELECTED_GAME").as_deref(), Some("iRacing"));
    assert_eq!(store.setting("selected_game").as_deref(), Some("iRacing"));
    assert_eq!(store.setting("Colon_And_Equals").as_deref(), Some("a = b"));
    assert_eq!(store.setting("empty_value").as_deref(), Some(""));
    // [DEFAULT] keys show through in every section.
    assert_eq!(store.setting("shared_key").as_deref(), Some("from default"));
    // configparser reads the indented line as a continuation, not a key.
    assert_eq!(store.setting("simhub_enabled"), None);
    assert_eq!(
        store.setting("fanatec_enabled").as_deref(),
        Some("yes\nSimHub_Enabled   =   OFF")
    );
    // ...which makes it no longer a boolean word.
    assert!(!store.bool_setting("fanatec_enabled", false));
    assert!(store.bool_setting("garage61_enabled", false));
    // Not a boolean word: Python raises, the port falls back.
    assert!(store.bool_setting("crew_chief_v4_enabled", true));
    assert!(!store.bool_setting("crew_chief_v4_enabled", false));
}

#[test]
fn windows_1252_file_with_non_ascii_paths() {
    let dir = dir_with("cp1252");
    // The fixture is Python's locale-encoded write, not valid UTF-8.
    assert!(String::from_utf8(config_bytes(dir.path())).is_err());

    let mut store = ConfigStore::load(dir.path());
    assert_eq!(store.setting("selected_game").as_deref(), Some("rFactor 2"));

    // A saved non-ASCII path that exists is returned.
    let exe_dir = dir.path().join("Équipe Müller");
    fs::create_dir(&exe_dir).unwrap();
    let exe = exe_dir.join("CrewChiefV4.exe");
    fs::write(&exe, b"").unwrap();
    let exe = exe.to_str().unwrap();
    assert!(store.set_app_path("crew_chief_v4", exe));
    // Written back as UTF-8, read back as UTF-8.
    assert!(
        String::from_utf8(config_bytes(dir.path()))
            .unwrap()
            .contains(exe)
    );
    assert_eq!(
        ConfigStore::load(dir.path())
            .app_path("crew_chief_v4")
            .as_deref(),
        Some(exe)
    );
}

#[test]
fn windows_1252_path_that_exists_is_found() {
    let dir = tempfile::tempdir().unwrap();
    let exe_dir = dir.path().join("Équipe Müller");
    fs::create_dir(&exe_dir).unwrap();
    let exe = exe_dir.join("x.exe");
    fs::write(&exe, b"").unwrap();
    let exe = exe.to_str().unwrap();

    // Encode the line the way Python's cp1252 write would (Latin-1 range only).
    let text = format!("[AppPaths]\r\nfanatec = {exe}\r\n\r\n[Settings]\r\n\r\n");
    let bytes: Vec<u8> = text
        .chars()
        .map(|c| u8::try_from(u32::from(c)).expect("temp path outside Latin-1"))
        .collect();
    fs::write(dir.path().join("config.ini"), bytes).unwrap();

    assert_eq!(
        ConfigStore::load(dir.path()).app_path("fanatec").as_deref(),
        Some(exe)
    );
}

#[test]
fn app_path_only_when_it_exists_on_disk() {
    let dir = tempfile::tempdir().unwrap();
    let exe = dir.path().join("Fanatec.exe");
    let mut store = ConfigStore::load(dir.path());
    assert_eq!(store.app_path("fanatec"), None);

    assert!(store.set_app_path("fanatec", exe.to_str().unwrap()));
    assert_eq!(store.app_path("fanatec"), None, "saved but missing on disk");

    fs::write(&exe, b"").unwrap();
    assert_eq!(store.app_path("fanatec").as_deref(), exe.to_str());
    assert_eq!(
        store.app_path("FANATEC").as_deref(),
        exe.to_str(),
        "keys ignore case"
    );

    // os.path.exists is true for a directory too.
    assert!(store.set_app_path("simhub", dir.path().to_str().unwrap()));
    assert_eq!(store.app_path("simhub").as_deref(), dir.path().to_str());

    assert!(store.set_app_path("game_iracing", "steam://rungameid/266410"));
    assert_eq!(store.app_path("game_iracing"), None);
}

#[test]
fn set_app_path_saves_immediately() {
    let dir = tempfile::tempdir().unwrap();
    let mut store = ConfigStore::load(dir.path());
    assert!(!dir.path().join("config.ini").exists());
    assert!(store.set_app_path("Fanatec", r"C:\Fanatec.exe"));
    assert_eq!(
        config_bytes(dir.path()),
        b"[AppPaths]\r\nfanatec = C:\\Fanatec.exe\r\n\r\n[Settings]\r\n\r\n"
    );
    // Replacing a value keeps its position.
    assert!(store.set_app_path("simhub", r"C:\SimHub.exe"));
    assert!(store.set_app_path("fanatec", r"D:\Fanatec.exe"));
    assert_eq!(
        config_bytes(dir.path()),
        b"[AppPaths]\r\nfanatec = D:\\Fanatec.exe\r\nsimhub = C:\\SimHub.exe\r\n\r\n[Settings]\r\n\r\n"
    );
}

#[test]
fn set_setting_does_not_save() {
    let dir = dir_with("app_written");
    let before = config_bytes(dir.path());
    let mut store = ConfigStore::load(dir.path());
    store.set_setting("Fanatec_Enabled", "False");
    store.set_setting("selected_game", "rFactor 2");
    assert_eq!(config_bytes(dir.path()), before);
    assert!(!store.bool_setting("fanatec_enabled", true));
    assert_eq!(store.setting("selected_game").as_deref(), Some("rFactor 2"));

    assert!(store.save());
    let text = String::from_utf8(config_bytes(dir.path())).unwrap();
    assert!(text.contains("\r\nfanatec_enabled = False\r\n"));
    assert!(text.contains("\r\nselected_game = rFactor 2\r\n"));
    assert_eq!(text.matches("fanatec_enabled").count(), 1);
}

#[test]
fn every_boolean_word_in_any_case() {
    let dir = tempfile::tempdir().unwrap();
    let mut store = ConfigStore::load(dir.path());
    for word in ["1", "yes", "true", "on", "YES", "True", "On"] {
        store.set_setting("k", word);
        assert!(store.bool_setting("k", false), "{word}");
    }
    for word in ["0", "no", "false", "off", "NO", "False", "OFF"] {
        store.set_setting("k", word);
        assert!(!store.bool_setting("k", true), "{word}");
    }
    for word in ["", "2", "y", "enabled"] {
        store.set_setting("k", word);
        assert!(store.bool_setting("k", true), "{word:?}");
        assert!(!store.bool_setting("k", false), "{word:?}");
    }
}

#[test]
fn lone_percent_is_rejected_like_configparser_set() {
    let dir = tempfile::tempdir().unwrap();
    let mut store = ConfigStore::load(dir.path());
    // configparser.set raises ValueError: nothing stored, nothing written.
    assert!(!store.set_app_path("fanatec", r"C:\100% Racing\Fanatec.exe"));
    assert!(!dir.path().join("config.ini").exists());
    store.set_setting("selected_game", "50% off");
    assert_eq!(store.setting("selected_game"), None);

    // An escaped percent is stored raw and read back unescaped.
    let exe = dir.path().join("100% Racing.exe");
    fs::write(&exe, b"").unwrap();
    let escaped = exe.to_str().unwrap().replace('%', "%%");
    assert!(store.set_app_path("fanatec", &escaped));
    assert_eq!(store.app_path("fanatec").as_deref(), exe.to_str());
}

#[test]
fn missing_directory_loads_empty_and_save_fails() {
    let dir = tempfile::tempdir().unwrap();
    let gone = dir.path().join("does-not-exist");
    let mut store = ConfigStore::load(&gone);
    assert_eq!(store.setting("selected_game"), None);
    assert!(!store.save());
    assert!(!store.set_app_path("fanatec", r"C:\x.exe"));
    assert!(!gone.exists());
}

#[test]
fn save_replaces_the_file_and_leaves_no_temporary() {
    let dir = dir_with("app_written");
    let mut store = ConfigStore::load(dir.path());
    store.set_setting("selected_game", "iRacing");
    assert!(store.save());
    let names: Vec<_> = fs::read_dir(dir.path())
        .unwrap()
        .map(|e| e.unwrap().file_name())
        .collect();
    assert_eq!(names, ["config.ini"]);
    assert!(
        String::from_utf8(config_bytes(dir.path()))
            .unwrap()
            .contains("selected_game = iRacing\r\n")
    );
}

#[test]
fn failed_save_keeps_the_old_file_and_removes_the_temporary() {
    let dir = dir_with("app_written");
    let before = config_bytes(dir.path());
    let path = dir.path().join("config.ini");
    let mut store = ConfigStore::load(dir.path());
    store.set_setting("selected_game", "iRacing");

    // A read-only config.ini cannot be replaced: the save fails after the
    // temporary file was written.
    let writable = fs::metadata(&path).unwrap().permissions();
    let mut read_only = writable.clone();
    read_only.set_readonly(true);
    fs::set_permissions(&path, read_only).unwrap();
    assert!(!store.save());
    assert_eq!(config_bytes(dir.path()), before);
    assert!(!dir.path().join("config.ini.tmp").exists());
    fs::set_permissions(&path, writable).unwrap();

    // A directory in the way of the temporary file: nothing is written.
    fs::create_dir(dir.path().join("config.ini.tmp")).unwrap();
    assert!(!store.save());
    assert_eq!(config_bytes(dir.path()), before);
}

#[test]
fn config_keys() {
    assert_eq!(config_key("Fanatec"), "fanatec");
    assert_eq!(config_key("Crew Chief V4"), "crew_chief_v4");
    assert_eq!(config_key("Elgato Stream Deck"), "elgato_stream_deck");
    assert_eq!(
        config_key("MarvinsAIRA Refactored"),
        "marvinsaira_refactored"
    );
    assert_eq!(
        config_key("Assetto Corsa Competizione"),
        "assetto_corsa_competizione"
    );
}
