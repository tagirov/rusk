use anyhow::{Context, Result};
use chrono::{Duration, Local, Months, NaiveDate};

pub fn is_cli_date_help_value(s: &str) -> bool {
    matches!(s.trim(), "-h" | "--help")
}

pub fn is_cli_date_clear_value(s: &str) -> bool {
    s.trim() == "_"
}

pub fn parse_cli_date(date_str: &str) -> Result<NaiveDate> {
    parse_cli_date_with_base(date_str, Local::now().date_naive())
}

pub fn parse_cli_date_optional_empty(s: &str) -> Result<Option<NaiveDate>> {
    let trimmed = s.trim();
    if trimmed.is_empty() || is_cli_date_clear_value(trimmed) {
        return Ok(None);
    }
    parse_cli_date(trimmed).map(Some)
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
        if (0..=99).contains(&y) {
            2000 + y
        } else {
            y
        }
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
    let trimmed = date_str.trim();
    if trimmed.is_empty() {
        anyhow::bail!("Date cannot be empty");
    }

    let absolute_only = trimmed.contains('-') || trimmed.contains('/') || trimmed.contains('.');
    if !absolute_only {
        let b = trimmed.as_bytes();
        if !b.is_empty() && b[0].is_ascii_digit() {
            return parse_and_apply_relative_cli_date(trimmed, base);
        }
    }

    let dash_form = trimmed.replace(['/', '.'], "-");
    if let Some(d) = parse_english_abbrev_dash_date(&dash_form) {
        return Ok(d);
    }

    let normalized = normalize_date_string(trimmed);
    NaiveDate::parse_from_str(&normalized, "%d-%m-%Y").with_context(|| {
        format!(
            "Invalid date '{}': use DD-MM-YYYY, DD/MM/YYYY, or DD.MM.YYYY (D-M-YY is OK), \
or DD-Mon-YY / DD-Mon-YYYY (e.g. 11-jan-25), \n\
or a relative offset such as 2d, 2w, 5m, 3q, 2y (combinable, e.g. 10d5w)",
            trimmed
        )
    })
}

pub fn normalize_date_string(date_str: &str) -> String {
    let mut normalized = date_str.replace('/', "-").replace('.', "-");

    let parts: Vec<&str> = normalized.split('-').collect();
    if parts.len() == 3 {
        if let Some(year_str) = parts.get(2) {
            let year_str = year_str.trim();
            if year_str.len() <= 2 && !year_str.is_empty() {
                if let Ok(year) = year_str.parse::<u16>() {
                    if year < 100 {
                        let full_year = 2000 + year;
                        normalized = format!("{}-{}-{}", parts[0], parts[1], full_year);
                    }
                }
            }
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

fn apply_relative_cli_segments(base: NaiveDate, segments: &[(u32, char)]) -> Result<NaiveDate> {
    let mut d = base;
    for &(n, u) in segments {
        d = match u {
            'd' => d
                .checked_add_signed(Duration::days(n as i64))
                .with_context(|| format!("Date out of range after adding {n} day(s)"))?,
            'w' => {
                let days = (n as i64)
                    .checked_mul(7)
                    .ok_or_else(|| anyhow::anyhow!("Week count too large in relative date"))?;
                d.checked_add_signed(Duration::days(days))
                    .with_context(|| format!("Date out of range after adding {n} week(s)"))?
            }
            'm' => d
                .checked_add_months(Months::new(n))
                .with_context(|| format!("Date out of range after adding {n} month(s)"))?,
            'q' => {
                let qm = n.checked_mul(3).ok_or_else(|| {
                    anyhow::anyhow!("Quarter count too large in relative date")
                })?;
                d.checked_add_months(Months::new(qm))
                    .with_context(|| format!("Date out of range after adding {n} quarter(s)"))?
            }
            'y' => {
                let ym = n.checked_mul(12).ok_or_else(|| {
                    anyhow::anyhow!("Year count too large in relative date")
                })?;
                d.checked_add_months(Months::new(ym))
                    .with_context(|| format!("Date out of range after adding {n} year(s)"))?
            }
            _ => unreachable!(),
        };
    }
    Ok(d)
}

fn parse_and_apply_relative_cli_date(trimmed: &str, base: NaiveDate) -> Result<NaiveDate> {
    let segments = parse_relative_cli_segments(trimmed)?;
    apply_relative_cli_segments(base, &segments)
}

#[cfg(test)]
mod tests {
    use super::{parse_cli_date_optional_empty, parse_cli_date_with_base};
    use chrono::{Local, NaiveDate};

    fn d(y: i32, m: u32, day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, day).unwrap()
    }

    #[test]
    fn relative_days_and_weeks() {
        let base = d(2025, 1, 1);
        assert_eq!(parse_cli_date_with_base("2d", base).unwrap(), d(2025, 1, 3));
        assert_eq!(parse_cli_date_with_base("1w", base).unwrap(), d(2025, 1, 8));
        assert_eq!(parse_cli_date_with_base("10d5w", base).unwrap(), d(2025, 2, 15));
    }

    #[test]
    fn relative_months_quarters_years() {
        let base = d(2025, 1, 15);
        assert_eq!(parse_cli_date_with_base("5m", base).unwrap(), d(2025, 6, 15));
        assert_eq!(parse_cli_date_with_base("3q", base).unwrap(), d(2025, 10, 15));
        assert_eq!(parse_cli_date_with_base("2y", base).unwrap(), d(2027, 1, 15));
        assert_eq!(parse_cli_date_with_base("12d2q1y", base).unwrap(), d(2026, 7, 27));
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

    #[test]
    fn optional_empty_returns_none() {
        assert_eq!(parse_cli_date_optional_empty("").unwrap(), None);
        assert_eq!(parse_cli_date_optional_empty("  \t ").unwrap(), None);
    }

    #[test]
    fn optional_nonempty_matches_parse_cli_date() {
        let today = Local::now().date_naive();
        assert_eq!(
            parse_cli_date_optional_empty("5d").unwrap().unwrap(),
            parse_cli_date_with_base("5d", today).unwrap()
        );
        let abs = parse_cli_date_optional_empty("01-06-2026").unwrap().unwrap();
        assert_eq!(abs, NaiveDate::from_ymd_opt(2026, 6, 1).unwrap());
    }
}
