//! Jalali (Solar Hijri) calendar.
//!
//! Dates inside the app are Julian Day Numbers: one integer that sorts, compares
//! and subtracts correctly with no timezone attached. Jalali is a presentation
//! of that number, produced here.
//!
//! The conversion is the standard 33-year break table used by every tested
//! Jalali library (jalaali-js, jdatetime, moment-jalaali). Division here is
//! *truncating*, as in those implementations, not Euclidean: the algorithm
//! relies on `-7 / 6 == -1`.

use chrono::{Local, NaiveDate};

pub const MONTHS: [&str; 12] = [
    "فروردین",
    "اردیبهشت",
    "خرداد",
    "تیر",
    "مرداد",
    "شهریور",
    "مهر",
    "آبان",
    "آذر",
    "دی",
    "بهمن",
    "اسفند",
];

/// Saturday-first, the way a Persian calendar week is printed.
pub const WEEKDAYS: [&str; 7] = [
    "شنبه",
    "یکشنبه",
    "دوشنبه",
    "سه‌شنبه",
    "چهارشنبه",
    "پنجشنبه",
    "جمعه",
];

const BREAKS: [i64; 20] = [
    -61, 9, 38, 199, 426, 686, 756, 818, 1111, 1181, 1210, 1635, 2060, 2097, 2192, 2262, 2324, 2394,
    2456, 3178,
];

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct JDate {
    pub year: i64,
    pub month: i64,
    pub day: i64,
}

impl JDate {
    pub fn new(year: i64, month: i64, day: i64) -> Self {
        Self { year, month, day }
    }

    pub fn month_name(&self) -> &'static str {
        MONTHS[(self.month - 1).clamp(0, 11) as usize]
    }

    /// `۸ آبان ۱۴۰۵` — returned with ASCII digits; the Persian glyphs come from
    /// the font, which is the one place digit style is decided.
    pub fn long(&self) -> String {
        format!("{} {} {}", crate::money::fa_digits(&self.day.to_string()), self.month_name(), crate::money::fa_digits(&self.year.to_string()))
    }

    /// `۱۴۰۵/۰۸/۰۸`
    pub fn short(&self) -> String {
        crate::money::fa_digits(&format!("{:04}/{:02}/{:02}", self.year, self.month, self.day))
    }

    pub fn jdn(&self) -> i64 {
        j2d(self.year, self.month, self.day)
    }
}

fn div(a: i64, b: i64) -> i64 {
    a / b
}

fn modulo(a: i64, b: i64) -> i64 {
    a % b
}

/// (leap, gregorian year, day-of-march of 1 Farvardin)
fn jal_cal(jy: i64, without_leap: bool) -> Result<(i64, i64, i64), String> {
    let bl = BREAKS.len();
    let gy = jy + 621;
    let mut leap_j: i64 = -14;
    let mut jp = BREAKS[0];
    if jy < jp || jy >= BREAKS[bl - 1] {
        return Err(format!("سال نامعتبر: {jy}"));
    }
    let mut jump = 0i64;
    for i in 1..bl {
        let jm = BREAKS[i];
        jump = jm - jp;
        if jy < jm {
            break;
        }
        leap_j += div(jump, 33) * 8 + div(modulo(jump, 33), 4);
        jp = jm;
    }
    let mut n = jy - jp;
    leap_j += div(n, 33) * 8 + div(modulo(n, 33) + 3, 4);
    if modulo(jump, 33) == 4 && jump - n == 4 {
        leap_j += 1;
    }
    let leap_g = div(gy, 4) - div((div(gy, 100) + 1) * 3, 4) - 150;
    let march = 20 + leap_j - leap_g;

    let mut leap: i64 = 0;
    if !without_leap {
        if jump - n < 6 {
            n = n - jump + div(jump + 4, 33) * 33;
        }
        leap = modulo(modulo(n + 1, 33) - 1, 4);
        if leap == -1 {
            leap = 4;
        }
    }
    Ok((leap, gy, march))
}

/// Gregorian -> Julian Day Number.
pub fn g2d(gy: i64, gm: i64, gd: i64) -> i64 {
    let mut d = div((gy + div(gm - 8, 6) + 100100) * 1461, 4)
        + div(153 * modulo(gm + 9, 12) + 2, 5)
        + gd
        - 34840408;
    d = d - div(div(gy + 100100 + div(gm - 8, 6), 100) * 3, 4) + 752;
    d
}

/// Julian Day Number -> Gregorian (year, month, day).
pub fn d2g(jdn: i64) -> (i64, i64, i64) {
    let mut j = 4 * jdn + 139361631;
    j = j + div(div(4 * jdn + 183187720, 146097) * 3, 4) * 4 - 3908;
    let i = div(modulo(j, 1461), 4) * 5 + 308;
    let gd = div(modulo(i, 153), 5) + 1;
    let gm = modulo(div(i, 153), 12) + 1;
    let gy = div(j, 1461) - 100100 + div(8 - gm, 6);
    (gy, gm, gd)
}

pub fn j2d(jy: i64, jm: i64, jd: i64) -> i64 {
    let Ok((_, gy, march)) = jal_cal(jy, true) else {
        return 0;
    };
    g2d(gy, 3, march) + (jm - 1) * 31 - div(jm, 7) * (jm - 7) + jd - 1
}

pub fn d2j(jdn: i64) -> JDate {
    let (gy, _, _) = d2g(jdn);
    let mut jy = gy - 621;
    let Ok((leap, _, march)) = jal_cal(jy, false) else {
        return JDate::new(jy, 1, 1);
    };
    let jdn1f = g2d(gy, 3, march);
    let mut k = jdn - jdn1f;
    let jm;
    let jd;
    if k >= 0 {
        if k <= 185 {
            jm = 1 + div(k, 31);
            jd = modulo(k, 31) + 1;
            return JDate::new(jy, jm, jd);
        } else {
            k -= 186;
        }
    } else {
        jy -= 1;
        k += 179;
        if leap == 1 {
            k += 1;
        }
    }
    jm = 7 + div(k, 30);
    jd = modulo(k, 30) + 1;
    JDate::new(jy, jm, jd)
}

/// 0 = شنبه … 6 = جمعه
pub fn weekday(jdn: i64) -> usize {
    (((jdn % 7) + 2) % 7) as usize
}

pub fn weekday_name(jdn: i64) -> &'static str {
    WEEKDAYS[weekday(jdn)]
}

pub fn is_leap(jy: i64) -> bool {
    jal_cal(jy, false).map(|(l, _, _)| l == 0).unwrap_or(false)
}

pub fn month_len(jy: i64, jm: i64) -> i64 {
    if jm <= 6 {
        31
    } else if jm <= 11 {
        30
    } else if is_leap(jy) {
        30
    } else {
        29
    }
}

/// Today, in the shop's own timezone.
pub fn today() -> JDate {
    let now = Local::now().date_naive();
    let jdn = g2d(
        chrono::Datelike::year(&now) as i64,
        chrono::Datelike::month(&now) as i64,
        chrono::Datelike::day(&now) as i64,
    );
    d2j(jdn)
}

pub fn gregorian(jdn: i64) -> NaiveDate {
    let (y, m, d) = d2g(jdn);
    NaiveDate::from_ymd_opt(y as i32, m as u32, d as u32).unwrap_or_else(|| {
        NaiveDate::from_ymd_opt(2026, 1, 1).expect("epoch fallback")
    })
}

/// `۱۴۰۵/۰۸/۰۸` for a stored day number.
pub fn short(jdn: i64) -> String {
    d2j(jdn).short()
}

pub fn long(jdn: i64) -> String {
    d2j(jdn).long()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Anchors that are unambiguous and widely published.
    #[test]
    fn known_dates() {
        // 22 بهمن ۱۳۵۷ = 11 February 1979
        assert_eq!(j2d(1357, 11, 22), g2d(1979, 2, 11));
        assert_eq!(d2j(g2d(1979, 2, 11)), JDate::new(1357, 11, 22));

        // 1 فروردین ۱۴۰۰ = 21 March 2021
        assert_eq!(j2d(1400, 1, 1), g2d(2021, 3, 21));

        // 13 آبان ۱۳۹۸ = 4 November 2019
        assert_eq!(j2d(1398, 8, 13), g2d(2019, 11, 4));
    }

    #[test]
    fn round_trips_every_day_for_six_years() {
        let start = g2d(2021, 1, 1);
        let end = g2d(2027, 1, 1);
        for jdn in start..end {
            let j = d2j(jdn);
            assert!(
                (1..=12).contains(&j.month) && (1..=31).contains(&j.day),
                "bad jalali {j:?} at jdn {jdn}"
            );
            assert_eq!(j2d(j.year, j.month, j.day), jdn, "round trip {j:?}");
        }
    }

    #[test]
    fn month_lengths_and_leap_years() {
        assert_eq!(month_len(1403, 12), 30); // 1403 is leap
        assert_eq!(month_len(1404, 12), 29);
        assert_eq!(month_len(1404, 1), 31);
        assert_eq!(month_len(1404, 7), 30);
        assert!(is_leap(1403));
        assert!(!is_leap(1404));
    }

    #[test]
    fn weekday_anchor() {
        // 2000-01-01 was a Saturday.
        assert_eq!(weekday_name(g2d(2000, 1, 1)), "شنبه");
        // 2026-10-30 is a Friday.
        assert_eq!(weekday_name(g2d(2026, 10, 30)), "جمعه");
    }
}
