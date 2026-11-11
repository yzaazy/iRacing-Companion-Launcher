//! Ports `core/activity_logger.py`: builds timestamped activity log lines.

use crate::contract::{LogLevel, LogLine};

/// Prefixes `[HH:MM:SS]` in local time.
pub fn line(level: LogLevel, message: &str) -> LogLine {
    todo!()
}

pub fn divider() -> LogLine {
    todo!()
}
