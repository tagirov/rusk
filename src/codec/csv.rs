//! CSV database format (RFC 4180, fixed `id,text,date,done,priority,after`
//! schema) for spreadsheet interop: LibreOffice/Excel edit the file in
//! place, Google Sheets can import it. Files with the older header without
//! the `after` column still load (the column is treated as empty).
//!
//! Made for editing in a spreadsheet: a row of empty cells is skipped, a row
//! with an empty id cell is a new task (it gets the lowest free id on load),
//! and the `after` cell takes ids separated by spaces or commas. Errors name
//! the row as the spreadsheet counts it, and the line of the file when a
//! multi-line cell above makes the two differ.

use crate::model::{Task, TaskId};
use anyhow::{Result, bail};

const CSV_COLUMNS: [&str; 6] = ["id", "text", "date", "done", "priority", "after"];

fn csv_field(s: &str) -> String {
    if s.contains(',') || s.contains('"') || s.contains('\n') || s.contains('\r') {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_string()
    }
}

/// RFC 4180 with CRLF line endings (what Sheets/Excel produce and expect).
/// Dates are ISO `YYYY-MM-DD`; multiline task text stays inside one quoted
/// field; `after` ids are space-separated so the cell needs no quoting.
pub fn to_csv(tasks: &[Task]) -> String {
    let mut out = String::from("id,text,date,done,priority,after\r\n");
    for task in tasks {
        let date = task.date.map(|d| d.to_string()).unwrap_or_default();
        let after = task
            .after
            .iter()
            .map(|id| id.to_string())
            .collect::<Vec<_>>()
            .join(" ");
        out.push_str(&format!(
            "{},{},{},{},{},{}\r\n",
            task.id,
            csv_field(&task.text),
            date,
            task.done,
            task.priority,
            after
        ));
    }
    out
}

/// One CSV record and where it starts: `row` as a spreadsheet counts rows
/// (blank ones included, the header is row 1), `line` as the file counts
/// lines (a quoted cell may span several).
struct Record {
    fields: Vec<String>,
    row: usize,
    line: usize,
}

impl Record {
    /// A row of empty cells (or a blank line): what spreadsheets leave
    /// behind, not a task.
    fn is_blank(&self) -> bool {
        self.fields.iter().all(|field| field.trim().is_empty())
    }

    /// Where the record is, for error messages.
    fn at(&self) -> String {
        if self.row == self.line {
            format!("CSV row {}", self.row)
        } else {
            format!("CSV row {} (line {})", self.row, self.line)
        }
    }
}

/// Splits CSV into records, honoring quoted fields (escaped quotes, embedded
/// commas and newlines) and both LF and CRLF row separators.
fn parse_csv_records(data: &str) -> Vec<Record> {
    let mut records: Vec<Record> = Vec::new();
    let mut fields: Vec<String> = Vec::new();
    let mut field = String::new();
    let mut in_quotes = false;
    let mut line = 1;
    let mut starts_at = 1;
    let mut chars = data.chars().peekable();

    let mut flush = |fields: &mut Vec<String>, field: &mut String, starts_at: usize| {
        fields.push(std::mem::take(field));
        records.push(Record {
            fields: std::mem::take(fields),
            row: records.len() + 1,
            line: starts_at,
        });
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
                if c == '\n' || (c == '\r' && chars.peek() != Some(&'\n')) {
                    line += 1;
                }
                field.push(c);
            }
        } else {
            match c {
                '"' if field.is_empty() => in_quotes = true,
                ',' => fields.push(std::mem::take(&mut field)),
                '\r' | '\n' => {
                    if c == '\r' && chars.peek() == Some(&'\n') {
                        chars.next();
                    }
                    flush(&mut fields, &mut field, starts_at);
                    line += 1;
                    starts_at = line;
                }
                _ => field.push(c),
            }
        }
    }
    if !field.is_empty() || !fields.is_empty() {
        flush(&mut fields, &mut field, starts_at);
    }
    records
}

fn parse_csv_bool(value: &str, column: &str, record: &Record) -> Result<bool> {
    // Sheets/Excel export booleans as TRUE/FALSE; also accept 1/0.
    match value.trim().to_ascii_lowercase().as_str() {
        "true" | "1" => Ok(true),
        "false" | "0" | "" => Ok(false),
        other => bail!(
            "{}: invalid {column} value '{other}' (expected true/false)",
            record.at()
        ),
    }
}

/// An empty cell is a new task: id 0, one is handed out on load.
fn parse_csv_id(value: &str, record: &Record) -> Result<TaskId> {
    match value.trim() {
        "" => Ok(0),
        id => id.parse().map_err(|_| {
            anyhow::anyhow!(
                "{}: invalid id '{id}' (expected a whole number from 1 to {}, or an empty cell \
                 for a new task)",
                record.at(),
                TaskId::MAX
            )
        }),
    }
}

/// Task ids in the `after` column, separated by spaces or commas ("19 22",
/// "19,22"); empty cell → no dependencies.
fn parse_csv_after(value: &str, record: &Record) -> Result<Vec<TaskId>> {
    value
        .split(|c: char| c == ',' || c.is_whitespace())
        .filter(|part| !part.is_empty())
        .map(|part| {
            part.parse::<TaskId>().map_err(|_| {
                anyhow::anyhow!(
                    "{}: invalid after id '{part}' (expected task ids separated by spaces or commas)",
                    record.at()
                )
            })
        })
        .collect()
}

/// Whether `data` holds a record that is not blank. In a file that decoded
/// to no tasks that record is the header: the file is an empty database,
/// while one of blank rows only is not a database at all.
pub(crate) fn has_records(data: &str) -> bool {
    parse_csv_records(data).iter().any(|record| !record.is_blank())
}

pub fn from_csv(data: &str) -> Result<Vec<Task>> {
    let mut records = parse_csv_records(data)
        .into_iter()
        .filter(|record| !record.is_blank());
    let Some(header) = records.next() else {
        return Ok(Vec::new());
    };

    let normalized: Vec<String> = header
        .fields
        .iter()
        .map(|h| h.trim().to_ascii_lowercase())
        .collect();
    // Legacy databases predate the trailing `after` column.
    if normalized != CSV_COLUMNS && normalized != CSV_COLUMNS[..5] {
        bail!(
            "CSV header must be exactly `id,text,date,done,priority,after` (the `after` column may be omitted), got `{}`",
            header.fields.join(",")
        );
    }
    let column_count = normalized.len();

    let mut tasks = Vec::new();
    for record in records {
        let cells = &record.fields;
        if cells.len() != column_count {
            bail!(
                "{}: expected {} columns, got {}",
                record.at(),
                column_count,
                cells.len()
            );
        }
        let date = match cells[2].trim() {
            "" => None,
            d => Some(d.parse::<chrono::NaiveDate>().map_err(|_| {
                anyhow::anyhow!("{}: invalid date '{d}' (expected YYYY-MM-DD)", record.at())
            })?),
        };
        tasks.push(Task {
            id: parse_csv_id(&cells[0], &record)?,
            text: cells[1].clone(),
            date,
            done: parse_csv_bool(&cells[3], "done", &record)?,
            priority: parse_csv_bool(&cells[4], "priority", &record)?,
            after: parse_csv_after(cells.get(5).map(String::as_str).unwrap_or(""), &record)?,
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
            priority: false, after: Vec::new(),
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
        // BOM, CRLF, uppercase booleans — typical Sheets/Excel export. The
        // BOM is stripped for every format at once (`codec::content`), so
        // this goes through the format rather than straight into the parser.
        let csv = "\u{feff}id,text,date,done,priority\r\n1,Buy milk,2026-07-10,TRUE,FALSE\r\n2,\"a, b\",,FALSE,TRUE\r\n";
        let tasks = crate::codec::DbFormat::Csv.decode(csv).unwrap();
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
            from_csv("id,text,date,done,priority,after\n").unwrap(),
            Vec::<Task>::new()
        );
    }

    #[test]
    fn after_column_roundtrips_and_legacy_header_loads() {
        let mut t = task(3, "blocked");
        t.after = vec![1, 2];
        let csv = to_csv(&[t.clone()]);
        assert!(csv.contains(",1 2\r\n"), "{csv}");
        assert_eq!(from_csv(&csv).unwrap(), vec![t]);

        // Pre-`after` databases: 5-column header, no after data.
        let legacy = "id,text,date,done,priority\n1,old,,false,false\n";
        let tasks = from_csv(legacy).unwrap();
        assert_eq!(tasks[0].after, Vec::<TaskId>::new());

        let bad = "id,text,date,done,priority,after\n1,x,,false,false,nope\n";
        let err = from_csv(bad).unwrap_err().to_string();
        assert!(err.contains("after"), "{err}");
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

    #[test]
    fn spreadsheet_leftovers_and_new_rows() {
        // A row of empty cells and a line of spaces are skipped; a row
        // without an id is a task without one (the load hands it out).
        let csv = "id,text,date,done,priority,after\r\n,,,,,\r\n  \r\n,typed in,,FALSE,FALSE,\r\n";
        let tasks = from_csv(csv).unwrap();
        assert_eq!(tasks, vec![task(0, "typed in")]);
    }

    #[test]
    fn after_ids_may_be_separated_by_commas() {
        let csv = "id,text,date,done,priority,after\n3,c,,false,false,\"1,2 , 4\"\n";
        assert_eq!(from_csv(csv).unwrap()[0].after, vec![1, 2, 4]);
    }

    #[test]
    fn errors_name_the_spreadsheet_row_and_the_file_line() {
        let err = |csv: &str| from_csv(csv).unwrap_err().to_string();
        // Blank lines are rows too.
        let e = err("id,text,date,done,priority\n\n1,x,,false,false\n\nx,y,,false,false\n");
        assert!(e.starts_with("CSV row 5: invalid id 'x'"), "{e}");
        // A cell over three lines is one row.
        let e = err("id,text,date,done,priority\n1,\"a\r\nb\nc\",,false,false\n2,z,,maybe,false\n");
        assert!(e.starts_with("CSV row 3 (line 5): invalid done value"), "{e}");
        assert!(err("id,text,date,done,priority\n-1,x,,false,false\n").contains("4294967295"));
    }
}
