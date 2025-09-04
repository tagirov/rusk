use anyhow::Result;
use rusk::TaskManager;
use std::fs;
use tempfile::TempDir;

mod common;
use common::{create_test_task, create_test_task_with_date};

#[test]
fn test_restore_from_backup() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let db_path = temp_dir.path().join("test.json");
    
    // Create initial TaskManager with some tasks
    let mut tm = TaskManager::new_empty()?;
    tm.db_path = db_path.clone();
    tm.tasks.push(create_test_task(1, "Original task 1", false));
    tm.tasks.push(create_test_task_with_date(2, "Original task 2", false, "2025-01-15"));
    tm.save()?;
    
    // Modify tasks and save (this creates a backup)
    tm.tasks[0].text = "Modified task 1".to_string();
    tm.tasks.push(create_test_task(3, "New task 3", false));
    tm.save()?;
    
    // Verify current state
    assert_eq!(tm.tasks.len(), 3);
    assert_eq!(tm.tasks[0].text, "Modified task 1");
    
    // Restore from backup
    tm.restore_from_backup()?;
    
    // Verify restored state
    assert_eq!(tm.tasks.len(), 2);
    assert_eq!(tm.tasks[0].text, "Original task 1");
    assert_eq!(tm.tasks[1].text, "Original task 2");
    assert_eq!(tm.tasks[1].date.as_ref().unwrap().to_string(), "2025-01-15");
    
    Ok(())
}

#[test]
fn test_restore_no_backup_file() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let db_path = temp_dir.path().join("test.json");
    
    let mut tm = TaskManager::new_empty()?;
    tm.db_path = db_path.clone();
    
    // Try to restore without backup file
    let result = tm.restore_from_backup();
    
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("No backup file found"));
    
    Ok(())
}

#[test]
fn test_restore_corrupted_backup() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let db_path = temp_dir.path().join("test.json");
    let backup_path = temp_dir.path().join("test.json.backup");
    
    let mut tm = TaskManager::new_empty()?;
    tm.db_path = db_path.clone();
    
    // Create corrupted backup file
    fs::write(&backup_path, "invalid json content")?;
    
    // Try to restore from corrupted backup
    let result = tm.restore_from_backup();
    
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("Failed to parse"));
    
    Ok(())
}

#[test]
fn test_restore_creates_before_restore_backup() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let db_path = temp_dir.path().join("test.json");
    let backup_path = temp_dir.path().join("test.json.backup");
    let before_restore_path = temp_dir.path().join("test.json.before_restore");
    
    // Create TaskManager with current data
    let mut tm = TaskManager::new_empty()?;
    tm.db_path = db_path.clone();
    tm.tasks.push(create_test_task(1, "Current task", false));
    tm.save()?;
    
    // Create backup with different data
    let backup_tasks = vec![create_test_task(2, "Backup task", false)];
    let backup_json = serde_json::to_string_pretty(&backup_tasks)?;
    fs::write(&backup_path, backup_json)?;
    
    // Restore from backup
    tm.restore_from_backup()?;
    
    // Verify that before_restore backup was created
    assert!(before_restore_path.exists());
    
    // Verify before_restore backup contains original data
    let before_restore_data = fs::read_to_string(&before_restore_path)?;
    assert!(before_restore_data.contains("Current task"));
    
    // Verify current data is from backup
    assert_eq!(tm.tasks.len(), 1);
    assert_eq!(tm.tasks[0].text, "Backup task");
    
    Ok(())
}

#[test]
fn test_restore_with_corrupted_current_database() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let db_path = temp_dir.path().join("test.json");
    let backup_path = temp_dir.path().join("test.json.backup");
    
    let mut tm = TaskManager::new_empty()?;
    tm.db_path = db_path.clone();
    
    // Create valid backup
    let backup_tasks = vec![create_test_task(1, "Backup task", false)];
    let backup_json = serde_json::to_string_pretty(&backup_tasks)?;
    fs::write(&backup_path, backup_json)?;
    
    // Create corrupted current database
    fs::write(&db_path, "corrupted data")?;
    
    // Restore should work despite corrupted current database
    tm.restore_from_backup()?;
    
    // Verify restored data
    assert_eq!(tm.tasks.len(), 1);
    assert_eq!(tm.tasks[0].text, "Backup task");
    
    Ok(())
}

#[test]
fn test_restore_empty_backup() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let db_path = temp_dir.path().join("test.json");
    let backup_path = temp_dir.path().join("test.json.backup");
    
    let mut tm = TaskManager::new_empty()?;
    tm.db_path = db_path.clone();
    tm.tasks.push(create_test_task(1, "Current task", false));
    tm.save()?;
    
    // Create empty backup
    let empty_backup = "[]";
    fs::write(&backup_path, empty_backup)?;
    
    // Restore from empty backup
    tm.restore_from_backup()?;
    
    // Verify all tasks were cleared
    assert_eq!(tm.tasks.len(), 0);
    
    Ok(())
}
