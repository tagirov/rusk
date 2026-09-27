use anyhow::{Context, Result};
use chrono::{Datelike, Duration, Local, Months, NaiveDate};

pub fn is_cli_date_help_value(s: &str) -> bool {
    matches!(s.trim(), "-h" | "--help")
}

pub fn is_cli_date_clear_value(s: &str) -> bool {
    s.trim() == "_"
}

pub fn parse_cli_date(date_str: &str) -> Result<NaiveDate> {
    parse_cli_date_with_base(date_str, Local::now().date_naive())
}

/// Leading `+` means relative offsets apply from `task_date`, or today when it is `None`.
/// Without `+`, same as [`parse_cli_date`] (offsets from today).
pub fn parse_cli_date_for_edit(date_str: &str, task_date: Option<NaiveDate>) -> Result<NaiveDate> {
    let trimmed = date_str.trim();
    if trimmed.is_empty() {
        anyhow::bail!("Date cannot be empty");
    }
    if let Some(rest) = trimmed.strip_prefix('+') {
        let rest = rest.trim_start();
        if rest.is_empty() {
            anyhow::bail!("Date cannot be empty");
        }
        let base = task_date.unwrap_or_else(|| Local::now().date_naive());
        // The message quotes the value as typed, `+` and all.
        return within_years(parse_any_year(rest, base, trimmed)?, trimmed);
    }
    parse_cli_date(trimmed)
}

/// Syntax check for a `--date` value before bulk edit: `+` offsets are validated per segment,
/// absolute dates parse the same for any base.
pub fn validate_cli_date_edit_arg(s: &str) -> Result<()> {
    let t = s.trim();
    if is_cli_date_clear_value(t) {
        return Ok(());
    }
    // A `None` base makes `+` offsets count from today.
    parse_cli_date_for_edit(t, None).map(|_| ())
}

/// Absolute dates like `11-jan-25`, `1-Feb-2026` (day, English month, 2- or 4-digit year).
fn parse_english_abbrev_dash_date(s: &str) -> Option<NaiveDate> {
    let parts: Vec<&str> = s.split('-').collect();
    if parts.len() != 3 {
        return None;
    }
    let day: u32 = parts[0].parse().ok()?;
    let mtoken = parts[1].trim();
    if mtoken.is_empty() || !mtoken.as_bytes().iter().all(|b| b.is_ascii_alphabetic()) {
        return None;
    }
    let month = english_month_abbrev_to_u32(mtoken)?;
    let y_str = parts[2].trim();
    let year: i32 = if y_str.len() <= 2 {
        let y: i32 = y_str.parse().ok()?;
        if y < 0 {
            return None;
        }
        if (0..=99).contains(&y) { 2000 + y } else { y }
    } else {
        y_str.parse().ok()?
    };
    NaiveDate::from_ymd_opt(year, month, day)
}

fn english_month_abbrev_to_u32(s: &str) -> Option<u32> {
    match s.to_ascii_lowercase().as_str() {
        "jan" | "january" => Some(1),
        "feb" | "february" => Some(2),
        "mar" | "march" => Some(3),
        "apr" | "april" => Some(4),
        "may" => Some(5),
        "jun" | "june" => Some(6),
        "jul" | "july" => Some(7),
        "aug" | "august" => Some(8),
        "sep" | "sept" | "september" => Some(9),
        "oct" | "october" => Some(10),
        "nov" | "november" => Some(11),
        "dec" | "december" => Some(12),
        _ => None,
    }
}

pub fn parse_cli_date_with_base(date_str: &str, base: NaiveDate) -> Result<NaiveDate> {
    let shown = date_str.trim();
    within_years(parse_any_year(date_str, base, shown)?, shown)
}

/// A date of a year of four digits, as every database writes one (REVIEW
/// №18). `shown` is the value as the user gave it.
fn within_years(date: NaiveDate, shown: &str) -> Result<NaiveDate> {
    if !crate::model::YEARS.contains(&date.year()) {
        anyhow::bail!(
            "Invalid date '{shown}': the year {} is out of range (dates run from 1000 to 9999)",
            date.year()
        );
    }
    Ok(date)
}

/// An offset that runs past what a date can hold ends beyond the years
/// too, and says so as [`within_years`] does.
fn past_the_years(shown: &str) -> anyhow::Error {
    anyhow::anyhow!("Invalid date '{shown}': the year is out of range (dates run from 1000 to 9999)")
}

/// The year of an absolute date is written with two digits (20xx) or four:
/// `02025`, `+2025` or `205` are not taken for a year they happen to parse
/// to (REVIEW №18, review of R25).
fn check_year_shape(dash_form: &str, shown: &str) -> Result<()> {
    let parts: Vec<&str> = dash_form.split('-').collect();
    if let [_, _, year] = parts.as_slice() {
        let year = year.trim();
        let digits = year.bytes().all(|b| b.is_ascii_digit());
        if !year.is_empty() && digits && !matches!(year.len(), 1 | 2 | 4) {
            anyhow::bail!(
                "Invalid date '{shown}': a year has two digits (20xx) or four \
                 (dates run from 1000 to 9999)"
            );
        }
        if !year.is_empty() && !digits && year.bytes().any(|b| b.is_ascii_digit()) {
            anyhow::bail!("Invalid date '{shown}': a year is written with digits only");
        }
    }
    Ok(())
}

fn parse_any_year(date_str: &str, base: NaiveDate, shown: &str) -> Result<NaiveDate> {
    let trimmed = date_str.trim();
    if trimmed.is_empty() {
        anyhow::bail!("Date cannot be empty");
    }

    match trimmed.to_ascii_lowercase().as_str() {
        "today" => return Ok(base),
        "tomorrow" => {
            return base
                .checked_add_signed(Duration::days(1))
                .context("Date out of range after adding 1 day");
        }
        _ => {}
    }

    let absolute_only = trimmed.contains('-') || trimmed.contains('/') || trimmed.contains('.');
    if !absolute_only {
        let b = trimmed.as_bytes();
        if !b.is_empty() && b[0].is_ascii_digit() {
            return parse_and_apply_relative_cli_date(trimmed, base, shown);
        }
    }

    let dash_form = trimmed.replace(['/', '.'], "-");
    check_year_shape(&dash_form, shown)?;
    if let Some(d) = parse_english_abbrev_dash_date(&dash_form) {
        return Ok(d);
    }

    let normalized = normalize_date_string(trimmed);
    // chrono's reason ("input is out of range" for 31-02) goes right after
    // the value: as a cause it would trail the whole syntax help.
    NaiveDate::parse_from_str(&normalized, "%d-%m-%Y").map_err(|reason| {
        anyhow::anyhow!(
            "Invalid date '{}' ({reason}): use DD-MM-YYYY, DD/MM/YYYY, or DD.MM.YYYY (D-M-YY is OK), \
or DD-Mon-YY / DD-Mon-YYYY (e.g. 11-jan-25), \n\
or `today` / `tomorrow`, \
or a relative offset such as 2d, 2w, 5m, 3q, 2y (combinable, e.g. 10d5w); \
with a leading + when editing, count from the task's current due date (today if none)",
            trimmed
        )
    })
}

pub fn normalize_date_string(date_str: &str) -> String {
    let normalized = date_str.replace(['/', '.'], "-");

    let parts: Vec<&str> = normalized.split('-').collect();
    if let [day, month, year_str] = parts.as_slice() {
        let year_str = year_str.trim();
        if !year_str.is_empty()
            && year_str.len() <= 2
            && let Ok(year) = year_str.parse::<u16>()
            && year < 100
        {
            return format!("{}-{}-{}", day, month, 2000 + year);
        }
    }

    normalized
}

fn parse_relative_cli_segments(s: &str) -> Result<Vec<(u32, char)>> {
    let mut i = 0;
    let mut segments = Vec::new();
    let bytes = s.as_bytes();
    while i < bytes.len() {
        if !bytes[i].is_ascii_digit() {
            anyhow::bail!(
                "Invalid relative date '{}': expected a digit at position {}",
                s,
                i + 1
            );
        }
        let start = i;
        while i < bytes.len() && bytes[i].is_ascii_digit() {
            i += 1;
        }
        let n: u32 = s[start..i]
            .parse()
            .map_err(|_| anyhow::anyhow!("Invalid number in relative date '{}'", s))?;
        if n == 0 {
            anyhow::bail!("Relative date amounts must be positive (got 0)");
        }
        if i >= bytes.len() {
            anyhow::bail!(
                "Invalid relative date '{}': expected unit d, w, m, q, or y after {}",
                s,
                &s[start..i]
            );
        }
        let c = s[i..].chars().next().unwrap();
        let clen = c.len_utf8();
        match c {
            'd' | 'w' | 'm' | 'q' | 'y' => {
                segments.push((n, c));
                i += clen;
            }
            _ => {
                anyhow::bail!(
                    "Invalid relative date '{}': unknown unit {:?} (use d, w, m, q, y)",
                    s,
                    c
                );
            }
        }
    }
    if segments.is_empty() {
        anyhow::bail!("Relative date cannot be empty");
    }
    Ok(segments)
}

/// Adds the segments to `base`: all the months first (m, q = 3, y = 12), in
/// one step, then all the days (d, w = 7). One step, so that `+1m1m` is
/// `+2m`: from the 31st, a month with fewer days ends the date at its last
/// day (31-01 + 1m = 28-02, or 29-02 in a leap year), and it does that once,
/// not once per segment (REVIEW №132). The order of the segments does not
/// matter.
/// `None` when the sum runs past what a date can hold.
fn apply_relative_cli_segments(base: NaiveDate, segments: &[(u32, char)]) -> Option<NaiveDate> {
    let (mut months, mut days) = (0u32, 0i64);
    for &(n, unit) in segments {
        match unit {
            'd' => days = days.checked_add(n as i64)?,
            'w' => days = days.checked_add((n as i64).checked_mul(7)?)?,
            'm' => months = months.checked_add(n)?,
            'q' => months = months.checked_add(n.checked_mul(3)?)?,
            'y' => months = months.checked_add(n.checked_mul(12)?)?,
            _ => unreachable!(),
        }
    }
    base.checked_add_months(Months::new(months))
        .and_then(|d| d.checked_add_signed(Duration::try_days(days)?))
}

fn parse_and_apply_relative_cli_date(trimmed: &str, base: NaiveDate, shown: &str) -> Result<NaiveDate> {
    let segments = parse_relative_cli_segments(trimmed)?;
    apply_relative_cli_segments(base, &segments).ok_or_else(|| past_the_years(shown))
}

#[cfg(test)]
mod tests {
    use super::{parse_cli_date_for_edit, parse_cli_date_with_base};
    use chrono::{Local, NaiveDate};

    fn d(y: i32, m: u32, day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, day).unwrap()
    }

    /// REVIEW №18: a year of one to three digits, with a sign or with five
    /// digits was taken, and showed as a two-digit year nobody could read.
    #[test]
    fn a_year_has_four_digits() {
        for value in ["01-01--2025", "11-jan-20255", "01-01-20255", "11-jan-025", "1-1-100", "1-1-999"] {
            let err = parse_cli_date_with_base(value, d(2026, 9, 27)).unwrap_err().to_string();
            assert!(err.starts_with(&format!("Invalid date '{value}'")), "{value}: {err}");
        }
        assert_eq!(parse_cli_date_with_base("1-1-25", d(2026, 1, 1)).unwrap(), d(2025, 1, 1));
        assert_eq!(parse_cli_date_with_base("11-jan-25", d(2026, 1, 1)).unwrap(), d(2025, 1, 11));
        assert_eq!(parse_cli_date_with_base("1.1.1999", d(2026, 1, 1)).unwrap(), d(1999, 1, 1));
        assert!(parse_cli_date_with_base("8000y", d(2026, 1, 1)).is_err());
        // The shape of the year, not only its value (review of R25).
        for value in ["01-01-+2025", "11-jan-02025", "11-jan-+5", "1/1/02025"] {
            let err = parse_cli_date_with_base(value, d(2026, 1, 1)).unwrap_err().to_string();
            assert!(err.starts_with(&format!("Invalid date '{value}': a year")), "{value}: {err}");
        }
    }

    /// Review of R25: every date past the years says so, in the same
    /// words, quoting the value as typed — `+` too — and never panics.
    #[test]
    fn a_date_past_the_years_says_so() {
        let range = "(dates run from 1000 to 9999)";
        let base = d(2026, 1, 1);
        for value in ["1-1-10000", "1-1-0999", "3000000d", "100000000d", "4294967295w4294967295w4294967295w4294967295w"] {
            let err = parse_cli_date_with_base(value, base).unwrap_err().to_string();
            assert!(err.starts_with(&format!("Invalid date '{value}': ")), "{value}: {err}");
            assert!(err.ends_with(range), "{value}: {err}");
        }
        let err = parse_cli_date_for_edit("+1d", Some(d(9999, 12, 31))).unwrap_err().to_string();
        assert_eq!(err, format!("Invalid date '+1d': the year 10000 is out of range {range}"));
    }

    /// REVIEW №132: each month segment ended the date at the end of a short
    /// month on its own: `+1m1m` from 31-01 gave 28-03, `+2m` 31-03.
    #[test]
    fn months_are_added_in_one_step() {
        let base = d(2026, 1, 31);
        assert_eq!(parse_cli_date_with_base("1m1m", base).unwrap(), d(2026, 3, 31));
        assert_eq!(parse_cli_date_with_base("2m", base).unwrap(), d(2026, 3, 31));
        assert_eq!(parse_cli_date_with_base("1m", base).unwrap(), d(2026, 2, 28));
        assert_eq!(parse_cli_date_with_base("1y", d(2024, 2, 29)).unwrap(), d(2025, 2, 28));
        // Months first, then days, whatever the order they are written in.
        assert_eq!(parse_cli_date_with_base("1m10d", base).unwrap(), d(2026, 3, 10));
        assert_eq!(parse_cli_date_with_base("10d1m", base).unwrap(), d(2026, 3, 10));
        assert_eq!(parse_cli_date_with_base("1q1y", base).unwrap(), d(2027, 4, 30));
    }

    #[test]
    fn relative_days_and_weeks() {
        let base = d(2025, 1, 1);
        assert_eq!(parse_cli_date_with_base("2d", base).unwrap(), d(2025, 1, 3));
        assert_eq!(parse_cli_date_with_base("1w", base).unwrap(), d(2025, 1, 8));
        assert_eq!(
            parse_cli_date_with_base("10d5w", base).unwrap(),
            d(2025, 2, 15)
        );

        assert_eq!(
            parse_cli_date_for_edit("+1w", Some(base)).unwrap(),
            d(2025, 1, 8)
        );
        assert_eq!(
            parse_cli_date_for_edit("+ 2d", Some(base)).unwrap(),
            d(2025, 1, 3)
        );
        let today = Local::now().date_naive();
        assert_eq!(
            parse_cli_date_for_edit("+3d", None).unwrap(),
            parse_cli_date_with_base("3d", today).unwrap()
        );
    }

    #[test]
    fn relative_months_quarters_years() {
        let base = d(2025, 1, 15);
        assert_eq!(
            parse_cli_date_with_base("5m", base).unwrap(),
            d(2025, 6, 15)
        );
        assert_eq!(
            parse_cli_date_with_base("3q", base).unwrap(),
            d(2025, 10, 15)
        );
        assert_eq!(
            parse_cli_date_with_base("2y", base).unwrap(),
            d(2027, 1, 15)
        );
        assert_eq!(
            parse_cli_date_with_base("12d2q1y", base).unwrap(),
            d(2026, 7, 27)
        );
    }

    #[test]
    fn absolute_still_parsed_when_hyphenated() {
        let base = d(2020, 1, 1);
        assert_eq!(
            parse_cli_date_with_base("10-10-2015", base).unwrap(),
            d(2015, 10, 10)
        );
    }

    #[test]
    fn absolute_parsed_with_dot_separator() {
        let base = d(2020, 1, 1);
        assert_eq!(
            parse_cli_date_with_base("10.1.25", base).unwrap(),
            d(2025, 1, 10)
        );
        assert_eq!(
            parse_cli_date_with_base("01.06.2026", base).unwrap(),
            d(2026, 6, 1)
        );
    }

    #[test]
    fn today_and_tomorrow_words() {
        let base = d(2025, 1, 31);
        assert_eq!(
            parse_cli_date_with_base("today", base).unwrap(),
            d(2025, 1, 31)
        );
        assert_eq!(
            parse_cli_date_with_base("Tomorrow", base).unwrap(),
            d(2025, 2, 1)
        );
        assert_eq!(
            parse_cli_date_with_base(" TODAY ", base).unwrap(),
            d(2025, 1, 31)
        );
    }

    #[test]
    fn relative_rejects_zero_and_bad_unit() {
        let base = d(2025, 1, 1);
        assert!(parse_cli_date_with_base("0d", base).is_err());
        assert!(parse_cli_date_with_base("2x", base).is_err());
        assert!(parse_cli_date_with_base("12", base).is_err());
    }

    #[test]
    fn non_digit_without_separator_falls_back_to_absolute_error() {
        let base = d(2025, 1, 1);
        let err = parse_cli_date_with_base("invalid", base).unwrap_err();
        assert!(err.to_string().contains("Invalid date"));
    }

    /// Why the date is not one comes right after the value, before the
    /// syntax help (as a cause it would trail all of it).
    #[test]
    fn invalid_date_says_why_next_to_the_value() {
        let err = parse_cli_date_with_base("31-02-2025", d(2025, 1, 1)).unwrap_err();
        let message = format!("{err:#}");
        assert!(
            message.starts_with("Invalid date '31-02-2025' (input is out of range): use "),
            "{message}"
        );
        assert_eq!(message.matches("out of range").count(), 1, "{message}");
    }
}
