//! Civil-day arithmetic, independent of UTC offsets and daylight-saving changes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(super) struct Day(pub i32);

impl Day {
    pub(super) fn parse(value: &str) -> Option<Self> {
        if !value.is_ascii() || value.len() != 10 || &value[4..5] != "-" || &value[7..8] != "-" {
            return None;
        }
        if !value.bytes().enumerate().all(|(i, b)| {
            if i == 4 || i == 7 {
                b == b'-'
            } else {
                b.is_ascii_digit()
            }
        }) {
            return None;
        }
        let y = value[..4].parse::<i32>().ok()?;
        let m = value[5..7].parse::<u32>().ok()?;
        let d = value[8..].parse::<u32>().ok()?;
        if !(1..=9999).contains(&y) || !(1..=12).contains(&m) || !(1..=31).contains(&d) {
            return None;
        }
        let result = Self::ymd(y, m, d);
        (result.parts() == (y, m, d)).then_some(result)
    }
    fn ymd(year: i32, month: u32, day: u32) -> Self {
        let y = year - i32::from(month <= 2);
        let era = y.div_euclid(400);
        let yo = y - era * 400;
        let m = month as i32 + if month > 2 { -3 } else { 9 };
        let doy = (153 * m + 2) / 5 + day as i32 - 1;
        Self(era * 146097 + yo * 365 + yo / 4 - yo / 100 + doy - 719468)
    }
    pub(super) fn parts(self) -> (i32, u32, u32) {
        let z = self.0 + 719468;
        let era = z.div_euclid(146097);
        let doe = z - era * 146097;
        let yo = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
        let y = yo + era * 400;
        let doy = doe - (365 * yo + yo / 4 - yo / 100);
        let mp = (5 * doy + 2) / 153;
        let d = doy - (153 * mp + 2) / 5 + 1;
        let m = mp + if mp < 10 { 3 } else { -9 };
        (y + i32::from(m <= 2), m as u32, d as u32)
    }
    pub(super) fn iso(self) -> String {
        let (y, m, d) = self.parts();
        format!("{y:04}-{m:02}-{d:02}")
    }
    pub(super) fn month(self) -> Self {
        let (y, m, _) = self.parts();
        Self::ymd(y, m, 1)
    }
    pub(super) fn shift_month(self, delta: i32) -> Self {
        let (y, m, _) = self.parts();
        let total = y * 12 + m as i32 - 1 + delta;
        Self::ymd(total.div_euclid(12), (total.rem_euclid(12) + 1) as u32, 1)
    }
    pub(super) fn weekday(self) -> u32 {
        (self.0 + 4).rem_euclid(7) as u32
    }
    pub(super) fn short(self, months: &[String]) -> String {
        let (y, m, d) = self.parts();
        let suffix = if (11..=13).contains(&(d % 100)) {
            "th"
        } else {
            match d % 10 {
                1 => "st",
                2 => "nd",
                3 => "rd",
                _ => "th",
            }
        };
        format!("{} {d}{suffix} {:02}", months[m as usize - 1], y % 100)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn civil_days_cross_leap_year_and_dst_without_elapsed_seconds() {
        assert_eq!(
            Day::parse("2024-02-28").unwrap().0 + 2,
            Day::parse("2024-03-01").unwrap().0
        );
        assert!(Day::parse("2025-02-29").is_none());
        assert_eq!(
            Day::parse("2026-03-08").unwrap().0 + 1,
            Day::parse("2026-03-09").unwrap().0
        );
        assert_eq!(
            Day::parse("2026-12-31").unwrap().shift_month(1).iso(),
            "2027-01-01"
        );
    }
    #[test]
    fn exact_shape_and_centuries_are_not_lenient_integer_parsing() {
        for raw in [
            "+001-01-01",
            "2026-+1-01",
            "2026-01-+1",
            "2026-1-01",
            "2026-01-1",
            "2026-01-01 ",
            "1900-02-29",
            "2100-02-29",
            "0000-01-01",
            "2026-04-31",
            "中000-01-01",
        ] {
            assert!(Day::parse(raw).is_none(), "{raw}");
        }
        for raw in [
            "0099-01-01",
            "2000-02-29",
            "2400-02-29",
            "0001-01-01",
            "9999-12-31",
        ] {
            assert_eq!(Day::parse(raw).unwrap().iso(), raw);
        }
        assert_eq!(
            Day::parse("0099-12-31").unwrap().0 + 1,
            Day::parse("0100-01-01").unwrap().0
        );
    }
}
