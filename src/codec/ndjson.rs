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

pub fn decode(data: &str) -> Result<Vec<Task>> {
    data.trim_start_matches('\u{feff}')
        .lines()
        .enumerate()
        .filter(|(_, line)| !line.trim().is_empty())
        .map(|(i, line)| {
            serde_json::from_str(line).with_context(|| format!("NDJSON line {}", i + 1))
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
        assert!(err.contains("line 2"), "{err}");
    }

    #[test]
    fn empty_input() {
        assert_eq!(decode("").unwrap(), Vec::<Task>::new());
    }
}
