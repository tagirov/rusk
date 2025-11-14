use rusk::TaskManager;
use tempfile::tempdir;

#[test]
fn test_mark_tasks_persistence() {
    let temp_dir = tempdir().unwrap();
    let db_path = temp_dir.path().join("test_mark.json");

    // Create TaskManager with custom path
    let mut tm = TaskManager::new_empty_with_path(db_path.clone());

    // Add a task
    tm.add_task(vec!["Test task".to_string()], None).unwrap();
    assert_eq!(tm.tasks().len(), 1);
    assert!(!tm.tasks()[0].done);

    // Mark the task as done
    let (_marked, not_found) = tm.mark_tasks(vec![1]).unwrap();
    assert!(not_found.is_empty());
    assert!(tm.tasks()[0].done);

    // Verify the file was saved
    assert!(db_path.exists());

    // Load from file and verify persistence
    let loaded_tasks = TaskManager::load_tasks_from_path(&db_path).unwrap();
    assert_eq!(loaded_tasks.len(), 1);
    assert_eq!(loaded_tasks[0].id, 1);
    assert_eq!(loaded_tasks[0].text, "Test task");
    assert!(loaded_tasks[0].done); // This should be true after the fix
}

#[test]
fn test_edit_tasks_persistence() {
    let temp_dir = tempdir().unwrap();
    let db_path = temp_dir.path().join("test_edit.json");

    // Create TaskManager with custom path
    let mut tm = TaskManager::new_empty_with_path(db_path.clone());

    // Add a task
    tm.add_task(vec!["Original text".to_string()], None)
        .unwrap();
    assert_eq!(tm.tasks().len(), 1);
    assert_eq!(tm.tasks()[0].text, "Original text");

    // Edit the task
    let (_edited, _unchanged, not_found) = tm
        .edit_tasks(
            vec![1],
            Some(vec!["New".to_string(), "text".to_string()]),
            None,
        )
        .unwrap();
    assert!(not_found.is_empty());
    assert_eq!(tm.tasks()[0].text, "New text");

    // Verify the file was saved
    assert!(db_path.exists());

    // Load from file and verify persistence
    let loaded_tasks = TaskManager::load_tasks_from_path(&db_path).unwrap();
    assert_eq!(loaded_tasks.len(), 1);
    assert_eq!(loaded_tasks[0].id, 1);
    assert_eq!(loaded_tasks[0].text, "New text"); // This should be "New text" after the fix
    assert!(!loaded_tasks[0].done);
}

#[test]
fn test_mark_nonexistent_task_no_save() {
    let temp_dir = tempdir().unwrap();
    let db_path = temp_dir.path().join("test_no_save.json");

    // Create TaskManager with custom path
    let mut tm = TaskManager::new_empty_with_path(db_path.clone());

    // Try to mark non-existent task
    let (_marked, not_found) = tm.mark_tasks(vec![255]).unwrap();
    assert_eq!(not_found, vec![255]);

    // File should not be created because no changes were made
    assert!(!db_path.exists());
}

#[test]
fn test_edit_nonexistent_task_no_save() {
    let temp_dir = tempdir().unwrap();
    let db_path = temp_dir.path().join("test_no_save_edit.json");

    // Create TaskManager with custom path
    let mut tm = TaskManager::new_empty_with_path(db_path.clone());

    // Try to edit non-existent task
    let (_edited, _unchanged, not_found) = tm
        .edit_tasks(vec![255], Some(vec!["New text".to_string()]), None)
        .unwrap();
    assert_eq!(not_found, vec![255]);

    // File should not be created because no changes were made
    assert!(!db_path.exists());
}
