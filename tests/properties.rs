//! Property tests for input the app reads at startup, where a panic would stop
//! it from opening: `config.ini`, and the game names the window wraps. The
//! state file and the command line have theirs beside their private code
//! (`src/process/state_file.rs`, `src/process/sys.rs`).

#![allow(clippy::unwrap_used, clippy::expect_used)] // strategies outside #[test] functions; a panic there fails the test

use iracing_companion_launcher::core::config::{ConfigStore, SetPathError};
use iracing_companion_launcher::ui::geometry::wrap_game_name;
use proptest::prelude::*;

const CONFIG: &str = "config.ini";
/// `GameCard._wrap_text`'s `max_length`.
const WRAP: usize = 18;

/// Characters that steer the INI reader: delimiters, comments, sections,
/// interpolation, continuation indents and line ends.
fn ini_char() -> impl Strategy<Value = char> {
    prop_oneof![
        3 => prop::sample::select(vec![
            '[', ']', '=', ':', '#', ';', '%', '(', ')', 's', ' ', '\t', '\n', '\r', 'a', 'K',
        ]),
        1 => any::<char>(),
    ]
}

/// One line of something like a `config.ini`.
fn ini_line() -> impl Strategy<Value = String> {
    let word = "[A-Za-z_ %()]{0,8}";
    prop_oneof![
        prop::sample::select(vec!["AppPaths", "Settings", "DEFAULT", "Other", "appPaths"])
            .prop_map(|s| format!("[{s}]")),
        (
            word,
            prop::sample::select(vec![" = ", "=", ": ", ":"]),
            word
        )
            .prop_map(|(k, d, v)| format!("{k}{d}{v}")),
        word.prop_map(|v| format!("\t{v}")),
        word.prop_map(|v| format!("# {v}")),
        Just(String::new()),
        prop::collection::vec(ini_char(), 0..16).prop_map(String::from_iter),
    ]
}

fn ini_text() -> impl Strategy<Value = Vec<u8>> {
    (
        prop::collection::vec(ini_line(), 0..12),
        prop::sample::select(vec!["\n", "\r\n"]),
        any::<bool>(),
    )
        .prop_map(|(lines, end, bom)| {
            let mut bytes = if bom { vec![0xEF, 0xBB, 0xBF] } else { vec![] };
            bytes.extend(lines.join(end).into_bytes());
            bytes
        })
}

/// Arbitrary bytes, or text shaped like a config file.
fn config_bytes() -> impl Strategy<Value = Vec<u8>> {
    prop_oneof![
        prop::collection::vec(any::<u8>(), 0..256),
        prop::collection::vec(ini_char(), 0..128).prop_map(|c| String::from_iter(c).into_bytes()),
        ini_text(),
    ]
}

/// A value configparser stores and reads back: no control characters, no
/// whitespace at either end (the reader strips it), and `%` only as `%%`.
fn accepted_value() -> impl Strategy<Value = String> {
    prop::collection::vec(
        prop_oneof![
            4 => "[^%\\p{Cc}]".prop_map(|s: String| s),
            1 => Just("%%".to_owned()),
        ],
        0..16,
    )
    .prop_map(|parts| parts.concat())
    .prop_filter("the reader strips surrounding whitespace", |v| {
        v.trim() == v
    })
}

/// A value with a `%` that starts neither `%%` nor `%(name)s`.
fn refused_value() -> impl Strategy<Value = String> {
    ("[^%\\p{Cc}]{0,8}", "([^%(\\p{Cc}][^%\\p{Cc}]{0,7})?")
        .prop_map(|(before, after)| format!("{before}%{after}"))
}

fn key() -> impl Strategy<Value = String> {
    "[a-z][a-z0-9_]{0,11}"
}

/// The case `config_save_is_stable` found, with the bytes Python 3.13 writes
/// (`ConfigParser.read_string` with its `ParsingError` swallowed, then `write`).
#[test]
fn key_less_line_settles_after_one_save_like_python() {
    let dir = tempfile::tempdir().unwrap();
    let input = b"[AppPaths]\na = \n=";
    let first = saved_bytes(dir.path(), &load_bytes(dir.path(), input));
    assert_eq!(
        first,
        b"[AppPaths]\r\na = \r\n = \r\n\r\n[Settings]\r\n\r\n"
    );
    let second = saved_bytes(dir.path(), &ConfigStore::load(dir.path()));
    assert_eq!(
        second,
        b"[AppPaths]\r\na = \r\n\t=\r\n\r\n[Settings]\r\n\r\n"
    );
    let third = saved_bytes(dir.path(), &ConfigStore::load(dir.path()));
    assert_eq!(third, second);
}

fn load_bytes(dir: &std::path::Path, bytes: &[u8]) -> ConfigStore {
    std::fs::write(dir.join(CONFIG), bytes).unwrap();
    ConfigStore::load(dir)
}

fn saved_bytes(dir: &std::path::Path, store: &ConfigStore) -> Vec<u8> {
    store.save().unwrap();
    std::fs::read(dir.join(CONFIG)).unwrap()
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    /// Whatever is on disk, loading and reading it never panics.
    #[test]
    fn config_load_never_panics(bytes in config_bytes(), key in key()) {
        let dir = tempfile::tempdir().unwrap();
        let store = load_bytes(dir.path(), &bytes);
        let _ = store.app_path(&key);
        let _ = store.setting(&key);
        let _ = store.bool_setting(&key, true);
    }

    /// What a save writes, a load and a second save write again byte for byte.
    ///
    /// One exception, Python's too: a line with no key before its delimiter
    /// (`=`) is kept as an option with an empty name and written as ` = `,
    /// which the next load reads as an indented continuation of the option
    /// above it. From the second save on the bytes are then stable. Python
    /// 3.13: `[AppPaths]\na = \n=` saves as `a = \n = \n`, then as `a = \n\t=\n`.
    #[test]
    fn config_save_is_stable(bytes in config_bytes()) {
        let dir = tempfile::tempdir().unwrap();
        let first = saved_bytes(dir.path(), &load_bytes(dir.path(), &bytes));
        let again = saved_bytes(dir.path(), &ConfigStore::load(dir.path()));
        let empty_name = String::from_utf8(first.clone())
            .unwrap()
            .lines()
            .any(|line| line.starts_with(" = "));
        if !empty_name {
            prop_assert_eq!(&first, &again);
        }
        let twice = saved_bytes(dir.path(), &ConfigStore::load(dir.path()));
        prop_assert_eq!(again, twice);
    }

    /// A value configparser accepts survives set, save, load and get, with
    /// `%%` read back as `%`.
    #[test]
    fn accepted_value_round_trips(key in key(), value in accepted_value(), bytes in ini_text()) {
        let dir = tempfile::tempdir().unwrap();
        let mut store = load_bytes(dir.path(), &bytes);
        store.set_setting(&key, &value);
        store.save().unwrap();
        let read = ConfigStore::load(dir.path()).setting(&key);
        prop_assert_eq!(read, Some(value.replace("%%", "%")));
    }

    /// A value configparser refuses is neither stored nor saved.
    #[test]
    fn refused_value_is_refused(key in key(), value in refused_value()) {
        let dir = tempfile::tempdir().unwrap();
        let mut store = ConfigStore::load(dir.path());
        prop_assert!(matches!(store.set_app_path(&key, &value), Err(SetPathError::Refused)));
        prop_assert!(!dir.path().join(CONFIG).exists());
        store.set_setting(&key, &value);
        prop_assert_eq!(store.setting(&key), None);
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(4096))]

    /// Names of 18 characters or fewer stay as they are; longer ones get one
    /// line break, a first line of at most 18 characters, and lose nothing but
    /// the space they broke at.
    #[test]
    fn game_name_wrap(name in "[^\\n]{0,40}|[a-zA-Z ]{0,40}") {
        let wrapped = wrap_game_name(&name);
        if name.chars().count() <= WRAP {
            prop_assert_eq!(wrapped, name);
        } else {
            prop_assert_eq!(wrapped.matches('\n').count(), 1);
            let (first, rest) = wrapped.split_once('\n').unwrap();
            prop_assert!(first.chars().count() <= WRAP);
            let at_space = format!("{first} {rest}");
            let at_limit = format!("{first}{rest}");
            prop_assert!(
                at_space == name || (at_limit == name && first.chars().count() == WRAP),
                "{:?} -> {:?}", name, wrapped
            );
        }
    }
}
