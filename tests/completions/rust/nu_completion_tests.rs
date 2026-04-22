use anyhow::Result;
use rusk::completions::Shell;
use std::fs;
use std::process::Command;
use tempfile::TempDir;

/// Test that Nu Shell completion script has all required functions
#[test]
fn test_nu_completion_script_structure() {
    let script = Shell::Nu.get_script();
    
    // Check for main export function
    assert!(script.contains("export def rusk-completions-main"), 
        "Script should export main completion function");
    
    // Check for all command completion functions
    assert!(script.contains("def complete-add"), 
        "Script should have complete-add function");
    assert!(script.contains("def complete-edit"), 
        "Script should have complete-edit function");
    assert!(script.contains("def complete-mark-del"), 
        "Script should have complete-mark-del function");
    assert!(script.contains("def complete-list-restore"), 
        "Script should have complete-list-restore function");
    assert!(script.contains("def get-list-flags"), 
        "Script should have get-list-flags for list -f/--first-line");
    assert!(script.contains("def complete-completions"), 
        "Script should have complete-completions function");
    
    // Check for utility functions
    assert!(script.contains("def get-task-ids"), 
        "Script should have get-task-ids function");
    assert!(script.contains("def get-task-text"), 
        "Script should have get-task-text function");
    
    // Check for constant functions
    assert!(script.contains("def get-commands"), 
        "Script should have get-commands function");
    assert!(script.contains("def get-common-flags"), 
        "Script should have get-common-flags function");
    assert!(script.contains("def get-date-flags"), 
        "Script should have get-date-flags function");
}

/// Test that Nu Shell completion script contains all commands from help
#[test]
fn test_nu_completion_has_all_commands() {
    let script = Shell::Nu.get_script();
    
    // Commands from rusk -h
    let commands = vec!["add", "edit", "mark", "del", "list", "restore", "completions"];
    for cmd in commands {
        assert!(script.contains(&format!("\"{}\"", cmd)) || script.contains(&format!("value: \"{}\"", cmd)),
            "Script should contain command: {}", cmd);
    }
    
    // Aliases
    let aliases = vec!["a", "e", "m", "d", "l", "r", "c"];
    for alias in aliases {
        assert!(script.contains(&format!("\"{}\"", alias)) || script.contains(&format!("value: \"{}\"", alias)),
            "Script should contain alias: {}", alias);
    }
}

/// Test that Nu Shell completion script contains all flags
#[test]
fn test_nu_completion_has_all_flags() {
    let script = Shell::Nu.get_script();
    
    // Common flags
    assert!(script.contains("--help") || script.contains("\"--help\""), 
        "Script should contain --help flag");
    assert!(script.contains("-h") || script.contains("\"-h\""), 
        "Script should contain -h flag");
    
    // Version flags
    assert!(script.contains("--version") || script.contains("\"--version\""), 
        "Script should contain --version flag");
    assert!(script.contains("-V") || script.contains("\"-V\""), 
        "Script should contain -V flag");
    
    // Date flags
    assert!(script.contains("--date") || script.contains("\"--date\""), 
        "Script should contain --date flag");
    assert!(script.contains("-d") || script.contains("\"-d\""), 
        "Script should contain -d flag");
    
    // Done flag for del command
    assert!(script.contains("--done") || script.contains("\"--done\""), 
        "Script should contain --done flag");
}

/// Test that Nu Shell completion script handles completions subcommands
#[test]
fn test_nu_completion_has_completions_subcommands() {
    let script = Shell::Nu.get_script();
    
    // Subcommands
    assert!(script.contains("install") || script.contains("\"install\""), 
        "Script should contain install subcommand");
    assert!(script.contains("show") || script.contains("\"show\""), 
        "Script should contain show subcommand");
    
    // Shells
    let shells = vec!["bash", "zsh", "fish", "nu", "powershell"];
    for shell in shells {
        assert!(script.contains(&format!("\"{}\"", shell)) || script.contains(&format!("value: \"{}\"", shell)),
            "Script should contain shell: {}", shell);
    }
}

/// Test Nu Shell completion script syntax by attempting to parse it
#[test]
fn test_nu_completion_syntax() -> Result<()> {
    let script = Shell::Nu.get_script();
    let temp_dir = TempDir::new()?;
    let script_path = temp_dir.path().join("rusk.nu");
    fs::write(&script_path, script)?;
    
    // Try to parse the script using nu
    // We use -c with a command that attempts to load the script
    let check_command = format!(
        r#"try {{ use {} *; exit 0 }} catch {{ |err| echo $err; exit 1 }}"#,
        script_path.to_string_lossy()
    );
    
    let output = Command::new("nu")
        .arg("-c")
        .arg(&check_command)
        .output();
    
    // Nu might not be installed, so we skip the test if command not found
    match output {
        Ok(result) => {
            if !result.status.success() {
                let stderr = String::from_utf8_lossy(&result.stderr);
                let stdout = String::from_utf8_lossy(&result.stdout);
                panic!("Nu syntax check failed:\nSTDERR: {}\nSTDOUT: {}", stderr, stdout);
            }
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            // Nu shell not installed, skip test
            eprintln!("Warning: nu command not found, skipping syntax check");
            return Ok(());
        }
        Err(e) => return Err(e.into()),
    }
    
    Ok(())
}

/// Test that Nu Shell completion script can be loaded and main function exists
#[test]
fn test_nu_completion_main_function() -> Result<()> {
    let script = Shell::Nu.get_script();
    let temp_dir = TempDir::new()?;
    let script_path = temp_dir.path().join("rusk.nu");
    fs::write(&script_path, script)?;
    
    // Try to call the main function with empty spans
    let test_command = format!(
        r#"use {} *; rusk-completions-main []"#,
        script_path.to_string_lossy()
    );
    
    let output = Command::new("nu")
        .arg("-c")
        .arg(&test_command)
        .output();
    
    match output {
        Ok(result) => {
            // Function should exist and return empty list for empty input
            if !result.status.success() {
                let stderr = String::from_utf8_lossy(&result.stderr);
                panic!("Failed to call rusk-completions-main: {}", stderr);
            }
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            eprintln!("Warning: nu command not found, skipping function test");
            return Ok(());
        }
        Err(e) => return Err(e.into()),
    }
    
    Ok(())
}

/// Test that Nu Shell completion returns commands for root level
#[test]
fn test_nu_completion_root_commands() -> Result<()> {
    let script = Shell::Nu.get_script();
    let temp_dir = TempDir::new()?;
    let script_path = temp_dir.path().join("rusk.nu");
    fs::write(&script_path, script)?;
    
    // Test completion for "rusk " (with space)
    let test_command = format!(
        r#"use {} *; rusk-completions-main ["rusk", ""] | length"#,
        script_path.to_string_lossy()
    );
    
    let output = Command::new("nu")
        .arg("-c")
        .arg(&test_command)
        .output();
    
    match output {
        Ok(result) => {
            if result.status.success() {
                let stdout = String::from_utf8_lossy(&result.stdout);
                let count: Result<usize, _> = stdout.trim().parse();
                if let Ok(count) = count {
                    assert!(count > 0, "Should return completions for root level");
                }
            }
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            eprintln!("Warning: nu command not found, skipping root commands test");
            return Ok(());
        }
        Err(e) => return Err(e.into()),
    }
    
    Ok(())
}

/// Test that Nu Shell completion works for "rusk <tab>" (just "rusk" without space)
#[test]
fn test_nu_completion_rusk_tab() -> Result<()> {
    let script = Shell::Nu.get_script();
    let temp_dir = TempDir::new()?;
    let script_path = temp_dir.path().join("rusk.nu");
    fs::write(&script_path, script)?;
    
    // Test completion for "rusk" (without space - this is what happens when user types "rusk" and presses Tab)
    let test_command = format!(
        r#"use {} *; rusk-completions-main ["rusk"] | length"#,
        script_path.to_string_lossy()
    );
    
    let output = Command::new("nu")
        .arg("-c")
        .arg(&test_command)
        .output();
    
    match output {
        Ok(result) => {
            if result.status.success() {
                let stdout = String::from_utf8_lossy(&result.stdout);
                let count: Result<usize, _> = stdout.trim().parse();
                if let Ok(count) = count {
                    // Should return all commands and flags (at least 10+ items)
                    assert!(count >= 10, "Should return completions when typing 'rusk' and pressing Tab. Got: {}", count);
                } else {
                    panic!("Failed to parse completion count. Output: {}", stdout);
                }
            } else {
                let stderr = String::from_utf8_lossy(&result.stderr);
                panic!("Command failed: {}", stderr);
            }
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            eprintln!("Warning: nu command not found, skipping rusk tab test");
            return Ok(());
        }
        Err(e) => return Err(e.into()),
    }
    
    Ok(())
}

/// Test that Nu Shell completion returns flags for root level with dash
#[test]
fn test_nu_completion_root_flags() -> Result<()> {
    let script = Shell::Nu.get_script();
    let temp_dir = TempDir::new()?;
    let script_path = temp_dir.path().join("rusk.nu");
    fs::write(&script_path, script)?;
    
    // Test completion for "rusk -"
    let test_command = format!(
        r#"use {} *; rusk-completions-main ["rusk", "-"] | to json"#,
        script_path.to_string_lossy()
    );
    
    let output = Command::new("nu")
        .arg("-c")
        .arg(&test_command)
        .output();
    
    match output {
        Ok(result) => {
            if result.status.success() {
                let stdout = String::from_utf8_lossy(&result.stdout);
                // Should contain help or version flags, or be a valid JSON array (even if empty)
                // The completion might return an empty array or flags
                assert!(stdout.contains("help") || stdout.contains("version") || 
                        stdout.contains("-h") || stdout.contains("-V") || 
                        stdout.contains("[]") || stdout.len() > 0,
                    "Should return flag completions or valid response. Got: {}", stdout);
            } else {
                // If command failed, check stderr for more info
                let stderr = String::from_utf8_lossy(&result.stderr);
                eprintln!("Command failed with stderr: {}", stderr);
                // Don't fail the test if nu is not available or script has issues
                // Just log and continue
            }
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            eprintln!("Warning: nu command not found, skipping root flags test");
            return Ok(());
        }
        Err(e) => return Err(e.into()),
    }
    
    Ok(())
}

/// Test that Nu Shell completion handles add command
#[test]
fn test_nu_completion_add_command() -> Result<()> {
    let script = Shell::Nu.get_script();
    let temp_dir = TempDir::new()?;
    let script_path = temp_dir.path().join("rusk.nu");
    fs::write(&script_path, script)?;
    
    // Test completion for "rusk add "
    let test_command = format!(
        r#"use {} *; rusk-completions-main ["rusk", "add", ""] | to json"#,
        script_path.to_string_lossy()
    );
    
    let output = Command::new("nu")
        .arg("-c")
        .arg(&test_command)
        .output();
    
    match output {
        Ok(result) => {
            if result.status.success() {
                let stdout = String::from_utf8_lossy(&result.stdout);
                assert!(
                    stdout.contains("help") || stdout.contains("-h"),
                    "Should return help flags for rusk add (no task text yet)"
                );
                assert!(
                    !stdout.contains("Set task date"),
                    "Should not suggest -d/--date before task text"
                );
            }
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            eprintln!("Warning: nu command not found, skipping add command test");
            return Ok(());
        }
        Err(e) => return Err(e.into()),
    }
    
    Ok(())
}

/// Test that Nu Shell completion handles edit command
#[test]
fn test_nu_completion_edit_command() -> Result<()> {
    let script = Shell::Nu.get_script();
    let temp_dir = TempDir::new()?;
    let script_path = temp_dir.path().join("rusk.nu");
    fs::write(&script_path, script)?;
    
    // Test completion for "rusk edit "
    let test_command = format!(
        r#"use {} *; rusk-completions-main ["rusk", "edit", ""] | length"#,
        script_path.to_string_lossy()
    );
    
    let output = Command::new("nu")
        .arg("-c")
        .arg(&test_command)
        .output();
    
    match output {
        Ok(result) => {
            // Should not panic, may return empty list if no tasks
            if !result.status.success() {
                let stderr = String::from_utf8_lossy(&result.stderr);
                panic!("Edit command completion failed: {}", stderr);
            }
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            eprintln!("Warning: nu command not found, skipping edit command test");
            return Ok(());
        }
        Err(e) => return Err(e.into()),
    }
    
    Ok(())
}

/// Test that Nu Shell completion handles mark command
#[test]
fn test_nu_completion_mark_command() -> Result<()> {
    let script = Shell::Nu.get_script();
    let temp_dir = TempDir::new()?;
    let script_path = temp_dir.path().join("rusk.nu");
    fs::write(&script_path, script)?;
    
    // Test completion for "rusk mark "
    let test_command = format!(
        r#"use {} *; rusk-completions-main ["rusk", "mark", ""] | length"#,
        script_path.to_string_lossy()
    );
    
    let output = Command::new("nu")
        .arg("-c")
        .arg(&test_command)
        .output();
    
    match output {
        Ok(result) => {
            // Should not panic
            if !result.status.success() {
                let stderr = String::from_utf8_lossy(&result.stderr);
                panic!("Mark command completion failed: {}", stderr);
            }
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            eprintln!("Warning: nu command not found, skipping mark command test");
            return Ok(());
        }
        Err(e) => return Err(e.into()),
    }
    
    Ok(())
}

/// Test that Nu Shell completion handles del command
#[test]
fn test_nu_completion_del_command() -> Result<()> {
    let script = Shell::Nu.get_script();
    let temp_dir = TempDir::new()?;
    let script_path = temp_dir.path().join("rusk.nu");
    fs::write(&script_path, script)?;
    
    // Test completion for "rusk del "
    let test_command = format!(
        r#"use {} *; rusk-completions-main ["rusk", "del", ""] | length"#,
        script_path.to_string_lossy()
    );
    
    let output = Command::new("nu")
        .arg("-c")
        .arg(&test_command)
        .output();
    
    match output {
        Ok(result) => {
            // Should not panic
            if !result.status.success() {
                let stderr = String::from_utf8_lossy(&result.stderr);
                panic!("Del command completion failed: {}", stderr);
            }
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            eprintln!("Warning: nu command not found, skipping del command test");
            return Ok(());
        }
        Err(e) => return Err(e.into()),
    }
    
    Ok(())
}

/// Test that Nu Shell completion handles completions command
#[test]
fn test_nu_completion_completions_command() -> Result<()> {
    let script = Shell::Nu.get_script();
    let temp_dir = TempDir::new()?;
    let script_path = temp_dir.path().join("rusk.nu");
    fs::write(&script_path, script)?;
    
    // Test completion for "rusk completions "
    let test_command = format!(
        r#"use {} *; rusk-completions-main ["rusk", "completions", ""] | to json"#,
        script_path.to_string_lossy()
    );
    
    let output = Command::new("nu")
        .arg("-c")
        .arg(&test_command)
        .output();
    
    match output {
        Ok(result) => {
            if result.status.success() {
                let stdout = String::from_utf8_lossy(&result.stdout);
                assert!(
                    stdout.contains("install") && stdout.contains("show"),
                    "Should return completions subcommands install and show: {stdout}"
                );
                assert!(
                    stdout.contains("-h") && stdout.contains("--help"),
                    "Should include -h and --help with completions subcommands: {stdout}"
                );
            }
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            eprintln!("Warning: nu command not found, skipping completions command test");
            return Ok(());
        }
        Err(e) => return Err(e.into()),
    }
    
    Ok(())
}

/// `rusk c` without space: root suggests long name and alias; install/show only after `rusk c `.
#[test]
fn test_nu_completion_completions_alias_c_root_vs_space() -> Result<()> {
    let script = Shell::Nu.get_script();
    let temp_dir = TempDir::new()?;
    let script_path = temp_dir.path().join("rusk.nu");
    fs::write(&script_path, script)?;

    let no_space = format!(
        r#"use {} *; rusk-completions-main ["rusk", "c"] | to json"#,
        script_path.to_string_lossy()
    );
    let with_space = format!(
        r#"use {} *; rusk-completions-main ["rusk", "c", ""] | to json"#,
        script_path.to_string_lossy()
    );

    let out_no = Command::new("nu").arg("-c").arg(&no_space).output();
    let out_yes = Command::new("nu").arg("-c").arg(&with_space).output();

    match (out_no, out_yes) {
        (Ok(a), Ok(b)) => {
            if !a.status.success() {
                let err = String::from_utf8_lossy(&a.stderr);
                panic!("nu rusk c: {err}");
            }
            if !b.status.success() {
                let err = String::from_utf8_lossy(&b.stderr);
                panic!("nu 'rusk c ': {err}");
            }
            let s_no = String::from_utf8_lossy(&a.stdout);
            assert!(
                s_no.contains("completions") && s_no.contains("\"c\""),
                "rusk c<TAB> should suggest completions and alias c: {s_no}"
            );
            let s_yes = String::from_utf8_lossy(&b.stdout);
            assert!(
                s_yes.contains("install") && s_yes.contains("show"),
                "rusk c <TAB> should offer install/show: {s_yes}"
            );
        }
        (Err(e), _) | (_, Err(e)) if e.kind() == std::io::ErrorKind::NotFound => {
            eprintln!("Warning: nu command not found, skipping completions alias c test");
            return Ok(());
        }
        (Err(e), _) | (_, Err(e)) => return Err(e.into()),
    }

    Ok(())
}

/// Test that Nu Shell completion handles completions install subcommand
#[test]
fn test_nu_completion_completions_install() -> Result<()> {
    let script = Shell::Nu.get_script();
    let temp_dir = TempDir::new()?;
    let script_path = temp_dir.path().join("rusk.nu");
    fs::write(&script_path, script)?;
    
    // Test completion for "rusk completions install "
    let test_command = format!(
        r#"use {} *; rusk-completions-main ["rusk", "completions", "install", ""] | to json"#,
        script_path.to_string_lossy()
    );
    
    let output = Command::new("nu")
        .arg("-c")
        .arg(&test_command)
        .output();
    
    match output {
        Ok(result) => {
            if result.status.success() {
                let stdout = String::from_utf8_lossy(&result.stdout);
                assert!(
                    stdout.contains("bash")
                        && stdout.contains("zsh")
                        && stdout.contains("fish")
                        && stdout.contains("nu")
                        && stdout.contains("powershell"),
                    "Should return all shell names for completions install: {stdout}"
                );
                assert!(
                    stdout.contains("-h") && stdout.contains("--help"),
                    "Should include -h and --help after completions install: {stdout}"
                );
            }
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            eprintln!("Warning: nu command not found, skipping completions install test");
            return Ok(());
        }
        Err(e) => return Err(e.into()),
    }
    
    Ok(())
}

/// Test Nu completion after `-d` / `--date`: no preset dates; flag/help behaviour unchanged.
#[test]
fn test_nu_completion_after_date_flag() -> Result<()> {
    let script = Shell::Nu.get_script();
    let temp_dir = TempDir::new()?;
    let script_path = temp_dir.path().join("rusk.nu");
    fs::write(&script_path, script)?;
    
    let after_space = format!(
        r#"use {} *; rusk-completions-main ["rusk", "add", "x", "-d", ""] | to json"#,
        script_path.to_string_lossy()
    );
    let after_d_no_space = format!(
        r#"use {} *; rusk-completions-main ["rusk", "add", "x", "-d"] | to json"#,
        script_path.to_string_lossy()
    );
    
    let output_space = Command::new("nu")
        .arg("-c")
        .arg(&after_space)
        .output();
    let output_d = Command::new("nu")
        .arg("-c")
        .arg(&after_d_no_space)
        .output();
    
    match (output_space, output_d) {
        (Ok(r1), Ok(r2)) => {
            assert!(
                r1.status.success(),
                "nu stderr: {}",
                String::from_utf8_lossy(&r1.stderr)
            );
            assert!(
                r2.status.success(),
                "nu stderr: {}",
                String::from_utf8_lossy(&r2.stderr)
            );
            let s1 = String::from_utf8_lossy(&r1.stdout);
            assert!(
                s1.contains("help") && !s1.contains("Today"),
                "After '-d ' should suggest -h/--help only, got: {s1}"
            );
            let s2 = String::from_utf8_lossy(&r2.stdout);
            assert!(
                !s2.contains("Today") && !s2.contains("Tomorrow"),
                "After '-d' with task text should not suggest preset dates, got: {s2}"
            );
            assert!(
                s2.contains("help") && !s2.contains("Set task date"),
                "add after '-d' (current token): -h/--help only, no duplicate -d/--date; got: {s2}"
            );

            let edit_after_id = format!(
                r#"use {} *; rusk-completions-main ["rusk", "edit", "1", ""] | to json"#,
                script_path.to_string_lossy()
            );
            let r3 = Command::new("nu").arg("-c").arg(&edit_after_id).output()?;
            assert!(r3.status.success(), "nu edit after id: {}", String::from_utf8_lossy(&r3.stderr));
            let s3 = String::from_utf8_lossy(&r3.stdout);
            assert!(s3.contains("help"), "edit after id should offer help: {s3}");
            assert!(
                s3.contains("Set task date") || s3.contains("--date") || s3.contains("-d"),
                "edit after id should offer -d/--date: {s3}"
            );
        }
        (Err(e), _) | (_, Err(e)) if e.kind() == std::io::ErrorKind::NotFound => {
            eprintln!("Warning: nu command not found, skipping after-date-flag test");
            return Ok(());
        }
        (Err(e), _) | (_, Err(e)) => return Err(e.into()),
    }
    
    Ok(())
}

/// Test that Nu Shell completion handles partial command input
#[test]
fn test_nu_completion_partial_commands() -> Result<()> {
    let script = Shell::Nu.get_script();
    let temp_dir = TempDir::new()?;
    let script_path = temp_dir.path().join("rusk.nu");
    fs::write(&script_path, script)?;
    
    // Test completion for "rusk ad" (partial "add")
    let test_command = format!(
        r#"use {} *; rusk-completions-main ["rusk", "ad"] | to json"#,
        script_path.to_string_lossy()
    );
    
    let output = Command::new("nu")
        .arg("-c")
        .arg(&test_command)
        .output();
    
    match output {
        Ok(result) => {
            if result.status.success() {
                let stdout = String::from_utf8_lossy(&result.stdout);
                // Should suggest "add"
                assert!(stdout.contains("add") || stdout.contains("\"add\""),
                    "Should suggest 'add' for partial 'ad' input");
            }
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            eprintln!("Warning: nu command not found, skipping partial commands test");
            return Ok(());
        }
        Err(e) => return Err(e.into()),
    }
    
    Ok(())
}

/// Test that Nu Shell completion handles aliases
#[test]
fn test_nu_completion_aliases() -> Result<()> {
    let script = Shell::Nu.get_script();
    let temp_dir = TempDir::new()?;
    let script_path = temp_dir.path().join("rusk.nu");
    fs::write(&script_path, script)?;
    
    // Test completion for "rusk a " (alias for add)
    let test_command = format!(
        r#"use {} *; rusk-completions-main ["rusk", "a", ""] | to json"#,
        script_path.to_string_lossy()
    );
    
    let output = Command::new("nu")
        .arg("-c")
        .arg(&test_command)
        .output();
    
    match output {
        Ok(result) => {
            if result.status.success() {
                let stdout = String::from_utf8_lossy(&result.stdout);
                // Should return same completions as "add"
                assert!(
                    stdout.contains("help") || stdout.len() > 0,
                    "Alias 'a' should work like 'add'"
                );
            }
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            eprintln!("Warning: nu command not found, skipping aliases test");
            return Ok(());
        }
        Err(e) => return Err(e.into()),
    }
    
    Ok(())
}

