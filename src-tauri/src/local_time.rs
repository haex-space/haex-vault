//! Local date and time as the device sees it: a civil date with wall-clock time and the
//! window's offset from UTC. The voice assistant speaks dates relative to it (spec 066 research
//! R10); the chat tells the model what day and time it is.

use std::time::{SystemTime, UNIX_EPOCH};

/// A local calendar date and wall-clock time.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Civil {
    pub year: i32,
    pub month: u32,
    pub day: u32,
    pub hour: u32,
    pub minute: u32,
}

impl Civil {
    pub fn new(year: i32, month: u32, day: u32, hour: u32, minute: u32) -> Self {
        Self {
            year,
            month,
            day,
            hour,
            minute,
        }
    }

    /// Days since 1970-01-01 of the date (H. Hinnant's `days_from_civil`).
    pub(crate) fn days(&self) -> i64 {
        let y = i64::from(self.year) - i64::from(self.month <= 2);
        let era = y.div_euclid(400);
        let yoe = y - era * 400;
        let m = i64::from(self.month);
        let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + i64::from(self.day) - 1;
        let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
        era * 146_097 + doe - 719_468
    }

    pub(crate) fn from_days(days: i64, hour: u32, minute: u32) -> Self {
        let z = days + 719_468;
        let era = z.div_euclid(146_097);
        let doe = z - era * 146_097;
        let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
        let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
        let mp = (5 * doy + 2) / 153;
        let day = (doy - (153 * mp + 2) / 5 + 1) as u32;
        let month = (if mp < 10 { mp + 3 } else { mp - 9 }) as u32;
        let year = (yoe + era * 400 + i64::from(month <= 2)) as i32;
        Self::new(year, month, day, hour, minute)
    }

    #[cfg_attr(not(feature = "voice"), allow(dead_code))] // only the spoken dates use it
    pub(crate) fn minutes(&self) -> i64 {
        self.days() * 1440 + i64::from(self.hour) * 60 + i64::from(self.minute)
    }

    pub(crate) fn from_minutes(minutes: i64) -> Self {
        let days = minutes.div_euclid(1440);
        let rest = minutes.rem_euclid(1440);
        Self::from_days(days, (rest / 60) as u32, (rest % 60) as u32)
    }

    /// Monday = 0.
    pub(crate) fn weekday(&self) -> usize {
        (self.days() + 3).rem_euclid(7) as usize
    }

    #[cfg_attr(not(feature = "voice"), allow(dead_code))] // only the spoken dates use it
    pub(crate) fn valid(&self) -> bool {
        (1..=12).contains(&self.month)
            && self.day >= 1
            && self.day <= 31
            && self.hour < 24
            && self.minute < 60
            && Self::from_days(self.days(), self.hour, self.minute) == *self
    }
}

/// "Now" as the device sees it: local time and the offset of local time from UTC, so a time with
/// an offset ("…Z", "…+02:00") is spoken in local time.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Clock {
    pub now: Civil,
    pub utc_offset_minutes: i32,
}

impl Clock {
    /// The clock at `unix_secs` on a device whose local time is `utc_offset_minutes` ahead of
    /// UTC, as the window reports it.
    pub fn at(unix_secs: i64, utc_offset_minutes: i32) -> Self {
        Self {
            now: Civil::from_minutes(unix_secs.div_euclid(60) + i64::from(utc_offset_minutes)),
            utc_offset_minutes,
        }
    }

    /// The clock now, `utc_offset_minutes` ahead of UTC; an offset beyond ±14 hours is cut to it.
    pub fn now(utc_offset_minutes: i32) -> Self {
        let unix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |d| d.as_secs() as i64);
        Self::at(unix, utc_offset_minutes.clamp(-14 * 60, 14 * 60))
    }
}
