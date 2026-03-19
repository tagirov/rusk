#compdef rusk

# Zsh completion script for rusk
#
# Installation:
#   1. Automatic (recommended):
#      rusk completions install zsh
#
#   2. Manual:
#      Generate script using rusk command:
#      mkdir -p ~/.zsh/completions
#      rusk completions show zsh > ~/.zsh/completions/_rusk
#
#      Then add to your ~/.zshrc:
#      fpath=(~/.zsh/completions $fpath)
#      autoload -U compinit && compinit

# Find rusk binary
_rusk_cmd() {
    command -v rusk 2>/dev/null || echo "rusk"
}

# Get list of task IDs from rusk list output
_rusk_get_task_ids() {
    local rusk_cmd=$(_rusk_cmd)
    # Check if RUSK_DB is set in command line (use full command line buffer)
    local rusk_db=""
    local -a buffer_words
    buffer_words=(${(z)LBUFFER})
    for word in "${buffer_words[@]}"; do
        if [[ "$word" =~ ^RUSK_DB=(.+)$ ]]; then
            rusk_db="${match[1]}"
            break
        fi
    done
    
    if [ -n "$rusk_db" ]; then
        env RUSK_DB="$rusk_db" "$rusk_cmd" list 2>/dev/null | grep -oE '^\s*[•✔]\s+[0-9]+' | grep -oE '[0-9]+' | sort -n
    else
        "$rusk_cmd" list 2>/dev/null | grep -oE '^\s*[•✔]\s+[0-9]+' | grep -oE '[0-9]+' | sort -n
    fi
}

# Check if text contains special characters that require quoting
_rusk_needs_quotes() {
    local text="$1"
    # Special chars: | ; & > < ( ) [ ] { } $ " ' ` \ * ? ~ # @ ! % ^ = + - / : ,
    # Using case statement for reliability across different zsh versions
    case "$text" in
        *[\|\;\&\>\<\(\)\[\]\{\}\$\"\'\`\\*\?\~\#\@\!\%\^\=\+\-\/\:\,]*)
            return 0
            ;;
    esac
    return 1
}

# Check if text contains single quote
_rusk_contains_single_quote() {
    case "$1" in
        *"'"*)
            return 0
            ;;
    esac
    return 1
}

# Quote text if it contains special characters
# Use single quotes if no single quote in text, otherwise use double quotes with escaping
_rusk_quote_text() {
    local text="$1"
    if ! _rusk_needs_quotes "$text"; then
        echo "$text"
        return
    fi
    
    # If no single quote in text, use single quotes (no escaping needed)
    if ! _rusk_contains_single_quote "$text"; then
        echo "'$text'"
    else
        # Use double quotes with escaping
        local escaped="${text//\"/\"}"
        # Escape backticks to prevent command substitution
        escaped="${escaped//\`/\\\`}"
        # Escape dollar signs to prevent variable expansion
        escaped="${escaped//\$/\$}"
        # Escape backslashes
        escaped="${escaped//\\/\\\\}"
        echo "\"$escaped\""
    fi
}

# Get task text by ID (supports multi-line tasks via rusk list --for-completion)
_rusk_get_task_text() {
    local task_id="$1"
    local rusk_cmd=$(_rusk_cmd)
    local rusk_db=""
    local -a buffer_words
    buffer_words=(${(z)LBUFFER})
    for word in "${buffer_words[@]}"; do
        if [[ "$word" =~ ^RUSK_DB=(.+)$ ]]; then
            rusk_db="${match[1]}"
            break
        fi
    done
    
    local output
    if [ -n "$rusk_db" ]; then
        output=$(env RUSK_DB="$rusk_db" "$rusk_cmd" list --for-completion 2>/dev/null)
    else
        output=$("$rusk_cmd" list --for-completion 2>/dev/null)
    fi
    
    local text="" collecting=0 id rest
    while IFS= read -r line; do
        if [[ "$line" =~ ^[0-9]+$'\t' ]]; then
            id="${line%%$'\t'*}"
            rest="${line#*$'\t'}"
            if [[ "$id" == "$task_id" ]]; then
                text="$rest"
                collecting=1
            else
                collecting=0
            fi
        elif (( collecting )); then
            text="${text}"$'\n'"${line}"
        fi
    done <<< "$output"
    
    if [ -n "$text" ]; then
        _rusk_quote_text "$text"
    fi
}

# Get raw task text by ID (no quoting) - quoting is applied later for zsh
_rusk_get_task_text_raw() {
    local task_id="$1"
    local rusk_cmd=$(_rusk_cmd)
    local rusk_db=""
    local -a buffer_words
    buffer_words=(${(z)LBUFFER})
    for word in "${buffer_words[@]}"; do
        if [[ "$word" =~ ^RUSK_DB=(.+)$ ]]; then
            rusk_db="${match[1]}"
            break
        fi
    done
    
    local output
    if [ -n "$rusk_db" ]; then
        output=$(env RUSK_DB="$rusk_db" "$rusk_cmd" list --for-completion 2>/dev/null)
    else
        output=$("$rusk_cmd" list --for-completion 2>/dev/null)
    fi
    
    local text="" collecting=0 id rest
    while IFS= read -r line; do
        if [[ "$line" =~ ^[0-9]+$'\t' ]]; then
            id="${line%%$'\t'*}"
            rest="${line#*$'\t'}"
            if [[ "$id" == "$task_id" ]]; then
                text="$rest"
                collecting=1
            else
                collecting=0
            fi
        elif (( collecting )); then
            text="${text}"$'\n'"${line}"
        fi
    done <<< "$output"
    
    if [ -n "$text" ]; then
        echo "$text"
    fi
}

# Get date options (today, tomorrow, week ahead, two weeks ahead)
_rusk_get_date_options() {
    local today=$(date +%d-%m-%Y 2>/dev/null)
    local tomorrow=$(date -d '+1 day' +%d-%m-%Y 2>/dev/null || date -v+1d +%d-%m-%Y 2>/dev/null || date +%d-%m-%Y)
    local week_ahead=$(date -d '+1 week' +%d-%m-%Y 2>/dev/null || date -v+1w +%d-%m-%Y 2>/dev/null || date +%d-%m-%Y)
    local two_weeks_ahead=$(date -d '+2 weeks' +%d-%m-%Y 2>/dev/null || date -v+2w +%d-%m-%Y 2>/dev/null || date +%d-%m-%Y)
    echo "$today" "$tomorrow" "$week_ahead" "$two_weeks_ahead"
}

# Get entered task IDs from command line
_rusk_get_entered_ids() {
    local -a entered_ids
    local i
    # Find rusk command index
    local rusk_idx=-1
    for ((i=1; i<=${#words[@]}; i++)); do
        if [[ "${words[i]}" == "rusk" ]]; then
            rusk_idx=$i
            break
        fi
    done
    # Start from word after command (rusk_idx + 2: skip "rusk" and command like "edit")
    local start_idx=$((rusk_idx + 2))
    for ((i=start_idx; i<${#words[@]}; i++)); do
        if [[ "${words[i]}" =~ ^[0-9]+$ ]]; then
            entered_ids+=("${words[i]}")
        fi
    done
    echo "${entered_ids[@]}"
}

# Filter out already entered IDs from task ID list
_rusk_filter_ids() {
    local -a ids=("${(@f)$(_rusk_get_task_ids)}")
    local -a entered_ids=($(_rusk_get_entered_ids))
    
    if [ ${#entered_ids[@]} -eq 0 ]; then
        echo "${ids[@]}"
        return
    fi
    
    local -a filtered_ids
    for id in "${ids[@]}"; do
        local found=0
        for entered in "${entered_ids[@]}"; do
            if [ "$id" = "$entered" ]; then
                found=1
                break
            fi
        done
        if [ $found -eq 0 ]; then
            filtered_ids+=("$id")
        fi
    done
    echo "${filtered_ids[@]}"
}

# Count how many IDs have been entered
_rusk_count_ids() {
    local count=0
    local i
    # Find rusk command index
    local rusk_idx=-1
    for ((i=1; i<=${#words[@]}; i++)); do
        if [[ "${words[i]}" == "rusk" ]]; then
            rusk_idx=$i
            break
        fi
    done
    # Start from word after command (rusk_idx + 2: skip "rusk" and command like "edit")
    # Stop before CURRENT (word being completed), matching bash COMP_CWORD semantics:
    # `rusk edit 5<TAB>` → count 0 → task text; `rusk edit 5 <TAB>` → count 1 → flags
    local start_idx=$((rusk_idx + 2))
    local end_idx=${CURRENT:-${#words[@]}}
    for ((i=start_idx; i<end_idx; i++)); do
        if [[ "${words[i]}" =~ ^[0-9]+$ ]]; then
            ((count++))
        fi
    done
    echo $count
}

# Complete task IDs with filtering
_rusk_complete_task_ids() {
    local -a ids=($(_rusk_filter_ids))
    if [ ${#ids[@]} -gt 0 ]; then
        compadd $ids
        return 0
    fi
    return 1
}

# Complete date values
_rusk_complete_date() {
    local -a dates=($(_rusk_get_date_options))
    compadd $dates
}

_rusk_main() {
    # Find rusk command index
    local rusk_idx=-1
    local i
    for ((i=1; i<=${#words[@]}; i++)); do
        if [[ "${words[i]}" == "rusk" ]]; then
            rusk_idx=$i
            break
        fi
    done
    
    # Complete commands (if we're right after rusk command)
    if [ $rusk_idx -ge 0 ] && [ -n "$CURRENT" ] && [ "$CURRENT" -eq $((rusk_idx + 1)) ] 2>/dev/null; then
        compadd add edit mark del list restore completions a e m d l r
        return
    fi
    
    # Get command (word after rusk)
    local cmd=""
    if [ $rusk_idx -ge 0 ] && [ $((rusk_idx + 1)) -le ${#words[@]} ]; then
        cmd="$words[$((rusk_idx + 1))]"
    fi
    
    local prev=""
    local cur=""
    
    if [ -n "$CURRENT" ] && [ "$CURRENT" -gt 1 ] 2>/dev/null; then
        prev="$words[$((CURRENT-1))]"
    fi
    if [ -n "$CURRENT" ] && [ "$CURRENT" -le ${#words[@]} ] 2>/dev/null; then
        cur="$words[$CURRENT]"
    fi
    
    case "$cmd" in
        add|a)
            if [[ "$prev" == "--date" ]] || [[ "$prev" == "-d" ]]; then
                _rusk_complete_date
            # For `rusk add <tab>` or when starting a flag, offer flags
            elif [[ -z "$cur" ]] || [[ "$cur" == -* ]]; then
                # Offer flags: -d --date -h --help
                compadd -- -d --date -h --help
            fi
            ;;
            
        edit|e)
            # Complete date flag
            if [[ "$prev" == "--date" ]] || [[ "$prev" == "-d" ]]; then
                _rusk_complete_date
            # After single ID with a space: suggest ONLY flags
            elif [[ "$prev" =~ ^[0-9]+$ ]] && [[ -z "$cur" ]]; then
                if [ $(_rusk_count_ids) -eq 1 ]; then
                    compadd -- -d --date -h --help
                fi
            # Support completion without a space after ID: `rusk edit <id><TAB>`
            # Here `$cur` is the numeric ID being edited; we append task text after it.
            elif [[ "$cur" =~ ^[0-9]+$ ]] && [[ ("$prev" == "edit" || "$prev" == "e") ]]; then
                local count_ids=$(_rusk_count_ids)
                if [ "$count_ids" -eq 0 ]; then
                    local raw_text=$(_rusk_get_task_text_raw "$cur")
                    if [ -n "$raw_text" ]; then
                        # Quote only when needed; do not escape plain spaces.
                        local quoted_text="$(_rusk_quote_text "$raw_text")"
                        # Do not assign to BUFFER in zsh completion: it's read-only in real completion context.
                        # Instead, return a completion candidate that includes the ID plus the quoted task text.
                        local completion_value="${cur} ${quoted_text}"
                        reply=()
                        # In real completion context, `compadd` will insert the value into the buffer.
                        # In our unit tests, `compadd` may be disallowed (no completion context), so we
                        # keep `reply` as the runtime evidence for what would be inserted.
                        # -Q helps zsh insert the candidate as-is (instead of backslash-escaped spaces)
                        compadd -Q -S '' -- "$completion_value" 2>/dev/null || reply=("$completion_value")
                        return 0
                    fi
                fi
            # Complete flags
            elif [[ "$cur" == -* ]]; then
                # Offer flags: -d --date -h --help
                compadd -- -d --date -h --help
            fi
            # No task ID completion for edit
            ;;
            
        mark|m|del|d)
            # For del, complete flags first
            if [[ ("$cmd" == "del" || "$cmd" == "d") && "$cur" == -* ]]; then
                compadd --done
            else
                :
            fi
            ;;
            
        list|l|restore|r)
            # No arguments
            ;;
            
        completions)
            # Third word: subcommands install/show
            if [ -n "$CURRENT" ] && [ "$CURRENT" -eq 3 ] 2>/dev/null; then
                compadd install show
            else
                # After install/show: suggest shells that haven't been used yet
                local -a all_shells=("bash" "zsh" "fish" "nu" "powershell")
                local -a selected_shells=()

                # Find index of install/show
                local install_idx=-1
                local i
                for ((i=1; i<=${#words[@]}; i++)); do
                    if [[ "${words[i]}" == "install" || "${words[i]}" == "show" ]]; then
                        install_idx=$i
                        break
                    fi
                done

                if (( install_idx > 0 )); then
                    for ((i=install_idx+1; i<=${#words[@]}; i++)); do
                        local w="${words[i]}"
                        for sh in "${all_shells[@]}"; do
                            if [[ "$w" == "$sh" ]]; then
                                selected_shells+=("$w")
                            fi
                        done
                    done
                fi

                local -a remaining_shells=()
                for sh in "${all_shells[@]}"; do
                    local found=0
                    for sel in "${selected_shells[@]}"; do
                        if [[ "$sh" == "$sel" ]]; then
                            found=1
                            break
                        fi
                    done
                    if (( ! found )); then
                        remaining_shells+=("$sh")
                    fi
                done

                if [ ${#remaining_shells[@]} -gt 0 ]; then
                    compadd "${remaining_shells[@]}"
                fi
            fi
            ;;
    esac
}

if [[ -z ${_RUSK_ZSH_SKIP_ENTRY:-} ]]; then
  _rusk_main "$@"
fi

_rusk() {
  _rusk_main "$@"
}
