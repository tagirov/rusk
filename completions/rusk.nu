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
    {value: "restore", aliases: ["r"], description: "Restore from backup"}
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

# Extract task IDs from rusk list output
def get-task-ids [] {
  try {
    let rusk_cmd = (get-rusk-cmd)
    let output = (do { ^$rusk_cmd list } | complete)
    if ($output.exit_code == 0) {
      ($output.stdout
      | lines 
      | where ($it | str contains "•") or ($it | str contains "✔")
      | parse -r '^\s+[•✔]\s+(?<id>\d+)\s+' 
      | get id 
      | into int)
    } else {
      []
    }
  } catch {
    []
  }
}

# Get task text by ID
def get-task-text [task_id: int] {
  try {
    let rusk_cmd = (get-rusk-cmd)
    let task_id_str = ($task_id | into string)
    let output = (do { ^$rusk_cmd list } | complete)
    if ($output.exit_code == 0) {
      ($output.stdout
      | lines 
      | where ($it | str contains "•") or ($it | str contains "✔")
      | parse -r '^\s+[•✔]\s+(?<id>\d+)\s+(?<date>[0-9]{2}-[0-9]{2}-[0-9]{4}\s+)?(?<text>.*)$'
      | where id == $task_id_str
      | get text 
      | first
      | str trim)
    } else {
      null
    }
  } catch {
    null
  }
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

# Check if text needs to be quoted (contains special characters that require escaping)
def needs-quotes [text: string] {
  let special_chars = ["|", ";", "&", ">", "<", "(", ")", "[", "]", "{", "}", "$", '"', "'", "`", "\\", "*", "?", "~", "#", "@", "!", "%", "^", "=", "+", "-", "/", ":", ",", "."]
  let chars = ($text | split chars)
  ($chars | any {|char| $char in $special_chars})
}

# Wrap text in quotes if it contains special characters
def quote-if-needed [text: string] {
  if (needs-quotes $text) {
    let escaped = ($text | str replace '"' '\\"')
    $"\"($escaped)\""
  } else {
    $text
  }
}

# Get entered task IDs from spans (skip "rusk" and command)
# Handles both space-separated and comma-separated IDs
def get-entered-ids [spans: list<string>] {
  let args = ($spans | skip 2 | where $it != "")
  
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

# Check if task text has already been entered (after IDs and flags)
def has-task-text [spans: list<string>] {
  let args = ($spans | skip 2 | where $it != "")
  if ($args | is-empty) {
    return false
  }
  
  def is-date-value [arg: string] {
    ($arg | parse -r '^\d{2}-\d{2}-\d{4}$' | length) > 0
  }
  
  ($args | enumerate | any {|item|
    let idx = $item.index
    let arg = $item.item
    let prev_arg = if $idx > 0 { ($args | get ($idx - 1)) } else { "" }
    
    if ($prev_arg == "-d" or $prev_arg == "--date") {
      false
    } else if $arg == "-d" or $arg == "--date" {
      false
    } else if ($arg | str starts-with "-") {
      false
    } else if (is-date-value $arg) {
      false
    } else {
      let is_id = (try { ($arg | into int | ignore); true } catch { false })
      if $is_id {
        false
      } else {
        true
      }
    }
  })
}

# Generate date completion options
def get-date-options [] {
  try {
    let today = (date now | format date "%d-%m-%Y")
    let tomorrow = ((date now) + 1day | format date "%d-%m-%Y")
    let week_ahead = ((date now) + 7day | format date "%d-%m-%Y")
    let two_weeks_ahead = ((date now) + 14day | format date "%d-%m-%Y")
    [
      {value: $today, description: $"Today ($today)"}
      {value: $tomorrow, description: $"Tomorrow ($tomorrow)"}
      {value: $week_ahead, description: $"One week ahead ($week_ahead)"}
      {value: $two_weeks_ahead, description: $"Two weeks ahead ($two_weeks_ahead)"}
    ]
  } catch {
    []
  }
}

# Complete task IDs with descriptions
def complete-task-ids [entered_ids: list<int>] {
  let all_ids = (get-task-ids)
  let filtered_ids = if ($entered_ids | is-empty) {
    $all_ids
  } else {
    $all_ids | where {|id| not ($entered_ids | any {|entered| $entered == $id }) }
  }
  
  ($filtered_ids | reverse | each {|id| 
    let task_text = (get-task-text $id)
    let id_str = ($id | into string)
    let description = if ($task_text != null) {
      let text_len = ($task_text | str length)
      let text = if $text_len > 80 {
        ($task_text | split chars | first 80 | str join "") + "..."
      } else {
        $task_text
      }
      $"Task ID ($id_str): ($text)"
    } else {
      $"Task ID ($id_str)"
    }
    {value: $id_str, description: $description}
  })
}

# ============================================================================
# Completion Helper Functions
# ============================================================================

# Filter completions by current input (narrowing)
def filter-by-prefix [completions: list<record>, prefix: string] {
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

# Check if word is a date flag
def is-date-flag [word: string] {
  ($word == "--date" or $word == "-d")
}

# Complete date flag values
def complete-date-flag [cur: string, prev: string] {
  let cur_is_date_flag = (is-date-flag $cur)
  let prev_is_date_flag = (is-date-flag $prev)
  let cur_starts_with_d = ($cur | str starts-with "-d")
  
  if $cur_is_date_flag or $cur_starts_with_d or $prev_is_date_flag {
    let date_options = (get-date-options)
    
    if $cur_is_date_flag {
      let flag = $cur
      ($date_options | each {|item|
        {value: $"($flag) ($item.value)", description: $item.description}
      })
    } else if $cur_starts_with_d {
      $date_options
    } else {
      $date_options
    }
  } else {
    []
  }
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

# Complete task IDs with comma handling
def complete-task-ids-with-comma [entered_ids: list<int>, cur: string, prev: string] {
  let cur_ends_with_comma = (ends-with-comma $cur)
  let prev_ends_with_comma = (ends-with-comma $prev)
  let prev_id = (extract-id $prev)
  let cur_id = (extract-id $cur)
  
  if (($prev_ends_with_comma and (is-number $prev_id)) or ($cur_ends_with_comma and (is-number $cur_id))) {
    let prefix = if $cur_ends_with_comma { $cur } else { "" }
    let completions = (complete-task-ids $entered_ids)
    
    if ($prefix | str length) > 0 {
      ($completions | each {|item|
        {value: $"($prefix)($item.value)", description: $item.description}
      })
    } else {
      $completions
    }
  } else {
    []
  }
}

# Get task text completion for edit command
def get-task-text-completion [task_id: int] {
  let task_text = (get-task-text $task_id)
  if ($task_text != null) {
    let id_str = ($task_id | into string)
    let cyan_start = (char -u "001b") + "[36m"
    let reset = (char -u "001b") + "[0m"
    let description = $"Current text[($cyan_start)($id_str)($reset)]:"
    let quoted_text = (quote-if-needed $task_text)
    [{value: $quoted_text, description: $description}]
  } else {
    []
  }
}

# ============================================================================
# Command-Specific Completion Functions
# ============================================================================

# Complete add command
def complete-add [cur: string, prev: string] {
  # Handle date flag completion
  let date_completions = (complete-date-flag $cur $prev)
  if ($date_completions | length) > 0 {
    return $date_completions
  }
  
  # Complete flags
  if ($cur == "") or ($cur | str starts-with "-") {
    let all_flags = ((get-date-flags) | append (get-common-flags))
    
    if ($cur == "") {
      return $all_flags
    }
    
    let cur_starts_with_d = ($cur | str starts-with "-d")
    if not $cur_starts_with_d {
      return (complete-flags $all_flags $cur)
    }
  }
  
  []
}

# Complete edit command
def complete-edit [spans: list<string>, cur: string, prev: string, has_trailing_space: bool, command: string] {
  let entered_ids = (get-entered-ids $spans)
  
  # If task text has already been entered, stop completion
  if (has-task-text $spans) {
    return []
  }
  
  # Handle date flag completion
  let date_completions = (complete-date-flag $cur $prev)
  if ($date_completions | length) > 0 {
    return $date_completions
  }
  
  let prev_ends_with_comma = (ends-with-comma $prev)
  let cur_ends_with_comma = (ends-with-comma $cur)
  let prev_id = (extract-id $prev)
  let cur_id = (extract-id $cur)
  
  # Case 1: Comma after ID - suggest next IDs
  let comma_completions = (complete-task-ids-with-comma $entered_ids $cur $prev)
  if ($comma_completions | length) > 0 {
    return $comma_completions
  }
  
  # Case 2: Space after single ID - suggest task text
  if ($cur == "") and (is-number $prev_id) and not $prev_ends_with_comma {
    if ($entered_ids | length) == 1 {
      let task_id = ($prev_id | into int)
      return (get-task-text-completion $task_id)
    }
  }
  
  # Case 3: Partial ID input
  if ($cur != "") and (is-number $cur) and ($prev == "edit" or $prev == "e") and not $has_trailing_space {
    let all_ids = (get-task-ids)
    let matching_ids = ($all_ids | where {|id| 
      let id_str = ($id | into string)
      ($id_str | str starts-with $cur)
    })
    
    if ($matching_ids | length) > 0 {
      return ($matching_ids | each {|id| 
        let task_text = (get-task-text $id)
        let id_str = ($id | into string)
        let description = if ($task_text != null) {
          let text_len = ($task_text | str length)
          let text = if $text_len > 80 {
            ($task_text | split chars | first 80 | str join "") + "..."
          } else {
            $task_text
          }
          $"Task ID ($id_str): ($text)"
        } else {
          $"Task ID ($id_str)"
        }
        {value: $id_str, description: $description}
      })
    } else {
      if ($entered_ids | length) == 0 {
        try {
          let current_id = ($cur | into int)
          let task_text = (get-task-text $current_id)
          if ($task_text != null) {
            let id_str = ($current_id | into string)
            let quoted_text = (quote-if-needed $task_text)
            return [{value: $"($id_str) ($quoted_text)", description: "Append task text"}]
          }
        } catch {}
      }
    }
  }
  
  # Complete task IDs
  if ($cur == "") {
    return (complete-task-ids $entered_ids)
  } else if ($cur != $command) and (not ($cur | str starts-with "-")) and (not (is-number $cur)) {
    return (complete-task-ids $entered_ids)
  }
  
  # Complete flags
  if ($cur | str starts-with "-") {
    let all_flags = ((get-date-flags) | append (get-common-flags))
    return (complete-flags $all_flags $cur)
  }
  
  []
}

# Complete mark/del commands
def complete-mark-del [spans: list<string>, cur: string, prev: string, command: string] {
  let entered_ids = (get-entered-ids $spans)
  let cur_ends_with_comma = (ends-with-comma $cur)
  let prev_ends_with_comma = (ends-with-comma $prev)
  let prev_contains_comma = ($prev | str contains ",")
  
  # Determine if we should suggest IDs
  let should_suggest_ids = if ($cur != $command) {
    if $cur_ends_with_comma or $prev_ends_with_comma {
      true
    } else if $cur == "" {
      ($entered_ids | is-empty) or (not $prev_contains_comma) or $prev_ends_with_comma
    } else {
      false
    }
  } else {
    false
  }
  
  if $should_suggest_ids {
    let completions = (complete-task-ids $entered_ids)
    
    if $cur_ends_with_comma {
      let prefix = $cur
      return ($completions | each {|item|
        {value: $"($prefix)($item.value)", description: $item.description}
      })
    } else {
      return $completions
    }
  }
  
  # Complete flags
  if ($cur | str starts-with "-") {
    let all_flags = if ($command == "del" or $command == "d") {
      [
        {value: "--done", description: "Delete all completed tasks"}
      ] | append (get-common-flags)
    } else {
      get-common-flags
    }
    return (complete-flags $all_flags $cur)
  }
  
  []
}

# Complete list/restore commands
def complete-list-restore [cur: string] {
  if ($cur == "") or ($cur | str starts-with "-") {
    return (complete-flags (get-common-flags) $cur)
  }
  []
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
        # Don't suggest flags after shell is selected
        return []
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
  let word_count = ($filtered_spans | length)
  let command = if $word_count > 1 {
    try { ($filtered_spans | get 1) } catch { "" }
  } else {
    ""
  }
  
  let prev = if $has_trailing_space and $word_count > 1 {
    try { ($filtered_spans | last) } catch { "" }
  } else if $word_count > 1 {
    try { ($filtered_spans | get ($word_count - 2)) } catch { "" }
  } else {
    ""
  }
  
  let cur = if $has_trailing_space {
    ""
  } else {
    try { ($filtered_spans | last) } catch { "" }
  }
  
  {
    has_trailing_space: $has_trailing_space
    filtered_spans: $filtered_spans
    word_count: $word_count
    command: $command
    prev: $prev
    cur: $cur
  }
}

# Complete root-level commands and flags
def complete-root [ctx: record] {
  # Complete flags when typing "rusk -"
  if ($ctx.word_count <= 1) and ($ctx.cur | str starts-with "-") {
    let all_flags = ((get-common-flags) | append (get-version-flags))
    return (complete-flags $all_flags $ctx.cur)
  }
  
  # Complete commands when only "rusk" is typed
  # When cur is empty or equals "rusk", return all options
  if $ctx.word_count <= 1 {
    let commands = (get-commands)
    let all_options = ($commands | each {|cmd| 
      [
        {value: $cmd.value, description: $cmd.description}
        ($cmd.aliases | each {|alias| {value: $alias, description: $"Alias for ($cmd.value)"}})
      ]
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

# Main completion function
export def rusk-completions-main [spans: list<string>] {
  let ctx = (parse-spans $spans)
  
  # Handle empty spans
  if ($ctx.word_count == 0) {
    return []
  }
  
  # Complete root-level commands
  let root_completions = (complete-root $ctx)
  if ($root_completions | length) > 0 {
    return $root_completions
  }
  
  # Handle subcommands
  match $ctx.command {
    "add" | "a" => {
      complete-add $ctx.cur $ctx.prev
    }
    
    "edit" | "e" => {
      complete-edit $spans $ctx.cur $ctx.prev $ctx.has_trailing_space $ctx.command
    }
    
    "mark" | "m" | "del" | "d" => {
      complete-mark-del $spans $ctx.cur $ctx.prev $ctx.command
    }
    
    "list" | "l" | "restore" | "r" => {
      complete-list-restore $ctx.cur
    }
    
    "completions" | "c" => {
      complete-completions $spans $ctx.cur $ctx.prev $ctx.word_count $ctx.command
    }
    
    _ => {
      []
    }
  }
}
