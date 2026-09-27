# Nu Shell completion script for rusk
#
# Installation:
#   1. Automatic (recommended):
#      rusk completions install nu
#
#   2. Manual:
#      Generate script using rusk command:
#      On Windows:
#        New-Item -ItemType Directory -Force -Path "$env:APPDATA\nushell\completions"
#        rusk completions show nu | Out-File -FilePath "$env:APPDATA\nushell\completions\rusk.nu" -Encoding utf8
#      On Linux/macOS:
#        mkdir -p ~/.config/nushell/completions
#        rusk completions show nu > ~/.config/nushell/completions/rusk.nu
#
#      Then add to your config.nu:
#
#      # Load rusk completions module
#      use ($nu.config-path | path dirname | path join "completions" "rusk.nu") *
#
#      $env.config.completions.external = {
#        enable: true
#        completer: {|spans|
#          if ($spans.0 == "rusk") {
#            try {
#              rusk-completions-main $spans
#            } catch {
#              []
#            }
#          } else {
#            []
#          }
#        }
#      }

# ============================================================================
# Constants
# ============================================================================

# Command definitions with aliases and descriptions
def get-commands [] {
  [
    {value: "add", aliases: ["a"], description: "Add a new task"}
    {value: "edit", aliases: ["e"], description: "Edit tasks by id(s)"}
    {value: "mark", aliases: ["m"], description: "Mark tasks as done/undone"}
    {value: "del", aliases: ["d"], description: "Delete tasks by id(s)"}
    {value: "list", aliases: ["l"], description: "List all tasks"}
    {value: "search", aliases: ["s"], description: "Search tasks by text"}
    {value: "restore", aliases: ["r"], description: "Restore from backup"}
    {value: "gen", aliases: ["g"], description: "Generate a read-only HTML page"}
    {value: "serve", aliases: [], description: "Serve the web UI"}
    {value: "sync", aliases: [], description: "Synchronize with a remote"}
    {value: "completions", aliases: ["c"], description: "Install shell completions"}
  ]
}

# Common flags
def get-common-flags [] {
  [
    {value: "--help", description: "Show help"}
    {value: "-h", description: "Show help"}
  ]
}

# Version flags
def get-version-flags [] {
  [
    {value: "--version", description: "Show version"}
    {value: "-V", description: "Show version"}
  ]
}

# Date flags
def get-date-flags [] {
  [
    {value: "--date", description: "Set task date"}
    {value: "-d", description: "Set task date"}
  ]
}

# Dependency flags
def get-after-flags [] {
  [
    {value: "--after", description: "Depend on tasks (comma-separated ids)"}
    {value: "-a", description: "Depend on tasks (comma-separated ids)"}
  ]
}

# True when the token is a flag that takes a value (-d/--date, -a/--after)
def is-value-flag [token: string] {
  $token == "-d" or $token == "--date" or $token == "-a" or $token == "--after"
}

# list subcommand: compact view
def get-list-flags [] {
  [
    {value: "--compact", description: "Compact view: one row per task, … when it goes on"}
    {value: "-c", description: "Compact view: one row per task, … when it goes on"}
    {value: "--no-compact", description: "Full view for this run, even with compact = true"}
  ]
}

# search subcommand: IDs-only output
def get-search-flags [] {
  [
    {value: "--id", description: "Print only the IDs of matching tasks, one per line"}
  ]
}

# gen subcommand: output path (value is a free-form file path)
def get-gen-flags [] {
  [
    {value: "--output", description: "Output file path; pass `-` for stdout"}
    {value: "-o", description: "Output file path; pass `-` for stdout"}
  ]
}

# serve subcommand: bind address and port (values are free-form)
def get-serve-flags [] {
  [
    {value: "--host", description: "Bind address"}
    {value: "--port", description: "Port"}
  ]
}

# sync subcommands
def get-sync-subcommands [] {
  [
    {value: "push", description: "Upload local tasks to the remote"}
    {value: "pull", description: "Replace the local database with the remote tasks"}
  ]
}

# --force flag for sync push/pull
def get-force-flags [] {
  [
    {value: "--force", description: "Overwrite changes on the other side"}
  ]
}

# Shell options for completions command
def get-shells [] {
  [
    {value: "bash", description: "Bash shell"}
    {value: "zsh", description: "Zsh shell"}
    {value: "fish", description: "Fish shell"}
    {value: "nu", description: "Nu shell"}
    {value: "powershell", description: "PowerShell"}
  ]
}

# Completions subcommands
def get-completions-subcommands [] {
  [
    {value: "install", description: "Install completions for a shell"}
    {value: "show", description: "Show completion script"}
  ]
}

# ============================================================================
# Utility Functions
# ============================================================================

# Find rusk binary path
def get-rusk-cmd [] {
  try {
    (^which rusk | str trim)
  } catch {
    try {
      (^where rusk | str trim | lines | first)
    } catch {
      "rusk"
    }
  }
}

# Extract RUSK_DB from spans if present, or from environment
def get-rusk-db-from-env [spans: list<string>] {
  # First check spans for RUSK_DB=
  for $span in $spans {
    if ($span | str starts-with "RUSK_DB=") {
      return ($span | str replace "RUSK_DB=" "")
    }
  }
  # If not found in spans, check environment variable
  try {
    if ("RUSK_DB" in $env) {
      return $env.RUSK_DB
    }
  }
  null
}

# The text of a task from `rusk list --for-completion-lines`. Each task is one line,
# `<id><TAB><text>`, with backslash, line feed, carriage return and tab
# escaped in the text as \\, \n, \r and \t
def get-task-text [task_id: int, spans: list<string>] {
  try {
    let rusk_cmd = (get-rusk-cmd)
    let task_id_str = ($task_id | into string)
    let rusk_db = (get-rusk-db-from-env $spans)
    
    let output = if ($rusk_db != null) {
      with-env {RUSK_DB: $rusk_db} { ^$rusk_cmd list --for-completion-lines | complete }
    } else {
      ^$rusk_cmd list --for-completion-lines | complete
    }
    
    if ($output.exit_code == 0) {
      let prefix = $task_id_str + (char tab)
      let line = ($output.stdout | split row (char newline) | where {|l| $l | str starts-with $prefix } | get 0?)
      if ($line == null) { return null }
      let text = (decode-listed-text ($line | split row -n 2 (char tab) | get 1))
      if $text != "" { $text } else { null }
    } else {
      null
    }
  } catch {
    null
  }
}

# The escapes of a `--for-completion-lines` text undone. Every backslash of the text
# is doubled there, so splitting at \\ leaves pieces whose only escapes are
# \n, \r and \t
def decode-listed-text [text: string] {
  $text
  | split row '\\'
  | each {|piece|
    $piece
    | str replace --all '\n' (char newline)
    | str replace --all '\r' (char cr)
    | str replace --all '\t' (char tab)
  }
  | str join '\'
}

# Check if string is a number
def is-number [str: string] {
  try {
    ($str | into int | ignore)
    true
  } catch {
    false
  }
}

# True if the text would not come back as it is when put on the command line
# bare: a special character — | ; & > < ( ) [ ] { } $ " ' ` \ * ? ~ # @ ! % ^ = + - / : ,
# — a control character, or any whitespace but single spaces between words
def needs-quotes [text: string] {
  let special_chars = ["|", ";", "&", ">", "<", "(", ")", "[", "]", "{", "}", "$", '"', "'", "`", "\\", "*", "?", "~", "#", "@", "!", "%", "^", "=", "+", "-", "/", ":", ","]
  let chars = ($text | split chars)
  (($chars | any {|char| $char in $special_chars})
    or ($text =~ '[[:cntrl:]]|  |^ | $'))
}

# Wrap text in quotes if it contains special characters. Single quotes take
# every character as it is but ' itself; a text with one goes into a raw
# string r#'...'#, which ends at the first ' followed by its hashes, so it
# gets one # more than the longest run of them after a ' in the text
def quote-if-needed [text: string] {
  if not (needs-quotes $text) {
    return $text
  }
  if not ($text | str contains "'") {
    return $"'($text)'"
  }
  mut hashes = "#"
  while ($text | str contains $"'($hashes)") {
    $hashes = $hashes + "#"
  }
  $"r($hashes)'($text)'($hashes)"
}

# Get entered task IDs from spans (skip "rusk" and command)
# Handles both space-separated and comma-separated IDs
def get-entered-ids [spans: list<string>] {
  # Find rusk command index and skip past "rusk command"
  let filtered_spans = ($spans | where $it != "")
  
  let rusk_idx = try {
    ($filtered_spans | enumerate | where {|it| $it.item == "rusk"} | get 0.index)
  } catch {
    -1
  }
  
  let args = if $rusk_idx >= 0 {
    # Skip past "rusk" and command (2 elements after rusk_idx)
    ($filtered_spans | skip ($rusk_idx + 2))
  } else {
    # Fallback to old behavior
    ($filtered_spans | skip 2)
  }
  
  ($args | reduce --fold [] {|arg, acc|
    if ($arg | str starts-with "-") {
      $acc
    } else if ($arg | str contains ",") {
      let parts = ($arg | split row ",")
      let parsed_ids = ($parts 
        | each {|part| $part | str trim }
        | where {|part| ($part | str length) > 0 }
        | where {|part| 
          try { ($part | into int | ignore); true } catch { false }
        }
        | each {|part| $part | into int }
      )
      ($acc | append $parsed_ids)
    } else {
      let is_id = (try { ($arg | into int | ignore); true } catch { false })
      if $is_id {
        ($acc | append [($arg | into int)])
      } else {
        $acc
      }
    }
  })
}

# ============================================================================
# Completion Helper Functions
# ============================================================================

# Filter completions by current input (narrowing)
# (No list<record> annotation: flattened command+alias rows infer as oneof in some Nu versions.)
def filter-by-prefix [completions, prefix: string] {
  if ($prefix == "") {
    $completions
  } else {
    let matching = ($completions | where {|item| ($item.value | str starts-with $prefix) })
    if ($matching | length) > 0 {
      $matching
    } else {
      $completions
    }
  }
}

# Complete flags with filtering
def complete-flags [flags: list<record>, cur: string] {
  filter-by-prefix $flags $cur
}

# Check if word ends with comma
def ends-with-comma [word: string] {
  ($word | str ends-with ",")
}

# Extract ID from word (removing comma if present)
def extract-id [word: string] {
  if (ends-with-comma $word) {
    ($word | str substring 0..(($word | str length) - 1))
  } else {
    $word
  }
}

# ============================================================================
# Command-Specific Completion Functions
# ============================================================================

# Completed arguments after "rusk add|a" (excludes current partial token)
def add-completed-args-after [spans: list<string>] {
  let ctx = (parse-spans $spans)
  let filtered = ($spans | where $it != "")
  let rusk_idx = try {
    ($filtered | enumerate | where {|it| $it.item == "rusk"} | get 0.index)
  } catch {
    -1
  }
  if $rusk_idx < 0 {
    return []
  }
  mut after = ($filtered | skip ($rusk_idx + 2))
  if not $ctx.has_trailing_space and ($after | length) > 0 {
    $after = ($after | take (($after | length) - 1))
  }
  $after
}

# True if add has at least one task-text token before cursor (skips values after -d/--date, -a/--after)
def add-has-prior-task-text [spans: list<string>] {
  let args = (add-completed-args-after $spans)
  mut prev = ""
  for $arg in $args {
    if (is-value-flag $prev) {
      $prev = $arg
      continue
    }
    if (is-value-flag $arg) {
      $prev = $arg
      continue
    }
    if ($arg | str starts-with "-") {
      $prev = $arg
      continue
    }
    if ($arg | str length) > 0 {
      return true
    }
    $prev = $arg
  }
  false
}

# True if edit has at least one task id before cursor (skips -d value)
def edit-has-prior-id [spans: list<string>] {
  (get-entered-ids $spans | length) > 0
}

# Complete add command
def complete-add [spans: list<string>, cur: string, prev: string] {
  # After -d/--date or -a/--after: more flags only (-h/--help), including while the flag token is still current
  if (is-value-flag $prev) and (($cur == "") or ($cur | str starts-with "-")) {
    return (complete-flags (get-common-flags) $cur)
  }

  # Complete flags (-d/--date, -a/--after only after task text)
  if ($cur == "") or ($cur | str starts-with "-") {
    let has_text = (add-has-prior-task-text $spans)
    let args_done = (add-completed-args-after $spans)
    let has_date_on_line = ($args_done | any {|t| $t == "-d" or $t == "--date"}) or $cur == "-d" or $cur == "--date"
    let has_after_on_line = ($args_done | any {|t| $t == "-a" or $t == "--after"}) or $cur == "-a" or $cur == "--after"
    let all_flags = if $has_text {
      (if $has_date_on_line { [] } else { (get-date-flags) })
      | append (if $has_after_on_line { [] } else { (get-after-flags) })
      | append (get-common-flags)
    } else {
      (get-common-flags)
    }
    return (complete-flags $all_flags $cur)
  }

  []
}

# Complete edit: optional -d <date> / -a <ids> after id(s), else TUI (first line for date)
def complete-edit [spans: list<string>, cur: string, prev: string, has_trailing_space: bool, command: string] {
  if (is-value-flag $prev) and (($cur == "") or ($cur | str starts-with "-")) {
    return (complete-flags (get-common-flags) $cur)
  }

  let entered_ids = (get-entered-ids $spans)
  let args_done = (add-completed-args-after $spans)
  let has_date_on_line = ($args_done | any {|t| $t == "-d" or $t == "--date"}) or $cur == "-d" or $cur == "--date"
  let has_after_on_line = ($args_done | any {|t| $t == "-a" or $t == "--after"}) or $cur == "-a" or $cur == "--after"
  let has_id = (edit-has-prior-id $spans)

  let prev_ends_with_comma = (ends-with-comma $prev)
  let prev_id = (extract-id $prev)

  # Space after single ID: -d/--date, -a/--after and help if not on line yet, else -h only
  if ($cur == "") and (is-number $prev_id) and not $prev_ends_with_comma {
    if ($entered_ids | length) == 1 {
      let all_flags = if $has_id {
        (if $has_date_on_line { [] } else { (get-date-flags) })
        | append (if $has_after_on_line { [] } else { (get-after-flags) })
        | append (get-common-flags)
      } else {
        (get-common-flags)
      }
      return (complete-flags $all_flags $cur)
    }
  }
  
  # Case 3: Append task text when ID is immediately followed by <tab> (no space).
  # entered_ids includes the current token; exclude it so `rusk edit 5<TAB>` matches (like bash COMP_CWORD).
  if ($cur != "") and (is-number $cur) and ($prev == "edit" or $prev == "e") and not $has_trailing_space {
    let filtered = ($spans | where $it != "")
    let rusk_idx = try {
      ($filtered | enumerate | where {|it| $it.item == "rusk"} | get 0.index)
    } catch {
      -1
    }
    let rest = if $rusk_idx >= 0 {
      ($filtered | skip ($rusk_idx + 2))
    } else {
      ($filtered | skip 2)
    }
    let n = ($rest | length)
    let before_tokens = if $n > 1 { $rest | take ($n - 1) } else { [] }
    let ids_before_cur = ($before_tokens | where {|x|
      try { ($x | into int | ignore); true } catch { false }
    })
    let last_tok = try { $rest | last } catch { "" }
    if ($last_tok == $cur) and ($ids_before_cur | is-empty) {
      try {
        let current_id = ($cur | into int)
        let task_text = (get-task-text $current_id $spans)
        # The line editor drops control characters other than tab and line
        # feed from what it inserts (a CR, a bell, an escape sequence): such a
        # text would be stored changed, so it is not offered
        if ($task_text != null) and not ($task_text =~ '[\x00-\x08\x0b-\x1f]') {
          let id_str = ($current_id | into string)
          let quoted_text = (quote-if-needed $task_text)
          # A text that starts with `-` would be read as options: after `--` rusk takes it as text
          let dashes = if ($task_text | str starts-with "-") { "-- " } else { "" }
          return [{value: $"($id_str) ($dashes)($quoted_text)", description: "Append task text"}]
        }
      } catch {}
    }
  }
  
  if ($cur == "") or ($cur | str starts-with "-") {
    let all_flags = if $has_id {
      (if $has_date_on_line { [] } else { (get-date-flags) })
      | append (if $has_after_on_line { [] } else { (get-after-flags) })
      | append (get-common-flags)
    } else {
      (get-common-flags)
    }
    return (complete-flags $all_flags $cur)
  }

  []
}

# Complete mark/del commands
def complete-mark-del [cur: string, command: string, spans: list<string>] {
  # Complete flags (including empty cur after command)
  if ($cur == "") or ($cur | str starts-with "-") {
    let all_flags = if ($command == "del" or $command == "d") {
      # `--done` takes no ids: offered only before any
      let cmd_idx = ($spans | enumerate | where item == $command | get index | first)
      let has_ids = ($spans | skip ($cmd_idx + 1) | drop 1 | any {|w| $w =~ '^[0-9,]+$' })
      if $has_ids { [] } else {
        [{value: "--done", description: "Delete all completed tasks"}]
      } | append [
        {value: "--yes", description: "Delete without asking"},
        {value: "-y", description: "Delete without asking"}
      ] | append (get-common-flags)
    } else {
      [
        {value: "-p", description: "Toggle the priority flag"},
        {value: "--priority", description: "Toggle the priority flag"}
      ] | append (get-common-flags)
    }
    return (complete-flags $all_flags $cur)
  }
  
  []
}

# Complete list/restore commands (list adds -c/--compact)
def complete-list-restore [cur: string, subcommand: string] {
  if ($cur == "") or ($cur | str starts-with "-") {
    if $subcommand == "list" or $subcommand == "l" {
      return (complete-flags ((get-list-flags) | append (get-common-flags)) $cur)
    }
    return (complete-flags (get-common-flags) $cur)
  }
  []
}

# Complete search command (query is free text: --id and -h/--help)
def complete-search [cur: string] {
  if ($cur == "") or ($cur | str starts-with "-") {
    return (complete-flags ((get-search-flags) | append (get-common-flags)) $cur)
  }
  []
}

# Complete gen command (-o/--output value is a free-form file path: no candidates)
def complete-gen [cur: string, prev: string] {
  if $prev == "-o" or $prev == "--output" {
    return []
  }
  if ($cur == "") or ($cur | str starts-with "-") {
    return (complete-flags ((get-gen-flags) | append (get-common-flags)) $cur)
  }
  []
}

# Complete serve command (--host/--port values are free-form: no candidates)
def complete-serve [cur: string, prev: string] {
  if $prev == "--host" or $prev == "--port" {
    return []
  }
  if ($cur == "") or ($cur | str starts-with "-") {
    return (complete-flags ((get-serve-flags) | append (get-common-flags)) $cur)
  }
  []
}

# Complete sync command (optional push/pull subcommands; --force after them)
def complete-sync [spans: list<string>, cur: string] {
  let has_direction = ($spans | any {|s| $s == "push" or $s == "pull"})
  if $has_direction {
    if ($cur == "") or ($cur | str starts-with "-") {
      return (complete-flags ((get-force-flags) | append (get-common-flags)) $cur)
    }
    return []
  }
  if ($cur == "") {
    return ((get-sync-subcommands) | append (get-common-flags))
  } else if ($cur | str starts-with "-") {
    return (complete-flags (get-common-flags) $cur)
  } else {
    let matching = (filter-by-prefix (get-sync-subcommands) $cur)
    if ($matching | length) > 0 {
      return $matching
    }
    return []
  }
}

# Get available shells, excluding already selected ones
def get-available-shells [spans: list<string>] {
  let all_shells = (get-shells | get value)
  mut selected_shells: list<string> = []
  
  # Find install or show in spans
  mut install_show_index = -1
  for $i in 0..<($spans | length) {
    if ($spans | get $i) == "install" or ($spans | get $i) == "show" {
      $install_show_index = $i
      break
    }
  }
  
  # If we found install/show, collect all shell arguments after it
  if $install_show_index >= 0 {
    for $i in (($install_show_index + 1)..<($spans | length)) {
      let arg = ($spans | get $i)
      # Check if it's a valid shell name
      if ($all_shells | any {|shell| $shell == $arg}) {
        $selected_shells = ($selected_shells | append [$arg])
      }
    }
  }
  
  # Return shells that are not selected
  $all_shells | where {|shell| not ($selected_shells | any {|selected| $selected == $shell})}
}

# Check if we have already selected at least one shell after install/show
def has-selected-shell [spans: list<string>] {
  let available_shells = (get-available-shells $spans)
  let all_shells = (get-shells | get value)
  # If available_shells is shorter than all_shells, at least one shell was selected
  ($available_shells | length) < ($all_shells | length)
}

# Check if install or show is already in spans
def has-install-or-show [spans: list<string>] {
  ($spans | any {|s| $s == "install" or $s == "show"})
}

# Complete completions command
def complete-completions [spans: list<string>, cur: string, prev: string, word_count: int, command: string] {
  let is_after_install_or_show = ($prev == "install" or $prev == "show")
  let has_install_show = (has-install-or-show $spans)
  let cur_might_be_subcommand = ($cur != "") and (not ($cur | str starts-with "-")) and (
    ("install" | str starts-with $cur) or ("show" | str starts-with $cur)
  )
  let cur_might_be_shell = ($cur != "") and (not ($cur | str starts-with "-")) and (not $cur_might_be_subcommand) and ($cur != "install") and ($cur != "show")
  let has_shell_selected = (has-selected-shell $spans)
  
  # Show shells if after install/show (allow multiple shells)
  # Check if we're after install/show OR if we have install/show and word_count >= 3 (meaning we might be entering a shell)
  if $is_after_install_or_show or ($has_install_show and $word_count >= 3 and ($command == "completions" or $command == "c") and not $cur_might_be_subcommand) {
    # If shell is already selected, only show other shells (no flags, no install/show)
    if $has_shell_selected {
      if ($cur | str starts-with "-") {
        return (complete-flags (get-common-flags) $cur)
      }
      # Get available shells (excluding already selected) and filter by prefix
      let available_shells = (get-available-shells $spans)
      if ($available_shells | length) == 0 {
        return []
      }
      let shell_completions = ($available_shells | each {|shell|
        let shell_info = (get-shells | where {|s| $s.value == $shell} | first)
        if ($shell_info != null) {
          $shell_info
        } else {
          {value: $shell, description: $shell}
        }
      })
      if ($cur == "") {
        return $shell_completions
      } else {
        let matching = (filter-by-prefix $shell_completions $cur)
        return $matching
      }
    }
    
    # Before shell is selected, show shells and flags
    if ($cur == "") {
      # Get available shells (excluding already selected)
      let available_shells = (get-available-shells $spans)
      let shell_completions = ($available_shells | each {|shell|
        let shell_info = (get-shells | where {|s| $s.value == $shell} | first)
        if ($shell_info != null) {
          $shell_info
        } else {
          {value: $shell, description: $shell}
        }
      })
      return ($shell_completions | append (get-common-flags))
    } else if ($cur | str starts-with "-") {
      return (complete-flags (get-common-flags) $cur)
    } else {
      # Get available shells (excluding already selected) and filter by prefix
      let available_shells = (get-available-shells $spans)
      let shell_completions = ($available_shells | each {|shell|
        let shell_info = (get-shells | where {|s| $s.value == $shell} | first)
        if ($shell_info != null) {
          $shell_info
        } else {
          {value: $shell, description: $shell}
        }
      })
      let matching = (filter-by-prefix $shell_completions $cur)
      if ($matching | length) > 0 {
        return $matching
      } else {
        return []
      }
    }
  }
  
  # Complete subcommands (only if install/show not already entered)
  if (not $has_install_show) and ($prev == "completions" or $prev == "c" or $command == "completions" or $command == "c") {
    if ($cur == "") {
      return ((get-completions-subcommands) | append (get-common-flags))
    } else if ($cur | str starts-with "-") {
      return (complete-flags (get-common-flags) $cur)
    } else {
      let matching = (filter-by-prefix (get-completions-subcommands) $cur)
      if ($matching | length) > 0 {
        return $matching
      } else {
        return []
      }
    }
  }
  
  # Complete flags (only if not after install/show with shell selected)
  if (not $has_install_show) or (not $has_shell_selected) {
    if ($cur == "") or ($cur | str starts-with "-") {
      return (complete-flags (get-common-flags) $cur)
    }
  }
  
  []
}

# ============================================================================
# Main Completion Function
# ============================================================================

# Parse spans into context
def parse-spans [spans: list<string>] {
  if ($spans | is-empty) {
    return {
      has_trailing_space: false
      filtered_spans: []
      word_count: 0
      command: ""
      prev: ""
      cur: ""
    }
  }
  
  let has_trailing_space = (($spans | last) == "")
  let filtered_spans = ($spans | where $it != "")
  
  # Find rusk command index (skip environment variables like RUSK_DB=./)
  mut rusk_idx = -1
  for $i in 0..<($filtered_spans | length) {
    if ($filtered_spans | get $i) == "rusk" {
      $rusk_idx = $i
      break
    }
  }
  
  # Get spans after rusk command
  let rusk_spans = if $rusk_idx >= 0 {
    ($filtered_spans | skip ($rusk_idx + 1))
  } else {
    $filtered_spans
  }
  
  let word_count = ($rusk_spans | length)
  let command = if $word_count > 0 {
    try { ($rusk_spans | get 0) } catch { "" }
  } else {
    ""
  }
  
  # After `rusk e ` (trailing "" span): word_count is 1 but prev must be `e` for edit/mark/del logic.
  let prev = if $has_trailing_space {
    if $word_count >= 1 {
      try { ($rusk_spans | last) } catch { "" }
    } else {
      ""
    }
  } else if $word_count > 1 {
    try { ($rusk_spans | get ($word_count - 2)) } catch { "" }
  } else {
    ""
  }
  
  let cur = if $has_trailing_space {
    ""
  } else if $word_count > 0 {
    try { ($rusk_spans | last) } catch { "" }
  } else {
    ""
  }
  
  {
    has_trailing_space: $has_trailing_space
    filtered_spans: $filtered_spans
    word_count: $word_count
    command: $command
    prev: ($prev | default "")
    cur: ($cur | default "")
  }
}

# Complete root-level commands and flags
def complete-root [ctx: record] {
  # Complete flags when typing "rusk -"
  if ($ctx.word_count <= 1) and ($ctx.cur | str starts-with "-") {
    let all_flags = ((get-common-flags) | append (get-version-flags))
    return (complete-flags $all_flags $ctx.cur)
  }
  
  # Full subcommand name only (not short aliases): after `rusk c` + Tab offer `completions`/`c`;
  # after `rusk c ` + Tab delegate here (root returns []) so install/show come from complete-completions.
  let exact_subcmds = [add edit mark del list search restore gen serve sync completions]
  if ($ctx.word_count == 1) and (not $ctx.has_trailing_space) and ($ctx.cur in $exact_subcmds) {
    return []
  }
  
  # Complete commands when only "rusk" is typed (or partial command without trailing space)
  # When cur is empty or equals "rusk", return all options
  # But NOT if there's already a command and trailing space (word_count == 1 with trailing space means we're after the command)
  if ($ctx.word_count == 0) or ($ctx.word_count == 1 and not $ctx.has_trailing_space) {
    let commands = (get-commands)
    # append aliases (not `[record (each ...)]|flatten`) so nested alias lists flatten correctly in Nu
    let all_options = ($commands | each {|cmd| 
      [{value: $cmd.value, description: $cmd.description}]
      | append ($cmd.aliases | each {|alias| {value: $alias, description: $"Alias for ($cmd.value)"}})
    } | flatten)
    let all_options = ($all_options | append (get-common-flags) | append (get-version-flags))
    
    # If cur is empty or equals "rusk", return all options without filtering
    if ($ctx.cur == "") or ($ctx.cur == "rusk") {
      return $all_options
    }
    
    return (filter-by-prefix $all_options $ctx.cur)
  }
  
  # Handle partial command input
  if $ctx.word_count == 2 and not $ctx.has_trailing_space {
    let commands = (get-commands)
    let matching_commands = ($commands | where {|cmd|
      ($cmd.value | str starts-with $ctx.cur) or ($cmd.aliases | any {|alias| $alias | str starts-with $ctx.cur})
    })
    
    if ($matching_commands | length) > 0 {
      return ($matching_commands | each {|cmd| {value: $cmd.value, description: $cmd.description}})
    }
  }
  
  []
}

# When spans lack a trailing "" after the subcommand, cur stays on the command token; treat as empty for flags.
def normalize-subcommand-cur [ctx: record] {
  if ($ctx.word_count == 1) and (not $ctx.has_trailing_space) and ($ctx.cur == $ctx.command) {
    ""
  } else {
    $ctx.cur
  }
}

# Main completion function
export def rusk-completions-main [spans: list<string>] {
  let ctx = (parse-spans $spans)
  
  # Complete root-level commands
  let root_completions = (complete-root $ctx)
  if ($root_completions | length) > 0 {
    return $root_completions
  }
  
  # Handle empty spans (no command yet)
  if ($ctx.word_count == 0) {
    return []
  }
  
  let cur_n = (normalize-subcommand-cur $ctx)
  
  # Handle subcommands
  match $ctx.command {
    "add" | "a" => {
      complete-add $spans $cur_n $ctx.prev
    }
    
    "edit" | "e" => {
      complete-edit $spans $cur_n $ctx.prev $ctx.has_trailing_space $ctx.command
    }
    
    "mark" | "m" | "del" | "d" => {
      complete-mark-del $cur_n $ctx.command $spans
    }
    
    "list" | "l" | "restore" | "r" => {
      complete-list-restore $cur_n $ctx.command
    }

    "search" | "s" => {
      complete-search $cur_n
    }

    "gen" | "g" => {
      complete-gen $cur_n $ctx.prev
    }

    "serve" => {
      complete-serve $cur_n $ctx.prev
    }

    "sync" => {
      complete-sync $spans $cur_n
    }

    "completions" | "c" => {
      complete-completions $spans $cur_n $ctx.prev $ctx.word_count $ctx.command
    }
    
    _ => {
      []
    }
  }
}
