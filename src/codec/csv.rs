//! CSV database format (RFC 4180, fixed `id,text,date,done,priority`
//! schema) for spreadsheet interop: LibreOffice/Excel edit the file in
//! place, Google Sheets can import it.

use crate::model::{Task, TaskId};
use anyhow::{Result, bail};

const CSV_COLUMNS: [&str; 5] = ["id", "text", "date", "done", "priority"];

fn csv_field(s: &str) -> String {
    if s.contains(',') || s.contains('"') || s.contains('\n') || s.contains('\r') {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_string()
    }
}

/// RFC 4180 with CRLF line endings (what Sheets/Excel produce and expect).
/// Dates are ISO `YYYY-MM-DD`; multiline task text stays inside one quoted field.
pub fn to_csv(tasks: &[Task]) -> String {
    let mut out = String::from("id,text,date,done,priority\r\n");
    for task in tasks {
        let date = task.date.map(|d| d.to_string()).unwrap_or_default();
        out.push_str(&format!(
            "{},{},{},{},{}\r\n",
            task.id,
            csv_field(&task.text),
            date,
            task.done,
            task.priority
        ));
    }
    out
}

/// Splits CSV into records, honoring quoted fields (escaped quotes, embedded
/// commas and newlines) and both LF and CRLF row separators.
fn parse_csv_records(data: &str) -> Vec<Vec<String>> {
    let mut records: Vec<Vec<String>> = Vec::new();
    let mut record: Vec<String> = Vec::new();
    let mut field = String::new();
    let mut in_quotes = false;
    // Spreadsheet exports often start with a UTF-8 BOM.
    let mut chars = data.trim_start_matches('\u{feff}').chars().peekable();

    let flush_record =
        |records: &mut Vec<Vec<String>>, record: &mut Vec<String>, field: &mut String| {
            record.push(std::mem::take(field));
            // A lone empty field is a blank line, not a record.
            if record.len() > 1 || !record[0].is_empty() {
                records.push(std::mem::take(record));
            } else {
                record.clear();
            }
        };

    while let Some(c) = chars.next() {
        if in_quotes {
            if c == '"' {
                if chars.peek() == Some(&'"') {
                    chars.next();
                    field.push('"');
                } else {
                    in_quotes = false;
                }
            } else {
                field.push(c);
            }
        } else {
            match c {
                '"' if field.is_empty() => in_quotes = true,
                ',' => record.push(std::mem::take(&mut field)),
                '\r' => {
                    if chars.peek() == Some(&'\n') {
                        chars.next();
                    }
                    flush_record(&mut records, &mut record, &mut field);
                }
                '\n' => flush_record(&mut records, &mut record, &mut field),
                _ => field.push(c),
            }
        }
    }
    if !field.is_empty() || !record.is_empty() {
        flush_record(&mut records, &mut record, &mut field);
    }
    records
}

fn parse_csv_bool(value: &str, column: &str, row: usize) -> Result<bool> {
    // Sheets/Excel export booleans as TRUE/FALSE; also accept 1/0.
    match value.trim().to_ascii_lowercase().as_str() {
        "true" | "1" => Ok(true),
        "false" | "0" | "" => Ok(false),
        other => bail!("CSV row {row}: invalid {column} value '{other}' (expected true/false)"),
    }
}

pub fn from_csv(data: &str) -> Result<Vec<Task>> {
    let records = parse_csv_records(data);
    let Some(header) = records.first() else {
        return Ok(Vec::new());
    };

    let normalized: Vec<String> = header
        .iter()
        .map(|h| h.trim().to_ascii_lowercase())
        .collect();
    if normalized != CSV_COLUMNS {
        bail!(
            "CSV header must be exactly `id,text,date,done,priority`, got `{}`",
            header.join(",")
        );
    }

    let mut tasks = Vec::new();
    for (i, record) in records.iter().enumerate().skip(1) {
        let row = i + 1;
        if record.len() != CSV_COLUMNS.len() {
            bail!(
                "CSV row {row}: expected {} columns, got {}",
                CSV_COLUMNS.len(),
                record.len()
            );
        }
        let id: TaskId = record[0]
            .trim()
            .parse()
            .map_err(|_| anyhow::anyhow!("CSV row {row}: invalid id '{}' (1-255)", record[0]))?;
        let date = match record[2].trim() {
            "" => None,
            d => Some(d.parse::<chrono::NaiveDate>().map_err(|_| {
                anyhow::anyhow!("CSV row {row}: invalid date '{d}' (expected YYYY-MM-DD)")
            })?),
        };
        tasks.push(Task {
            id,
            text: record[1].clone(),
            date,
            done: parse_csv_bool(&record[3], "done", row)?,
            priority: parse_csv_bool(&record[4], "priority", row)?,
        });
    }
    Ok(tasks)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;

    fn task(id: TaskId, text: &str) -> Task {
        Task {
            id,
            text: text.to_string(),
            date: None,
            done: false,
            priority: false,
        }
    }

    #[test]
    fn roundtrip_with_special_characters() {
        let mut t1 = task(1, "multi\nline, with \"quotes\" and, commas");
        t1.date = Some(NaiveDate::from_ymd_opt(2026, 7, 8).unwrap());
        t1.priority = true;
        let t2 = task(2, "plain");
        let tasks = vec![t1, t2];

        let csv = to_csv(&tasks);
        let parsed = from_csv(&csv).unwrap();
        assert_eq!(parsed, tasks);
    }

    #[test]
    fn accepts_spreadsheet_style_output() {
        // BOM, CRLF, uppercase booleans — typical Sheets/Excel export.
        let csv = "\u{feff}id,text,date,done,priority\r\n1,Buy milk,2026-07-10,TRUE,FALSE\r\n2,\"a, b\",,FALSE,TRUE\r\n";
        let tasks = from_csv(csv).unwrap();
        assert_eq!(tasks.len(), 2);
        assert!(tasks[0].done);
        assert_eq!(
            tasks[0].date,
            Some(NaiveDate::from_ymd_opt(2026, 7, 10).unwrap())
        );
        assert_eq!(tasks[1].text, "a, b");
        assert!(tasks[1].priority);
        assert_eq!(tasks[1].date, None);
    }

    #[test]
    fn empty_input_and_header_only() {
        assert_eq!(from_csv("").unwrap(), Vec::<Task>::new());
        assert_eq!(
            from_csv("id,text,date,done,priority\n").unwrap(),
            Vec::<Task>::new()
        );
    }

    #[test]
    fn errors_carry_row_numbers() {
        let bad_id = "id,text,date,done,priority\nabc,x,,false,false\n";
        let err = from_csv(bad_id).unwrap_err().to_string();
        assert!(err.contains("row 2"), "{err}");

        let bad_cols = "id,text,date,done,priority\n1,x,,false\n";
        let err = from_csv(bad_cols).unwrap_err().to_string();
        assert!(err.contains("row 2"), "{err}");

        let bad_header = "id;text;date\n";
        let err = from_csv(bad_header).unwrap_err().to_string();
        assert!(err.contains("header"), "{err}");
    }

    #[test]
    fn blank_lines_are_skipped() {
        let csv = "id,text,date,done,priority\n\n1,x,,false,false\n\n";
        assert_eq!(from_csv(csv).unwrap().len(), 1);
    }
}
