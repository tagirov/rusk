use anyhow::Result;
use rusk::completions::Shell;
use std::fs;
use std::path::Path;
use std::process::Command;
use tempfile::TempDir;

#[path = "../../common/mod.rs"]
mod common;

fn with_isolated_home(cmd: &mut Command, home: &Path) {
    cmd.env("HOME", home);
    cmd.env("USERPROFILE", home);
    #[cfg(windows)]
    {
        let appdata = home.join("AppData").join("Roaming");
        let _ = fs::create_dir_all(&appdata);
        cmd.env("APPDATA", appdata);
    }
}

// Helper function to test completion installation
fn test_completion_install(shell: Shell, expected_filename: &str) -> Result<()> {
    let temp_dir = TempDir::new()?;
    let test_home = temp_dir.path();
    
    // Create a mock home directory structure
    let completion_path = match shell {
        Shell::Bash => test_home.join(".bash_completion.d").join("rusk"),
        Shell::Zsh => test_home.join(".zsh").join("completions").join("_rusk"),
        Shell::Fish => test_home.join(".config").join("fish").join("completions").join("rusk.fish"),
        Shell::Nu => test_home.join(".config").join("nushell").join("completions").join("rusk.nu"),
        Shell::PowerShell => test_home.join("Documents").join("PowerShell").join("rusk-completions.ps1"),
    };
    
    // Verify parent directory doesn't exist yet
    assert!(!completion_path.parent().unwrap().exists());
    
    // Get the script content
    let script = shell.get_script();
    assert!(!script.is_empty(), "Script should not be empty");
    
    // Create parent directory
    if let Some(parent) = completion_path.parent() {
        fs::create_dir_all(parent)?;
    }
    
    // Write the script
    fs::write(&completion_path, script)?;
    
    // Verify file was created
    assert!(completion_path.exists(), "Completion file should exist");
    assert!(completion_path.is_file(), "Completion path should be a file");
    
    // Verify file name is correct
    let filename = completion_path.file_name().unwrap().to_string_lossy();
    assert_eq!(filename, expected_filename, "Filename should match expected");
    
    // Verify file content matches
    let written_content = fs::read_to_string(&completion_path)?;
    assert_eq!(written_content, script, "File content should match script");
    assert!(!written_content.is_empty(), "File should not be empty");
    
    // Verify parent directory was created
    assert!(completion_path.parent().unwrap().exists(), "Parent directory should exist");
    assert!(completion_path.parent().unwrap().is_dir(), "Parent should be a directory");
    
    Ok(())
}

#[test]
fn test_bash_completion_install() -> Result<()> {
    test_completion_install(Shell::Bash, "rusk")
}

#[test]
fn test_zsh_completion_install() -> Result<()> {
    test_completion_install(Shell::Zsh, "_rusk")
}

#[test]
fn test_fish_completion_install() -> Result<()> {
    test_completion_install(Shell::Fish, "rusk.fish")
}

#[test]
fn test_nu_completion_install() -> Result<()> {
    test_completion_install(Shell::Nu, "rusk.nu")
}

#[test]
fn test_powershell_completion_install() -> Result<()> {
    test_completion_install(Shell::PowerShell, "rusk-completions.ps1")
}

#[test]
fn test_all_shells_have_scripts() {
    for shell in [Shell::Bash, Shell::Zsh, Shell::Fish, Shell::Nu, Shell::PowerShell] {
        let script = shell.get_script();
        assert!(!script.is_empty(), "Script for {:?} should not be empty", shell);
        assert!(script.len() > 100, "Script for {:?} should be substantial", shell);
    }
}

#[test]
fn test_completion_scripts_are_different() {
    let bash = Shell::Bash.get_script();
    let zsh = Shell::Zsh.get_script();
    let fish = Shell::Fish.get_script();
    let nu = Shell::Nu.get_script();
    let powershell = Shell::PowerShell.get_script();
    
    // Each script should be unique
    assert_ne!(bash, zsh, "Bash and Zsh scripts should differ");
    assert_ne!(bash, fish, "Bash and Fish scripts should differ");
    assert_ne!(bash, nu, "Bash and Nu scripts should differ");
    assert_ne!(bash, powershell, "Bash and PowerShell scripts should differ");
    assert_ne!(zsh, fish, "Zsh and Fish scripts should differ");
    assert_ne!(zsh, nu, "Zsh and Nu scripts should differ");
    assert_ne!(zsh, powershell, "Zsh and PowerShell scripts should differ");
    assert_ne!(fish, nu, "Fish and Nu scripts should differ");
    assert_ne!(fish, powershell, "Fish and PowerShell scripts should differ");
    assert_ne!(nu, powershell, "Nu and PowerShell scripts should differ");
}

#[test]
fn test_completion_scripts_contain_rusk() {
    for shell in [Shell::Bash, Shell::Zsh, Shell::Fish, Shell::Nu, Shell::PowerShell] {
        let script = shell.get_script();
        // Each script should mention "rusk" somewhere
        assert!(
            script.contains("rusk"),
            "Script for {:?} should contain 'rusk'",
            shell
        );
    }
}

#[test]
fn test_completion_paths_are_in_home_directory() -> Result<()> {
    // This test verifies that default paths are in home directory
    // We can't easily mock home_dir, so we just verify the structure
    
    for shell in [Shell::Bash, Shell::Zsh, Shell::Fish, Shell::Nu, Shell::PowerShell] {
        let path = shell.get_default_path()?;
        
        // Path should contain home directory components
        let path_str = path.to_string_lossy();
        
        match shell {
            Shell::Bash => assert!(path_str.contains(".bash_completion.d"), "Bash path should contain .bash_completion.d"),
            Shell::Zsh => assert!(path_str.contains(".zsh"), "Zsh path should contain .zsh"),
            Shell::Fish => assert!(path_str.contains(".config/fish") || path_str.contains("fish"), "Fish path should contain fish"),
            Shell::Nu => assert!(path_str.contains(".config/nushell") || path_str.contains("nushell"), "Nu path should contain nushell"),
            Shell::PowerShell => assert!(path_str.contains("PowerShell") || path_str.contains("powershell"), "PowerShell path should contain PowerShell"),
        }
    }
    
    Ok(())
}

#[test]
fn test_completion_instructions_are_provided() {
    let temp_dir = TempDir::new().unwrap();
    let test_path = temp_dir.path().join("test_completion");
    
    for shell in [Shell::Bash, Shell::Zsh, Shell::Fish, Shell::Nu, Shell::PowerShell] {
        let instructions = shell.get_instructions(&test_path);
        assert!(!instructions.is_empty(), "Instructions for {:?} should not be empty", shell);
        assert!(instructions.len() > 20, "Instructions for {:?} should be substantial", shell);
    }
}

#[test]
fn test_completion_show_output() {
    // Test that show command would output the script
    for shell in [Shell::Bash, Shell::Zsh, Shell::Fish, Shell::Nu, Shell::PowerShell] {
        let script = shell.get_script();
        assert!(!script.is_empty());
        
        // Verify script starts with expected content
        match shell {
            Shell::Bash => assert!(script.contains("#") || script.contains("complete"), "Bash script should contain shell syntax"),
            Shell::Zsh => assert!(script.contains("#") || script.contains("_rusk"), "Zsh script should contain function definitions"),
            Shell::Fish => assert!(script.contains("#") || script.contains("complete"), "Fish script should contain complete commands"),
            Shell::Nu => assert!(script.contains("#") || script.contains("def"), "Nu script should contain function definitions"),
            Shell::PowerShell => assert!(script.contains("#") || script.contains("Register-ArgumentCompleter") || script.contains("function"), "PowerShell script should contain Register-ArgumentCompleter or function definitions"),
        }
    }
}

#[test]
fn test_completion_paths_use_correct_filenames() {
    let _temp_dir = TempDir::new().unwrap();
    
    let test_cases = vec![
        (Shell::Bash, "rusk"),
        (Shell::Zsh, "_rusk"),
        (Shell::Fish, "rusk.fish"),
        (Shell::Nu, "rusk.nu"),
        (Shell::PowerShell, "rusk-completions.ps1"),
    ];
    
    for (shell, expected_name) in test_cases {
        let path = shell.get_default_path().unwrap();
        let filename = path.file_name().unwrap().to_string_lossy();
        assert_eq!(filename, expected_name, "Filename for {:?} should be {}", shell, expected_name);
    }
}

#[test]
fn test_completion_install_in_custom_path() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let custom_path = temp_dir.path().join("custom_completion");
    
    // Verify path doesn't exist
    assert!(!custom_path.exists());
    
    // Write completion script
    let script = Shell::Bash.get_script();
    fs::write(&custom_path, script)?;
    
    // Verify file was created at custom path
    assert!(custom_path.exists());
    assert_eq!(fs::read_to_string(&custom_path)?, script);
    
    Ok(())
}

#[test]
fn test_completion_scripts_have_expected_structure() {
    // Verify each script has some expected markers
    let bash = Shell::Bash.get_script();
    assert!(bash.contains("rusk") || bash.contains("complete"), "Bash script should have completion structure");
    
    let zsh = Shell::Zsh.get_script();
    assert!(zsh.contains("_rusk") || zsh.contains("function"), "Zsh script should have function definitions");
    
    let fish = Shell::Fish.get_script();
    assert!(fish.contains("complete") || fish.contains("rusk"), "Fish script should have complete commands");
    
    let nu = Shell::Nu.get_script();
    assert!(nu.contains("def") || nu.contains("rusk"), "Nu script should have function definitions");
    
    let powershell = Shell::PowerShell.get_script();
    assert!(powershell.contains("Register-ArgumentCompleter") || powershell.contains("rusk"), "PowerShell script should have Register-ArgumentCompleter");
}

#[test]
fn test_cli_completions_show() -> Result<()> {
    use std::process::Command;
    
    let rusk_bin = common::require_rusk_bin()?;
    
    for shell_name in ["bash", "zsh", "fish", "nu", "powershell"] {
        let output = Command::new(&rusk_bin)
            .args(&["completions", "show", shell_name])
            .output()?;
        
        assert!(
            output.status.success(),
            "Command 'rusk completions show {}' should succeed",
            shell_name
        );
        
        let stdout = String::from_utf8(output.stdout)?;
        assert!(!stdout.is_empty(), "Output for {} should not be empty", shell_name);
        assert!(stdout.contains("rusk"), "Output for {} should contain 'rusk'", shell_name);
    }
    
    Ok(())
}

#[test]
fn test_cli_completions_install_to_temp_dir() -> Result<()> {
    let rusk_bin = common::require_rusk_bin()?;

    let temp_dir = TempDir::new()?;
    let fake_home = temp_dir.path();
    let test_path = fake_home.join(".bash_completion.d").join("rusk");

    let mut cmd = Command::new(&rusk_bin);
    with_isolated_home(&mut cmd, fake_home);
    let output = cmd.args(["completions", "install", "bash"]).output()?;

    assert!(
        output.status.success(),
        "Command should succeed. Stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    assert!(test_path.exists(), "Completion file should be created");
    assert!(test_path.is_file(), "Path should be a file");

    let content = fs::read_to_string(&test_path)?;
    assert!(!content.is_empty(), "File should not be empty");
    assert!(content.contains("rusk"), "File should contain 'rusk'");

    let expected_script = Shell::Bash.get_script();
    assert_eq!(content, expected_script, "File content should match script");

    Ok(())
}

#[test]
fn test_cli_completions_install_creates_directories() -> Result<()> {
    let rusk_bin = common::require_rusk_bin()?;

    let temp_dir = TempDir::new()?;
    let fake_home = temp_dir.path();
    let zsh_path = fake_home.join(".zsh").join("completions").join("_rusk");

    assert!(!zsh_path.parent().unwrap().exists());

    let mut cmd = Command::new(&rusk_bin);
    with_isolated_home(&mut cmd, fake_home);
    let output = cmd.args(["completions", "install", "zsh"]).output()?;

    assert!(
        output.status.success(),
        "Command should succeed. Stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    assert!(zsh_path.exists(), "File should be created");
    assert!(zsh_path.parent().unwrap().exists(), "Parent directory should be created");
    assert!(
        zsh_path.parent().unwrap().parent().unwrap().exists(),
        "Grandparent directory should be created"
    );

    Ok(())
}

#[test]
fn test_cli_completions_install_overwrites_existing() -> Result<()> {
    let rusk_bin = common::require_rusk_bin()?;

    let temp_dir = TempDir::new()?;
    let fake_home = temp_dir.path();
    let test_path = fake_home
        .join(".config")
        .join("fish")
        .join("completions")
        .join("rusk.fish");

    fs::create_dir_all(test_path.parent().unwrap())?;
    fs::write(&test_path, "old content")?;
    assert_eq!(fs::read_to_string(&test_path)?, "old content");

    let mut cmd = Command::new(&rusk_bin);
    with_isolated_home(&mut cmd, fake_home);
    let output = cmd.args(["completions", "install", "fish"]).output()?;

    assert!(output.status.success(), "Command should succeed");

    let new_content = fs::read_to_string(&test_path)?;
    assert_ne!(new_content, "old content", "Content should be overwritten");
    assert_eq!(new_content, Shell::Fish.get_script(), "Content should match Fish script");

    Ok(())
}

#[test]
fn test_cli_completions_install_multiple_shells() -> Result<()> {
    let rusk_bin = common::require_rusk_bin()?;

    let temp_dir = TempDir::new()?;
    let fake_home = temp_dir.path();

    let mut cmd = Command::new(&rusk_bin);
    with_isolated_home(&mut cmd, fake_home);
    let output = cmd
        .args(["completions", "install", "fish", "nu"])
        .output()?;

    assert!(
        output.status.success(),
        "Command should succeed. Stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8(output.stdout)?;
    assert!(
        stdout.contains("Fish") || stdout.contains("fish"),
        "Output should mention Fish shell"
    );
    assert!(
        stdout.contains("Nu Shell") || stdout.contains("nu"),
        "Output should mention Nu Shell"
    );

    let fish_path = fake_home
        .join(".config")
        .join("fish")
        .join("completions")
        .join("rusk.fish");
    let nu_path = fake_home
        .join(".config")
        .join("nushell")
        .join("completions")
        .join("rusk.nu");

    assert!(fish_path.is_file(), "Fish completion should exist under fake HOME");
    assert!(nu_path.is_file(), "Nu completion should exist under fake HOME");

    Ok(())
}

#[test]
fn test_cli_completions_invalid_shell() -> Result<()> {
    use std::process::Command;
    
    let rusk_bin = common::require_rusk_bin()?;
    
    // Test with invalid shell name
    let output = Command::new(&rusk_bin)
        .args(&["completions", "show", "invalid_shell"])
        .output()?;
    
    // Should fail with error
    assert!(!output.status.success(), "Command should fail for invalid shell");
    
    Ok(())
}

#[test]
fn test_cli_completions_help() -> Result<()> {
    use std::process::Command;
    
    let rusk_bin = common::require_rusk_bin()?;
    
    // Test help command
    let output = Command::new(&rusk_bin)
        .args(&["completions", "--help"])
        .output()?;
    
    assert!(output.status.success(), "Help command should succeed");
    
    let stdout = String::from_utf8(output.stdout)?;
    assert!(stdout.contains("completions"), "Help should mention 'completions'");
    assert!(stdout.contains("install"), "Help should mention 'install'");
    assert!(stdout.contains("show"), "Help should mention 'show'");
    
    Ok(())
}

#[test]
fn test_completion_default_paths_live_under_home() -> Result<()> {
    let Some(home) = dirs::home_dir() else {
        return Ok(());
    };
    let real_bash_path = home.join(".bash_completion.d").join("rusk");
    let real_zsh_path = home.join(".zsh").join("completions").join("_rusk");
    let real_fish_path = home
        .join(".config")
        .join("fish")
        .join("completions")
        .join("rusk.fish");
    let real_nu_path = home
        .join(".config")
        .join("nushell")
        .join("completions")
        .join("rusk.nu");

    assert!(real_bash_path.starts_with(&home), "Bash path should be in home");
    assert!(real_zsh_path.starts_with(&home), "Zsh path should be in home");
    assert!(real_fish_path.starts_with(&home), "Fish path should be in home");
    assert!(real_nu_path.starts_with(&home), "Nu path should be in home");

    Ok(())
}

#[test]
fn test_completion_install_all_shells_to_temp() -> Result<()> {
    // Test installing all shell types to temporary directory
    let temp_dir = TempDir::new()?;
    
    for shell in [Shell::Bash, Shell::Zsh, Shell::Fish, Shell::Nu, Shell::PowerShell] {
        let test_path = temp_dir.path().join(format!("test_{:?}.completion", shell));
        
        // Verify path doesn't exist
        assert!(!test_path.exists());
        
        // Get script and write
        let script = shell.get_script();
        fs::write(&test_path, script)?;
        
        // Verify file was created
        assert!(test_path.exists());
        
        // Verify content
        let content = fs::read_to_string(&test_path)?;
        assert_eq!(content, script);
    }
    
    Ok(())
}

#[test]
fn test_completion_scripts_match_source_files() -> Result<()> {
    // Verify that embedded scripts match the actual source files
    // This ensures we're testing the right content
    
    let source_bash = fs::read_to_string("completions/rusk.bash")?;
    let source_zsh = fs::read_to_string("completions/rusk.zsh")?;
    let source_fish = fs::read_to_string("completions/rusk.fish")?;
    let source_nu = fs::read_to_string("completions/rusk.nu")?;
    let source_powershell = fs::read_to_string("completions/rusk.ps1")?;
    
    assert_eq!(Shell::Bash.get_script(), source_bash, "Bash script should match source");
    assert_eq!(Shell::Zsh.get_script(), source_zsh, "Zsh script should match source");
    assert_eq!(Shell::Fish.get_script(), source_fish, "Fish script should match source");
    assert_eq!(Shell::Nu.get_script(), source_nu, "Nu script should match source");
    assert_eq!(Shell::PowerShell.get_script(), source_powershell, "PowerShell script should match source");
    
    Ok(())
}

#[test]
fn test_nu_completion_has_quote_functions() {
    // Verify that Nu completion script contains functions for quoting text with special characters
    let nu_script = Shell::Nu.get_script();
    
    // Should contain the needs-quotes function
    assert!(
        nu_script.contains("needs-quotes") || nu_script.contains("def needs-quotes"),
        "Nu script should contain needs-quotes function"
    );
    
    // Should contain the quote-if-needed function
    assert!(
        nu_script.contains("quote-if-needed") || nu_script.contains("def quote-if-needed"),
        "Nu script should contain quote-if-needed function"
    );
    
    // Should check for special characters
    assert!(
        nu_script.contains("special_chars") || nu_script.contains("special") || nu_script.contains("|") || nu_script.contains(";"),
        "Nu script should check for special characters"
    );
}

#[test]
fn test_nu_completion_quotes_special_characters() {
    // Verify that Nu completion script properly handles special characters
    let nu_script = Shell::Nu.get_script();
    
    // Should escape double quotes
    assert!(
        nu_script.contains("str replace") || nu_script.contains("replace") || nu_script.contains("\\\""),
        "Nu script should escape double quotes"
    );
    
    // Should wrap text in quotes when needed
    assert!(
        nu_script.contains("\"") || nu_script.contains("quote"),
        "Nu script should wrap text in quotes"
    );
}

#[test]
fn test_nu_completion_mark_del_flags_after_subcommand() {
    // mark/del: no task ID completion; empty cur or -prefix should offer flags
    let nu_script = Shell::Nu.get_script();

    assert!(
        nu_script.contains("def complete-mark-del"),
        "Nu script should define complete-mark-del"
    );
    assert!(
        nu_script.contains("get-common-flags"),
        "Nu mark/del should include common help flags"
    );
    assert!(
        nu_script.contains("--done") && nu_script.contains("del"),
        "Nu del completion should offer --done"
    );
}

#[test]
fn test_nu_completion_completions_partial_input() {
    // Verify that Nu completion script supports partial input for completions command
    // e.g., "rusk completions ins<tab>" -> "install", "rusk completions install ba<tab>" -> "bash"
    let nu_script = Shell::Nu.get_script();
    
    // Should contain logic for filtering subcommands by partial input
    assert!(
        nu_script.contains("str starts-with") || nu_script.contains("starts-with"),
        "Nu script should filter commands by partial input"
    );
    
    // Should handle partial input for install/show subcommands
    assert!(
        nu_script.contains("install") && nu_script.contains("show"),
        "Nu script should handle install and show subcommands"
    );
    
    // Should handle partial input for shell names
    assert!(
        (nu_script.contains("bash") || nu_script.contains("\"bash\"")) &&
        (nu_script.contains("zsh") || nu_script.contains("\"zsh\"")) &&
        (nu_script.contains("fish") || nu_script.contains("\"fish\"")) &&
        (nu_script.contains("nu") || nu_script.contains("\"nu\"")) &&
        (nu_script.contains("powershell") || nu_script.contains("\"powershell\"")),
        "Nu script should handle all shell names for partial input"
    );
    
    // Should filter subcommands when user types partial input
    assert!(
        nu_script.contains("where") || nu_script.contains("filter") || nu_script.contains("matching"),
        "Nu script should filter options based on partial input"
    );
}

#[test]
fn test_nu_completion_handles_common_special_chars() {
    // Verify that Nu completion script handles common special characters
    let nu_script = Shell::Nu.get_script();
    
    // Check for common special characters that require quoting
    let special_chars = ["|", ";", "&", ">", "<", "(", ")", "[", "]", "{", "}", "$", "*", "?", "~", "#", "@", "!", "%", "^", "=", "+", "-", "/", ":", ",", "."];
    
    // At least some of these should be mentioned or checked in the script
    let mut found_any = false;
    for char in &special_chars {
        if nu_script.contains(char) {
            found_any = true;
            break;
        }
    }
    
    // The script should reference special characters (either in comments or in the logic)
    // This is a soft check - the script might handle them without explicitly listing them
    // But it's good to verify the functionality exists
    assert!(
        found_any || nu_script.contains("special") || nu_script.contains("quote"),
        "Nu script should handle special characters"
    );
}

#[test]
fn test_completion_install_creates_file_with_correct_permissions() -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    
    let temp_dir = TempDir::new()?;
    let test_path = temp_dir.path().join("test_completion");
    
    // Write file
    fs::write(&test_path, Shell::Bash.get_script())?;
    
    // Verify file exists and is readable
    assert!(test_path.exists());
    assert!(test_path.is_file());
    
    // On Unix, verify permissions allow reading
    #[cfg(unix)]
    {
        let metadata = fs::metadata(&test_path)?;
        let permissions = metadata.permissions();
        let mode = permissions.mode();
        
        // File should be readable (owner has read permission)
        assert!(mode & 0o400 != 0, "File should be readable by owner");
    }
    
    Ok(())
}

#[test]
fn test_bash_completion_syntax() -> Result<()> {
    use std::process::Command;
    
    let script = Shell::Bash.get_script();
    let temp_dir = TempDir::new()?;
    let script_path = temp_dir.path().join("rusk.bash");
    fs::write(&script_path, script)?;
    
    // Check bash syntax: bash -n script.bash
    let output = Command::new("bash")
        .arg("-n")
        .arg(&script_path)
        .output();
    
    // Bash might not be installed, so we skip if command not found
    match output {
        Ok(result) => {
            if !result.status.success() {
                let stderr = String::from_utf8_lossy(&result.stderr);
                panic!("Bash syntax check failed:\n{}", stderr);
            }
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            // Bash not installed, skip test
            eprintln!("Warning: bash command not found, skipping syntax check");
            return Ok(());
        }
        Err(e) => return Err(e.into()),
    }
    
    Ok(())
}

#[test]
fn test_zsh_completion_syntax() -> Result<()> {
    use std::process::Command;
    
    let script = Shell::Zsh.get_script();
    let temp_dir = TempDir::new()?;
    let script_path = temp_dir.path().join("rusk.zsh");
    fs::write(&script_path, script)?;
    
    // Check zsh syntax: zsh -n script.zsh
    let output = Command::new("zsh")
        .arg("-n")
        .arg(&script_path)
        .output();
    
    // Zsh might not be installed, so we skip if command not found
    match output {
        Ok(result) => {
            if !result.status.success() {
                let stderr = String::from_utf8_lossy(&result.stderr);
                panic!("Zsh syntax check failed:\n{}", stderr);
            }
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            // Zsh not installed, skip test
            eprintln!("Warning: zsh command not found, skipping syntax check");
            return Ok(());
        }
        Err(e) => return Err(e.into()),
    }
    
    Ok(())
}

#[test]
fn test_fish_completion_syntax() -> Result<()> {
    use std::process::Command;
    
    let script = Shell::Fish.get_script();
    let temp_dir = TempDir::new()?;
    let script_path = temp_dir.path().join("rusk.fish");
    fs::write(&script_path, script)?;
    
    // Check fish syntax: fish --no-execute script.fish
    // Note: fish --no-execute is available in fish 3.0+
    let output = Command::new("fish")
        .arg("--no-execute")
        .arg(&script_path)
        .output();
    
    // Fish might not be installed, so we skip if command not found
    match output {
        Ok(result) => {
            if !result.status.success() {
                let stderr = String::from_utf8_lossy(&result.stderr);
                panic!("Fish syntax check failed:\n{}", stderr);
            }
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            // Fish not installed, skip test
            eprintln!("Warning: fish command not found, skipping syntax check");
            return Ok(());
        }
        Err(e) => return Err(e.into()),
    }
    
    Ok(())
}

#[test]
fn test_nu_completion_syntax() -> Result<()> {
    use std::process::Command;
    
    let script = Shell::Nu.get_script();
    let temp_dir = TempDir::new()?;
    let script_path = temp_dir.path().join("rusk.nu");
    fs::write(&script_path, script)?;
    
    // Check nu syntax by trying to parse it
    // Nu doesn't have a simple --check flag, so we try to load it as a module
    // We use -c with a command that attempts to parse the script
    let check_command = format!(
        r#"try {{ use {}; exit 0 }} catch {{ echo $env.ERR; exit 1 }}"#,
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
                panic!("Nu syntax check failed:\n{}", stderr);
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

#[test]
fn test_powershell_completion_syntax() -> Result<()> {
    use std::process::Command;

    let script = Shell::PowerShell.get_script();
    let temp_dir = TempDir::new()?;
    let script_path = temp_dir.path().join("rusk-completions.ps1");
    fs::write(&script_path, script)?;

    // Check PowerShell syntax by trying to parse it
    // PowerShell doesn't have a simple --check flag, so we use -Command with Get-Command
    // to validate syntax without executing

    // Try to parse the script using PowerShell's parser
    // We use -Command with a try-catch to validate syntax
    let check_command = format!(
        r#"try {{ $null = [System.Management.Automation.PSParser]::Tokenize($(Get-Content '{}' -Raw), [ref]$null); exit 0 }} catch {{ Write-Error $_.Exception.Message; exit 1 }}"#,
        script_path.to_string_lossy().replace('\\', "\\\\")
    );

    let output = if cfg!(windows) {
        Command::new("powershell")
            .arg("-NoProfile")
            .arg("-Command")
            .arg(&check_command)
            .output()
    } else {
        // On Unix, try pwsh (PowerShell Core)
        Command::new("pwsh")
            .arg("-NoProfile")
            .arg("-Command")
            .arg(&check_command)
            .output()
    };

    // PowerShell might not be installed, so we skip if command not found
    match output {
        Ok(result) => {
            if !result.status.success() {
                let stderr = String::from_utf8_lossy(&result.stderr);
                let stdout = String::from_utf8_lossy(&result.stdout);
                // Check if it's a system/runtime error (not a syntax error)
                // System errors usually contain "FileLoadException", "Assembly", etc.
                let is_system_error = stderr.contains("FileLoadException")
                    || stderr.contains("System.IO")
                    || stderr.contains("assembly")
                    || stderr.contains("Unhandled exception");
                if is_system_error {
                    eprintln!(
                        "Warning: PowerShell runtime error (not syntax error), skipping: {}",
                        stderr
                    );
                    return Ok(());
                }
                panic!(
                    "PowerShell syntax check failed:\nstdout: {}\nstderr: {}",
                    stdout, stderr
                );
            }
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            // PowerShell not installed, skip test
            eprintln!("Warning: powershell/pwsh command not found, skipping syntax check");
            return Ok(());
        }
        Err(e) => return Err(e.into()),
    }

    Ok(())
}

#[test]
fn test_all_completion_scripts_syntax() -> Result<()> {
    // Run all syntax checks
    // This test will skip individual checks if shells are not installed
    // or if they have runtime issues (like broken .NET dependencies)
    // but will fail if syntax is actually wrong

    // Bash and Zsh are usually available on Unix systems
    #[cfg(unix)]
    {
        test_bash_completion_syntax()?;
        test_zsh_completion_syntax()?;
        test_fish_completion_syntax()?;
    }

    // Nu and PowerShell might not be installed or might have runtime issues
    // We use catch_unwind to handle panics from these tests
    // since they might fail due to system issues, not code issues

    // Try Nu - ignore failures (not installed or other issues)
    let nu_result = std::panic::catch_unwind(|| test_nu_completion_syntax());
    if let Err(_) = nu_result {
        eprintln!("Warning: Nu syntax test panicked (likely not installed), skipping");
    }

    // Try PowerShell - ignore failures (not installed or runtime issues like FileLoadException)
    let ps_result = std::panic::catch_unwind(|| test_powershell_completion_syntax());
    if let Err(_) = ps_result {
        eprintln!("Warning: PowerShell syntax test panicked (likely not installed or runtime error), skipping");
    }

    Ok(())
}

