use anyhow::Result;
use rusk::TaskManager;
use std::env;
use std::fs;
use std::sync::Mutex;
use tempfile::TempDir;

mod common;
use common::create_test_task;

// Mutex to ensure environment tests don't run in parallel
static ENV_TEST_MUTEX: Mutex<()> = Mutex::new(());

#[test]
fn test_rusk_db_as_directory() -> Result<()> {
    let _guard = ENV_TEST_MUTEX.lock().unwrap();

    // Save current environment state
    let original_rusk_db = env::var("RUSK_DB").ok();

    let temp_dir = TempDir::new()?;
    let custom_dir = temp_dir.path().join("custom_rusk");
    fs::create_dir_all(&custom_dir)?;

    // Set RUSK_DB to directory (with trailing slash)
    let custom_dir_str = format!("{}/", custom_dir.display());
    unsafe {
        env::set_var("RUSK_DB", &custom_dir_str);
    }

    // Get database path - in test mode, RUSK_DB is ignored, should use /tmp/rusk_debug/tasks.json
    let db_path = TaskManager::resolve_db_path();
    let expected_path = std::env::temp_dir().join("rusk_debug").join("tasks.json");

    assert_eq!(db_path, expected_path);

    // Test that TaskManager can use this path
    let mut tm = TaskManager::new_for_restore()?;
    tm.db_path = db_path.clone(); // Use the resolved path
    tm.tasks.push(create_test_task(1, "Test task", false));
    tm.save()?;

    // Verify file was created in custom directory
    assert!(expected_path.exists());

    // Restore original environment state
    unsafe {
        match original_rusk_db {
            Some(value) => env::set_var("RUSK_DB", value),
            None => env::remove_var("RUSK_DB"),
        }
    }

    Ok(())
}

#[test]
fn test_rusk_db_as_file() -> Result<()> {
    let _guard = ENV_TEST_MUTEX.lock().unwrap();

    // Save current environment state
    let original_rusk_db = env::var("RUSK_DB").ok();

    let temp_dir = TempDir::new()?;
    let custom_file = temp_dir.path().join("my_tasks.json");

    // Set RUSK_DB to specific file
    unsafe {
        env::set_var("RUSK_DB", custom_file.to_str().unwrap());
    }

    // Get database path - in test mode, RUSK_DB is ignored, should use /tmp/rusk_debug/tasks.json
    let db_path = TaskManager::resolve_db_path();
    let expected_path = std::env::temp_dir().join("rusk_debug").join("tasks.json");

    assert_eq!(db_path, expected_path);

    // Test that TaskManager can use this path
    let mut tm = TaskManager::new_for_restore()?;
    tm.db_path = db_path.clone(); // Use the resolved path
    tm.tasks
        .push(create_test_task(1, "Custom file task", false));
    tm.save()?;

    // Verify file was created in test mode path (/tmp/rusk_debug/tasks.json)
    assert!(expected_path.exists());

    // Verify backup is created with correct extension
    let backup_path = expected_path.with_extension("json.backup");
    tm.tasks.push(create_test_task(2, "Another task", false));
    tm.save()?;
    assert!(backup_path.exists());

    // Restore original environment state
    unsafe {
        match original_rusk_db {
            Some(value) => env::set_var("RUSK_DB", value),
            None => env::remove_var("RUSK_DB"),
        }
    }

    Ok(())
}

#[test]
fn test_rusk_db_default_path() -> Result<()> {
    let _guard = ENV_TEST_MUTEX.lock().unwrap();

    // Save current environment state
    let original_rusk_db = env::var("RUSK_DB").ok();

    // Ensure RUSK_DB is not set
    unsafe {
        env::remove_var("RUSK_DB");
    }

    let db_path = TaskManager::resolve_db_path();

    // In test mode, should use /tmp/rusk_debug/tasks.json (same as debug mode)
    assert!(db_path.to_string_lossy().contains("rusk_debug"));
    assert!(db_path.to_string_lossy().ends_with("tasks.json"));

    // Should be in a "rusk_debug" subdirectory
    let parent = db_path.parent().unwrap();
    assert!(parent.file_name().unwrap() == "rusk_debug");

    // Restore original environment state
    unsafe {
        match original_rusk_db {
            Some(value) => env::set_var("RUSK_DB", value),
            None => env::remove_var("RUSK_DB"),
        }
    }

    Ok(())
}

#[test]
fn test_resolve_db_path_integration_harness_uses_temp() -> Result<()> {
    let _guard = ENV_TEST_MUTEX.lock().unwrap();

    // Save current env and simulate integration test harness
    let original_rusk_db = env::var("RUSK_DB").ok();
    let original_threads = env::var("RUST_TEST_THREADS").ok();

    unsafe {
        env::remove_var("RUSK_DB");
        env::set_var("RUST_TEST_THREADS", "1");
    }

    let db_path = TaskManager::resolve_db_path();
    let path_str = db_path.to_string_lossy();
    assert!(
        path_str.contains("rusk_debug"),
        "expected temp debug path, got {path_str}"
    );
    assert!(path_str.ends_with("tasks.json"));

    // Cleanup vars
    unsafe {
        match original_rusk_db {
            Some(v) => env::set_var("RUSK_DB", v),
            None => env::remove_var("RUSK_DB"),
        }
        match original_threads {
            Some(v) => env::set_var("RUST_TEST_THREADS", v),
            None => env::remove_var("RUST_TEST_THREADS"),
        }
    }

    Ok(())
}

#[test]
fn test_get_db_dir() -> Result<()> {
    let _guard = ENV_TEST_MUTEX.lock().unwrap();

    // Save current environment state
    let original_rusk_db = env::var("RUSK_DB").ok();

    let temp_dir = TempDir::new()?;
    let subdir = temp_dir.path().join("subdir");
    fs::create_dir_all(&subdir)?;
    let custom_file = subdir.join("tasks.json");

    // Set RUSK_DB to file in subdirectory
    unsafe {
        env::set_var("RUSK_DB", custom_file.to_str().unwrap());
    }

    // In test mode, RUSK_DB is ignored, should use /tmp/rusk_debug/tasks.json
    let db_dir = TaskManager::get_db_dir();
    let expected_dir = std::env::temp_dir().join("rusk_debug");

    assert_eq!(db_dir, expected_dir);

    // Restore original environment state
    unsafe {
        match original_rusk_db {
            Some(value) => env::set_var("RUSK_DB", value),
            None => env::remove_var("RUSK_DB"),
        }
    }

    Ok(())
}

#[test]
fn test_rusk_db_with_backup_and_restore() -> Result<()> {
    let _guard = ENV_TEST_MUTEX.lock().unwrap();

    // Save current environment state
    let original_rusk_db = env::var("RUSK_DB").ok();

    let temp_dir = TempDir::new()?;
    let custom_file = temp_dir.path().join("custom_backup_test.json");

    unsafe {
        env::set_var("RUSK_DB", custom_file.to_str().unwrap());
    }

    // Create TaskManager and add tasks
    let mut tm = TaskManager::new_for_restore()?;
    tm.db_path = custom_file.clone(); // Use the custom file path
    tm.tasks.push(create_test_task(1, "Task 1", false));
    tm.save()?;

    // Add more tasks to create backup
    tm.tasks.push(create_test_task(2, "Task 2", false));
    tm.save()?;

    // Verify backup was created with custom path
    let backup_path = custom_file.with_extension("json.backup");
    assert!(backup_path.exists());

    // Remove main file and restore
    fs::remove_file(&custom_file)?;
    tm.restore_from_backup()?;

    // Verify restore worked
    assert_eq!(tm.tasks.len(), 1);
    assert_eq!(tm.tasks[0].text, "Task 1");

    // Restore original environment state
    unsafe {
        match original_rusk_db {
            Some(value) => env::set_var("RUSK_DB", value),
            None => env::remove_var("RUSK_DB"),
        }
    }

    Ok(())
}
