// Shell completion scripts embedded in the binary
// These are included at compile time using include_str!

pub mod scripts {
    pub const BASH: &str = include_str!("../completions/rusk.bash");
    pub const ZSH: &str = include_str!("../completions/rusk.zsh");
    pub const FISH: &str = include_str!("../completions/rusk.fish");
    pub const NU: &str = include_str!("../completions/rusk.nu");
    pub const POWERSHELL: &str = include_str!("../completions/rusk.ps1");
}

/// Where fish and PowerShell (on Unix) keep their configuration:
/// `$XDG_CONFIG_HOME` when it is an absolute path, else `~/.config` — on
/// macOS as well, where neither of them uses `~/Library` (REVIEW №65).
fn xdg_config_home(home: &std::path::Path) -> std::path::PathBuf {
    std::env::var_os("XDG_CONFIG_HOME")
        .map(std::path::PathBuf::from)
        .filter(|dir| dir.is_absolute())
        .unwrap_or_else(|| home.join(".config"))
}

/// Nushell's configuration directory, as its `$nu.default-config-dir`
/// says: `$XDG_CONFIG_HOME/nushell` when that is set, else the platform's
/// own (`~/.config`, `~/Library/Application Support`, `%APPDATA%`).
fn nu_config_dir(home: &std::path::Path) -> std::path::PathBuf {
    std::env::var_os("XDG_CONFIG_HOME")
        .map(std::path::PathBuf::from)
        .filter(|dir| dir.is_absolute())
        .or_else(dirs::config_dir)
        .unwrap_or_else(|| home.join(".config"))
        .join("nushell")
}

/// `path` as a word of a command in `shell`: as it is when it holds nothing
/// the shell would read, else in single quotes — `$XDG_CONFIG_HOME` may be
/// `/home/u/my config` (review of R26).
fn shell_word(shell: Shell, path: &std::path::Path) -> String {
    let text = path.display().to_string();
    // A backslash separates the parts of a Windows path in PowerShell; in
    // the other shells it escapes.
    let plain = !text.is_empty()
        && text.chars().all(|c| {
            c.is_ascii_alphanumeric() || "/._-+,:@%=".contains(c) || (shell == Shell::PowerShell && c == '\\')
        });
    if plain {
        return text;
    }
    match shell {
        Shell::Bash | Shell::Zsh => format!("'{}'", text.replace('\'', r"'\''")),
        Shell::Fish => format!("'{}'", text.replace('\\', r"\\").replace('\'', r"\'")),
        Shell::PowerShell => format!("'{}'", text.replace('\'', "''")),
        // Nushell: a raw string holds anything but its own closing `'#`.
        Shell::Nu => format!("r#'{text}'#"),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
pub enum Shell {
    Bash,
    Zsh,
    Fish,
    Nu,
    #[value(name = "powershell")]
    PowerShell,
}

impl Shell {
    pub fn get_script(&self) -> &'static str {
        use scripts::*;
        match self {
            Shell::Bash => BASH,
            Shell::Zsh => ZSH,
            Shell::Fish => FISH,
            Shell::Nu => NU,
            Shell::PowerShell => POWERSHELL,
        }
    }

    pub fn get_default_path(&self) -> Result<std::path::PathBuf, anyhow::Error> {
        let home = dirs::home_dir()
            .ok_or_else(|| anyhow::anyhow!("Could not determine home directory"))?;

        let path = match self {
            Shell::Bash => {
                // Prefer user-specific location (doesn't require root)
                // Works on Unix/Linux, Git Bash on Windows, and WSL
                home.join(".bash_completion.d").join("rusk")
            }
            Shell::Zsh => {
                // Works on Unix/Linux, macOS, and WSL with Zsh
                home.join(".zsh").join("completions").join("_rusk")
            }
            Shell::Fish => {
                // Where fish looks: its `fish_complete_path` has
                // `$XDG_CONFIG_HOME/fish/completions` (or `~/.config/...`).
                xdg_config_home(&home)
                    .join("fish")
                    .join("completions")
                    .join("rusk.fish")
            }
            Shell::Nu => nu_config_dir(&home).join("completions").join("rusk.nu"),
            Shell::PowerShell => {
                // PowerShell 7+ default: Documents\PowerShell\rusk-completions.ps1
                // PowerShell 5.1 uses Documents\WindowsPowerShell\ — use `completions show powershell` and install manually if needed
                #[cfg(windows)]
                {
                    if let Some(documents) = dirs::document_dir() {
                        documents.join("PowerShell").join("rusk-completions.ps1")
                    } else {
                        home.join("Documents")
                            .join("PowerShell")
                            .join("rusk-completions.ps1")
                    }
                }
                #[cfg(not(windows))]
                {
                    // On Unix/Linux/macOS with PowerShell Core
                    xdg_config_home(&home)
                        .join("powershell")
                        .join("rusk-completions.ps1")
                }
            }
        };

        Ok(path)
    }

    pub fn get_instructions(&self, path: &std::path::Path) -> String {
        match self {
            Shell::Bash => {
                // Check for system-wide installation (Unix/Linux only)
                #[cfg(not(windows))]
                {
                    if path.starts_with("/etc") {
                        return "Completions installed system-wide. Restart your shell or run: source /etc/bash_completion.d/rusk".to_string();
                    }
                }
                // On Windows, Git Bash and WSL use Unix-style paths
                format!("Add to your ~/.bashrc:\n  source {}", shell_word(*self, path))
            }
            Shell::Zsh => {
                format!(
                    "Add to your ~/.zshrc:\n  fpath=({} $fpath)\n  autoload -U compinit && compinit",
                    shell_word(*self, path.parent().unwrap())
                )
            }
            Shell::Fish => {
                format!("Completions installed. Restart your shell or run: source {}", shell_word(*self, path))
            }
            Shell::Nu => {
                // `<config dir>/completions/rusk.nu`: config.nu is next to
                // `completions`.
                let config_path = path
                    .parent()
                    .and_then(|completions| completions.parent())
                    .map(|dir| dir.join("config.nu").display().to_string())
                    .unwrap_or_else(|| "config.nu".to_string());
                format!("Add to your config.nu ({}):\n  # Load rusk completions module\n  use ($nu.config-path | path dirname | path join \"completions\" \"rusk.nu\") *\n\n  $env.config.completions.external = {{\n    enable: true\n    completer: {{|spans|\n      if ($spans.0 == \"rusk\") {{\n        try {{\n          rusk-completions-main $spans\n        }} catch {{\n          []\n        }}\n      }} else {{\n        []\n      }}\n    }}\n  }}", config_path)
            }
            Shell::PowerShell => {
                let profile_path = if cfg!(windows) {
                    "$PROFILE".to_string()
                } else {
                    path.with_file_name("Microsoft.PowerShell_profile.ps1").display().to_string()
                };
                let script = shell_word(*self, path);
                format!(
                    "Add to your PowerShell profile ({}):\n  . {}\n\nOr source it manually:\n  . {}",
                    profile_path, script, script
                )
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    /// Review of R26: a path with a space was printed as two words.
    #[test]
    fn a_path_in_a_command_is_one_word() {
        let plain = Path::new("/home/u/.config/fish/completions/rusk.fish");
        for shell in [Shell::Bash, Shell::Zsh, Shell::Fish, Shell::PowerShell] {
            assert_eq!(shell_word(shell, plain), plain.display().to_string());
        }
        let spaced = Path::new("/home/u/my config/it's/rusk.fish");
        assert_eq!(shell_word(Shell::Bash, spaced), r"'/home/u/my config/it'\''s/rusk.fish'");
        assert_eq!(shell_word(Shell::Fish, spaced), r"'/home/u/my config/it\'s/rusk.fish'");
        assert_eq!(shell_word(Shell::PowerShell, spaced), "'/home/u/my config/it''s/rusk.fish'");
        assert_eq!(shell_word(Shell::PowerShell, Path::new(r"C:\Users\u\x.ps1")), r"C:\Users\u\x.ps1");
        assert_eq!(shell_word(Shell::Bash, Path::new(r"C:\Users\u\x")), r"'C:\Users\u\x'");
        let instructions = Shell::Fish.get_instructions(spaced);
        assert!(instructions.ends_with(r"source '/home/u/my config/it\'s/rusk.fish'"), "{instructions}");
    }
}
