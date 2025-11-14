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
      | into int 
      | sort)
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
      | parse -r '^\s+[•✔]\s+(?<id>\d+)\s+(?<date>[^\s]+)?\s+(?<text>.*)$'
      | where id == $task_id_str
      | get text 
      | first)
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

# Get entered task IDs from spans (skip "rusk" and command)
def get-entered-ids [spans: list<string>] {
  $spans 
  | skip 2 
  | where {|it| try { ($it | into int) | ignore } catch { false } } 
  | each {|it| $it | into int}
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
  
  $filtered_ids | each {|id| 
    let task_text = (get-task-text $id)
    let id_str = ($id | into string)
    let description = if ($task_text != null) {
      let text_len = ($task_text | str length)
      let text = if $text_len > 40 {
        ($task_text | str substring 0..40) + "..."
      } else {
        $task_text
      }
      $"Task ID ($id_str): ($text)"
    } else {
      $"Task ID ($id_str)"
    }
    {value: $id_str, description: $description}
  }
}

# Main completion function
export def rusk-completions-main [spans: list<string>] {
  # Handle empty spans
  if ($spans | is-empty) {
    return []
  }

  # Check if last element is empty (indicates space after last word)
  let has_trailing_space = (($spans | last) == "")
  
  # Filter out empty strings from spans
  let filtered_spans = ($spans | where $it != "")
  
  let word_count = ($filtered_spans | length)
  let command = if $word_count > 1 {
    try { ($filtered_spans | get 1) } catch { "" }
  } else {
    ""
  }
  
  # Previous word (second-to-last element)
  # If there's a trailing space, prev is the last filtered element (the command)
  # Otherwise, it's the second-to-last element
  let prev = if $has_trailing_space and $word_count > 1 {
    try { ($filtered_spans | last) } catch { "" }
  } else if $word_count > 1 {
    try { ($filtered_spans | get ($word_count - 2)) } catch { "" }
  } else {
    ""
  }
  
  # Current word (last element)
  # If there's a trailing space, cur should be empty, otherwise it's the last filtered element
  let cur = if $has_trailing_space {
    ""
  } else {
    try { ($filtered_spans | last) } catch { "" }
  }

  # Complete commands when only "rusk" is typed
  if $word_count <= 1 {
    return [
      {value: "add", description: "Add a new task"}
      {value: "edit", description: "Edit tasks by id(s)"}
      {value: "mark", description: "Mark tasks as done/undone"}
      {value: "del", description: "Delete tasks by id(s)"}
      {value: "list", description: "List all tasks"}
      {value: "restore", description: "Restore from backup"}
      {value: "completions", description: "Install shell completions"}
      {value: "a", description: "Alias for add"}
      {value: "e", description: "Alias for edit"}
      {value: "m", description: "Alias for mark"}
      {value: "d", description: "Alias for del"}
      {value: "l", description: "Alias for list"}
      {value: "r", description: "Alias for restore"}
    ]
  }
  
  # Handle partial command completion (e.g., "rusk e<tab>")
  # Only suggest commands when there's no trailing space (user is still typing the command)
  if $word_count == 2 and not $has_trailing_space and ($cur == "e" or $cur == "a" or $cur == "m" or $cur == "d" or $cur == "l" or $cur == "r") {
    return (match $cur {
      "e" => [
        {value: "e", description: "Alias for edit"}
        {value: "edit", description: "Edit tasks by id(s)"}
      ]
      "a" => [
        {value: "a", description: "Alias for add"}
        {value: "add", description: "Add a new task"}
      ]
      "m" => [
        {value: "m", description: "Alias for mark"}
        {value: "mark", description: "Mark tasks as done/undone"}
      ]
      "d" => [
        {value: "d", description: "Alias for del"}
        {value: "del", description: "Delete tasks by id(s)"}
      ]
      "l" => [
        {value: "l", description: "Alias for list"}
        {value: "list", description: "List all tasks"}
      ]
      "r" => [
        {value: "r", description: "Alias for restore"}
        {value: "restore", description: "Restore from backup"}
      ]
      _ => []
    })
  }

  # Handle subcommands
  match $command {
    "add" | "a" => {
      # Complete --date flag with date options
      if ($prev == "--date" or $prev == "-d" or $cur == "--date" or $cur == "-d") {
        return (get-date-options)
      }
      # Complete flags
      if ($cur | str starts-with "-") {
        return [
          {value: "--date", description: "Set task date"}
          {value: "-d", description: "Set task date"}
          {value: "--help", description: "Show help"}
          {value: "-h", description: "Show help"}
        ]
      }
      return []
    }
    
    "edit" | "e" => {
      let entered_ids = (get-entered-ids $spans)
      
      # Complete --date flag with date options
      if ($prev == "--date" or $prev == "-d" or $cur == "--date" or $cur == "-d") {
        return (get-date-options)
      }
      
      # Suggest task text when appropriate
      # Case 1: "rusk e 1 <tab>" (with space after ID)
      if $has_trailing_space and (is-number $cur) {
        let task_id = ($cur | into int)
        let task_text = (get-task-text $task_id)
        if ($task_text != null) {
          return [{value: $task_text, description: "Current task text"}]
        }
      }
      
      # Case 2: "rusk e 1<tab>" (without space, first ID only)
      if (is-number $cur) and ($prev == "edit" or $prev == "e") and not $has_trailing_space {
        if ($entered_ids | length) == 0 {
          let task_id = ($cur | into int)
          let task_text = (get-task-text $task_id)
          if ($task_text != null) {
            let id_str = ($task_id | into string)
            return [{value: $"($id_str) ($task_text)", description: "Append task text"}]
          }
        }
      }
      
      # Case 3: "rusk e 1 <tab>" (space after single ID, suggest text)
      if (is-number $prev) and ($cur == "") and ($entered_ids | length) == 1 {
        let task_id = ($prev | into int)
        let task_text = (get-task-text $task_id)
        if ($task_text != null) {
          return [{value: $task_text, description: "Current task text"}]
        }
      }
      
      # Complete task IDs
      # Show IDs when: cur is empty (space after command), cur is a number, or cur doesn't start with "-"
      # But don't show IDs when cur is the command itself (user is still typing the command)
      if ($cur == "") {
        # Space after command - show IDs
        return (complete-task-ids $entered_ids)
      } else if ($cur != $command) and ((is-number $cur) or (not ($cur | str starts-with "-"))) {
        # Not the command itself, and either a number or not a flag - show IDs
        return (complete-task-ids $entered_ids)
      }
      
      # Complete flags
      if ($cur | str starts-with "-") {
        return [
          {value: "--date", description: "Set task date"}
          {value: "-d", description: "Set task date"}
          {value: "--help", description: "Show help"}
          {value: "-h", description: "Show help"}
        ]
      }
      
      return []
    }
    
    "mark" | "m" | "del" | "d" => {
      let entered_ids = (get-entered-ids $spans)
      
      # Complete task IDs (only if not completing the command itself)
      # Don't suggest IDs when current word is the command (user hasn't typed space yet)
      if ($cur != $command) and ($cur == "" or (is-number $prev) or (is-number $cur)) {
        return (complete-task-ids $entered_ids)
      }
      
      # For del, complete --done flag
      if ($command == "del" or $command == "d") {
        if ($cur | str starts-with "-") {
          return [
            {value: "--done", description: "Delete all completed tasks"}
            {value: "--help", description: "Show help"}
            {value: "-h", description: "Show help"}
          ]
        }
      }
      
      return []
    }
    
    "list" | "l" | "restore" | "r" => {
      # These commands don't take arguments
      return []
    }
    
    "completions" => {
      # Complete completions subcommands
      if ($prev == "completions") {
        return [
          {value: "install", description: "Install completions for a shell"}
          {value: "show", description: "Show completion script"}
        ]
      }
      if ($prev == "install" or $prev == "show") {
        return [
          {value: "bash", description: "Bash shell"}
          {value: "zsh", description: "Zsh shell"}
          {value: "fish", description: "Fish shell"}
          {value: "nu", description: "Nu shell"}
          {value: "powershell", description: "PowerShell"}
        ]
      }
      return []
    }
    
    _ => {
      return []
    }
  }
}
