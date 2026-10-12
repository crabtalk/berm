//! Times as feeds and pages spell them, in Unix milliseconds.

/// Midnight UTC starting `day` of `month` (1-based) of `year`. `None` before
/// 1970 or for a month or day out of range.
pub fn date(year: i64, month: i64, day: i64) -> Option<u64> {
    if !(1..=12).contains(&month) || !(1..=31).contains(&day) {
        return None;
    }
    // Days from 1970-01-01 to the civil date, after Howard Hinnant's
    // `days_from_civil`.
    let y = if month <= 2 { year - 1 } else { year };
    let era = y.div_euclid(400);
    let year_of_era = y - era * 400;
    let day_of_year = (153 * (month + if month > 2 { -3 } else { 9 }) + 2) / 5 + day - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    let days = era * 146_097 + day_of_era - 719_468;
    u64::try_from(days * 86_400_000).ok()
}

/// An RFC 3339 time, `2026-10-11T06:55:00+00:00`. Fractional seconds are
/// dropped; a time with no zone is UTC; a date alone is its midnight UTC.
pub fn rfc3339(text: &str) -> Option<u64> {
    let text = text.trim();
    let digits = |range: core::ops::Range<usize>| text.get(range)?.parse::<i64>().ok();
    let day = date(digits(0..4)?, digits(5..7)?, digits(8..10)?)?;
    if text.len() == 10 {
        return Some(day);
    }
    let (hour, minute, second) = (digits(11..13)?, digits(14..16)?, digits(17..19)?);
    let zone = text
        .get(19..)?
        .trim_start_matches(|c: char| c == '.' || c.is_ascii_digit());
    let offset = match zone {
        "" | "Z" | "z" => 0,
        _ => offset(zone)?,
    };
    clock(day, hour, minute, second, offset)
}

/// An RFC 822 time as RSS spells it, `Fri, 09 Oct 2026 23:27:00 +0000`. The
/// weekday and seconds are optional, a two-digit year is 19xx from 50 and 20xx
/// below, and the zone is an offset or `GMT`, `UT`, `UTC`, `Z` or a North
/// American zone name.
pub fn rfc822(text: &str) -> Option<u64> {
    let text = text.trim();
    let text = text.split_once(',').map_or(text, |(_, rest)| rest);
    let mut parts = text.split_ascii_whitespace();
    let day = parts.next()?.parse().ok()?;
    let month = month(parts.next()?)?;
    let year: i64 = parts.next()?.parse().ok()?;
    let year = match year {
        0..50 => year + 2000,
        50..100 => year + 1900,
        _ => year,
    };
    let mut clock_parts = parts.next()?.split(':');
    let hour = clock_parts.next()?.parse().ok()?;
    let minute = clock_parts.next()?.parse().ok()?;
    let second = clock_parts.next().map_or(Some(0), |s| s.parse().ok())?;
    let offset = match parts.next()? {
        "GMT" | "UT" | "UTC" | "Z" => 0,
        "EDT" => -4 * 3600,
        "EST" | "CDT" => -5 * 3600,
        "CST" | "MDT" => -6 * 3600,
        "MST" | "PDT" => -7 * 3600,
        "PST" => -8 * 3600,
        zone => offset(zone)?,
    };
    clock(date(year, month, day)?, hour, minute, second, offset)
}

/// The month named by its first three letters, any case, 1-based.
pub fn month(name: &str) -> Option<i64> {
    const MONTHS: [&str; 12] = [
        "jan", "feb", "mar", "apr", "may", "jun", "jul", "aug", "sep", "oct", "nov", "dec",
    ];
    let name = name.get(..3)?;
    let at = MONTHS.iter().position(|m| m.eq_ignore_ascii_case(name))?;
    Some(at as i64 + 1)
}

/// `+hhmm` or `+hh:mm`, in seconds east of UTC.
fn offset(zone: &str) -> Option<i64> {
    let sign = match zone.as_bytes().first()? {
        b'+' => 1,
        b'-' => -1,
        _ => return None,
    };
    let hours: i64 = zone.get(1..3)?.parse().ok()?;
    let minutes: i64 = zone.get(3..)?.trim_start_matches(':').parse().ok()?;
    Some(sign * (hours * 3600 + minutes * 60))
}

/// `day`, in Unix milliseconds, at the given wall clock `offset` seconds east
/// of UTC.
fn clock(day: u64, hour: i64, minute: i64, second: i64, offset: i64) -> Option<u64> {
    let seconds = hour * 3600 + minute * 60 + second - offset;
    u64::try_from(day as i64 + seconds * 1000).ok()
}
