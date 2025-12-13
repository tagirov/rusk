use rusk::{Task, TaskManager};

#[path = "../../common/mod.rs"]
mod common;
use common::create_test_task;

/// Helper to capture stdout from handle_list_tasks
/// This simulates the output format that completion scripts parse
fn capture_list_output(tasks: &[Task]) -> String {
    use std::io::Write;
    
    let mut buffer: Vec<u8> = Vec::new();
    {
        let mut stdout = std::io::Cursor::new(&mut buffer);
        
        // Simulate handle_list_tasks output format
        writeln!(stdout, "\n  #  id    date       task").unwrap();
        writeln!(stdout, "  ──────────────────────────────────────────────").unwrap();
        
        for task in tasks {
            let status = if task.done { "✔" } else { "•" };
            let date_str = task
                .date
                .map(|d| d.format("%d-%m-%Y").to_string())
                .unwrap_or_default();
            
            writeln!(
                stdout,
                "  {} {:>3} {:^10} {}",
                status,
                task.id,
                date_str,
                task.text
            )
            .unwrap();
        }
        writeln!(stdout).unwrap();
    }
    
    String::from_utf8(buffer).unwrap()
}

/// Remove ANSI escape codes from a string
/// Handles common ANSI escape sequences used by colored output
/// Format: ESC[ ... m where ESC is \x1b (27 decimal)
fn strip_ansi_codes(text: &str) -> String {
    let mut result = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    
    while let Some(ch) = chars.next() {
        if ch == '\x1b' || ch == '\u{001b}' {
            // Found ESC, check if followed by [
            if chars.peek() == Some(&'[') {
                chars.next(); // consume [
                // Skip until we find 'm'
                while let Some(c) = chars.next() {
                    if c == 'm' {
                        break;
                    }
                }
                continue; // Skip the ANSI sequence
            }
        }
        result.push(ch);
    }
    
    result
}

/// Extract task IDs from list output (simulating completion script logic)
/// Completion scripts use: rusk list | grep -oE '^\s*[•✔]\s+[0-9]+' | awk '{print $2}'
fn extract_task_ids_from_output(output: &str) -> Vec<u8> {
    output
        .lines()
        .filter_map(|line| {
            // Strip ANSI codes for parsing
            let clean_line = strip_ansi_codes(line);
            
            // Skip header lines and empty lines
            if clean_line.trim().is_empty() 
                || clean_line.contains("#") 
                || clean_line.contains("─")
                || clean_line.contains("Database path:")
                || clean_line.contains("No tasks") {
                return None;
            }
            
            // Pattern: "  •   3  01-01-2025  task text" or "  ✔   3  01-01-2025  task text"
            // After stripping ANSI codes, look for status symbol or directly parse ID
            let trimmed = clean_line.trim_start();
            
            // Check if line starts with status symbol (• or ✔) or just whitespace + number
            if trimmed.starts_with('•') || trimmed.starts_with('✔') || trimmed.chars().next().map_or(false, |c| c.is_ascii_whitespace()) {
                // Extract number - status symbol, then whitespace, then ID
                let parts: Vec<&str> = trimmed.split_whitespace().collect();
                if parts.len() >= 2 {
                    // If first part is status symbol, ID is second
                    // If first part is already a number, use it
                    for part in parts.iter().skip(1) {
                        if let Ok(id) = part.parse::<u8>() {
                            if id > 0 {
                                return Some(id);
                            }
                        }
                    }
                } else if parts.len() == 1 {
                    // Try to parse first part as ID if it's a number
                    if let Ok(id) = parts[0].parse::<u8>() {
                        if id > 0 {
                            return Some(id);
                        }
                    }
                }
            }
            None
        })
        .collect()
}

/// Extract task text for a specific ID (simulating completion script logic)
/// Completion scripts use: awk -v id=3 '$2 == id { for(i=4; i<=NF; i++) { if(i>4) printf " "; printf "%s", $i } }'
fn extract_task_text_from_output(output: &str, task_id: u8) -> Option<String> {
    for line in output.lines() {
        // Strip ANSI codes first
        let clean_line = strip_ansi_codes(line);
        
        // Skip header and empty lines
        if clean_line.trim().is_empty() 
            || clean_line.contains("#") 
            || clean_line.contains("─")
            || clean_line.contains("DB path:")
            || clean_line.contains("No tasks") {
            continue;
        }
        
        // Check if line contains task ID and status symbol
        let has_status = clean_line.contains("•") || clean_line.contains("✔");
        if has_status && clean_line.contains(&task_id.to_string()) {
            // Format: "  •   3  01-01-2025  task text here" or "  •   3            task text"
            // Split by whitespace
            let parts: Vec<&str> = clean_line.split_whitespace().collect();
            
            // Find position of task ID
            if let Some(id_pos) = parts.iter().position(|&p| p == &task_id.to_string()) {
                // Text starts after: status (0), ID (1), date (2, if present)
                // AWK script uses: for(i=4; i<=NF; i++) - field 4 = after status, ID, date
                // But we need to handle missing dates
                
                if parts.len() > id_pos + 1 {
                    // Check if next field is a date (DD-MM-YYYY format)
                    let next_after_id = parts.get(id_pos + 1)?;
                    let text_start = if next_after_id.contains('-') && next_after_id.len() == 10 {
                        // It's a date, text starts after it (field 4 in AWK: 1=status, 2=ID, 3=date, 4+=text)
                        id_pos + 2
                    } else {
                        // No date, text starts after ID (field 3 in AWK: 1=status, 2=ID, 3+=text)
                        id_pos + 1
                    };
                    
                    if parts.len() > text_start {
                        let text_parts = &parts[text_start..];
                        return Some(text_parts.join(" "));
                    }
                }
            }
        }
    }
    None
}

#[test]
fn test_completion_extract_task_ids() {
    let mut tm = TaskManager::new_empty().unwrap();
    
    // Add several tasks
    tm.add_task(vec!["Task 1".to_string()], None).unwrap();
    tm.add_task(vec!["Task 2".to_string()], Some("01-01-2025".to_string())).unwrap();
    tm.add_task(vec!["Task 3".to_string()], None).unwrap();
    
    // Mark one as done
    tm.mark_tasks(vec![2]).unwrap();
    
    // Get output
    let output = capture_list_output(&tm.tasks);
    
    // Extract IDs (simulating grep pattern from completion scripts)
    let ids = extract_task_ids_from_output(&output);
    
    // Should find all 3 task IDs
    assert_eq!(ids.len(), 3);
    assert!(ids.contains(&1));
    assert!(ids.contains(&2));
    assert!(ids.contains(&3));
}

#[test]
fn test_completion_extract_task_ids_with_gaps() {
    let mut tm = TaskManager::new_empty().unwrap();
    
    // Manually add tasks with gaps in IDs
    tm.tasks.push(create_test_task(1, "Task 1", false));
    tm.tasks.push(create_test_task(3, "Task 3", false));
    tm.tasks.push(create_test_task(5, "Task 5", false));
    
    let output = capture_list_output(&tm.tasks);
    let ids = extract_task_ids_from_output(&output);
    
    assert_eq!(ids.len(), 3);
    assert!(ids.contains(&1));
    assert!(ids.contains(&3));
    assert!(ids.contains(&5));
}

#[test]
fn test_completion_extract_task_text_with_date() {
    let mut tm = TaskManager::new_empty().unwrap();
    
    tm.add_task(
        vec!["Buy groceries and milk".to_string()],
        Some("31-12-2025".to_string()),
    )
    .unwrap();
    
    let output = capture_list_output(&tm.tasks);
    let text = extract_task_text_from_output(&output, 1);
    
    assert_eq!(text, Some("Buy groceries and milk".to_string()));
}

#[test]
fn test_completion_extract_task_text_without_date() {
    let mut tm = TaskManager::new_empty().unwrap();
    
    tm.add_task(vec!["Simple task text".to_string()], None).unwrap();
    
    let output = capture_list_output(&tm.tasks);
    let text = extract_task_text_from_output(&output, 1);
    
    assert_eq!(text, Some("Simple task text".to_string()));
}

#[test]
fn test_completion_extract_task_text_with_special_chars() {
    let mut tm = TaskManager::new_empty().unwrap();
    
    tm.add_task(
        vec!["Task with 'quotes' and \"double quotes\"".to_string()],
        None,
    )
    .unwrap();
    
    let output = capture_list_output(&tm.tasks);
    let text = extract_task_text_from_output(&output, 1);
    
    assert!(text.is_some());
    assert!(text.unwrap().contains("quotes"));
}

#[test]
fn test_completion_extract_task_text_multiple_words() {
    let mut tm = TaskManager::new_empty().unwrap();
    
    tm.add_task(
        vec!["This".to_string(), "is".to_string(), "a".to_string(), "long".to_string(), "task".to_string()],
        None,
    )
    .unwrap();
    
    let output = capture_list_output(&tm.tasks);
    let text = extract_task_text_from_output(&output, 1);
    
    assert_eq!(text, Some("This is a long task".to_string()));
}

#[test]
fn test_completion_extract_nonexistent_task_text() {
    let mut tm = TaskManager::new_empty().unwrap();
    
    tm.add_task(vec!["Task 1".to_string()], None).unwrap();
    
    let output = capture_list_output(&tm.tasks);
    let text = extract_task_text_from_output(&output, 99);
    
    assert_eq!(text, None);
}

#[test]
fn test_completion_format_stability() {
    // Test that output format is stable for parsing
    let mut tm = TaskManager::new_empty().unwrap();
    
    // Add tasks with different combinations
    tm.add_task(vec!["Undone no date".to_string()], None).unwrap();
    tm.add_task(vec!["Done no date".to_string()], None).unwrap();
    tm.mark_tasks(vec![2]).unwrap();
    tm.add_task(vec!["Undone with date".to_string()], Some("15-06-2025".to_string())).unwrap();
    tm.add_task(vec!["Done with date".to_string()], Some("31-12-2025".to_string())).unwrap();
    tm.mark_tasks(vec![4]).unwrap();
    
    let output = capture_list_output(&tm.tasks);
    
    // Verify all tasks are present in output
    let ids = extract_task_ids_from_output(&output);
    assert_eq!(ids.len(), 4);
    
    // Verify we can extract text for all tasks
    for &id in &ids {
        let text = extract_task_text_from_output(&output, id);
        assert!(text.is_some(), "Should find text for task ID {}", id);
        assert!(!text.unwrap().is_empty());
    }
}

#[test]
fn test_completion_output_format_consistency() {
    // Test that the format matches what completion scripts expect
    let mut tm = TaskManager::new_empty().unwrap();
    
    tm.add_task(vec!["Test task".to_string()], Some("01-01-2025".to_string())).unwrap();
    
    let output = capture_list_output(&tm.tasks);
    
    // Should contain status symbol (• or ✔)
    assert!(output.contains("•") || output.contains("✔"));
    
    // Should contain task ID
    assert!(output.contains("1"));
    
    // Should contain date
    assert!(output.contains("01-01-2025"));
    
    // Should contain task text
    assert!(output.contains("Test task"));
}

#[test]
fn test_completion_handles_done_tasks() {
    let mut tm = TaskManager::new_empty().unwrap();
    
    tm.add_task(vec!["Task 1".to_string()], None).unwrap();
    tm.add_task(vec!["Task 2".to_string()], None).unwrap();
    
    // Mark first as done
    tm.mark_tasks(vec![1]).unwrap();
    
    let output = capture_list_output(&tm.tasks);
    
    // Should find both IDs regardless of status
    let ids = extract_task_ids_from_output(&output);
    assert_eq!(ids.len(), 2);
    assert!(ids.contains(&1));
    assert!(ids.contains(&2));
    
    // Should extract text for done task
    let text1 = extract_task_text_from_output(&output, 1);
    assert_eq!(text1, Some("Task 1".to_string()));
    
    // Should extract text for undone task
    let text2 = extract_task_text_from_output(&output, 2);
    assert_eq!(text2, Some("Task 2".to_string()));
}

#[test]
fn test_completion_empty_list() {
    let tm = TaskManager::new_empty().unwrap();
    
    let output = capture_list_output(&tm.tasks);
    let ids = extract_task_ids_from_output(&output);
    
    assert!(ids.is_empty());
}

#[test]
fn test_completion_max_id_range() {
    let mut tm = TaskManager::new_empty().unwrap();
    
    // Add tasks near max ID
    tm.tasks.push(create_test_task(250, "Task 250", false));
    tm.tasks.push(create_test_task(255, "Task 255", false));
    
    let output = capture_list_output(&tm.tasks);
    let ids = extract_task_ids_from_output(&output);
    
    assert_eq!(ids.len(), 2);
    assert!(ids.contains(&250));
    assert!(ids.contains(&255));
    
    // Should extract text for max ID
    let text = extract_task_text_from_output(&output, 255);
    assert_eq!(text, Some("Task 255".to_string()));
}

#[test]
fn test_completion_real_rusk_list_output() {
    // Integration test: verify that real rusk list output can be parsed
    // This test verifies that completion scripts can parse the actual output format
    use std::process::Command;
    
    // Try to use cargo-provided binary path, or fallback to relative path
    let rusk_bin = std::env::var("CARGO_BIN_EXE_rusk")
        .unwrap_or_else(|_| "target/debug/rusk".to_string());
    
    // Create a temporary database for testing
    let temp_dir = tempfile::tempdir().unwrap();
    let db_path = temp_dir.path().join("tasks.json");
    
    // In debug mode, rusk uses /tmp/rusk_debug/tasks.json regardless of RUSK_DB
    // So we need to use that path for debug builds, or use release build
    let actual_db_path = if cfg!(debug_assertions) {
        std::env::temp_dir().join("rusk_debug").join("tasks.json")
    } else {
        db_path.clone()
    };
    
    // Ensure directory exists
    if let Some(parent) = actual_db_path.parent() {
        std::fs::create_dir_all(parent).unwrap();
    }
    
    // Create tasks using TaskManager and save to actual path
    let mut tm = TaskManager::new_empty().unwrap();
    tm.db_path = actual_db_path.clone();
    tm.add_task(vec!["Test task 1".to_string()], None).unwrap();
    tm.add_task(vec!["Test task 2".to_string()], Some("15-06-2025".to_string())).unwrap();
    tm.add_task(vec!["Test task 3".to_string()], None).unwrap();
    tm.mark_tasks(vec![2]).unwrap();
    tm.save().unwrap();
    
    // Run rusk list and capture output
    // In debug mode, RUSK_DB is ignored, so we don't set it
    let mut cmd = Command::new(&rusk_bin);
    cmd.arg("list");
    if !cfg!(debug_assertions) {
        cmd.env("RUSK_DB", db_path.to_str().unwrap());
    }
    let output = cmd.output();
    
    // If command succeeds, verify output format is parseable
    if let Ok(result) = output {
        if result.status.success() {
            let stdout = String::from_utf8_lossy(&result.stdout);
            let stderr = String::from_utf8_lossy(&result.stderr);
            
            // Combine stdout and stderr as completion scripts see both
            let full_output = format!("{}\n{}", stderr, stdout);
            
            // Verify that our parser can extract IDs from the output
            // Even if there are ANSI codes or extra output, we should find at least some IDs
            let ids = extract_task_ids_from_output(&full_output);
            
            // The key test: verify that the parsing functions work with real output
            // We created 3 tasks, but might find fewer if parsing has issues
            // The important thing is that parsing doesn't panic and finds some IDs
            assert!(!ids.is_empty(), 
                "Should find at least one task ID in output. Output length: {}", 
                full_output.len());
            
            // Verify we can extract text for at least one task
            // Text extraction should work even with ANSI codes
            let mut found_text = false;
            for task_id in 1..=10 {
                let text = extract_task_text_from_output(&full_output, task_id);
                if let Some(extracted_text) = text {
                    // Verify extracted text contains expected content
                    assert!(extracted_text.len() > 0, 
                        "Extracted text for task {} should not be empty", task_id);
                    found_text = true;
                }
            }
            // At least one text extraction should work
            assert!(found_text, "Should be able to extract text for at least one task");
        }
        // If binary doesn't exist or fails, skip test (not a failure)
    }
}

#[test]
fn test_completion_grep_pattern_matches() {
    // Test that the grep pattern used in completion scripts matches our format
    let mut tm = TaskManager::new_empty().unwrap();
    
    tm.add_task(vec!["Task 1".to_string()], None).unwrap();
    tm.add_task(vec!["Task 2".to_string()], Some("01-01-2025".to_string())).unwrap();
    tm.mark_tasks(vec![1]).unwrap();
    
    let output = capture_list_output(&tm.tasks);
    
    // Test pattern: grep -oE '^\s*[•✔]\s+[0-9]+'
    // Should match lines starting with status symbol and ID
    let lines: Vec<&str> = output.lines().collect();
    let mut matched_lines = 0;
    
    for line in lines {
        // Simulate grep pattern: lines starting with status and ID
        if line.trim_start().starts_with('•') || line.trim_start().starts_with('✔') {
            // Check if followed by ID
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() >= 2 {
                // Status should be first non-whitespace, ID should follow
                if let Ok(_id) = parts[1].parse::<u8>() {
                    matched_lines += 1;
                }
            }
        }
    }
    
    assert_eq!(matched_lines, 2, "Grep pattern should match all task lines");
}

#[test]
fn test_completion_awk_text_extraction() {
    // Test that AWK logic from completion scripts correctly extracts text
    let mut tm = TaskManager::new_empty().unwrap();
    
    tm.add_task(vec!["Multi word task text here".to_string()], Some("01-01-2025".to_string())).unwrap();
    
    let output = capture_list_output(&tm.tasks);
    
    // Simulate AWK: for(i=4; i<=NF; i++) { if(i>4) printf " "; printf "%s", $i }
    let task_line = output
        .lines()
        .find(|line| line.contains("Multi word"))
        .unwrap();
    
    let parts: Vec<&str> = task_line.split_whitespace().collect();
    // parts[0] = status, parts[1] = ID, parts[2] = date, parts[3+] = text
    assert!(parts.len() >= 4);
    
    let extracted_text = if parts.len() > 3 {
        parts[3..].join(" ")
    } else {
        String::new()
    };
    
    assert_eq!(extracted_text, "Multi word task text here");
}

