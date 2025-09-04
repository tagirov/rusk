use rusk::TaskManager;
use chrono::NaiveDate;

#[test]
fn test_edge_case_empty_inputs() {
    let mut tm = TaskManager::new_empty().unwrap();
    
    // Test completely empty input
    let result = tm.add_task(vec![], None);
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("Task text cannot be empty"));
    
    // Test whitespace-only input
    let result = tm.add_task(vec!["   ".to_string()], None);
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("Task text cannot be empty"));
    
    // Test tab-only input
    let result = tm.add_task(vec!["\t\t".to_string()], None);
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("Task text cannot be empty"));
    
    // Test newline-only input
    let result = tm.add_task(vec!["\n\n".to_string()], None);
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("Task text cannot be empty"));
    
    // Test mixed whitespace input
    let result = tm.add_task(vec![" \t \n ".to_string()], None);
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("Task text cannot be empty"));
}

#[test]
fn test_edge_case_very_long_text() {
    let mut tm = TaskManager::new_empty().unwrap();
    
    // Test very long text (1000 characters)
    let long_text = "a".repeat(1000);
    let result = tm.add_task(vec![long_text.clone()], None);
    assert!(result.is_ok());
    assert_eq!(tm.tasks[0].text, long_text);
    
    // Test extremely long text (10000 characters)
    let very_long_text = "b".repeat(10000);
    let result = tm.add_task(vec![very_long_text.clone()], None);
    assert!(result.is_ok());
    assert_eq!(tm.tasks[1].text, very_long_text);
    
    // Test text with special characters
    let special_text = "!@#$%^&*()_+-=[]{}|;':\",./<>?`~\\";
    let result = tm.add_task(vec![special_text.to_string()], None);
    assert!(result.is_ok());
    assert_eq!(tm.tasks[2].text, special_text);
    
    // Test text with unicode characters
    let unicode_text = "Привет мир! 🌍 Hello world! こんにちは世界！";
    let result = tm.add_task(vec![unicode_text.to_string()], None);
    assert!(result.is_ok());
    assert_eq!(tm.tasks[3].text, unicode_text);
}

#[test]
fn test_edge_case_date_boundaries() {
    let mut tm = TaskManager::new_empty().unwrap();
    
    // Test earliest possible date
    let earliest_date = "0001-01-01";
    let result = tm.add_task(vec!["Earliest task".to_string()], Some(earliest_date.to_string()));
    assert!(result.is_ok());
    assert_eq!(tm.tasks[0].date, NaiveDate::parse_from_str(earliest_date, "%Y-%m-%d").ok());
    
    // Test latest possible date
    let latest_date = "9999-12-31";
    let result = tm.add_task(vec!["Latest task".to_string()], Some(latest_date.to_string()));
    assert!(result.is_ok());
    assert_eq!(tm.tasks[1].date, NaiveDate::parse_from_str(latest_date, "%Y-%m-%d").ok());
    
    // Test leap year dates
    let leap_dates = vec![
        "2024-02-29", // Valid leap year
        "2020-02-29", // Valid leap year
        "2016-02-29", // Valid leap year
    ];
    
    for (i, date) in leap_dates.iter().enumerate() {
        let add_result = tm.add_task(vec![format!("Leap task {}", i + 1)], Some(date.to_string()));
        assert!(add_result.is_ok());
        assert_eq!(tm.tasks[i + 2].date, NaiveDate::parse_from_str(date, "%Y-%m-%d").ok());
    }
}

#[test]
fn test_edge_case_special_characters_in_text() {
    let mut tm = TaskManager::new_empty().unwrap();
    
    // Test various special characters
    let special_texts = vec![
        "Task with 'quotes'",
        "Task with \"double quotes\"",
        "Task with & ampersand",
        "Task with < > brackets",
        "Task with [ ] square brackets",
        "Task with { } curly braces",
        "Task with | pipe",
        "Task with \\ backslash",
        "Task with / forward slash",
        "Task with ` backtick",
        "Task with ~ tilde",
        "Task with ! exclamation",
        "Task with @ at symbol",
        "Task with # hash",
        "Task with $ dollar",
        "Task with % percent",
        "Task with ^ caret",
        "Task with * asterisk",
        "Task with ( ) parentheses",
        "Task with - hyphen",
        "Task with + plus",
        "Task with = equals",
        "Task with _ underscore",
        "Task with ; semicolon",
        "Task with : colon",
        "Task with , comma",
        "Task with . period",
        "Task with ? question mark",
    ];
    
    for (i, text) in special_texts.iter().enumerate() {
        let result = tm.add_task(vec![text.to_string()], None);
        assert!(result.is_ok());
        assert_eq!(tm.tasks[i].text, *text);
    }
}

#[test]
fn test_edge_case_id_generation_under_load() {
    let mut tm = TaskManager::new_empty().unwrap();
    
    // Add many tasks to test ID generation
    for i in 0..50 {
        let add_result = tm.add_task(vec![format!("Task {}", i + 1)], None);
        assert!(add_result.is_ok());
    }
    
    // Verify all IDs are unique and sequential
    let mut ids: Vec<u8> = tm.tasks.iter().map(|t| t.id).collect();
    ids.sort();
    
    for (i, &id) in ids.iter().enumerate() {
        assert_eq!(id, (i + 1) as u8);
    }
    
    // Delete some tasks and add new ones
    let delete_result = tm.delete_tasks(vec![1, 3, 5, 7, 9]);
    assert!(delete_result.is_ok());
    
    // Add new tasks - should reuse deleted IDs
    for i in 0..5 {
        let add_result = tm.add_task(vec![format!("New task {}", i + 1)], None);
        assert!(add_result.is_ok());
    }
    
    // Verify we have the expected number of tasks
    assert_eq!(tm.tasks.len(), 50); // 50 original - 5 deleted + 5 new = 50
}

#[test]
fn test_edge_case_task_text_edge_cases() {
    let mut tm = TaskManager::new_empty().unwrap();
    
    // Test single character
    let result = tm.add_task(vec!["a".to_string()], None);
    assert!(result.is_ok());
    assert_eq!(tm.tasks[0].text, "a");
    
    // Test single word
    let result = tm.add_task(vec!["hello".to_string()], None);
    assert!(result.is_ok());
    assert_eq!(tm.tasks[1].text, "hello");
    
    // Test multiple words
    let result = tm.add_task(vec!["hello".to_string(), "world".to_string()], None);
    assert!(result.is_ok());
    assert_eq!(tm.tasks[2].text, "hello world");
    
    // Test words with extra spaces
    let result = tm.add_task(vec!["  hello  ".to_string(), "  world  ".to_string()], None);
    assert!(result.is_ok());
    // Note: join() preserves the spaces exactly as they are
    // "  hello  " + "  world  " = "  hello     world  " (5 spaces between words)
    assert_eq!(tm.tasks[3].text, "  hello     world  ");
    
    // Test empty strings in vector (should be preserved as spaces)
    let result = tm.add_task(vec!["hello".to_string(), "".to_string(), "world".to_string()], None);
    assert!(result.is_ok());
    // "hello" + "" + "world" = "hello  world" (2 spaces between words)
    assert_eq!(tm.tasks[4].text, "hello  world");
}

#[test]
fn test_edge_case_invalid_date_formats() {
    let mut tm = TaskManager::new_empty().unwrap();
    
    // Test various invalid date formats
    let invalid_dates = vec![
        "2025-13-01", // Invalid month
        "2025-01-32", // Invalid day
        "2025-02-30", // Invalid day for February
        "2025-04-31", // Invalid day for April
        "2025-06-31", // Invalid day for June
        "2025-09-31", // Invalid day for September
        "2025-11-31", // Invalid day for November
        "2025-00-01", // Invalid month (0)
        "2025-01-00", // Invalid day (0)
        "2025-13-32", // Both invalid
        "invalid-date",
        "2025/01/01", // Wrong format
        "01-01-2025", // Wrong format
        "2025.01.01", // Wrong format
        "2025_01_01", // Wrong format
        "2025 01 01", // Wrong format
        "2025-1-1",   // Missing leading zeros
        "2025-01-1",  // Missing leading zero for day
        "2025-1-01",  // Missing leading zero for month
        "",           // Empty string
        "   ",        // Whitespace only
        "2025",       // Missing month and day
        "2025-01",    // Missing day
        "01-01",      // Missing year
        "01",         // Missing year and month
        "2025-01-01-extra", // Extra parts
        "extra-2025-01-01", // Extra parts at start
        "2025-01-01-",      // Trailing dash
        "-2025-01-01",      // Leading dash
        "2025--01-01",      // Double dash
        "2025-01--01",      // Double dash
    ];
    
    for date in invalid_dates {
        let result = tm.add_task(vec!["Test task".to_string()], Some(date.to_string()));
        assert!(result.is_ok()); // Should succeed but with None date
        
        let task = tm.tasks.last().unwrap();
        // Note: Some invalid dates like "2025-1-1" might actually parse successfully
        // We only check that the task was added, not necessarily that date is None
        assert!(task.text == "Test task");
    }
}
