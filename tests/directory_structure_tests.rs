use anyhow::Result;
use rusk::TaskManager;
use std::fs;
use tempfile::TempDir;

mod common;
use common::create_test_task;

#[test]
fn test_default_directory_structure() -> Result<()> {
    // Test mode pins the path regardless of RUSK_DB, so the process
    // environment is left alone (mutating it races with parallel tests).
    let backend = rusk::Backend::resolve()?;
    let db_path = backend.local_path().expect("a local database").to_path_buf();

    // In test mode, should use <tmp>/rusk-<uid>/debug/tasks.json (same as
    // debug mode): a directory of this user's own, closed to others
    // (REVIEW П3).
    assert!(db_path.file_name().unwrap() == "tasks.json");

    let parent = db_path.parent().unwrap();
    assert_eq!(parent.file_name().unwrap(), "debug", "{}", db_path.display());
    let own = parent.parent().unwrap();
    assert_eq!(own.file_name().unwrap(), common::private_dir_name().as_str(), "{}", db_path.display());
    assert_eq!(own.parent().unwrap(), std::env::temp_dir());
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = fs::metadata(own)?.permissions().mode() & 0o777;
        assert_eq!(mode, 0o700, "{}: {mode:o}", own.display());
    }

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
fn test_local_db_dir_function() -> Result<()> {
    // In test mode the database always lives under <tmp>/rusk-<uid>/debug.
    let db_dir = TaskManager::local_db_dir().expect("a local database");
    let expected_dir = std::env::temp_dir().join(common::private_dir_name()).join("debug");
    assert_eq!(db_dir, expected_dir);
    Ok(())
}

/// RUSK_DB is ignored in test mode. Checked on a child process so the
/// environment of this (multi-threaded) test process is never mutated. The
/// `rusk` command has a test mode in debug builds only (REVIEW №12).
#[test]
#[cfg(debug_assertions)]
fn test_rusk_db_is_ignored_in_test_mode() -> Result<()> {
    let sb = common::Sandbox::new();
    let custom_file = sb.path().join("subdir").join("tasks.json");

    let out = sb
        .cmd()
        .env("RUSK_DB", &custom_file)
        .args(["add", "pinned", "location"])
        .output()?;
    assert!(out.status.success(), "stderr={}", String::from_utf8_lossy(&out.stderr));

    assert!(!custom_file.exists(), "RUSK_DB must be ignored in test mode");
    assert!(sb.read_db().contains("pinned location"));
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
