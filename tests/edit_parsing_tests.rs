use rusk::TaskManager;
mod common;
use common::create_test_task;

#[test]
fn test_edit_tasks_saves_only_when_changed() {
    let temp_dir = tempfile::tempdir().unwrap();
    let db_path = temp_dir.path().join("test_save_behavior.json");
    
    let mut tm = TaskManager::new_empty_with_path(db_path.clone());
    tm.tasks = vec![create_test_task(1, "Original text", false)];
    
    // Save initial state
    tm.save().unwrap();
    let initial_metadata = std::fs::metadata(&db_path).unwrap();
    
    // Wait a bit to ensure different modification time
    std::thread::sleep(std::time::Duration::from_millis(10));
    
    // Try to edit with same text (should not save)
    let (_edited, unchanged, _not_found) = tm.edit_tasks(
        vec![1], 
        Some(vec!["Original".to_string(), "text".to_string()]), 
        None
    ).unwrap();
    
    assert_eq!(unchanged, vec![1]);
    
    let after_unchanged_metadata = std::fs::metadata(&db_path).unwrap();
    assert_eq!(
        initial_metadata.modified().unwrap(),
        after_unchanged_metadata.modified().unwrap(),
        "File should not be modified when no changes are made"
    );
    
    // Now make a real change (should save)
    std::thread::sleep(std::time::Duration::from_millis(10));
    let (edited, _unchanged, _not_found) = tm.edit_tasks(
        vec![1], 
        Some(vec!["New".to_string(), "text".to_string()]), 
        None
    ).unwrap();
    
    assert_eq!(edited, vec![1]);
    
    let after_changed_metadata = std::fs::metadata(&db_path).unwrap();
    assert!(
        after_changed_metadata.modified().unwrap() > initial_metadata.modified().unwrap(),
        "File should be modified when changes are made"
    );
}

#[test]
fn test_edit_tasks_text_joining() {
    let mut tm = TaskManager::new_empty().unwrap();
    tm.tasks = vec![create_test_task(1, "Original", false)];
    
    // Test that multiple words are joined with spaces
    let (edited, _unchanged, _not_found) = tm.edit_tasks(
        vec![1], 
        Some(vec!["Multiple".to_string(), "word".to_string(), "text".to_string(), "here".to_string()]), 
        None
    ).unwrap();
    
    assert_eq!(edited, vec![1]);
    assert_eq!(tm.tasks[0].text, "Multiple word text here");
}

#[test]
fn test_edit_tasks_date_parsing_validation() {
    let mut tm = TaskManager::new_empty().unwrap();
    tm.tasks = vec![create_test_task(1, "Task", false)];
    
    // Valid date format
    let (_edited, _unchanged, _not_found) = tm.edit_tasks(
        vec![1], 
        None,
        Some("2025-12-31".to_string())
    ).unwrap();
    
    assert_eq!(tm.tasks[0].date, chrono::NaiveDate::parse_from_str("2025-12-31", "%Y-%m-%d").ok());
    
    // Invalid date format should result in None (parsed as None, which changes the date)
    let (edited, _unchanged, _not_found) = tm.edit_tasks(
        vec![1], 
        None,
        Some("invalid-date".to_string())
    ).unwrap();
    
    // Should change from valid date to None due to invalid parsing
    assert_eq!(edited, vec![1]); // Task was edited because date changed
    assert_eq!(tm.tasks[0].date, None);
}

#[test]
fn test_edit_tasks_comprehensive_scenario() {
    let mut tm = TaskManager::new_empty().unwrap();
    tm.tasks = vec![
        create_test_task(1, "Task 1", false),
        create_test_task(2, "Task 2", false),
        create_test_task(3, "Different text", false),
    ];
    
    // Complex edit: some changed, some unchanged, some not found
    let (edited, unchanged, not_found) = tm.edit_tasks(
        vec![1, 2, 3, 99], 
        Some(vec!["Task".to_string(), "2".to_string()]), 
        Some("2025-06-15".to_string())
    ).unwrap();
    
    // Task 1: text changes from "Task 1" to "Task 2" 
    // Task 2: text stays "Task 2", but date changes
    // Task 3: text changes from "Different text" to "Task 2"
    // Task 99: not found
    
    assert_eq!(edited, vec![1, 2, 3]); // All existing tasks have some change
    assert!(unchanged.is_empty()); // None are completely unchanged
    assert_eq!(not_found, vec![99]);
    
    // Verify final state
    assert_eq!(tm.tasks[0].text, "Task 2");
    assert_eq!(tm.tasks[1].text, "Task 2");
    assert_eq!(tm.tasks[2].text, "Task 2");
    
    let expected_date = chrono::NaiveDate::parse_from_str("2025-06-15", "%Y-%m-%d").ok();
    assert_eq!(tm.tasks[0].date, expected_date);
    assert_eq!(tm.tasks[1].date, expected_date);
    assert_eq!(tm.tasks[2].date, expected_date);
}
