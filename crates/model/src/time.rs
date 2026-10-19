//! UTC timestamps without a date library.

use std::time::{SystemTime, UNIX_EPOCH};

pub fn iso_now() -> String {
    let secs = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| i64::try_from(d.as_secs()).unwrap_or(0)).unwrap_or(0);
    iso_from_secs(secs)
}

pub fn iso_from_secs(secs: i64) -> String {
    let days = secs.div_euclid(86_400);
    let rem = secs.rem_euclid(86_400);
    let (y, m, d) = civil_from_days(days);
    format!("{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}Z", rem / 3600, rem % 3600 / 60, rem % 60)
}

/// Days since 1970-01-01 to (year, month, day). Howard Hinnant's algorithm.
fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = u32::try_from(doy - (153 * mp + 2) / 5 + 1).unwrap_or(1);
    let m = u32::try_from(if mp < 10 { mp + 3 } else { mp - 9 }).unwrap_or(1);
    (if m <= 2 { y + 1 } else { y }, m, d)
}

const WEEKDAYS: [&str; 7] = ["Thursday", "Friday", "Saturday", "Sunday", "Monday", "Tuesday", "Wednesday"];
const MONTHS: [&str; 12] = ["January", "February", "March", "April", "May", "June", "July", "August", "September", "October", "November", "December"];

/// The day alone, as a note writes it: "Saturday, 17 October 2026".
pub fn date_from_iso(iso: &str) -> Option<String> {
    let whole = long_from_iso(iso)?;
    Some(whole.rsplit_once(", ")?.0.to_string())
}

/// The clock alone: "18:29".
pub fn clock_from_iso(iso: &str) -> Option<String> {
    Some(long_from_iso(iso)?.rsplit_once(", ")?.1.to_string())
}

/// A timestamp the way a note shows it, under the page title: "Saturday, 17 October 2026,
/// 18:29". English only for now, and the clock is UTC, like the stored timestamp.
pub fn long_from_iso(iso: &str) -> Option<String> {
    let (date, time) = iso.split_once('T')?;
    let mut parts = date.split('-');
    let y: i64 = parts.next()?.parse().ok()?;
    let m: u32 = parts.next()?.parse().ok()?;
    let d: u32 = parts.next()?.parse().ok()?;
    if !(1..=12).contains(&m) || !(1..=31).contains(&d) {
        return None;
    }
    let hm = time.get(..5).filter(|s| {
        let b = s.as_bytes();
        b.get(2) == Some(&b':') && b.iter().enumerate().all(|(i, c)| i == 2 || c.is_ascii_digit())
    })?;
    // 1970-01-01 was a Thursday, so the weekday follows from the day count.
    let days = days_from_civil(y, m, d);
    let weekday = WEEKDAYS.get(days.rem_euclid(7) as usize)?;
    let month = MONTHS.get((m - 1) as usize)?;
    Some(format!("{weekday}, {d} {month} {y}, {hm}"))
}

/// The inverse of [`civil_from_days`].
fn days_from_civil(y: i64, m: u32, d: u32) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let mp = i64::from(if m > 2 { m - 3 } else { m + 9 });
    let doy = (153 * mp + 2) / 5 + i64::from(d) - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn long_dates_read_like_a_note() {
        assert_eq!(long_from_iso("2026-10-17T18:29:00Z").as_deref(), Some("Saturday, 17 October 2026, 18:29"));
        assert_eq!(long_from_iso("1970-01-01T00:00:00Z").as_deref(), Some("Thursday, 1 January 1970, 00:00"));
        assert_eq!(long_from_iso("2024-02-29T23:59:59Z").as_deref(), Some("Thursday, 29 February 2024, 23:59"));
        assert_eq!(long_from_iso("2000-12-25T09:05:00Z").as_deref(), Some("Monday, 25 December 2000, 09:05"));
        // Junk is None, never a panic.
        for bad in ["", "2026-10-17", "x", "2026-13-01T00:00:00Z", "2026-10-99T00:00:00Z", "2026-10-17Txx:yy", "----T::"] {
            assert_eq!(long_from_iso(bad), None, "{bad}");
        }
    }

    #[test]
    fn the_day_alone_and_the_clock_alone_are_the_two_halves_of_it() {
        let iso = "2026-10-17T18:29:00Z";
        assert_eq!(date_from_iso(iso).as_deref(), Some("Saturday, 17 October 2026"));
        assert_eq!(clock_from_iso(iso).as_deref(), Some("18:29"));
        // The two halves put back together are the whole, which is what makes them halves.
        let (day, clock) = (date_from_iso(iso).expect("day"), clock_from_iso(iso).expect("clock"));
        assert_eq!(format!("{day}, {clock}"), long_from_iso(iso).expect("whole"));
        for bad in ["", "2026-10-17", "x", "2026-13-01T00:00:00Z"] {
            assert_eq!(date_from_iso(bad), None, "{bad}");
            assert_eq!(clock_from_iso(bad), None, "{bad}");
        }
    }

    #[test]
    fn the_day_count_round_trips() {
        for secs in [0_i64, 1_000_000, 1_700_000_000, -86_400 * 365] {
            let (y, m, d) = civil_from_days(secs.div_euclid(86_400));
            assert_eq!(days_from_civil(y, m, d), secs.div_euclid(86_400));
        }
    }

    #[test]
    fn dates() {
        assert_eq!(iso_from_secs(0), "1970-01-01T00:00:00Z");
        assert_eq!(iso_from_secs(19_723 * 86_400), "2024-01-01T00:00:00Z");
        assert_eq!(iso_from_secs(-1), "1969-12-31T23:59:59Z");
    }

    #[test]
    fn now_looks_like_iso() {
        let n = iso_now();
        assert_eq!(n.len(), 20);
        assert!(n.ends_with('Z'));
    }
}
