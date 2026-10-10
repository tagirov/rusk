<h1 align="center" id="rusk-shell-completions">Rusk Shell Completions</h1>

<br />

- [Quick install](#quick-install-recommended)
- [What is completed](#what-is-completed)
- [Windows](#windows)
- [Manual installation](#manual-installation)
  - [Bash](#bash)
  - [Zsh](#zsh)
  - [Fish](#fish)
  - [Nu Shell](#nu-shell)
  - [PowerShell](#powershell)

## Quick install (recommended)

Use the built-in command to install completions automatically:

```bash
# Install for a single shell (auto-detects the path)
rusk completions install bash
rusk completions install zsh
rusk completions install fish
rusk completions install nu
rusk completions install powershell

# Install for several shells at once
rusk completions install bash zsh
rusk completions install fish nu powershell

# Dump the script for a manual install
rusk completions show zsh > ~/.zsh/completions/_rusk
```

## What is completed

- **Commands**: `add`, `edit`, `mark`, `del`, `completions`, etc. and their
  aliases.
- **Flags**: `--date` / `--after` (`add`, and `edit` after an id), `--done`,
  etc.
- **Task text**: `rusk edit <id><tab>` appends the text of that task, quoted
  so that running the line stores exactly that text.
  - The text is wrapped in single quotes when it contains shell-special
    characters (``| ; & > < ( ) [ ] { } $ " ' \` * ? ~ # @ ! % ^ = + - / : ,``),
    line breaks, tabs, runs of spaces or spaces at either end. In nu a raw
    string `r#'…'#` is used if the text has a `'`.
  - A text that starts with `-` is put after `--`, so it is not read as
    options.
  - In fish the id completes first and the next `<tab>` inserts the text,
    escaped by fish itself (see [Fish](#fish)).

## Windows

- **Git Bash**: works with the `bash` completions (Unix-style paths).
- **WSL**: works with the `bash`, `zsh`, `fish` and `nu` completions.
- **Nu Shell**: works natively (`%APPDATA%\nushell\completions\`).
- **PowerShell**: works natively (`Documents\PowerShell\rusk-completions.ps1`).
- **CMD**: the basic commands work (add, list, mark, del, edit with text).
  Due dates go on the first line of the interactive editor (`rusk edit`
  without text) or in `rusk edit <id> -d <date>`. Interactive editing
  requires Windows 10+ and may have limited functionality. Tab completion
  is not supported. Colors work on Windows 10+ (build 1511 and later).

## Manual installation

If you prefer manual installation or need to customize the setup:

### Bash

```bash
# Get the script from rusk and save it
rusk completions show bash > ~/.bash_completion.d/rusk

# Or install it system-wide (requires root)
rusk completions show bash | sudo tee /etc/bash_completion.d/rusk > /dev/null

# Then, in your .bashrc, one of:
source ~/.bash_completion.d/rusk
source /etc/bash_completion.d/rusk
```

### Zsh

The completion file **must** be named `_rusk` (leading underscore). Put it
on `fpath` **before** `compinit` runs, otherwise custom completions are
ignored.

```bash
# Get the script from rusk and save it
mkdir -p ~/.zsh/completions
rusk completions show zsh > ~/.zsh/completions/_rusk

# Add to your ~/.zshrc (order matters: fpath first, then compinit)
echo 'fpath=(~/.zsh/completions $fpath)' >> ~/.zshrc
echo 'autoload -Uz compinit && compinit' >> ~/.zshrc
```

If you use **Powerlevel10k instant prompt** (or similar), define
`fpath=(~/.zsh/completions $fpath)` *above* the instant-prompt block, so
that `rusk` completion is available on the first Tab in a new session.

### Fish

```bash
# Get the script from rusk and save it. When XDG_CONFIG_HOME is set, use
# $XDG_CONFIG_HOME/fish/completions instead: that is where fish looks then.
mkdir -p ~/.config/fish/completions
rusk completions show fish > ~/.config/fish/completions/rusk.fish
```

- fish escapes what it completes itself and inserts one word per Tab, so
  `rusk edit 3<TAB>` completes the id and the next `<TAB>` puts the task
  text after it, exactly as it is.
- A text that starts with `-` gets `--` first. fish adds no space after a
  `-`: the next `<TAB>` does, and the one after it gives the text.
- A text with a tab character or one that starts with `~` is not offered:
  fish cannot insert it unchanged.
- The script adds completions only; it binds no keys.

### Nu Shell

```bash
# Get the script from rusk and save it.
# On Windows:
New-Item -ItemType Directory -Force -Path "$env:APPDATA\nushell\completions"
rusk completions show nu | Out-File -FilePath "$env:APPDATA\nushell\completions\rusk.nu" -Encoding utf8

# On Linux/macOS, into nu's own config directory (`$nu.default-config-dir`
# says which: ~/.config/nushell on Linux, ~/Library/Application Support/nushell
# on macOS, $XDG_CONFIG_HOME/nushell when that is set; `rusk completions
# install nu` puts it there):
mkdir -p ~/.config/nushell/completions
rusk completions show nu > ~/.config/nushell/completions/rusk.nu
```

Then add this to your `config.nu`, next to that `completions` directory
(`%APPDATA%\nushell\config.nu` on Windows, `~/.config/nushell/config.nu` on
Linux), to enable external completions:

```nu
# Load the rusk completions module
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
# Save the completion script to a file.
# On Windows (PowerShell 7+):
New-Item -ItemType Directory -Force -Path "$env:USERPROFILE\Documents\PowerShell"
rusk completions show powershell | Out-File -FilePath "$env:USERPROFILE\Documents\PowerShell\rusk-completions.ps1" -Encoding utf8

# On Windows (PowerShell 5.1 / Windows PowerShell), use the WindowsPowerShell directory instead:
New-Item -ItemType Directory -Force -Path "$env:USERPROFILE\Documents\WindowsPowerShell"
rusk completions show powershell | Out-File -FilePath "$env:USERPROFILE\Documents\WindowsPowerShell\rusk-completions.ps1" -Encoding utf8

# Add it to your PowerShell profile
Add-Content $PROFILE ". `"$env:USERPROFILE\Documents\PowerShell\rusk-completions.ps1`""

# On Linux/macOS with PowerShell Core ($XDG_CONFIG_HOME/powershell when
# XDG_CONFIG_HOME is set):
mkdir -p ~/.config/powershell
rusk completions show powershell > ~/.config/powershell/rusk-completions.ps1
Add-Content $PROFILE ". ~/.config/powershell/rusk-completions.ps1"
```

<br />
<p align="center"><a href="#rusk-shell-completions">Back to top</a></p>
