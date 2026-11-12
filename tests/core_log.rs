//! Log lines as `ActivityLogger` writes them, minus the trailing newline.

use iracing_companion_launcher::core::log::{divider, line};
use iracing_companion_launcher::model::LogLevel;

#[test]
fn line_has_a_local_time_prefix() {
    let l = line(LogLevel::Success, "Fanatec - configured");
    assert_eq!(l.level, LogLevel::Success);
    let (stamp, message) = l.text.split_at(11);
    assert_eq!(message, "Fanatec - configured");
    let b = stamp.as_bytes();
    assert_eq!(
        (b[0], b[3], b[6], b[9], b[10]),
        (b'[', b':', b':', b']', b' ')
    );
    let field = |i: usize| stamp[i..i + 2].parse::<u32>().unwrap();
    assert!(field(1) < 24 && field(4) < 60 && field(7) < 60, "{stamp}");
}

#[test]
fn divider_is_fifty_box_rules_without_timestamp() {
    let d = divider();
    assert_eq!(d.level, LogLevel::Divider);
    assert_eq!(d.text.chars().count(), 50);
    assert!(d.text.chars().all(|c| c == '─'));
}
