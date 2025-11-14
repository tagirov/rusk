// Shell completion scripts embedded in the binary
// These are included at compile time using include_str!

pub mod scripts {
    pub const BASH: &str = include_str!("../completions/rusk.bash");
    pub const ZSH: &str = include_str!("../completions/rusk.zsh");
    pub const FISH: &str = include_str!("../completions/rusk.fish");
    pub const NU: &str = include_str!("../completions/rusk.nu");
    pub const POWERSHELL: &str = include_str!("../completions/rusk.ps1");
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
                // Works on Unix/Linux, macOS, and WSL with Fish
                home.join(".config").join("fish").join("completions").join("rusk.fish")
            }
            Shell::Nu => {
                // Works on Unix/Linux, macOS, Windows, and WSL
                // On Windows, Nu Shell uses %APPDATA%\nushell\completions\
                // On Unix/Linux/macOS, uses ~/.config/nushell/completions/
                #[cfg(windows)]
                {
                    if let Some(appdata) = dirs::config_dir() {
                        appdata.join("nushell").join("completions").join("rusk.nu")
                    } else {
                        home.join("AppData").join("Roaming").join("nushell").join("completions").join("rusk.nu")
                    }
                }
                #[cfg(not(windows))]
                {
                    home.join(".config").join("nushell").join("completions").join("rusk.nu")
                }
            }
            Shell::PowerShell => {
                // PowerShell profile location on Windows
                // PowerShell 7+ uses: Documents\PowerShell\Microsoft.PowerShell_profile.ps1
                // PowerShell 5.1 uses: Documents\WindowsPowerShell\Microsoft.PowerShell_profile.ps1
                // We use PowerShell directory (for PS 7+) as default, user can override with --output
                #[cfg(windows)]
                {
                    // On Windows, use Documents\PowerShell\rusk-completions.ps1 (PowerShell 7+)
                    // For PowerShell 5.1, user should use --output to specify WindowsPowerShell directory
                    if let Some(documents) = dirs::document_dir() {
                        documents.join("PowerShell").join("rusk-completions.ps1")
                    } else {
                        home.join("Documents").join("PowerShell").join("rusk-completions.ps1")
                    }
                }
                #[cfg(not(windows))]
                {
                    // On Unix/Linux/macOS with PowerShell Core
                    home.join(".config").join("powershell").join("rusk-completions.ps1")
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
                format!("Add to your ~/.bashrc:\n  source {}", path.display())
            }
            Shell::Zsh => {
                format!("Add to your ~/.zshrc:\n  fpath=({} $fpath)\n  autoload -U compinit && compinit", 
                    path.parent().unwrap().display())
            }
            Shell::Fish => {
                "Completions installed. Restart your shell or run: source ~/.config/fish/completions/rusk.fish".to_string()
            }
            Shell::Nu => {
                let config_path = if cfg!(windows) {
                    "%APPDATA%\\nushell\\config.nu"
                } else {
                    "~/.config/nushell/config.nu"
                };
                format!("Add to your config.nu ({}):\n  # Load rusk completions module\n  use ($nu.config-path | path dirname | path join \"completions\" \"rusk.nu\") *\n\n  $env.config.completions.external = {{\n    enable: true\n    completer: {{|spans|\n      if ($spans.0 == \"rusk\") {{\n        try {{\n          rusk-completions-main $spans\n        }} catch {{\n          []\n        }}\n      }} else {{\n        []\n      }}\n    }}\n  }}", config_path)
            }
            Shell::PowerShell => {
                let profile_path = if cfg!(windows) {
                    "$PROFILE"
                } else {
                    "~/.config/powershell/Microsoft.PowerShell_profile.ps1"
                };
                format!(
                    "Add to your PowerShell profile ({}):\n  . {}\n\nOr source it manually:\n  . {}", 
                    profile_path,
                    path.display(),
                    path.display()
                )
            }
        }
    }
}

