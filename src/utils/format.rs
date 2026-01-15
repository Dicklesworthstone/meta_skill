//! Output formatting utilities

/// Truncate a string to a maximum length
pub fn truncate_string(s: &str, max_len: usize) -> String {
    let mut out = String::new();
    for (idx, ch) in s.chars().enumerate() {
        if idx >= max_len {
            break;
        }
        out.push(ch);
    }
    if s.chars().count() > max_len {
        if max_len >= 3 {
            let trimmed = out
                .chars()
                .take(max_len.saturating_sub(3))
                .collect::<String>();
            format!("{trimmed}...")
        } else {
            "...".to_string()
        }
    } else {
        out
    }
}

/// Format size in human-readable form
pub fn format_size(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = KB * 1024;
    const GB: u64 = MB * 1024;

    if bytes >= GB {
        format!("{:.1} GB", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.1} MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.1} KB", bytes as f64 / KB as f64)
    } else {
        format!("{} B", bytes)
    }
}

/// Format duration in human-readable form
pub fn format_duration(secs: u64) -> String {
    if secs >= 3600 {
        format!("{}h {}m", secs / 3600, (secs % 3600) / 60)
    } else if secs >= 60 {
        format!("{}m {}s", secs / 60, secs % 60)
    } else {
        format!("{}s", secs)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // =========================================================================
    // truncate_string tests
    // =========================================================================

    #[test]
    fn truncate_string_no_truncation() {
        assert_eq!(truncate_string("hello", 10), "hello");
    }

    #[test]
    fn truncate_string_exact_length() {
        assert_eq!(truncate_string("hello", 5), "hello");
    }

    #[test]
    fn truncate_string_with_truncation() {
        let result = truncate_string("hello world", 8);
        assert!(result.ends_with("..."));
        assert_eq!(result, "hello...");
    }

    #[test]
    fn truncate_string_very_short_max() {
        assert_eq!(truncate_string("hello", 3), "...");
    }

    #[test]
    fn truncate_string_max_less_than_3() {
        assert_eq!(truncate_string("hello", 2), "...");
        assert_eq!(truncate_string("hello", 1), "...");
        assert_eq!(truncate_string("hello", 0), "...");
    }

    #[test]
    fn truncate_string_empty() {
        assert_eq!(truncate_string("", 10), "");
    }

    #[test]
    fn truncate_string_multibyte_utf8() {
        // Japanese characters are multibyte
        let s = "日本語テスト";
        let result = truncate_string(s, 4);
        // Should truncate by char count, not bytes
        assert!(result.ends_with("..."));
        assert_eq!(result.chars().count(), 4); // 1 char + "..."
    }

    #[test]
    fn truncate_string_emoji() {
        let s = "🚀🎉🎊🎁";
        let result = truncate_string(s, 2);
        assert_eq!(result, "...");
    }

    // =========================================================================
    // format_size tests
    // =========================================================================

    #[test]
    fn format_size_bytes() {
        assert_eq!(format_size(0), "0 B");
        assert_eq!(format_size(512), "512 B");
        assert_eq!(format_size(1023), "1023 B");
    }

    #[test]
    fn format_size_kilobytes() {
        assert_eq!(format_size(1024), "1.0 KB");
        assert_eq!(format_size(1536), "1.5 KB");
        assert_eq!(format_size(10 * 1024), "10.0 KB");
    }

    #[test]
    fn format_size_megabytes() {
        assert_eq!(format_size(1024 * 1024), "1.0 MB");
        assert_eq!(format_size(5 * 1024 * 1024), "5.0 MB");
    }

    #[test]
    fn format_size_gigabytes() {
        assert_eq!(format_size(1024 * 1024 * 1024), "1.0 GB");
        assert_eq!(format_size(2 * 1024 * 1024 * 1024), "2.0 GB");
    }

    // =========================================================================
    // format_duration tests
    // =========================================================================

    #[test]
    fn format_duration_seconds() {
        assert_eq!(format_duration(0), "0s");
        assert_eq!(format_duration(30), "30s");
        assert_eq!(format_duration(59), "59s");
    }

    #[test]
    fn format_duration_minutes() {
        assert_eq!(format_duration(60), "1m 0s");
        assert_eq!(format_duration(90), "1m 30s");
        assert_eq!(format_duration(120), "2m 0s");
        assert_eq!(format_duration(3599), "59m 59s");
    }

    #[test]
    fn format_duration_hours() {
        assert_eq!(format_duration(3600), "1h 0m");
        assert_eq!(format_duration(3660), "1h 1m");
        assert_eq!(format_duration(7200), "2h 0m");
        assert_eq!(format_duration(7260), "2h 1m");
    }

    #[test]
    fn format_duration_large() {
        // 25 hours
        assert_eq!(format_duration(25 * 3600), "25h 0m");
        // 100 hours + 30 minutes
        assert_eq!(format_duration(100 * 3600 + 30 * 60), "100h 30m");
    }
}
