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
      | into int)
      # Don't sort - preserve the order from rusk list (which shows tasks in their natural order)
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
  # Special characters in Nushell that require quoting (excluding spaces)
  let special_chars = ["|", ";", "&", ">", "<", "(", ")", "[", "]", "{", "}", "$", '"', "'", "`", "\\", "*", "?", "~", "#", "@", "!", "%", "^", "=", "+", "-", "/", ":", ",", "."]
  let chars = ($text | split chars)
  ($chars | any {|char| $char in $special_chars})
}

# Wrap text in quotes if it contains special characters
def quote-if-needed [text: string] {
  if (needs-quotes $text) {
    # Escape double quotes inside the text and wrap in double quotes
    # Replace " with \" for proper escaping
    let escaped = ($text | str replace '"' '\\"')
    # Wrap in double quotes
    $"\"($escaped)\""
  } else {
    $text
  }
}

# Get entered task IDs from spans (skip "rusk" and command)
# Handles both space-separated and comma-separated IDs
def get-entered-ids [spans: list<string>] {
  let args = ($spans | skip 2 | where $it != "")
  
  # Process each argument and collect IDs
  ($args | reduce --fold [] {|arg, acc|
    # Skip if it's a flag
    if ($arg | str starts-with "-") {
      $acc
    } else if ($arg | str contains ",") {
      # Handle comma-separated IDs
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
      # Try to parse as single ID
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
  
  # Check if argument looks like a date (DD-MM-YYYY format)
  def is-date-value [arg: string] {
    ($arg | parse -r '^\d{2}-\d{2}-\d{4}$' | length) > 0
  }
  
  # Process arguments with index to check previous element
  ($args | enumerate | any {|item|
    let idx = $item.index
    let arg = $item.item
    let prev_arg = if $idx > 0 { ($args | get ($idx - 1)) } else { "" }
    
    # Skip if this is a date value following a date flag
    if ($prev_arg == "-d" or $prev_arg == "--date") {
      false
    } else if $arg == "-d" or $arg == "--date" {
      # Skip date flag itself
      false
    } else if ($arg | str starts-with "-") {
      # It's a flag, not task text
      false
    } else if (is-date-value $arg) {
      # It's a date value, not task text
      false
    } else {
      # Check if it's a number (ID)
      let is_id = (try { ($arg | into int | ignore); true } catch { false })
      if $is_id {
        false
      } else {
        # Not an ID, not a flag, not a date - this is task text
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
  
  # Reverse the order so that newer tasks (higher IDs) appear first
  # This prevents always starting with ID 1
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

  # Complete flags when user types "rusk -" or "rusk --"
  # Handle both "rusk -<tab>" (word_count == 2) and "rusk <tab>" with cur starting with "-" (word_count <= 1)
  # But only if we're not in a subcommand (word_count <= 1 means no command yet)
  if ($word_count <= 1) and ($cur | str starts-with "-") {
    let all_flags = [
      {value: "--help", description: "Show help"}
      {value: "-h", description: "Show help"}
      {value: "--version", description: "Show version"}
      {value: "-V", description: "Show version"}
    ]
    
    # Filter flags that start with current input (narrowing)
    let matching_flags = ($all_flags | where {|flag|
      ($flag.value | str starts-with $cur)
    })
    
    if ($matching_flags | length) > 0 {
      return $matching_flags
    } else {
      return $all_flags
    }
  }
  
  # Complete commands and flags when only "rusk" is typed
  if $word_count <= 1 {
    # List of all commands, aliases, and flags
    let all_options = [
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
      {value: "c", description: "Alias for completions"}
      {value: "--help", description: "Show help"}
      {value: "-h", description: "Show help"}
      {value: "--version", description: "Show version"}
      {value: "-V", description: "Show version"}
    ]
    
    # If current word is empty, return all options
    if ($cur == "") {
      return $all_options
    }
    
    # Filter options that start with current input (narrowing as user types)
    # This handles cases like "rusk ad<tab>" -> "add"
    let matching_options = ($all_options | where {|option|
      ($option.value | str starts-with $cur)
    })
    
    if ($matching_options | length) > 0 {
      return $matching_options
    } else {
      # If no matches but cur doesn't start with -, return empty (invalid input)
      # If cur starts with -, it's handled by the flag completion logic above
      return []
    }
  }
  
  # Handle partial command input (e.g., "rusk ad<tab>" -> "add", "rusk ma<tab>" -> "mark")
  # This works for both full commands and aliases
  if $word_count == 2 and not $has_trailing_space {
    # List of all commands and their aliases
    let commands = [
      {value: "add", aliases: ["a"], description: "Add a new task"}
      {value: "edit", aliases: ["e"], description: "Edit tasks by id(s)"}
      {value: "mark", aliases: ["m"], description: "Mark tasks as done/undone"}
      {value: "del", aliases: ["d"], description: "Delete tasks by id(s)"}
      {value: "list", aliases: ["l"], description: "List all tasks"}
      {value: "restore", aliases: ["r"], description: "Restore from backup"}
      {value: "completions", aliases: ["c"], description: "Install shell completions"}
    ]
    
    # Filter commands that match current input (either command name or alias starts with cur)
    let matching_commands = ($commands | where {|cmd|
      ($cmd.value | str starts-with $cur) or ($cmd.aliases | any {|alias| $alias | str starts-with $cur})
    })
    
    if ($matching_commands | length) > 0 {
      # Return matching commands with their full names
      return ($matching_commands | each {|cmd| {value: $cmd.value, description: $cmd.description}})
    }
    
    # If no matches, return empty (invalid input)
    return []
  }

  # Handle subcommands
  match $command {
    "add" | "a" => {
      # Complete --date flag with date options
      # Check if current word is the flag itself (user typed -d or --date without space)
      # Also check if current word starts with -d (e.g., "rusk a -d<tab>")
      let cur_is_date_flag = ($cur == "--date" or $cur == "-d")
      let cur_starts_with_d = ($cur | str starts-with "-d")
      let prev_is_date_flag = ($prev == "--date" or $prev == "-d")
      
      # If current word is exactly the date flag or starts with -d, or previous word was the flag
      # Only suggest dates when we're actually dealing with the date flag
      if $cur_is_date_flag or $cur_starts_with_d or $prev_is_date_flag {
        let date_options = (get-date-options)
        
        # If current word is exactly the flag (-d or --date), preserve it when suggesting dates
        # This prevents overwriting the flag when user selects a date
        if $cur_is_date_flag {
          let flag = $cur
          return ($date_options | each {|item|
            {value: $"($flag) ($item.value)", description: $item.description}
          })
        } else if $cur_starts_with_d {
          # Current word starts with -d (e.g., "-d"), suggest dates directly
          # The flag will be completed by the shell, so we just return dates
          return $date_options
        } else {
          # Previous word was the flag, suggest dates
          return $date_options
        }
      }
      
      # Complete flags
      # Offer flags when current word is empty or starts with -
      if ($cur == "") or ($cur | str starts-with "-") {
        let all_flags = [
          {value: "--date", description: "Set task date"}
          {value: "-d", description: "Set task date"}
          {value: "--help", description: "Show help"}
          {value: "-h", description: "Show help"}
        ]
        
        # If current word is empty, return all flags (not dates - dates are only for -d flag)
        if ($cur == "") {
          return $all_flags
        }
        
        # Filter flags that start with current input (narrowing)
        # But don't show flags if current word starts with -d (we want dates instead)
        if not $cur_starts_with_d {
          # When cur is just "-", return all flags that start with "-"
          if ($cur == "-") {
            return $all_flags
          }
          
          # Filter flags that start with current input
          let matching_flags = ($all_flags | where {|flag|
            ($flag.value | str starts-with $cur)
          })
          
          if ($matching_flags | length) > 0 {
            return $matching_flags
          } else {
            # If no matching flags but cur starts with -, return all flags starting with -
            if ($cur | str starts-with "-") {
              return $all_flags
            } else {
              return []
            }
          }
        }
      }
      return []
    }
    
    "edit" | "e" => {
      let entered_ids = (get-entered-ids $spans)
      
      # If task text has already been entered, stop completion
      if (has-task-text $spans) {
        return []
      }
      
      # Complete --date flag with date options
      # Check if current word is the flag itself (user typed -d or --date without space)
      let cur_is_date_flag = ($cur == "--date" or $cur == "-d")
      let prev_is_date_flag = ($prev == "--date" or $prev == "-d")
      
      if $cur_is_date_flag or $prev_is_date_flag {
        let date_options = (get-date-options)
        
        # If current word is the flag, preserve it when suggesting dates
        # This prevents overwriting the flag when user selects a date
        if $cur_is_date_flag {
          let flag = $cur
          return ($date_options | each {|item|
            {value: $"($flag) ($item.value)", description: $item.description}
          })
        } else {
          return $date_options
        }
      }
      
      # Check if previous word ends with comma (indicates comma-separated IDs)
      # Also check if current word ends with comma (user just typed comma)
      let prev_ends_with_comma = ($prev | str ends-with ",")
      let cur_ends_with_comma = ($cur | str ends-with ",")
      
      # Extract ID from prev or cur (removing comma if present)
      let prev_id = if $prev_ends_with_comma {
        ($prev | str substring 0..(($prev | str length) - 1))
      } else {
        $prev
      }
      
      let cur_id = if $cur_ends_with_comma {
        ($cur | str substring 0..(($cur | str length) - 1))
      } else {
        $cur
      }
      
      # Case 2: "rusk e 1,<tab>" (comma after ID, suggest next IDs)
      # Priority: comma after ID means user wants to add more IDs - check this FIRST
      # Check both: prev ends with comma OR cur ends with comma
      if (($prev_ends_with_comma and (is-number $prev_id)) or ($cur_ends_with_comma and (is-number $cur_id))) {
        # If cur ends with comma, preserve it when suggesting IDs
        # This prevents overwriting the previous ID when user selects a completion
        let prefix = if $cur_ends_with_comma {
          $cur  # Keep "1," as prefix
        } else {
          ""    # No prefix needed if comma is in prev
        }
        
        # Suggest next available IDs (excluding already entered ones)
        let completions = (complete-task-ids $entered_ids)
        
        # If we have a prefix, prepend it to all values
        if ($prefix | str length) > 0 {
          return ($completions | each {|item|
            {value: $"($prefix)($item.value)", description: $item.description}
          })
        } else {
          return $completions
        }
      }
      
      # Case 1: "rusk e 1 <tab>" (with space after ID, suggest text)
      # Priority: space after ID means user wants to edit text, not add more IDs
      # Only suggest text if there's exactly one ID (multiple IDs can't share the same text)
      # If multiple IDs and space after last ID, don't suggest anything
      if ($cur == "") and (is-number $prev_id) and not $prev_ends_with_comma {
        # If multiple IDs entered, don't suggest anything after space
        if ($entered_ids | length) > 1 {
          return []
        }
        # Only suggest text if there's exactly one ID entered
        if ($entered_ids | length) == 1 {
          let task_id = ($prev_id | into int)
          let task_text = (get-task-text $task_id)
          if ($task_text != null) {
            # Format: "Current text[id]:" with colored ID (cyan)
            let id_str = ($task_id | into string)
            # Use ANSI escape codes for cyan color
            let cyan_start = (char -u "001b") + "[36m"
            let reset = (char -u "001b") + "[0m"
            let description = $"Current text[($cyan_start)($id_str)($reset)]:"
            let quoted_text = (quote-if-needed $task_text)
            return [{value: $quoted_text, description: $description}]
          }
        }
      }
      
      # Case 3: "rusk e 1<tab>" (without space/comma, partial ID input)
      # Narrow down options as user types (e.g., "1" shows 1 and 11, "11" shows only 11)
      # Check if current word is a number (partial ID input) and previous is the command
      if ($cur != "") and (is-number $cur) and ($prev == "edit" or $prev == "e") and not $has_trailing_space {
        # Find all IDs that start with the current partial input
        let all_ids = (get-task-ids)
        let matching_ids = ($all_ids | where {|id| 
          let id_str = ($id | into string)
          ($id_str | str starts-with $cur)
        })
        
        if ($matching_ids | length) > 0 {
          # Show all matching IDs (narrowing down as user types)
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
            # No matching IDs found, try to suggest task text if current input is a valid ID
            # Only suggest text if no other IDs are entered (single ID editing)
            if ($entered_ids | length) == 0 {
              try {
                let current_id = ($cur | into int)
                let task_text = (get-task-text $current_id)
                if ($task_text != null) {
                  let id_str = ($current_id | into string)
                  let quoted_text = (quote-if-needed $task_text)
                  return [{value: $"($id_str) ($quoted_text)", description: "Append task text"}]
                }
              } catch {
                # Not a valid ID, do nothing
              }
            }
          }
      }
      
      # Complete task IDs
      # Show IDs when: cur is empty (space after command), or cur doesn't start with "-"
      # But don't show IDs when cur is the command itself (user is still typing the command)
      # Also skip if cur is a number (handled by Case 3 above)
      if ($cur == "") {
        # Space after command - show IDs
        return (complete-task-ids $entered_ids)
      } else if ($cur != $command) and (not ($cur | str starts-with "-")) and (not (is-number $cur)) {
        # Not the command itself, not a flag, and not a number - show IDs
        # This handles cases where user types partial non-numeric input
        return (complete-task-ids $entered_ids)
      }
      
      # Complete flags (only when typing -)
      # For edit command, flags are only shown when user types -, not when cur is empty
      if ($cur | str starts-with "-") {
        let all_flags = [
          {value: "--date", description: "Set task date"}
          {value: "-d", description: "Set task date"}
          {value: "--help", description: "Show help"}
          {value: "-h", description: "Show help"}
        ]
        
        # Filter flags that start with current input (narrowing)
        let matching_flags = ($all_flags | where {|flag|
          ($flag.value | str starts-with $cur)
        })
        
        if ($matching_flags | length) > 0 {
          return $matching_flags
        } else {
          return $all_flags
        }
      }
      
      return []
    }
    
    "mark" | "m" | "del" | "d" => {
      let entered_ids = (get-entered-ids $spans)
      
      # Check if current word ends with comma (indicates comma-separated IDs)
      let cur_ends_with_comma = ($cur | str ends-with ",")
      # Check if previous word ends with comma
      let prev_ends_with_comma = ($prev | str ends-with ",")
      # Check if previous word contains comma (indicates multiple IDs already entered)
      let prev_contains_comma = ($prev | str contains ",")
      
      # Complete task IDs if:
      # 1. There's a comma before tab (comma-separated IDs)
      # 2. Current word is empty (space after command, e.g., "rusk m <tab>" or "rusk d <tab>")
      #    BUT: Don't suggest IDs if prev contains comma and doesn't end with comma
      #    (e.g., "rusk mark 8,5 <tab>" or "rusk del 8,5 <tab>" - already have multiple IDs, don't suggest more)
      # Don't suggest IDs when current word is the command (user hasn't typed space yet)
      let should_suggest_ids = if ($cur != $command) {
        if $cur_ends_with_comma or $prev_ends_with_comma {
          true
        } else if $cur == "" {
          # Only suggest IDs if:
          # - No IDs entered yet, OR
          # - Previous word doesn't contain comma (single ID), OR
          # - Previous word ends with comma (user wants to add more IDs)
          ($entered_ids | is-empty) or (not $prev_contains_comma) or $prev_ends_with_comma
        } else {
          false
        }
      } else {
        false
      }
      
      if $should_suggest_ids {
        let completions = (complete-task-ids $entered_ids)
        
        # If cur ends with comma, preserve it when suggesting IDs
        # This prevents overwriting the previous ID when user selects a completion
        if $cur_ends_with_comma {
          let prefix = $cur  # Keep "1," as prefix
          return ($completions | each {|item|
            {value: $"($prefix)($item.value)", description: $item.description}
          })
        } else {
          return $completions
        }
      }
      
      # Complete flags only when current word starts with -
      if ($cur | str starts-with "-") {
        let all_flags = if ($command == "del" or $command == "d") {
          [
            {value: "--done", description: "Delete all completed tasks"}
            {value: "--help", description: "Show help"}
            {value: "-h", description: "Show help"}
          ]
        } else {
          [
            {value: "--help", description: "Show help"}
            {value: "-h", description: "Show help"}
          ]
        }
        
        # Filter flags that start with current input (narrowing)
        let matching_flags = ($all_flags | where {|flag|
          ($flag.value | str starts-with $cur)
        })
        
        if ($matching_flags | length) > 0 {
          return $matching_flags
        } else {
          return $all_flags
        }
      }
      
      return []
    }
    
    "list" | "l" | "restore" | "r" => {
      # Complete flags when current word is empty or starts with -
      if ($cur == "") or ($cur | str starts-with "-") {
        let all_flags = [
          {value: "--help", description: "Show help"}
          {value: "-h", description: "Show help"}
        ]
        
        # If current word is empty, return all flags
        if ($cur == "") {
          return $all_flags
        }
        
        # Filter flags that start with current input (narrowing)
        let matching_flags = ($all_flags | where {|flag|
          ($flag.value | str starts-with $cur)
        })
        
        if ($matching_flags | length) > 0 {
          return $matching_flags
        } else {
          return $all_flags
        }
      }
      return []
    }
    
    "completions" | "c" => {
      # Check if we're after "install" or "show" subcommand FIRST
      # This handles "rusk completions install <tab>" and "rusk completions install ba<tab>"
      # We need to check this BEFORE checking if we're after "completions" command
      # to avoid showing subcommands when we should show shells
      let is_after_install_or_show = ($prev == "install" or $prev == "show")
      # Check if current word might be a subcommand (install or show) being typed
      let cur_might_be_subcommand = ($cur != "") and (not ($cur | str starts-with "-")) and (
        ("install" | str starts-with $cur) or ("show" | str starts-with $cur)
      )
      # Also check if current word starts with shell name (partial input)
      # But only if it's NOT a subcommand being typed
      let cur_might_be_shell = ($cur != "") and (not ($cur | str starts-with "-")) and (not $cur_might_be_subcommand) and ($cur != "install") and ($cur != "show")
      
      # Only show shells if we're definitely after install/show, not when typing subcommand
      if $is_after_install_or_show or ($word_count >= 3 and $cur_might_be_shell and ($command == "completions" or $command == "c") and not $cur_might_be_subcommand) {
        # When prev is "install" or "show", show shells and flags
        if ($cur == "") {
          return [
            {value: "bash", description: "Bash shell"}
            {value: "zsh", description: "Zsh shell"}
            {value: "fish", description: "Fish shell"}
            {value: "nu", description: "Nu shell"}
            {value: "powershell", description: "PowerShell"}
            {value: "--help", description: "Show help"}
            {value: "-h", description: "Show help"}
          ]
        } else if ($cur | str starts-with "-") {
          # User is typing a flag, show only flags
          let all_flags = [
            {value: "--help", description: "Show help"}
            {value: "-h", description: "Show help"}
          ]
          let matching_flags = ($all_flags | where {|flag|
            ($flag.value | str starts-with $cur)
          })
          if ($matching_flags | length) > 0 {
            return $matching_flags
          } else {
            return $all_flags
          }
        } else {
          # User is typing a shell name, show matching shells (partial input support)
          # This handles "rusk completions install ba<tab>" -> "bash"
          let shells = [
            {value: "bash", description: "Bash shell"}
            {value: "zsh", description: "Zsh shell"}
            {value: "fish", description: "Fish shell"}
            {value: "nu", description: "Nu shell"}
            {value: "powershell", description: "PowerShell"}
          ]
          let matching = ($shells | where {|item| ($item.value | str starts-with $cur) })
          if ($matching | length) > 0 {
            return $matching
          } else {
            # If no match, return empty (invalid input)
            return []
          }
        }
      }
      
      # Complete completions subcommands
      # Check if we're after "completions" or "c" command
      # This handles both "rusk completions <tab>" and "rusk completions ins<tab>"
      # This check comes AFTER the install/show check to avoid conflicts
      if ($prev == "completions" or $prev == "c" or $command == "completions" or $command == "c") {
        # When prev is "completions" or "c" and cur is empty, show subcommands and flags
        if ($cur == "") {
          return [
            {value: "install", description: "Install completions for a shell"}
            {value: "show", description: "Show completion script"}
            {value: "--help", description: "Show help"}
            {value: "-h", description: "Show help"}
          ]
        } else if ($cur | str starts-with "-") {
          # User is typing a flag, show only flags
          let all_flags = [
            {value: "--help", description: "Show help"}
            {value: "-h", description: "Show help"}
          ]
          let matching_flags = ($all_flags | where {|flag|
            ($flag.value | str starts-with $cur)
          })
          if ($matching_flags | length) > 0 {
            return $matching_flags
          } else {
            return $all_flags
          }
        } else {
          # User is typing a subcommand, show matching subcommands (partial input support)
          # This handles "rusk completions ins<tab>" -> "install"
          let subcommands = [
            {value: "install", description: "Install completions for a shell"}
            {value: "show", description: "Show completion script"}
          ]
          let matching = ($subcommands | where {|item| ($item.value | str starts-with $cur) })
          if ($matching | length) > 0 {
            return $matching
          } else {
            # If no match, return empty (invalid input)
            return []
          }
        }
      }
      
      # Complete flags when current word is empty or starts with -
      if ($cur == "") or ($cur | str starts-with "-") {
        let all_flags = [
          {value: "--help", description: "Show help"}
          {value: "-h", description: "Show help"}
        ]
        
        # If current word is empty, return all flags
        if ($cur == "") {
          return $all_flags
        }
        
        # Filter flags that start with current input (narrowing)
        let matching_flags = ($all_flags | where {|flag|
          ($flag.value | str starts-with $cur)
        })
        
        if ($matching_flags | length) > 0 {
          return $matching_flags
        } else {
          return $all_flags
        }
      }
      
      return []
    }
    
    _ => {
      return []
    }
  }
}
