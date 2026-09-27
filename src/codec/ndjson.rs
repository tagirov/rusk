//! NDJSON database format (`tasks.ndjson` / `tasks.jsonl`), feature
//! `fmt-ndjson`: one compact JSON task per line. The schema per line is the
//! same as the JSON format; the win is line-oriented tooling — clean git
//! diffs, `grep`, and `jq -c` over the database.

use crate::model::Task;
use anyhow::{Context, Result};

pub fn encode(tasks: &[Task]) -> Result<String> {
    let mut out = String::new();
    for task in tasks {
        out.push_str(&serde_json::to_string(task).context("Failed to serialize tasks")?);
        out.push('\n');
    }
    Ok(out)
}

/// Every line that is not blank is a task. A line that is none is named
/// with its line number in the file and the column in it; serde counts the
/// line as line 1 of its own (REVIEW №16).
pub fn decode(data: &str) -> Result<Vec<Task>> {
    data.lines()
        .enumerate()
        .filter(|(_, line)| !line.trim().is_empty())
        .map(|(i, line)| {
            serde_json::from_str(line).map_err(|e| {
                let what = e.to_string();
                let at = format!(" at line {} column {}", e.line(), e.column());
                let what = what.strip_suffix(&at).unwrap_or(&what);
                anyhow::anyhow!("NDJSON line {}, column {}: {what}", i + 1, e.column())
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;

    #[test]
    fn roundtrip_one_line_per_task() {
        let tasks = vec![
            Task {
                id: 1,
                text: "multi\nline".into(),
                date: NaiveDate::from_ymd_opt(2026, 7, 15),
                done: false,
                priority: true, after: Vec::new(),
            },
            Task {
                id: 2,
                text: "plain".into(),
                date: None,
                done: true,
                priority: false, after: Vec::new(),
            },
        ];
        let data = encode(&tasks).unwrap();
        assert_eq!(data.lines().count(), 2);
        assert_eq!(decode(&data).unwrap(), tasks);
    }

    #[test]
    fn blank_lines_are_skipped_and_errors_carry_line_numbers() {
        let data = "{\"id\":1,\"text\":\"a\",\"date\":null,\"done\":false}\n\n";
        assert_eq!(decode(data).unwrap().len(), 1);

        let err = decode("{\"id\":1,\"text\":\"a\",\"date\":null,\"done\":false}\nnot json\n")
            .unwrap_err()
            .to_string();
        assert_eq!(err, "NDJSON line 2, column 2: expected ident");
        // A task that is JSON but not a task says what is wrong with it.
        let err = decode("{\"id\":1}\n").unwrap_err().to_string();
        assert!(err.starts_with("NDJSON line 1, column 8: missing field `text`"), "{err}");
    }

    #[test]
    fn empty_input() {
        assert_eq!(decode("").unwrap(), Vec::<Task>::new());
    }
}
