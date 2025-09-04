use rusk::TaskManager;
use chrono::NaiveDate;

#[test]
fn test_cli_add_command() {
    let mut tm = TaskManager::new_empty().unwrap();
    
    // Test adding single word task
    let result = tm.add_task(vec!["hello".to_string()], None);
    assert!(result.is_ok());
    assert_eq!(tm.tasks.len(), 1);
    assert_eq!(tm.tasks[0].text, "hello");
    assert_eq!(tm.tasks[0].id, 1);
    
    // Test adding multi-word task
    let result = tm.add_task(vec!["buy".to_string(), "groceries".to_string()], None);
    assert!(result.is_ok());
    assert_eq!(tm.tasks.len(), 2);
    assert_eq!(tm.tasks[1].text, "buy groceries");
    assert_eq!(tm.tasks[1].id, 2);
    
    // Test adding task with date
    let result = tm.add_task(vec!["meeting".to_string()], Some("2025-01-15".to_string()));
    assert!(result.is_ok());
    assert_eq!(tm.tasks.len(), 3);
    assert_eq!(tm.tasks[2].text, "meeting");
    assert_eq!(tm.tasks[2].date, NaiveDate::parse_from_str("2025-01-15", "%Y-%m-%d").ok());
}

#[test]
fn test_cli_delete_command() {
    let mut tm = TaskManager::new_empty().unwrap();
    
    // Add tasks using TaskManager to get proper IDs
    let result = tm.add_task(vec!["Task 1".to_string()], None);
    assert!(result.is_ok());
    
    let result = tm.add_task(vec!["Task 2".to_string()], None);
    assert!(result.is_ok());
    
    let result = tm.add_task(vec!["Task 3".to_string()], None);
    assert!(result.is_ok());
    
    let result = tm.add_task(vec!["Task 4".to_string()], None);
    assert!(result.is_ok());
    
    // Mark tasks 2 and 4 as done
    let result = tm.mark_tasks(vec![2, 4]);
    assert!(result.is_ok());
    
    // Verify initial state
    assert_eq!(tm.tasks.len(), 4);
    assert!(!tm.tasks[0].done); // Task 1
    assert!(tm.tasks[1].done);  // Task 2
    assert!(!tm.tasks[2].done); // Task 3
    assert!(tm.tasks[3].done);  // Task 4
    
    // Test deleting specific tasks (1 and 3)
    let result = tm.delete_tasks(vec![1, 3]);
    assert!(result.is_ok());
    assert_eq!(tm.tasks.len(), 2);
    
    // After deletion, remaining tasks should have IDs 2 and 4
    let remaining_ids: Vec<u8> = tm.tasks.iter().map(|t| t.id).collect();
    assert!(remaining_ids.contains(&2));
    assert!(remaining_ids.contains(&4));
    
    // Test deleting all done tasks
    let result = tm.delete_all_done();
    assert!(result.is_ok());
    assert_eq!(tm.tasks.len(), 0); // All remaining tasks were done, so all were deleted
}

#[test]
fn test_cli_mark_command() {
    let mut tm = TaskManager::new_empty().unwrap();
    
    // Add tasks
    let result = tm.add_task(vec!["Task 1".to_string()], None);
    assert!(result.is_ok());
    
    let result = tm.add_task(vec!["Task 2".to_string()], None);
    assert!(result.is_ok());
    
    let result = tm.add_task(vec!["Task 3".to_string()], None);
    assert!(result.is_ok());
    
    // Test marking single task
    let result = tm.mark_tasks(vec![1]);
    assert!(result.is_ok());
    assert!(tm.tasks[0].done);
    
    // Test marking multiple tasks
    let result = tm.mark_tasks(vec![2, 3]);
    assert!(result.is_ok());
    assert!(tm.tasks[1].done);   // Task 2 was false, now true
    assert!(tm.tasks[2].done);   // Task 3 was false, now true
    
    // Test marking already done task (should toggle to undone)
    let result = tm.mark_tasks(vec![1]);
    assert!(result.is_ok());
    assert!(!tm.tasks[0].done);  // Task 1 was true, now false
}

#[test]
fn test_cli_edit_command() {
    let mut tm = TaskManager::new_empty().unwrap();
    
    // Add tasks
    let result = tm.add_task(vec!["Original task 1".to_string()], None);
    assert!(result.is_ok());
    
    let result = tm.add_task(vec!["Original task 2".to_string()], None);
    assert!(result.is_ok());
    
    // Test editing text only
    let result = tm.edit_tasks(vec![1], Some(vec!["Updated".to_string(), "text".to_string()]), None);
    assert!(result.is_ok());
    assert_eq!(tm.tasks[0].text, "Updated text");
    assert_eq!(tm.tasks[1].text, "Original task 2"); // Unchanged
    
    // Test editing date only
    let result = tm.edit_tasks(vec![2], None, Some("2025-06-15".to_string()));
    assert!(result.is_ok());
    assert_eq!(tm.tasks[0].date, None); // Unchanged
    assert_eq!(tm.tasks[1].date, NaiveDate::parse_from_str("2025-06-15", "%Y-%m-%d").ok());
    
    // Test editing both text and date
    let result = tm.edit_tasks(vec![1], Some(vec!["Final".to_string(), "version".to_string()]), Some("2025-12-31".to_string()));
    assert!(result.is_ok());
    assert_eq!(tm.tasks[0].text, "Final version");
    assert_eq!(tm.tasks[0].date, NaiveDate::parse_from_str("2025-12-31", "%Y-%m-%d").ok());
}

#[test]
fn test_cli_list_command() {
    let mut tm = TaskManager::new_empty().unwrap();
    
    // Test empty list
    assert!(tm.tasks.is_empty());
    
    // Add some tasks
    tm.add_task(vec!["First task".to_string()], None).unwrap();
    tm.add_task(vec!["Second task".to_string()], Some("2025-01-15".to_string())).unwrap();
    tm.add_task(vec!["Third task".to_string()], None).unwrap();
    
    // Mark one as done
    tm.mark_tasks(vec![2]).unwrap();
    
    // Verify tasks are properly stored
    assert_eq!(tm.tasks.len(), 3);
    assert_eq!(tm.tasks[0].text, "First task");
    assert_eq!(tm.tasks[1].text, "Second task");
    assert_eq!(tm.tasks[2].text, "Third task");
    
    // Verify status
    assert!(!tm.tasks[0].done);
    assert!(tm.tasks[1].done);
    assert!(!tm.tasks[2].done);
    
    // Verify dates
    assert_eq!(tm.tasks[0].date, None);
    assert_eq!(tm.tasks[1].date, NaiveDate::parse_from_str("2025-01-15", "%Y-%m-%d").ok());
    assert_eq!(tm.tasks[2].date, None);
}

#[test]
fn test_cli_error_handling() {
    let mut tm = TaskManager::new_empty().unwrap();
    
    // Test adding empty task
    let result = tm.add_task(vec![], None);
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("Task text cannot be empty"));
    
    // Test adding whitespace-only task
    let result = tm.add_task(vec!["   ".to_string()], None);
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("Task text cannot be empty"));
    
    // Test editing non-existent task
    let result = tm.edit_tasks(vec![255], Some(vec!["New".to_string(), "text".to_string()]), None);
    assert!(result.is_ok()); // Should succeed but not change anything
    assert_eq!(tm.tasks.len(), 0); // No tasks were added
    
    // Test marking non-existent task
    let result = tm.mark_tasks(vec![255]);
    assert!(result.is_ok()); // Should succeed but not change anything
    
    // Test deleting non-existent task
    let result = tm.delete_tasks(vec![255]);
    assert!(result.is_ok()); // Should succeed but not change anything
}

#[test]
fn test_cli_date_handling() {
    let mut tm = TaskManager::new_empty().unwrap();
    
    // Test valid dates
    let valid_dates = vec![
        "2025-01-01",
        "2025-12-31",
        "2024-02-29", // Leap year
        "2025-06-15",
    ];
    
    for (i, date) in valid_dates.iter().enumerate() {
        let result = tm.add_task(vec![format!("Task {}", i + 1)], Some(date.to_string()));
        assert!(result.is_ok());
        
        let task = &tm.tasks[i];
        let parsed_date = NaiveDate::parse_from_str(date, "%Y-%m-%d").unwrap();
        assert_eq!(task.date, Some(parsed_date));
    }
    
    // Test invalid dates
    let invalid_dates = vec![
        "2025-13-01", // Invalid month
        "2025-01-32", // Invalid day
        "2025-02-30", // Invalid day for February
        "invalid-date",
        "2025/01/01", // Wrong format
        "01-01-2025", // Wrong format
    ];
    
    for date in invalid_dates {
        let result = tm.add_task(vec!["Test task".to_string()], Some(date.to_string()));
        assert!(result.is_ok()); // Should succeed but with None date
        
        let task = tm.tasks.last().unwrap();
        assert_eq!(task.date, None);
    }
}

