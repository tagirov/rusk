<h1 align="center" id="rusk-shell-completions">Rusk Shell Completions</h1>

<br />

## Quick Install (Recommended)

Use the built-in command to install completions automatically:

```bash
# Install for a single shell (auto-detects path)
rusk completions install bash
rusk completions install zsh
rusk completions install fish
rusk completions install nu
rusk completions install powershell

# Install for multiple shells at once
rusk completions install bash zsh
rusk completions install fish nu powershell

# Dump script for manual install
rusk completions show zsh > ~/.zsh/completions/_rusk
```

## Manual Installation

If you prefer manual installation or need to customize the setup:

### Bash
```bash
# Get script from rusk and save it
rusk completions show bash > ~/.bash_completion.d/rusk

# Or install system-wide (requires root)
rusk completions show bash | sudo tee /etc/bash_completion.d/rusk > /dev/null

source ~/.bash_completion.d/rusk ## Or
source /etc/bash_completion.d/rusk ## In your .bashrc
```

### Zsh
The completion file **must** be named `_rusk` (leading underscore). Put it on `fpath` **before** `compinit` runs, otherwise custom completions are ignored.

```bash
# Get script from rusk and save it
mkdir -p ~/.zsh/completions
rusk completions show zsh > ~/.zsh/completions/_rusk

# Add to your ~/.zshrc (order matters: fpath first, then compinit)
echo 'fpath=(~/.zsh/completions $fpath)' >> ~/.zshrc
echo 'autoload -Uz compinit && compinit' >> ~/.zshrc
```

If you use **Powerlevel10k instant prompt** (or similar), define `fpath=(~/.zsh/completions $fpath)` *above* the instant-prompt block so `rusk` completion is available on the first Tab in a new session.

### Fish
```bash
# Get script from rusk and save it
mkdir -p ~/.config/fish/completions
rusk completions show fish > ~/.config/fish/completions/rusk.fish
```

### Nu Shell
```bash
# Get script from rusk and save it
# On Windows:
New-Item -ItemType Directory -Force -Path "$env:APPDATA\nushell\completions"
rusk completions show nu | Out-File -FilePath "$env:APPDATA\nushell\completions\rusk.nu" -Encoding utf8

# On Linux/macOS:
mkdir -p ~/.config/nushell/completions
rusk completions show nu > ~/.config/nushell/completions/rusk.nu

# Add to your config.nu
# Windows: %APPDATA%\nushell\config.nu
# Linux/macOS: ~/.config/nushell/config.nu
# Add this to enable external completions:

# Load rusk completions module
use ($nu.config-path | path dirname | path join "completions" "rusk.nu") *

$env.config.completions.external = {
  enable: true
  completer: {|spans|
    if ($spans.0 == "rusk") {
      try {
        rusk-completions-main $spans
      } catch {
        []
      }
    } else {
      []
    }
  }
}
```

### PowerShell
```powershell
# Save completion script to file
# On Windows (PowerShell 7+):
New-Item -ItemType Directory -Force -Path "$env:USERPROFILE\Documents\PowerShell"
rusk completions show powershell | Out-File -FilePath "$env:USERPROFILE\Documents\PowerShell\rusk-completions.ps1" -Encoding utf8

# On Windows (PowerShell 5.1 / Windows PowerShell):
# Use WindowsPowerShell directory instead:
New-Item -ItemType Directory -Force -Path "$env:USERPROFILE\Documents\WindowsPowerShell"
rusk completions show powershell | Out-File -FilePath "$env:USERPROFILE\Documents\WindowsPowerShell\rusk-completions.ps1" -Encoding utf8

# Add to your PowerShell profile
Add-Content $PROFILE ". `"$env:USERPROFILE\Documents\PowerShell\rusk-completions.ps1`""

# On Linux/macOS with PowerShell Core:
mkdir -p ~/.config/powershell
rusk completions show powershell > ~/.config/powershell/rusk-completions.ps1
Add-Content $PROFILE ". ~/.config/powershell/rusk-completions.ps1"
```
<br />
<p align="center"><a href="#rusk-shell-completions">Back to top</a></p>
