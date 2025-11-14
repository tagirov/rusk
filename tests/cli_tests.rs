use chrono::NaiveDate;
use rusk::TaskManager;

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
    let result = tm.add_task(vec!["meeting".to_string()], Some("15-01-2025".to_string()));
    assert!(result.is_ok());
    assert_eq!(tm.tasks.len(), 3);
    assert_eq!(tm.tasks[2].text, "meeting");
    assert_eq!(
        tm.tasks[2].date,
        NaiveDate::parse_from_str("15-01-2025", "%d-%m-%Y").ok()
    );
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
    assert!(tm.tasks[1].done); // Task 2
    assert!(!tm.tasks[2].done); // Task 3
    assert!(tm.tasks[3].done); // Task 4

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
fn test_cli_delete_with_done_flag() {
    // Test that --done flag logic works correctly
    // This tests the core functionality without interactive confirmation
    let mut tm = TaskManager::new_empty().unwrap();

    // Add tasks
    tm.add_task(vec!["Task 1".to_string()], None).unwrap();
    tm.add_task(vec!["Task 2".to_string()], None).unwrap();
    tm.add_task(vec!["Task 3".to_string()], None).unwrap();

    // Mark tasks 1 and 3 as done
    tm.mark_tasks(vec![1, 3]).unwrap();

    // Verify initial state
    assert_eq!(tm.tasks.len(), 3);
    assert!(tm.tasks[0].done); // Task 1
    assert!(!tm.tasks[1].done); // Task 2
    assert!(tm.tasks[2].done); // Task 3

    // Test deleting all done tasks (simulating --done flag behavior)
    // This directly tests delete_all_done which is what --done flag calls
    let deleted_count = tm.delete_all_done().unwrap();
    assert_eq!(deleted_count, 2); // Should delete 2 tasks
    assert_eq!(tm.tasks.len(), 1); // Only Task 2 should remain
    assert_eq!(tm.tasks[0].id, 2);
    assert!(!tm.tasks[0].done);
}

#[test]
fn test_cli_delete_error_no_ids_no_done() {
    use rusk::cli::HandlerCLI;
    let mut tm = TaskManager::new_empty().unwrap();

    // Add a task
    tm.add_task(vec!["Task 1".to_string()], None).unwrap();

    // Test error message when neither IDs nor --done flag are provided
    // This should print error message but not fail
    let result = HandlerCLI::handle_delete_tasks(&mut tm, vec![], false);
    assert!(result.is_ok()); // Function succeeds but prints error message
    assert_eq!(tm.tasks.len(), 1); // Task should remain
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
    assert!(tm.tasks[1].done); // Task 2 was false, now true
    assert!(tm.tasks[2].done); // Task 3 was false, now true

    // Test marking already done task (should toggle to undone)
    let result = tm.mark_tasks(vec![1]);
    assert!(result.is_ok());
    assert!(!tm.tasks[0].done); // Task 1 was true, now false
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
    let result = tm.edit_tasks(
        vec![1],
        Some(vec!["Updated".to_string(), "text".to_string()]),
        None,
    );
    assert!(result.is_ok());
    assert_eq!(tm.tasks[0].text, "Updated text");
    assert_eq!(tm.tasks[1].text, "Original task 2"); // Unchanged

    // Test editing date only
    let result = tm.edit_tasks(vec![2], None, Some("15-06-2025".to_string()));
    assert!(result.is_ok());
    assert_eq!(tm.tasks[0].date, None); // Unchanged
    assert_eq!(
        tm.tasks[1].date,
        NaiveDate::parse_from_str("15-06-2025", "%d-%m-%Y").ok()
    );

    // Test editing both text and date
    let result = tm.edit_tasks(
        vec![1],
        Some(vec!["Final".to_string(), "version".to_string()]),
        Some("31-12-2025".to_string()),
    );
    assert!(result.is_ok());
    assert_eq!(tm.tasks[0].text, "Final version");
    assert_eq!(
        tm.tasks[0].date,
        NaiveDate::parse_from_str("31-12-2025", "%d-%m-%Y").ok()
    );
}

#[test]
fn test_cli_list_command() {
    let mut tm = TaskManager::new_empty().unwrap();

    // Test empty list
    assert!(tm.tasks.is_empty());

    // Add some tasks
    tm.add_task(vec!["First task".to_string()], None).unwrap();
    tm.add_task(
        vec!["Second task".to_string()],
        Some("15-01-2025".to_string()),
    )
    .unwrap();
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
    assert_eq!(
        tm.tasks[1].date,
        NaiveDate::parse_from_str("15-01-2025", "%d-%m-%Y").ok()
    );
    assert_eq!(tm.tasks[2].date, None);
}

#[test]
fn test_cli_error_handling() {
    let mut tm = TaskManager::new_empty().unwrap();

    // Test adding empty task
    let result = tm.add_task(vec![], None);
    assert!(result.is_err());
    assert!(
        result
            .unwrap_err()
            .to_string()
            .contains("Task text cannot be empty")
    );

    // Test adding whitespace-only task
    let result = tm.add_task(vec!["   ".to_string()], None);
    assert!(result.is_err());
    assert!(
        result
            .unwrap_err()
            .to_string()
            .contains("Task text cannot be empty")
    );

    // Test editing non-existent task
    let result = tm.edit_tasks(
        vec![255],
        Some(vec!["New".to_string(), "text".to_string()]),
        None,
    );
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
    let valid_dates = [
        "01-01-2025",
        "31-12-2025",
        "29-02-2024", // Leap year
        "15-06-2025",
    ];

    for (i, date) in valid_dates.iter().enumerate() {
        let result = tm.add_task(vec![format!("Task {}", i + 1)], Some(date.to_string()));
        assert!(result.is_ok());

        let task = &tm.tasks[i];
        let parsed_date = NaiveDate::parse_from_str(date, "%d-%m-%Y").unwrap();
        assert_eq!(task.date, Some(parsed_date));
    }

    // Test invalid dates
    let invalid_dates = vec![
        "01-13-2025", // Invalid month
        "32-01-2025", // Invalid day
        "30-02-2025", // Invalid day for February
        "invalid-date",
        "01/01/2025", // Wrong format
        "2025-01-01", // Wrong format (old YYYY-MM-DD)
    ];

    for date in invalid_dates {
        let result = tm.add_task(vec!["Test task".to_string()], Some(date.to_string()));
        assert!(result.is_ok()); // Should succeed but with None date

        let task = tm.tasks.last().unwrap();
        assert_eq!(task.date, None);
    }
}
