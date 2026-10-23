use std::fmt;
use std::time::{SystemTime, UNIX_EPOCH};

pub const MONTHS: [&str; 12] = [
    "January",
    "February",
    "March",
    "April",
    "May",
    "June",
    "July",
    "August",
    "September",
    "October",
    "November",
    "December",
];
pub const WEEKDAYS: [&str; 7] = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Date {
    pub year: i32,
    pub month: u8,
    pub day: u8,
}
impl Date {
    pub fn new(year: i32, month: u8, day: u8) -> Option<Self> {
        (year >= 1
            && year <= 9999
            && (1..=12).contains(&month)
            && day >= 1
            && day <= Self::days_in_month(year, month))
        .then_some(Self { year, month, day })
    }
    pub fn parse(text: &str) -> Option<Self> {
        let b = text.as_bytes();
        if b.len() != 10
            || b[4] != b'-'
            || b[7] != b'-'
            || !b
                .iter()
                .enumerate()
                .all(|(i, b)| i == 4 || i == 7 || b.is_ascii_digit())
        {
            return None;
        }
        Self::new(
            text[..4].parse().ok()?,
            text[5..7].parse().ok()?,
            text[8..].parse().ok()?,
        )
    }
    pub const fn days_in_month(year: i32, month: u8) -> u8 {
        match month {
            2 if year % 4 == 0 && (year % 100 != 0 || year % 400 == 0) => 29,
            2 => 28,
            4 | 6 | 9 | 11 => 30,
            1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
            _ => 0,
        }
    }
    pub fn month_title(self) -> String {
        format!("{} {}", MONTHS[self.month as usize - 1], self.year)
    }
    pub fn shift_month(self, delta: i32) -> Option<Self> {
        let month = self
            .year
            .checked_mul(12)?
            .checked_add(self.month as i32 - 1)?
            .checked_add(delta)?;
        let year = month.div_euclid(12);
        let month = (month.rem_euclid(12) + 1) as u8;
        Self::new(year, month, self.day.min(Self::days_in_month(year, month)))
    }
    pub fn days(self) -> i64 {
        let year = i64::from(self.year) - i64::from(self.month <= 2);
        let era = year.div_euclid(400);
        let y = year - era * 400;
        let month = i64::from(self.month) + if self.month > 2 { -3 } else { 9 };
        era * 146097 + y * 365 + y / 4 - y / 100 + (153 * month + 2) / 5 + i64::from(self.day)
            - 1
            - 719468
    }
    pub fn from_days(days: i64) -> Option<Self> {
        let days = days.checked_add(719468)?;
        let era = days.div_euclid(146097);
        let d = days - era * 146097;
        let y = (d - d / 1460 + d / 36524 - d / 146096) / 365;
        let mut year = y + era * 400;
        let d = d - (365 * y + y / 4 - y / 100);
        let m = (5 * d + 2) / 153;
        let day = d - (153 * m + 2) / 5 + 1;
        let month = m + if m < 10 { 3 } else { -9 };
        year += i64::from(month <= 2);
        Self::new(year.try_into().ok()?, month as u8, day as u8)
    }
    pub fn weekday(self) -> usize {
        (self.days() + 4).rem_euclid(7) as usize
    }
    pub fn add_days(self, delta: i64) -> Option<Self> {
        Self::from_days(self.days().checked_add(delta)?)
    }
    pub fn month_grid(self) -> [Option<Self>; 42] {
        let first = Self { day: 1, ..self };
        let start = first.days() - first.weekday() as i64;
        std::array::from_fn(|i| Self::from_days(start + i as i64))
    }
    pub fn today() -> Option<Self> {
        let seconds = SystemTime::now().duration_since(UNIX_EPOCH).ok()?.as_secs();
        #[cfg(target_os = "linux")]
        {
            let stamp: libc::time_t = seconds.try_into().ok()?;
            let mut local = std::mem::MaybeUninit::<libc::tm>::uninit();
            // localtime_r initializes the supplied tm on success; no shared C buffer is used.
            if !unsafe { libc::localtime_r(&stamp, local.as_mut_ptr()) }.is_null() {
                let local = unsafe { local.assume_init() };
                return Self::new(
                    local.tm_year + 1900,
                    (local.tm_mon + 1) as u8,
                    local.tm_mday as u8,
                );
            }
        }
        // Match Binder's current native clock, which uses UTC civil dates.
        Self::from_days((seconds / 86400).try_into().ok()?)
    }
}
impl fmt::Display for Date {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:04}-{:02}-{:02}", self.year, self.month, self.day)
    }
}
pub fn parse_time(text: &str) -> Option<u16> {
    let b = text.as_bytes();
    if b.len() != 5 || b[2] != b':' || ![b[0], b[1], b[3], b[4]].iter().all(u8::is_ascii_digit) {
        return None;
    }
    let hour: u16 = text[..2].parse().ok()?;
    let minute: u16 = text[3..].parse().ok()?;
    (hour < 24 && minute < 60).then_some(hour * 60 + minute)
}
pub fn time_label(minutes: u16) -> String {
    format!("{:02}:{:02}", minutes / 60, minutes % 60)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn leap_centuries_and_invalid_input() {
        assert!(Date::parse("2000-02-29").is_some());
        for s in [
            "1900-02-29",
            "2026-02-29",
            "0000-01-01",
            "2026-13-01",
            "2026-04-31",
            "２０２６-01-01",
            "2026-1-01",
        ] {
            assert!(Date::parse(s).is_none(), "{s}");
        }
    }
    #[test]
    fn civil_dates_round_trip_across_the_full_supported_range() {
        for year in [1, 4, 100, 400, 1582, 1900, 1970, 2000, 2026, 9999] {
            for month in 1..=12 {
                for day in 1..=Date::days_in_month(year, month) {
                    let date = Date::new(year, month, day).unwrap();
                    assert_eq!(Date::from_days(date.days()), Some(date));
                }
            }
        }
        assert_eq!(Date::parse("1970-01-01").unwrap().weekday(), 4);
    }
    #[test]
    fn grids_and_navigation_cover_month_boundaries() {
        let date = Date::parse("2024-02-29").unwrap();
        let grid = date.month_grid();
        assert_eq!(grid[0].unwrap().to_string(), "2024-01-28");
        assert_eq!(grid.iter().flatten().filter(|d| d.month == 2).count(), 29);
        assert_eq!(
            Date::parse("2026-01-31")
                .unwrap()
                .shift_month(1)
                .unwrap()
                .to_string(),
            "2026-02-28"
        );
        assert!(Date::parse("0001-01-01").unwrap().shift_month(-1).is_none());
        assert!(
            Date::parse("9999-12-01")
                .unwrap()
                .month_grid()
                .iter()
                .any(Option::is_none)
        );
    }
    #[test]
    fn times_are_unambiguous_and_bounded() {
        for (s, n) in [("00:00", 0), ("09:05", 545), ("23:59", 1439)] {
            assert_eq!(parse_time(s), Some(n));
            assert_eq!(time_label(n), s);
        }
        for s in ["24:00", "12:60", "9:00", "aa:bb", "１２:００"] {
            assert_eq!(parse_time(s), None);
        }
    }
}
