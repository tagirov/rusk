use rusk::TaskManager;
use std::fs;
use tempfile::tempdir;

#[test]
fn test_corrupted_database_error_message() {
    let temp_dir = tempdir().unwrap();
    let db_path = temp_dir.path().join("corrupted.json");
    
    // Create a corrupted JSON file
    let corrupted_json = r#"[
  {
    "id": 1,
    "text": "Task 1",
    "done": false
  }
]invalid_trailing_content"#;
    
    fs::write(&db_path, corrupted_json).unwrap();
    
    // Try to load the corrupted file
    let result = TaskManager::load_tasks_from_path(&db_path);
    
    assert!(result.is_err());
    let error_msg = result.unwrap_err().to_string();
    
    // Check that the error message contains helpful information
    assert!(error_msg.contains("Failed to parse the database file"));
    assert!(error_msg.contains("corrupted"));
    assert!(error_msg.contains("trailing characters"));
    assert!(error_msg.contains("To fix this issue"));
    assert!(error_msg.contains("Delete the corrupted file"));
}

#[test]
fn test_empty_database_file() {
    let temp_dir = tempdir().unwrap();
    let db_path = temp_dir.path().join("empty.json");
    
    // Create empty file
    fs::write(&db_path, "").unwrap();
    
    let result = TaskManager::load_tasks_from_path(&db_path);
    
    assert!(result.is_err());
    let error_msg = result.unwrap_err().to_string();
    assert!(error_msg.contains("Failed to parse the database file"));
}

#[test]
fn test_invalid_json_structure() {
    let temp_dir = tempdir().unwrap();
    let db_path = temp_dir.path().join("invalid_structure.json");
    
    // Create JSON with wrong structure (object instead of array)
    let invalid_json = r#"{
  "tasks": [
    {
      "id": 1,
      "text": "Task 1",
      "done": false
    }
  ]
}"#;
    
    fs::write(&db_path, invalid_json).unwrap();
    
    let result = TaskManager::load_tasks_from_path(&db_path);
    
    assert!(result.is_err());
    let error_msg = result.unwrap_err().to_string();
    assert!(error_msg.contains("Failed to parse the database file"));
}

#[test]
fn test_backup_creation_on_save() {
    let temp_dir = tempdir().unwrap();
    let db_path = temp_dir.path().join("test_backup.json");
    let backup_path = db_path.with_extension("json.backup");
    
    let mut tm = TaskManager::new_empty_with_path(db_path.clone());
    
    // Add initial task and save
    tm.add_task(vec!["Initial task".to_string()], None).unwrap();
    assert!(db_path.exists());
    
    // Add another task (should create backup)
    tm.add_task(vec!["Second task".to_string()], None).unwrap();
    
    // Check that backup was created
    assert!(backup_path.exists());
    
    // Check that backup contains the previous state
    let backup_tasks = TaskManager::load_tasks_from_path(&backup_path).unwrap();
    assert_eq!(backup_tasks.len(), 1);
    assert_eq!(backup_tasks[0].text, "Initial task");
    
    // Check that current file contains both tasks
    let current_tasks = TaskManager::load_tasks_from_path(&db_path).unwrap();
    assert_eq!(current_tasks.len(), 2);
    assert_eq!(current_tasks[0].text, "Initial task");
    assert_eq!(current_tasks[1].text, "Second task");
}

#[test]
fn test_nonexistent_file_returns_empty() {
    let temp_dir = tempdir().unwrap();
    let nonexistent_path = temp_dir.path().join("nonexistent.json");
    
    let result = TaskManager::load_tasks_from_path(&nonexistent_path);
    
    assert!(result.is_ok());
    let tasks = result.unwrap();
    assert!(tasks.is_empty());
}
