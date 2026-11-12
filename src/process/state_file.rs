//! The Python-compatible `process_state.json`:
//! `{ "<App Name>": { "pid": int, "create_time": float, "exe_path": str } }`,
//! read in file order with Python's duplicate and non-dict rules, and written
//! with the bytes Python's `json.dump(state, f, indent=2)` produces on Windows.

use std::ffi::OsString;
use std::fmt;
use std::path::PathBuf;

use serde::de::{MapAccess, Visitor};
use serde::ser::SerializeMap;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde_json::Value;

/// The write half of `ProcessTracker::save`. A failed serialisation writes nothing: the file on disk keeps the
/// last good state rather than being replaced by an empty object.
pub(super) fn write_state(state_file: &std::path::Path, json: serde_json::Result<String>) {
    let Ok(json) = json else {
        return;
    };
    let mut tmp = OsString::from(state_file.as_os_str());
    tmp.push(".tmp");
    let tmp = PathBuf::from(tmp);
    if std::fs::write(&tmp, json).is_ok() {
        let _ = std::fs::rename(&tmp, state_file);
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(super) struct Entry {
    pub(super) pid: u64,
    pub(super) create_time: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(super) exe_path: Option<String>,
}

/// An entry as found in the file: well-formed, or anything else Python would
/// have loaded into its dict.
#[derive(Debug, Clone, PartialEq)]
pub(super) enum Slot {
    Entry(Entry),
    Other(Value),
}

impl Serialize for Slot {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        match self {
            Slot::Entry(e) => e.serialize(s),
            Slot::Other(v) => v.serialize(s),
        }
    }
}

/// Top-level object in file order, like a Python dict.
#[derive(Debug, Default)]
pub(super) struct State(pub(super) Vec<(String, Slot)>);

impl State {
    pub(super) fn get(&self, key: &str) -> Option<&Slot> {
        self.0.iter().find(|(k, _)| k == key).map(|(_, v)| v)
    }
}

impl Serialize for State {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let mut map = s.serialize_map(Some(self.0.len()))?;
        for (k, v) in &self.0 {
            map.serialize_entry(k, v)?;
        }
        map.end()
    }
}

/// Deserialises a JSON object; anything else is "not a dict" and yields None.
struct MaybeState(Option<State>);

impl<'de> Deserialize<'de> for MaybeState {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct V;
        impl<'de> Visitor<'de> for V {
            type Value = MaybeState;
            fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
                f.write_str("any JSON value")
            }
            fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<MaybeState, A::Error> {
                let mut out: Vec<(String, Slot)> = Vec::new();
                while let Some((k, v)) = map.next_entry::<String, Value>()? {
                    let slot = match serde_json::from_value::<Entry>(v.clone()) {
                        Ok(e) if v.is_object() => Slot::Entry(e),
                        _ => Slot::Other(v),
                    };
                    // Duplicate keys: the last value wins at the first position.
                    match out.iter_mut().find(|(key, _)| *key == k) {
                        Some((_, existing)) => *existing = slot,
                        None => out.push((k, slot)),
                    }
                }
                Ok(MaybeState(Some(State(out))))
            }
            fn visit_seq<A: serde::de::SeqAccess<'de>>(
                self,
                mut seq: A,
            ) -> Result<MaybeState, A::Error> {
                while seq.next_element::<serde::de::IgnoredAny>()?.is_some() {}
                Ok(MaybeState(None))
            }
            fn visit_bool<E>(self, _: bool) -> Result<MaybeState, E> {
                Ok(MaybeState(None))
            }
            fn visit_i64<E>(self, _: i64) -> Result<MaybeState, E> {
                Ok(MaybeState(None))
            }
            fn visit_u64<E>(self, _: u64) -> Result<MaybeState, E> {
                Ok(MaybeState(None))
            }
            fn visit_f64<E>(self, _: f64) -> Result<MaybeState, E> {
                Ok(MaybeState(None))
            }
            fn visit_str<E>(self, _: &str) -> Result<MaybeState, E> {
                Ok(MaybeState(None))
            }
            fn visit_unit<E>(self) -> Result<MaybeState, E> {
                Ok(MaybeState(None))
            }
        }
        d.deserialize_any(V)
    }
}

/// Python's `_load`: a missing file, unreadable file, invalid JSON (UTF-8 BOM
/// included) or a non-object all mean an empty state. Nothing is written back.
pub(super) fn load(path: &std::path::Path) -> State {
    let Ok(bytes) = std::fs::read(path) else {
        return State::default();
    };
    match serde_json::from_slice::<MaybeState>(&bytes) {
        Ok(MaybeState(Some(state))) => state,
        _ => State::default(),
    }
}

/// Python truthiness of a JSON value.
pub(super) fn is_falsy(v: &Value) -> bool {
    match v {
        Value::Null => true,
        Value::Bool(b) => !b,
        Value::Number(n) => n.as_f64() == Some(0.0),
        Value::String(s) => s.is_empty(),
        Value::Array(a) => a.is_empty(),
        Value::Object(o) => o.is_empty(),
    }
}

/// The bytes Python's `json.dump(state, f, indent=2)` writes through a text-mode
/// file on Windows: non-ASCII escaped (`ensure_ascii`), CRLF line endings.
pub(super) fn to_python_json(state: &impl Serialize) -> serde_json::Result<String> {
    let pretty = serde_json::to_string_pretty(state)?;
    let mut out = String::with_capacity(pretty.len() + 64);
    for c in pretty.chars() {
        match c {
            '\n' => out.push_str("\r\n"),
            c if (c as u32) < 0x7F => out.push(c),
            c => {
                let mut units = [0u16; 2];
                for u in c.encode_utf16(&mut units) {
                    out.push_str(&format!("\\u{u:04x}"));
                }
            }
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(json: &str) -> State {
        match serde_json::from_str::<MaybeState>(json) {
            Ok(MaybeState(Some(s))) => s,
            _ => State::default(),
        }
    }

    #[test]
    fn writes_python_json_bytes() {
        // Expected bytes from Python: json.dump(d, f, indent=2) via open(..., "w").
        let state = parse(
            r#"{"Crew Chief": {"pid": 1234, "create_time": 1762851234.5678234, "exe_path": "C:\\Caf\u00e9\\a.exe"}}"#,
        );
        assert_eq!(
            to_python_json(&state).unwrap(),
            "{\r\n  \"Crew Chief\": {\r\n    \"pid\": 1234,\r\n    \"create_time\": 1762851234.5678234,\r\n    \"exe_path\": \"C:\\\\Caf\\u00e9\\\\a.exe\"\r\n  }\r\n}"
        );
        assert_eq!(to_python_json(&State::default()).unwrap(), "{}");
    }

    #[test]
    fn escapes_astral_and_del_like_python() {
        let state = parse(r#"{"k": {"pid": 1, "create_time": 2.0, "exe_path": "\u007f😀"}}"#);
        let out = to_python_json(&state).unwrap();
        assert!(out.contains(r#""exe_path": "\u007f\ud83d\ude00""#), "{out}");
        assert!(out.contains(r#""create_time": 2.0"#), "{out}");
    }

    /// A value whose serialisation always fails.
    struct Unserialisable;

    impl Serialize for Unserialisable {
        fn serialize<S: Serializer>(&self, _: S) -> Result<S::Ok, S::Error> {
            Err(serde::ser::Error::custom("refused"))
        }
    }

    #[test]
    fn failed_serialisation_skips_the_save() {
        let dir = std::env::temp_dir().join(format!("ircl-tracker-skip-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("process_state.json");
        let tmp = dir.join("process_state.json.tmp");
        let _ = std::fs::remove_file(&tmp);
        let good = "{
  \"a\": 1
}";
        std::fs::write(&file, good).unwrap();

        let json = to_python_json(&Unserialisable);
        assert!(json.is_err());
        write_state(&file, json);
        assert_eq!(std::fs::read_to_string(&file).unwrap(), good);
        assert!(!tmp.exists());

        // A successful serialisation still replaces the file through the temp file.
        write_state(&file, to_python_json(&State::default()));
        assert_eq!(std::fs::read_to_string(&file).unwrap(), "{}");
        assert!(!tmp.exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn keeps_file_order_and_last_duplicate() {
        let s = parse(r#"{"b": 0, "a": {}, "b": {"pid": 5, "create_time": 1}}"#);
        let keys: Vec<&str> = s.0.iter().map(|(k, _)| k.as_str()).collect();
        assert_eq!(keys, ["b", "a"]);
        assert!(matches!(s.0[0].1, Slot::Entry(Entry { pid: 5, .. })));
    }

    #[test]
    fn non_object_top_level_is_empty() {
        for json in ["[1, 2]", "3", "\"x\"", "null", "true"] {
            assert!(matches!(
                serde_json::from_str::<MaybeState>(json),
                Ok(MaybeState(None))
            ));
        }
    }

    #[test]
    fn falsy_matches_python() {
        for v in ["null", "false", "0", "0.0", "\"\"", "[]", "{}"] {
            assert!(is_falsy(&serde_json::from_str(v).unwrap()), "{v}");
        }
        for v in ["true", "1", "\"a\"", "[0]", "{\"a\": 1}"] {
            assert!(!is_falsy(&serde_json::from_str(v).unwrap()), "{v}");
        }
    }
}
