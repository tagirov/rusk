<h1 align="center" id="rusk">rusk</h1>
<p align="center">A minimal cross-platform terminal task manager</p>
<p align="center">
    <a href="https://github.com/tagirov/rusk/releases"><img alt="rusk release version number" src="https://img.shields.io/github/v/release/tagirov/rusk?logo=github"></a>
</p>

- [Install](#install)
- [Basic Usage](#basic-usage)
  - [Working with Multiple Tasks](#working-with-multiple-tasks)
  - [Interactive Editing](#interactive-editing)
  - [Data Safety & Backup](#data-safety--backup)
    - [Automatic Backups](#automatic-backups)
    - [Manual Restore](#manual-restore)
  - [Aliases](#aliases)
- [Configuration](#configuration)
  - [Shell Completion](#shell-completion)
    - [Quick Install (Recommended)](#quick-install-recommended)
    - [Manual Installation](#manual-installation)
      - [Bash](#bash)
      - [Zsh](#zsh)
      - [Fish](#fish)
      - [Nu Shell](#nu-shell)
      - [PowerShell](#powershell)
  - [Database Location](#database-location)

# Install
#### Linux/MacOS/Windows
```bash
cargo install --git https://github.com/tagirov/rusk
```
The binary will be installed to:
- Linux/MacOS: `$HOME/.cargo/bin/rusk`
- Windows: `%USERPROFILE%\.cargo\bin\rusk.exe`

Make sure that these paths are added to your $PATH environment variable to use `rusk` command globally.

#### Arch Linux (AUR)
```bash
paru -S rusk
```

#### Manually
```bash
git clone https://github.com/tagirov/rusk && cd rusk
```
```bash
cargo build --release
```

Linux/MacOS

```bash
sudo install -m 755 ./target/release/rusk /usr/local/bin
```

Windows

```bash
copy .\target\release\rusk.exe "%USERPROFILE%\AppData\Local\Microsoft\WindowsApps\"
```



# Basic Usage

```bash
# Add a new task
rusk add Buy groceries

# Add a task with a deadline
rusk add Finish project report --date 31-12-2025
# Or with short year and slash separator:
rusk add Finish project report --date 31/12/25
# Leading zero for day is optional:
rusk add Finish project report --date 1-12-25

# View all tasks
rusk list
# or simply
rusk

# Mark a task as done
rusk mark 1

# Mark a task as undone (toggle)
rusk mark 1

# Edit task text (replace)
rusk edit 1 Complete the project documentation

# Edit task date
rusk edit 1 --date 25-12-2025

# Edit both text and date
rusk edit 1 Update documentation --date 23/12/25

# Delete a task
rusk del 1

# Delete all completed tasks
rusk del --done
```

## Working with Multiple Tasks

```bash
# Mark multiple tasks as done
rusk mark 1 2 3
# or with commas
rusk mark 1,2,3
# or mixed
rusk mark 1 2,3 4

# Edit multiple tasks with the same text
rusk edit 1 2 3 Update status to completed

# Delete multiple tasks
rusk del 1 2 5
```

## Interactive Editing

```bash
# Edit task text interactively (opens editor for text and date)
rusk edit 1

# Edit task text and date interactively
rusk edit 1 -d

# Interactive editing of tasks in sequence
rusk edit 1 4 5
```


## Data Safety & Backup
#### Automatic Backups
- Every save operation creates a `.json.backup` file
- Backups are stored in the same directory as your database
- Atomic writes prevent data corruption during saves

#### Manual Restore
```bash
# Restore from the automatic backup
rusk restore

# This will:
# 1. Validate the backup file
# 2. Create a safety backup of current database (if valid)
# 3. Restore tasks from backup
```


## Aliases
```bash
rusk a (add)
rusk l (list)
rusk m (mark)
rusk e (edit)
rusk d (del)
rusk r (restore)

-d (--date)
-h (--help)
-V (--version)
```


# Configuration

### Shell Completion

It provides autocomplete for commands, task IDs, and task text during editing by pressing `<TAB>` button.

**Features:**
- **Command completion**: Autocomplete commands (`add`, `edit`, `mark`, `del`, etc.) and their aliases
- **Task ID completion**: Tab-complete task IDs for `edit`, `mark`, and `del` commands
- **Smart text completion**: When typing `rusk edit 3 <TAB>`, automatically suggests the current task text for easy editing (`rusk edit 3<TAB>` for Nu Shell).
- **Flag completion**: Autocomplete `--date`, `--done`, etc.
- **Date suggestions**: When using `--date` or `-d` flag, suggests default dates: Today, Tomorrow, One week ahead, Two weeks ahead

#### Quick Install (Recommended)

Use the built-in command to install completions automatically:

```bash
# Install for your shell (auto-detects path)
rusk completions install bash
rusk completions install zsh
rusk completions install fish
rusk completions install nu
rusk completions install powershell

# Or specify custom path
rusk completions install bash --output ~/.bash_completion.d/rusk

# Show completion script (for manual installation)
rusk completions show zsh > ~/.zsh/completions/_rusk
```

**Windows Support:**
- **Git Bash**: Works with `bash` completions (uses Unix-style paths)
- **WSL**: Works with `bash`, `zsh`, `fish`, and `nu` completions
- **Nu Shell**: Works natively on Windows (uses `%APPDATA%\nushell\completions\`)
- **PowerShell**: Works natively on Windows (uses `Documents\PowerShell\rusk-completions.ps1`)
- **CMD**: Basic commands work (add, list, mark, del, edit with text/date). Interactive editing (`rusk edit` without arguments) requires Windows 10+ and may have limited functionality. Tab completion is not supported. Colors work on Windows 10+ (build 1511 and later).

#### Manual Installation

If you prefer manual installation or need to customize the setup:

##### Bash
```bash
# Get script from rusk and save it
rusk completions show bash > ~/.bash_completion.d/rusk

# Or install system-wide (requires root)
rusk completions show bash | sudo tee /etc/bash_completion.d/rusk > /dev/null

source ~/.bash_completion.d/rusk ## Or
source /etc/bash_completion.d/rusk ## In your .bashrc
```

##### Zsh
```bash
# Get script from rusk and save it
mkdir -p ~/.zsh/completions
rusk completions show zsh > ~/.zsh/completions/_rusk

# Add to your ~/.zshrc
echo 'fpath=(~/.zsh/completions $fpath)' >> ~/.zshrc
echo 'autoload -U compinit && compinit' >> ~/.zshrc
```

##### Fish
```bash
# Get script from rusk and save it
mkdir -p ~/.config/fish/completions
rusk completions show fish > ~/.config/fish/completions/rusk.fish
```

##### Nu Shell
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

##### PowerShell
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
### Database Location

By default, rusk stores tasks to:
- Linux/MacOS: `$HOME/.rusk/tasks.json`
- Windows: `%USERPROFILE%\.rusk\tasks.json`

You can customize the database location using the `RUSK_DB` environment variable:

```bash
# Use a custom database file
export RUSK_DB="/path/to/your/tasks.json"

# Use a custom directory (tasks.json will be created inside)
export RUSK_DB="/path/to/your/project/"

# Use different task lists for different projects
cd ~/projects/website
RUSK_DB="./tasks.json" rusk add Fix responsive layout

cd ~/projects/api
RUSK_DB="./tasks.json" rusk add Add authentication endpoint

# Each project has its own task list
```

**Debug Mode:**
When running in debug mode (`cargo run` or debug builds), rusk uses a temporary database location to avoid affecting your production data:
- Linux/MacOS: `$TMPDIR/rusk_debug/tasks.json` (usually `/tmp/rusk_debug/tasks.json`)
- Windows: `%TEMP%\rusk_debug\tasks.json` (usually `C:\Users\<user>\AppData\Local\Temp\rusk_debug\tasks.json`)

In debug mode, the `RUSK_DB` environment variable is ignored, and the database path is printed to the console when the program starts.


<a href="#rusk">
  <p align="center">Back to top</p>
</a>
