use anyhow::Result;
use rusk::TaskManager;
use std::env;
use std::fs;
use tempfile::TempDir;

mod common;
use common::create_test_task;

#[test]
fn test_default_directory_structure() -> Result<()> {
    // Remove RUSK_DB to ensure we're testing default behavior
    unsafe {
        env::remove_var("RUSK_DB");
    }

    let db_path = TaskManager::resolve_db_path();

    // In test mode, should use /tmp/rusk_debug/tasks.json (same as debug mode)
    assert!(db_path.file_name().unwrap() == "tasks.json");
    
    // Parent directory should be "rusk_debug" (from /tmp/rusk_debug/tasks.json)
    let parent = db_path.parent().unwrap();
    let parent_name = parent.file_name().unwrap().to_string_lossy();
    assert_eq!(parent_name, "rusk_debug", "Expected parent directory to be 'rusk_debug', got '{parent_name}'");

    Ok(())
}

#[test]
fn test_directory_creation_on_save() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let rusk_dir = temp_dir.path().join("rusk");
    let db_path = rusk_dir.join("tasks.json");

    // Ensure directory doesn't exist initially
    assert!(!rusk_dir.exists());

    // Create TaskManager with path in non-existent directory
    let mut tm = TaskManager::new_empty_with_path(db_path.clone());
    tm.tasks.push(create_test_task(1, "Test task", false));

    // Save should create the directory
    tm.save()?;

    // Verify directory and file were created
    assert!(rusk_dir.exists());
    assert!(rusk_dir.is_dir());
    assert!(db_path.exists());
    assert!(db_path.is_file());

    Ok(())
}

#[test]
fn test_backup_files_in_same_directory() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let rusk_dir = temp_dir.path().join("rusk");
    let db_path = rusk_dir.join("tasks.json");
    let backup_path = rusk_dir.join("tasks.json.backup");

    // Create TaskManager with custom path
    let mut tm = TaskManager::new_empty_with_path(db_path.clone());
    tm.tasks.push(create_test_task(1, "First task", false));
    tm.save()?;

    // Add another task to trigger backup creation
    tm.tasks.push(create_test_task(2, "Second task", false));
    tm.save()?;

    // Verify backup was created in same directory
    assert!(backup_path.exists());
    assert!(backup_path.is_file());

    // Verify backup is in the same directory as main file
    assert_eq!(backup_path.parent(), db_path.parent());

    Ok(())
}

#[test]
fn test_nested_directory_structure() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let nested_path = temp_dir
        .path()
        .join("level1")
        .join("level2")
        .join("level3")
        .join("deep_tasks.json");

    // Create TaskManager with deeply nested path
    let mut tm = TaskManager::new_empty_with_path(nested_path.clone());
    tm.tasks.push(create_test_task(1, "Deep task", false));

    // Save should create all necessary directories
    tm.save()?;

    // Verify all directories were created
    assert!(nested_path.exists());
    assert!(nested_path.parent().unwrap().exists());
    assert!(nested_path.parent().unwrap().parent().unwrap().exists());
    assert!(
        nested_path
            .parent()
            .unwrap()
            .parent()
            .unwrap()
            .parent()
            .unwrap()
            .exists()
    );

    Ok(())
}

#[test]
fn test_restore_files_in_custom_directory() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let custom_dir = temp_dir.path().join("custom_rusk_dir");
    let db_path = custom_dir.join("custom.json");
    let backup_path = custom_dir.join("custom.json.backup");
    let before_restore_path = custom_dir.join("custom.json.before_restore");

    // Create TaskManager with custom directory
    let mut tm = TaskManager::new_empty_with_path(db_path.clone());
    tm.tasks.push(create_test_task(1, "Original task", false));
    tm.save()?;

    // Modify and save to create backup
    tm.tasks[0].text = "Modified task".to_string();
    tm.save()?;

    // Restore from backup
    tm.restore_from_backup()?;

    // Verify all restore-related files are in custom directory
    assert!(backup_path.exists());
    assert!(before_restore_path.exists());
    assert_eq!(backup_path.parent(), Some(custom_dir.as_path()));
    assert_eq!(before_restore_path.parent(), Some(custom_dir.as_path()));

    // Verify restoration worked
    assert_eq!(tm.tasks[0].text, "Original task");

    Ok(())
}

#[test]
fn test_get_db_dir_function() -> Result<()> {
    let temp_dir = TempDir::new()?;
    
    // In test mode, RUSK_DB is ignored, so it should always use /tmp/rusk_debug/tasks.json
    // Even if RUSK_DB is set, it will be ignored
    unsafe {
        let custom_file = temp_dir.path().join("subdir").join("tasks.json");
        env::set_var("RUSK_DB", custom_file.to_str().unwrap());
    }

    let db_dir = TaskManager::get_db_dir();
    let expected_dir = std::env::temp_dir().join("rusk_debug");

    assert_eq!(db_dir, expected_dir);

    // Cleanup
    unsafe {
        env::remove_var("RUSK_DB");
    }

    Ok(())
}

#[test]
fn test_directory_permissions() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let rusk_dir = temp_dir.path().join("rusk");
    let db_path = rusk_dir.join("tasks.json");

    // Create TaskManager
    let mut tm = TaskManager::new_empty_with_path(db_path.clone());
    tm.tasks.push(create_test_task(1, "Permission test", false));

    // Save should create directory with proper permissions
    tm.save()?;

    // Verify directory was created and is readable/writable
    assert!(rusk_dir.exists());
    assert!(rusk_dir.is_dir());

    // Verify we can create additional files in the directory
    let test_file = rusk_dir.join("test.txt");
    fs::write(&test_file, "test")?;
    assert!(test_file.exists());

    Ok(())
}
