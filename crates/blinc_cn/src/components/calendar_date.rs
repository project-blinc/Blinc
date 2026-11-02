//! Civil dates for the calendar: day arithmetic across months and years, the
//! weekday, and today's local date.
//!
//! Dates count days from 1970-01-01 in the proleptic Gregorian calendar, so
//! arithmetic never passes through a clock time and daylight saving cannot
//! shift it.

/// A calendar date: a year, a month from 1 (January) to 12, and a day of the
/// month from 1.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct CalendarDay {
    pub year: i32,
    pub month: u32,
    pub day: u32,
}

impl CalendarDay {
    /// A date; the day is clamped into the month.
    pub fn new(year: i32, month: u32, day: u32) -> Self {
        let month = month.clamp(1, 12);
        Self {
            year,
            month,
            day: day.clamp(1, days_in_month(year, month)),
        }
    }

    /// Today in the local time zone.
    pub fn today() -> Self {
        let (year, month, day) = local_today();
        Self::new(year, month, day)
    }

    /// The date `n` days on (or back, for a negative `n`).
    pub fn add_days(self, n: i64) -> Self {
        from_days(to_days(self) + n)
    }

    /// The same day `n` months on, clamped to the end of a shorter month.
    pub fn add_months(self, n: i32) -> Self {
        let m = Month::of(self).shift(n);
        Self::new(m.year, m.month, self.day)
    }

    /// The day of the week, 0 for Sunday to 6 for Saturday.
    pub fn weekday(self) -> u32 {
        (to_days(self) + 4).rem_euclid(7) as u32
    }
}

/// A month of a year; `month` is 1 to 12.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) struct Month {
    pub year: i32,
    pub month: u32,
}

impl Month {
    pub fn of(day: CalendarDay) -> Self {
        Self {
            year: day.year,
            month: day.month,
        }
    }

    /// The month `n` months on.
    pub fn shift(self, n: i32) -> Self {
        let total = self.year * 12 + self.month as i32 - 1 + n;
        Self {
            year: total.div_euclid(12),
            month: total.rem_euclid(12) as u32 + 1,
        }
    }

    pub fn first(self) -> CalendarDay {
        CalendarDay {
            year: self.year,
            month: self.month,
            day: 1,
        }
    }

    /// The 42 days a six-week grid shows for this month, the first row
    /// starting on `week_start` (0 Sunday, 1 Monday).
    pub fn grid(self, week_start: u32) -> impl Iterator<Item = CalendarDay> {
        let first = self.first();
        let lead = (first.weekday() + 7 - week_start) % 7;
        (0..42).map(move |i| first.add_days(i - lead as i64))
    }
}

pub(crate) fn days_in_month(year: i32, month: u32) -> u32 {
    match month {
        2 if year % 4 == 0 && (year % 100 != 0 || year % 400 == 0) => 29,
        2 => 28,
        4 | 6 | 9 | 11 => 30,
        _ => 31,
    }
}

/// Days since 1970-01-01.
fn to_days(d: CalendarDay) -> i64 {
    let y = d.year as i64 - (d.month <= 2) as i64;
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let m = d.month as i64;
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + d.day as i64 - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

fn from_days(z: i64) -> CalendarDay {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let month = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    let year = (yoe + era * 400 + (month <= 2) as i64) as i32;
    CalendarDay { year, month, day }
}

#[cfg(all(unix, not(target_arch = "wasm32")))]
fn local_today() -> (i32, u32, u32) {
    // SAFETY: `time(NULL)` only reads the clock, and `localtime_r` writes
    // into the `tm` it is given, which lives for the call.
    unsafe {
        let now = libc::time(std::ptr::null_mut());
        let mut tm: libc::tm = std::mem::zeroed();
        if libc::localtime_r(&now, &mut tm).is_null() {
            return utc_today();
        }
        (tm.tm_year + 1900, tm.tm_mon as u32 + 1, tm.tm_mday as u32)
    }
}

#[cfg(windows)]
fn local_today() -> (i32, u32, u32) {
    use windows_sys::Win32::Foundation::SYSTEMTIME;
    // SAFETY: `GetLocalTime` writes the whole struct it is handed.
    let t: SYSTEMTIME = unsafe {
        let mut t = std::mem::zeroed();
        windows_sys::Win32::System::SystemInformation::GetLocalTime(&mut t);
        t
    };
    (t.wYear as i32, t.wMonth as u32, t.wDay as u32)
}

#[cfg(target_arch = "wasm32")]
fn local_today() -> (i32, u32, u32) {
    let now = js_sys::Date::new_0();
    (
        now.get_full_year() as i32,
        now.get_month() + 1,
        now.get_date(),
    )
}

#[cfg(not(any(unix, windows, target_arch = "wasm32")))]
fn local_today() -> (i32, u32, u32) {
    utc_today()
}

#[allow(dead_code)]
fn utc_today() -> (i32, u32, u32) {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    let d = from_days(secs.div_euclid(86_400));
    (d.year, d.month, d.day)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn days_round_trip_across_eras() {
        for z in [-800_000, -719_468, -1, 0, 1, 11_016, 19_000, 2_932_896] {
            assert_eq!(to_days(from_days(z)), z);
        }
        assert_eq!(from_days(0), CalendarDay::new(1970, 1, 1));
        assert_eq!(to_days(CalendarDay::new(2000, 3, 1)), 11_017);
    }

    #[test]
    fn weekdays() {
        assert_eq!(CalendarDay::new(1970, 1, 1).weekday(), 4);
        assert_eq!(CalendarDay::new(2026, 11, 1).weekday(), 0);
        assert_eq!(CalendarDay::new(2024, 2, 29).weekday(), 4);
    }

    #[test]
    fn month_lengths_and_leap_years() {
        assert_eq!(days_in_month(2024, 2), 29);
        assert_eq!(days_in_month(1900, 2), 28);
        assert_eq!(days_in_month(2000, 2), 29);
        assert_eq!(days_in_month(2026, 4), 30);
    }

    #[test]
    fn adding_crosses_months_and_years() {
        let d = CalendarDay::new(2026, 12, 31);
        assert_eq!(d.add_days(1), CalendarDay::new(2027, 1, 1));
        assert_eq!(d.add_days(-365), CalendarDay::new(2025, 12, 31));
        assert_eq!(
            CalendarDay::new(2026, 1, 31).add_months(1),
            CalendarDay::new(2026, 2, 28)
        );
        assert_eq!(
            CalendarDay::new(2026, 1, 15).add_months(-1),
            CalendarDay::new(2025, 12, 15)
        );
    }

    #[test]
    fn a_grid_starts_on_the_week_start() {
        // November 2026 begins on a Sunday.
        let nov = Month {
            year: 2026,
            month: 11,
        };
        let sunday: Vec<_> = nov.grid(0).collect();
        assert_eq!(sunday[0], CalendarDay::new(2026, 11, 1));
        let monday: Vec<_> = nov.grid(1).collect();
        assert_eq!(monday[0], CalendarDay::new(2026, 10, 26));
        assert_eq!(monday.len(), 42);
    }
}
