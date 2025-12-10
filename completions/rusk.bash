#!/bin/bash
# Bash completion script for rusk
#
# Installation:
#   1. Automatic (recommended):
#      rusk completions install bash
#
#   2. Manual:
#      Generate script using rusk command:
#      rusk completions show bash > ~/.bash_completion.d/rusk
#
#      Then add to your ~/.bashrc:
#      source ~/.bash_completion.d/rusk
#
#      System-wide (requires root):
#      rusk completions show bash | sudo tee /etc/bash_completion.d/rusk > /dev/null
#      System-wide completions are loaded automatically on bash startup

# Find rusk binary
_rusk_cmd() {
    command -v rusk 2>/dev/null || echo "rusk"
}

# Get list of task IDs from rusk list output
_rusk_get_task_ids() {
    local rusk_cmd=$(_rusk_cmd)
    "$rusk_cmd" list 2>/dev/null | grep -E '[•✔]' | grep -oE '^\s+[•✔]\s+[0-9]+\s+' | grep -oE '[0-9]+' | sort -n | tr '\n' ' '
}

# Get task text by ID
_rusk_get_task_text() {
    local task_id="$1"
    local rusk_cmd=$(_rusk_cmd)
    local task_line=$("$rusk_cmd" list 2>/dev/null | grep -E '[•✔]' | grep -E "^\s+[•✔]\s+$task_id\s+")
    if [ -n "$task_line" ]; then
        # Extract text after ID and date (remove status, ID, optional date)
        echo "$task_line" | sed -E 's/^\s+[•✔]\s+[0-9]+\s+([0-9]{2}-[0-9]{2}-[0-9]{4}\s+)?//' | sed 's/^[[:space:]]*//;s/[[:space:]]*$//'
    fi
}

# Get date options (today, tomorrow, week ahead, two weeks ahead)
_rusk_get_date_options() {
    local today=$(date +%d-%m-%Y 2>/dev/null)
    local tomorrow=$(date -d '+1 day' +%d-%m-%Y 2>/dev/null || date -v+1d +%d-%m-%Y 2>/dev/null || date +%d-%m-%Y)
    local week_ahead=$(date -d '+1 week' +%d-%m-%Y 2>/dev/null || date -v+1w +%d-%m-%Y 2>/dev/null || date +%d-%m-%Y)
    local two_weeks_ahead=$(date -d '+2 weeks' +%d-%m-%Y 2>/dev/null || date -v+2w +%d-%m-%Y 2>/dev/null || date +%d-%m-%Y)
    echo "$today $tomorrow $week_ahead $two_weeks_ahead"
}

# Get entered task IDs from command line
_rusk_get_entered_ids() {
    local entered_ids=""
    local i
    for ((i=2; i<COMP_CWORD; i++)); do
        if [[ "${COMP_WORDS[i]}" =~ ^[0-9]+$ ]]; then
            entered_ids="$entered_ids ${COMP_WORDS[i]}"
        fi
    done
    echo "$entered_ids"
}

# Filter out already entered IDs from task ID list
_rusk_filter_ids() {
    local ids="$1"
    local entered_ids="$2"
    
    if [ -z "$entered_ids" ]; then
        echo "$ids"
        return
    fi
    
    local filtered_ids=""
    for id in $ids; do
        if [[ ! "$entered_ids" =~ (^|[[:space:]])"$id"([[:space:]]|$) ]]; then
            filtered_ids="$filtered_ids $id"
        fi
    done
    echo "$filtered_ids" | sed 's/^[[:space:]]*//;s/[[:space:]]*$//'
}

# Count how many IDs have been entered
_rusk_count_ids() {
    local count=0
    local i
    for ((i=2; i<COMP_CWORD; i++)); do
        if [[ "${COMP_WORDS[i]}" =~ ^[0-9]+$ ]]; then
            ((count++))
        fi
    done
    echo $count
}

# Complete task IDs with filtering
_rusk_complete_task_ids() {
    local ids=$(_rusk_get_task_ids)
    if [ -z "$ids" ]; then
        return 1
    fi
    
    local entered_ids=$(_rusk_get_entered_ids)
    local filtered_ids=$(_rusk_filter_ids "$ids" "$entered_ids")
    
    if [ -n "$filtered_ids" ]; then
        COMPREPLY=($(compgen -W "$filtered_ids" -- "$cur"))
        return 0
    fi
    return 1
}

# Complete date values
_rusk_complete_date() {
    local dates=$(_rusk_get_date_options)
    COMPREPLY=($(compgen -W "$dates" -- "$cur"))
    return 0
}

# Complete flags for add/edit commands
_rusk_complete_add_edit_flags() {
    COMPREPLY=($(compgen -W "--date -d --help -h" -- "$cur"))
    return 0
}

# Complete flags for del command
_rusk_complete_del_flags() {
    COMPREPLY=($(compgen -W "--done --help -h" -- "$cur"))
    return 0
}

_rusk_completion() {
    local cur="${COMP_WORDS[COMP_CWORD]}"
    local prev=""
    local cmd=""
    
    # Get previous word if available
    if [ $COMP_CWORD -gt 0 ]; then
        prev="${COMP_WORDS[COMP_CWORD-1]}"
    fi
    
    # Get command (second word) if available
    if [ ${#COMP_WORDS[@]} -gt 1 ]; then
        cmd="${COMP_WORDS[1]}"
    fi
    
    # Complete commands
    if [ $COMP_CWORD -eq 1 ]; then
        COMPREPLY=($(compgen -W "add edit mark del list restore completions a e m d l r" -- "$cur"))
        return 0
    fi
    
    # Complete subcommands
    case "$cmd" in
        add|a)
            if [[ "$prev" == "--date" ]] || [[ "$prev" == "-d" ]]; then
                _rusk_complete_date
            elif [[ "$cur" == -* ]]; then
                _rusk_complete_add_edit_flags
            fi
            ;;
            
        edit|e)
            # Suggest task text if previous word is a single ID and current is empty
            if [[ "$prev" =~ ^[0-9]+$ ]] && [[ -z "$cur" ]]; then
                if [ $(_rusk_count_ids) -eq 1 ]; then
                    local task_text=$(_rusk_get_task_text "$prev")
                    if [ -n "$task_text" ]; then
                        COMPREPLY=("$task_text")
                        return 0
                    fi
                fi
            fi
            
            # Complete date flag
            if [[ "$prev" == "--date" ]] || [[ "$prev" == "-d" ]]; then
                _rusk_complete_date
            # Complete task IDs
            elif [[ "$prev" == "edit" ]] || [[ "$prev" == "e" ]] || [[ "$cur" =~ ^[0-9]+$ ]]; then
                _rusk_complete_task_ids && return 0
            # Complete flags
            elif [[ "$cur" == -* ]]; then
                _rusk_complete_add_edit_flags
            fi
            ;;
            
        mark|m|del|d)
            # Complete task IDs
            if [[ "$cur" =~ ^[0-9]*$ ]] || [[ "$prev" == "$cmd" ]]; then
                _rusk_complete_task_ids && return 0
            fi
            
            # For del, complete flags
            if [[ ("$cmd" == "del" || "$cmd" == "d") && "$cur" == -* ]]; then
                _rusk_complete_del_flags
            fi
            ;;
            
        list|l|restore|r)
            # These commands don't take arguments
            ;;
            
        completions)
            if [[ "$prev" == "completions" ]]; then
                COMPREPLY=($(compgen -W "install show" -- "$cur"))
            elif [[ "$prev" == "install" ]] || [[ "$prev" == "show" ]]; then
                COMPREPLY=($(compgen -W "bash zsh fish nu powershell" -- "$cur"))
            fi
            ;;
    esac
}

complete -F _rusk_completion rusk
