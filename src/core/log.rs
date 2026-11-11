//! Ports `core/activity_logger.py`: builds timestamped activity log lines.

use crate::contract::{LogLevel, LogLine};
use windows::Win32::Foundation::SYSTEMTIME;
use windows::Win32::System::SystemInformation::GetLocalTime;

/// Prefixes `[HH:MM:SS]` in local time.
pub fn line(level: LogLevel, message: &str) -> LogLine {
    // SAFETY: GetLocalTime only fills in a SYSTEMTIME it returns by value.
    let now = unsafe { GetLocalTime() };
    LogLine {
        level,
        text: format!("{} {message}", timestamp(&now)),
    }
}

/// `"─" * 50`, no timestamp, like `ActivityLogger.log_divider`.
pub fn divider() -> LogLine {
    LogLine {
        level: LogLevel::Divider,
        text: "─".repeat(50),
    }
}

/// `time.strftime("%H:%M:%S")` in brackets.
fn timestamp(t: &SYSTEMTIME) -> String {
    format!("[{:02}:{:02}:{:02}]", t.wHour, t.wMinute, t.wSecond)
}

#[cfg(test)]
mod tests {
    use super::timestamp;
    use windows::Win32::Foundation::SYSTEMTIME;

    #[test]
    fn timestamp_is_zero_padded_24_hour() {
        let t = SYSTEMTIME {
            wHour: 7,
            wMinute: 5,
            wSecond: 9,
            ..Default::default()
        };
        assert_eq!(timestamp(&t), "[07:05:09]");
        let t = SYSTEMTIME {
            wHour: 23,
            wMinute: 59,
            wSecond: 58,
            ..Default::default()
        };
        assert_eq!(timestamp(&t), "[23:59:58]");
    }
}
