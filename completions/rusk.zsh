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

_rusk_get_task_ids() {
    local rusk_cmd=$(_rusk_cmd)
    "$rusk_cmd" list 2>/dev/null | grep -oE '^\s*[•✔]\s+[0-9]+' | grep -oE '[0-9]+' | sort -n
}

_rusk_get_task_text() {
    local task_id="$1"
    local rusk_cmd=$(_rusk_cmd)
    local task_line=$("$rusk_cmd" list 2>/dev/null | grep -E "^\s*[•✔]\s+$task_id\s+")
    if [ -n "$task_line" ]; then
        echo "$task_line" | sed -E 's/^[[:space:]]*[•✔][[:space:]]+[0-9]+[[:space:]]+[0-9-]*[[:space:]]*//'
    fi
}

_rusk_get_date_options() {
    local today=$(date +%d-%m-%Y 2>/dev/null)
    local tomorrow=$(date -d '+1 day' +%d-%m-%Y 2>/dev/null || date -v+1d +%d-%m-%Y 2>/dev/null || date +%d-%m-%Y)
    local week_ahead=$(date -d '+1 week' +%d-%m-%Y 2>/dev/null || date -v+1w +%d-%m-%Y 2>/dev/null || date +%d-%m-%Y)
    local two_weeks_ahead=$(date -d '+2 weeks' +%d-%m-%Y 2>/dev/null || date -v+2w +%d-%m-%Y 2>/dev/null || date +%d-%m-%Y)
    echo "$today" "$tomorrow" "$week_ahead" "$two_weeks_ahead"
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
                local -a dates
                dates=($(_rusk_get_date_options))
                compadd $dates
            elif [[ "$cur" == -* ]]; then
                compadd --date -d
            fi
            ;;
            
        edit|e)
            if [[ "$prev" == "--date" ]] || [[ "$prev" == "-d" ]]; then
                local -a dates
                dates=($(_rusk_get_date_options))
                compadd $dates
            elif [[ "$prev" =~ ^[0-9]+$ ]] && [[ -z "$cur" ]]; then
                # Previous word is a task ID, suggest task text
                # But only if there's a single ID (not multiple IDs)
                # Count how many IDs have been entered
                local id_count=0
                local i
                for ((i=2; i<${#words[@]}; i++)); do
                    if [[ "${words[i]}" =~ ^[0-9]+$ ]]; then
                        ((id_count++))
                    fi
                done
                # Only suggest task text if there's exactly one ID
                if [ $id_count -eq 1 ]; then
                    local task_text=$(_rusk_get_task_text "$prev")
                    if [ -n "$task_text" ]; then
                        compadd -Q "$task_text"
                    fi
                fi
            elif [[ "$cur" == -* ]]; then
                compadd --date -d
            else
                # Complete task IDs
                local -a ids
                ids=($(_rusk_get_task_ids))
                # Exclude already entered IDs from suggestions
                local -a entered_ids
                local i
                for ((i=2; i<${#words[@]}; i++)); do
                    if [[ "${words[i]}" =~ ^[0-9]+$ ]]; then
                        entered_ids+=("${words[i]}")
                    fi
                done
                # Filter out entered IDs
                if [ ${#entered_ids[@]} -gt 0 ]; then
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
                    ids=("${filtered_ids[@]}")
                fi
                if [ ${#ids[@]} -gt 0 ]; then
                    compadd $ids
                fi
            fi
            ;;
            
        mark|m)
            local -a ids
            ids=($(_rusk_get_task_ids))
            # Exclude already entered IDs from suggestions
            local -a entered_ids
            local i
            for ((i=2; i<${#words[@]}; i++)); do
                if [[ "${words[i]}" =~ ^[0-9]+$ ]]; then
                    entered_ids+=("${words[i]}")
                fi
            done
            # Filter out entered IDs
            if [ ${#entered_ids[@]} -gt 0 ]; then
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
                ids=("${filtered_ids[@]}")
            fi
            if [ ${#ids[@]} -gt 0 ]; then
                compadd $ids
            fi
            ;;
            
        del|d)
            if [[ "$cur" == -* ]]; then
                compadd --done
            else
                local -a ids
                ids=($(_rusk_get_task_ids))
                # Exclude already entered IDs from suggestions
                local -a entered_ids
                local i
                for ((i=2; i<${#words[@]}; i++)); do
                    if [[ "${words[i]}" =~ ^[0-9]+$ ]]; then
                        entered_ids+=("${words[i]}")
                    fi
                done
                # Filter out entered IDs
                if [ ${#entered_ids[@]} -gt 0 ]; then
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
                    ids=("${filtered_ids[@]}")
                fi
                if [ ${#ids[@]} -gt 0 ]; then
                    compadd $ids
                fi
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
