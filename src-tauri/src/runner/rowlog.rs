//! The result file that sits beside a run's Excel/CSV list: one line per row the run took,
//! saying what came of it, so the operator can check a long run without reading the log.
//!
//! Like the `.used.txt` beside it, it is a separate file — the operator's own sheet is never
//! modified. It is a CSV with a UTF-8 byte-order mark, which is what makes Excel show Vietnamese
//! text correctly when the file is double-clicked.

use std::io::Write;

pub const HEADER: [&str; 6] = ["Thời gian", "Profile", "Giá trị", "Bình luận", "Kết quả", "Chi tiết"];

/// Where the results for `list` (the path of the Excel/CSV the rows came from) are kept.
pub fn results_path(list: &str) -> String {
    format!("{list}.results.csv")
}

/// One CSV line: every field quoted, quotes doubled. Newlines inside a field are flattened,
/// so one row of the file is always one result.
pub fn csv_line(fields: &[&str]) -> String {
    let cells: Vec<String> = fields
        .iter()
        .map(|f| format!("\"{}\"", f.replace(['\r', '\n'], " ").replace('"', "\"\"")))
        .collect();
    cells.join(",")
}

/// Appends one result. The file is created, with its header, on the first one.
pub fn append(list: &str, fields: &[&str; 6]) -> std::io::Result<()> {
    let path = results_path(list);
    let fresh = !std::path::Path::new(&path).exists();
    let mut f = std::fs::OpenOptions::new().create(true).append(true).open(&path)?;
    if fresh {
        f.write_all(b"\xEF\xBB\xBF")?;
        writeln!(f, "{}", csv_line(&HEADER))?;
    }
    writeln!(f, "{}", csv_line(fields))
}

/// `YYYY-MM-DD HH:MM:SS` for `unix` seconds shifted by `offset_secs` from UTC.
pub fn stamp(unix: u64, offset_secs: i64) -> String {
    let t = unix as i64 + offset_secs;
    let days = t.div_euclid(86_400);
    let rem = t.rem_euclid(86_400);
    // Days since 1970-01-01 to a civil date (Howard Hinnant's algorithm).
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    format!("{y:04}-{m:02}-{d:02} {:02}:{:02}:{:02}", rem / 3600, rem % 3600 / 60, rem % 60)
}

/// Seconds this machine's clock is ahead of UTC right now (+25200 in Vietnam).
pub fn local_offset_secs() -> i64 {
    #[cfg(windows)]
    {
        use windows_sys::Win32::System::Time::{GetTimeZoneInformation, TIME_ZONE_INFORMATION};
        // SAFETY: `GetTimeZoneInformation` only fills the struct it is handed.
        unsafe {
            let mut tz: TIME_ZONE_INFORMATION = std::mem::zeroed();
            let id = GetTimeZoneInformation(&mut tz);
            // UTC = local + bias, and daylight saving has a bias of its own.
            let extra = match id {
                2 => tz.DaylightBias,
                _ => tz.StandardBias,
            };
            -((tz.Bias + extra) as i64) * 60
        }
    }
    #[cfg(unix)]
    {
        // SAFETY: `localtime_r` fills the `tm` it is given and keeps no pointer to it.
        unsafe {
            let t = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs() as libc::time_t)
                .unwrap_or(0);
            let mut tm: libc::tm = std::mem::zeroed();
            if libc::localtime_r(&t, &mut tm).is_null() {
                return 0;
            }
            tm.tm_gmtoff as i64
        }
    }
    #[cfg(not(any(windows, unix)))]
    {
        0
    }
}

/// The time of a result, in this machine's own time.
pub fn local_stamp(unix: u64) -> String {
    stamp(unix, local_offset_secs())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stamps_are_civil_dates_in_the_offset_asked_for() {
        assert_eq!(stamp(0, 0), "1970-01-01 00:00:00");
        // 2026-10-07 13:45:10 UTC
        assert_eq!(stamp(1_791_380_710, 0), "2026-10-07 13:45:10");
        // Vietnam is UTC+7: the same instant, seven hours on, here across midnight.
        assert_eq!(stamp(1_791_380_710 + 11 * 3600, 7 * 3600), "2026-10-08 07:45:10");
        // A leap day, and the day before it.
        assert_eq!(stamp(1_709_164_800, 0), "2024-02-29 00:00:00");
        assert_eq!(stamp(1_709_164_799, 0), "2024-02-28 23:59:59");
        // West of UTC reaches back across midnight.
        assert_eq!(stamp(3_600, -2 * 3600), "1969-12-31 23:00:00");
    }

    #[test]
    fn this_machines_offset_is_a_real_one() {
        let o = local_offset_secs();
        assert!((-12 * 3600..=14 * 3600).contains(&o), "{o}");
        assert_eq!(o % 900, 0, "zones move in quarter hours: {o}");
    }

    #[test]
    fn csv_lines_survive_quotes_commas_and_newlines() {
        assert_eq!(csv_line(&["a", "b,c", "say \"hi\"", "two\nlines"]), "\"a\",\"b,c\",\"say \"\"hi\"\"\",\"two lines\"");
    }

    #[test]
    fn the_file_starts_with_a_bom_and_a_header_then_grows_by_one_line_per_result() {
        let dir = std::env::temp_dir().join(format!("hir-rowlog-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let list = dir.join("danh sach.xlsx").to_string_lossy().into_owned();
        let _ = std::fs::remove_file(results_path(&list));
        append(&list, &["2026-10-07 20:00:00", "p1", "https://x/1", "hay quá", "OK", ""]).unwrap();
        append(&list, &["2026-10-07 20:01:00", "p2", "https://x/2", "", "LỖI", "step 3 — no post"]).unwrap();
        let bytes = std::fs::read(results_path(&list)).unwrap();
        assert!(bytes.starts_with(b"\xEF\xBB\xBF"), "BOM so Excel reads UTF-8");
        let text = String::from_utf8(bytes[3..].to_vec()).unwrap();
        let lines: Vec<&str> = text.lines().collect();
        assert_eq!(lines.len(), 3, "{text}");
        assert!(lines[0].contains("Kết quả"));
        assert!(lines[1].contains("hay quá") && lines[1].contains("\"OK\""));
        assert!(lines[2].contains("LỖI") && lines[2].contains("step 3"));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
