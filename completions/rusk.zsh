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

# Get task text by ID
_rusk_get_task_text() {
    local task_id="$1"
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
    
    local task_line
    if [ -n "$rusk_db" ]; then
        task_line=$(env RUSK_DB="$rusk_db" "$rusk_cmd" list 2>/dev/null | grep -E "^\s*[•✔]\s+$task_id\s+")
    else
        task_line=$("$rusk_cmd" list 2>/dev/null | grep -E "^\s*[•✔]\s+$task_id\s+")
    fi
    
    if [ -n "$task_line" ]; then
        echo "$task_line" | sed -E 's/^[[:space:]]*[•✔][[:space:]]+[0-9]+[[:space:]]+[0-9-]*[[:space:]]*//'
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
    local start_idx=$((rusk_idx + 2))
    for ((i=start_idx; i<${#words[@]}; i++)); do
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

_rusk() {
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
            # Suggest task text if previous word is a single ID and current is empty
            elif [[ "$prev" =~ ^[0-9]+$ ]] && [[ -z "$cur" ]]; then
                if [ $(_rusk_count_ids) -eq 1 ]; then
                    local task_text=$(_rusk_get_task_text "$prev")
                    if [ -n "$task_text" ]; then
                        compadd -Q "$task_text"
                    fi
                fi
            # Complete flags
            elif [[ "$cur" == -* ]]; then
                # Offer flags: -d --date -h --help
                compadd -- -d --date -h --help
            # Complete task IDs
            else
                _rusk_complete_task_ids
            fi
            ;;
            
        mark|m|del|d)
            # For del, complete flags first
            if [[ ("$cmd" == "del" || "$cmd" == "d") && "$cur" == -* ]]; then
                compadd --done
            # Complete task IDs
            else
                _rusk_complete_task_ids
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
