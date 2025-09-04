use rusk::TaskManager;
use chrono::NaiveDate;
mod common;
use common::create_test_task;

#[test]
fn test_edit_tasks_unchanged_text() {
    let mut tm = TaskManager::new_empty().unwrap();
    tm.tasks = vec![
        create_test_task(1, "Same text", false),
        create_test_task(2, "Different text", false),
    ];
    
    // Try to set task 1 to the same text it already has
    let (edited, unchanged, not_found) = tm.edit_tasks(
        vec![1], 
        Some(vec!["Same".to_string(), "text".to_string()]), 
        None
    ).unwrap();
    
    assert!(edited.is_empty());
    assert_eq!(unchanged, vec![1]);
    assert!(not_found.is_empty());
    assert_eq!(tm.tasks[0].text, "Same text"); // Should remain unchanged
}

#[test]
fn test_edit_tasks_mixed_changed_unchanged() {
    let mut tm = TaskManager::new_empty().unwrap();
    tm.tasks = vec![
        create_test_task(1, "Original text", false),
        create_test_task(2, "New text", false),
        create_test_task(3, "Another text", false),
    ];
    
    // Set task 2 to same text, others to new text
    let (edited, unchanged, not_found) = tm.edit_tasks(
        vec![1, 2, 3], 
        Some(vec!["New".to_string(), "text".to_string()]), 
        None
    ).unwrap();
    
    assert_eq!(edited, vec![1, 3]); // Tasks that actually changed
    assert_eq!(unchanged, vec![2]); // Task that already had this text
    assert!(not_found.is_empty());
    
    // Verify final state
    assert_eq!(tm.tasks[0].text, "New text");
    assert_eq!(tm.tasks[1].text, "New text");
    assert_eq!(tm.tasks[2].text, "New text");
}

#[test]
fn test_edit_tasks_unchanged_date() {
    let mut tm = TaskManager::new_empty().unwrap();
    tm.tasks = vec![create_test_task(1, "Task 1", false)];
    
    // Set date first
    tm.tasks[0].date = NaiveDate::parse_from_str("2025-01-01", "%Y-%m-%d").ok();
    
    // Try to set the same date again
    let (edited, unchanged, not_found) = tm.edit_tasks(
        vec![1], 
        None,
        Some("2025-01-01".to_string())
    ).unwrap();
    
    assert!(edited.is_empty());
    assert_eq!(unchanged, vec![1]);
    assert!(not_found.is_empty());
}

#[test]
fn test_edit_tasks_mixed_text_and_date_changes() {
    let mut tm = TaskManager::new_empty().unwrap();
    tm.tasks = vec![create_test_task(1, "Same text", false)];
    
    // Set initial date
    tm.tasks[0].date = NaiveDate::parse_from_str("2025-01-01", "%Y-%m-%d").ok();
    
    // Change text but keep same date - should be considered changed
    let (edited, unchanged, not_found) = tm.edit_tasks(
        vec![1], 
        Some(vec!["New".to_string(), "text".to_string()]),
        Some("2025-01-01".to_string())
    ).unwrap();
    
    assert_eq!(edited, vec![1]); // Text changed, so task is edited
    assert!(unchanged.is_empty());
    assert!(not_found.is_empty());
    assert_eq!(tm.tasks[0].text, "New text");
}

#[test]
fn test_edit_tasks_all_unchanged() {
    let mut tm = TaskManager::new_empty().unwrap();
    tm.tasks = vec![
        create_test_task(1, "Text 1", false),
        create_test_task(2, "Text 1", false),
    ];
    
    // Try to set both to the same text they already have
    let (edited, unchanged, not_found) = tm.edit_tasks(
        vec![1, 2], 
        Some(vec!["Text".to_string(), "1".to_string()]), 
        None
    ).unwrap();
    
    assert!(edited.is_empty());
    assert_eq!(unchanged, vec![1, 2]);
    assert!(not_found.is_empty());
}

#[test]
fn test_edit_tasks_with_not_found_and_unchanged() {
    let mut tm = TaskManager::new_empty().unwrap();
    tm.tasks = vec![create_test_task(1, "Same text", false)];
    
    // Try to edit existing (unchanged) and non-existing tasks
    let (edited, unchanged, not_found) = tm.edit_tasks(
        vec![1, 99], 
        Some(vec!["Same".to_string(), "text".to_string()]), 
        None
    ).unwrap();
    
    assert!(edited.is_empty());
    assert_eq!(unchanged, vec![1]);
    assert_eq!(not_found, vec![99]);
}
