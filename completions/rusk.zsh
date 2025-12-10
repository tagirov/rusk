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
    "$rusk_cmd" list 2>/dev/null | grep -oE '^\s*[•✔]\s+[0-9]+' | grep -oE '[0-9]+' | sort -n
}

# Get task text by ID
_rusk_get_task_text() {
    local task_id="$1"
    local rusk_cmd=$(_rusk_cmd)
    local task_line=$("$rusk_cmd" list 2>/dev/null | grep -E "^\s*[•✔]\s+$task_id\s+")
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
    for ((i=2; i<${#words[@]}; i++)); do
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
    for ((i=2; i<${#words[@]}; i++)); do
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
    # Complete commands
    if [ -z "$CURRENT" ] || [ "$CURRENT" -eq 2 ] 2>/dev/null; then
        compadd add edit mark del list restore completions a e m d l r
        return
    fi
    
    local cmd="$words[2]"
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
            elif [[ "$cur" == -* ]]; then
                compadd --date -d
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
                compadd --date -d
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
            if [ -n "$CURRENT" ] && [ "$CURRENT" -eq 3 ] 2>/dev/null; then
                compadd install show
            elif [ -n "$CURRENT" ] && [ "$CURRENT" -eq 4 ] 2>/dev/null; then
                compadd bash zsh fish nu powershell
            fi
            ;;
    esac
}
