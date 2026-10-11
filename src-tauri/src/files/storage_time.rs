//! The times storages report (spec 044 US5): a listing's ISO time and a `HEAD`'s HTTP date, both as
//! milliseconds since 1970.

/// Days since 1970-01-01 of a date of the proleptic Gregorian calendar (Howard Hinnant's
/// `days_from_civil`).
fn days_from_civil(year: i64, month: u32, day: u32) -> i64 {
    let year = if month <= 2 { year - 1 } else { year };
    let era = year.div_euclid(400);
    let yoe = year - era * 400;
    let month = i64::from(month);
    let doy = (153 * (if month > 2 { month - 3 } else { month + 9 }) + 2) / 5 + i64::from(day) - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

fn millis(year: i64, month: u32, day: u32, hour: u32, minute: u32, second: u32) -> Option<i64> {
    if !(1..=12).contains(&month)
        || !(1..=31).contains(&day)
        || hour > 23
        || minute > 59
        || second > 60
    {
        return None;
    }
    let seconds = days_from_civil(year, month, day) * 86_400
        + i64::from(hour) * 3_600
        + i64::from(minute) * 60
        + i64::from(second);
    Some(seconds * 1_000)
}

/// The time of a listing (`2026-10-06T10:00:00.000Z`) in milliseconds since 1970.
pub fn parse_iso_ms(text: &str) -> Option<i64> {
    let (date, time) = text.trim_end_matches('Z').split_once('T')?;
    let mut date = date.split('-');
    let year = date.next()?.parse().ok()?;
    let month = date.next()?.parse().ok()?;
    let day = date.next()?.parse().ok()?;
    let mut time = time.split(':');
    let hour = time.next()?.parse().ok()?;
    let minute = time.next()?.parse().ok()?;
    let second = time.next()?.split('.').next()?.parse().ok()?;
    millis(year, month, day, hour, minute, second)
}

/// The time of an HTTP date (`Tue, 06 Oct 2026 10:00:00 GMT`) in milliseconds since 1970.
pub fn parse_http_date_ms(text: &str) -> Option<i64> {
    const MONTHS: [&str; 12] = [
        "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ];
    let mut words = text.split_whitespace().skip(1);
    let day = words.next()?.parse().ok()?;
    let month_name = words.next()?;
    let month = MONTHS.iter().position(|m| *m == month_name)?;
    let year = words.next()?.parse().ok()?;
    let mut time = words.next()?.split(':');
    let hour = time.next()?.parse().ok()?;
    let minute = time.next()?.parse().ok()?;
    let second = time.next()?.parse().ok()?;
    millis(
        year,
        u32::try_from(month).ok()? + 1,
        day,
        hour,
        minute,
        second,
    )
}

#[cfg(test)]
#[path = "storage_time_tests.rs"]
mod tests;
