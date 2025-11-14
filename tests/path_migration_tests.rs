use anyhow::Result;
use rusk::TaskManager;
use std::env;
use std::fs;
use tempfile::TempDir;

mod common;
use common::create_test_task;

#[test]
fn test_default_path_structure() -> Result<()> {
    // Save original environment
    let original_rusk_db = env::var("RUSK_DB").ok();

    // Ensure RUSK_DB is not set to test default behavior
    unsafe {
        env::remove_var("RUSK_DB");
    }

    let db_path = TaskManager::resolve_db_path();

    // In test mode, should use /tmp/rusk_debug/tasks.json (same as debug mode)
    assert!(db_path.to_string_lossy().contains("rusk_debug"));
    assert!(db_path.file_name().unwrap() == "tasks.json");

    let parent = db_path.parent().unwrap();
    assert!(parent.file_name().unwrap() == "rusk_debug");

    // Restore original environment
    unsafe {
        match original_rusk_db {
            Some(value) => env::set_var("RUSK_DB", value),
            None => env::remove_var("RUSK_DB"),
        }
    }

    Ok(())
}

#[test]
fn test_backup_files_naming_convention() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let rusk_dir = temp_dir.path().join(".rusk");
    fs::create_dir_all(&rusk_dir)?;

    let db_path = rusk_dir.join("tasks.json");

    let mut tm = TaskManager::new_empty()?;
    tm.db_path = db_path.clone();
    tm.tasks.push(create_test_task(1, "Test task", false));

    // First save creates the file
    tm.save()?;

    // Second save should create backup (since file now exists)
    tm.tasks.push(create_test_task(2, "Second task", false));
    tm.save()?;

    // Verify backup file naming
    let backup_path = rusk_dir.join("tasks.json.backup");
    assert!(backup_path.exists());

    // Test restore creates before_restore backup
    tm.restore_from_backup()?;
    let before_restore_path = rusk_dir.join("tasks.json.before_restore");
    assert!(before_restore_path.exists());

    Ok(())
}

#[test]
fn test_rusk_db_directory_with_tasks_json() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let custom_dir = temp_dir.path().join("custom");
    fs::create_dir_all(&custom_dir)?;

    // Save original environment
    let original_rusk_db = env::var("RUSK_DB").ok();

    // Set RUSK_DB to a directory - should create tasks.json inside
    unsafe {
        env::set_var("RUSK_DB", custom_dir.to_str().unwrap());
    }

    // In test mode, RUSK_DB is ignored, should use /tmp/rusk_debug/tasks.json
    let db_path = TaskManager::resolve_db_path();
    let expected_path = std::env::temp_dir().join("rusk_debug").join("tasks.json");

    assert_eq!(db_path, expected_path);
    assert!(db_path.file_name().unwrap() == "tasks.json");

    // Test that it actually works
    let mut tm = TaskManager::new_empty()?;
    tm.db_path = db_path.clone();
    tm.tasks.push(create_test_task(1, "Custom dir task", false));
    tm.save()?;

    assert!(expected_path.exists());

    // Restore original environment
    unsafe {
        match original_rusk_db {
            Some(value) => env::set_var("RUSK_DB", value),
            None => env::remove_var("RUSK_DB"),
        }
    }

    Ok(())
}

#[test]
fn test_nested_rusk_directory_creation() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let deep_path = temp_dir.path().join("level1").join("level2").join(".rusk");
    let db_path = deep_path.join("tasks.json");

    // Directory doesn't exist yet
    assert!(!deep_path.exists());

    let mut tm = TaskManager::new_empty()?;
    tm.db_path = db_path.clone();
    tm.tasks.push(create_test_task(1, "Deep path task", false));

    // First save creates all necessary directories and file
    tm.save()?;

    // Second save creates backup
    tm.tasks.push(create_test_task(2, "Another task", false));
    tm.save()?;

    assert!(deep_path.exists());
    assert!(db_path.exists());
    assert!(deep_path.join("tasks.json.backup").exists());

    Ok(())
}

#[test]
fn test_file_extension_consistency() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let rusk_dir = temp_dir.path().join(".rusk");
    fs::create_dir_all(&rusk_dir)?;

    let db_path = rusk_dir.join("tasks.json");

    let mut tm = TaskManager::new_empty()?;
    tm.db_path = db_path.clone();
    tm.tasks.push(create_test_task(1, "Extension test", false));

    // First save creates the file
    tm.save()?;

    // Second save creates backup
    tm.tasks.push(create_test_task(2, "Another task", false));
    tm.save()?;

    // Verify all files have consistent naming
    assert!(db_path.exists());
    assert_eq!(db_path.extension().unwrap(), "json");

    let backup_path = rusk_dir.join("tasks.json.backup");
    assert!(backup_path.exists());
    // backup file should have compound extension
    assert!(backup_path.to_string_lossy().ends_with(".json.backup"));

    Ok(())
}
