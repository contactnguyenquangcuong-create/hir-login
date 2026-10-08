//! This machine's wall-clock time as text, without a date-time library: the logs and result
//! files people read should say 22:49, not a Unix number and not UTC.

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

/// The time of `unix`, in this machine's own time.
pub fn local_stamp(unix: u64) -> String {
    stamp(unix, local_offset_secs())
}

/// Now, in this machine's own time.
pub fn now_local() -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    local_stamp(now)
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
        assert_eq!(now_local().len(), 19);
    }
}
