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
#


# Find rusk binary at script load time
_rusk_cmd() {
    command -v rusk 2>/dev/null || echo "rusk"
}

_rusk_get_task_ids() {
    # Get list of task IDs from rusk list output
    # Filter lines with • or ✔ to skip table headers
    local rusk_cmd=$(_rusk_cmd)
    "$rusk_cmd" list 2>/dev/null | grep -E '[•✔]' | grep -oE '^\s+[•✔]\s+[0-9]+\s+' | grep -oE '[0-9]+' | sort -n | tr '\n' ' '
}

_rusk_get_task_text() {
    local task_id="$1"
    # Get task text by parsing rusk list output
    # Format: "  •  3  2025-01-01  buy groceries" or "  •  1            hui"
    # Filter lines with • or ✔ to skip table headers
    local rusk_cmd=$(_rusk_cmd)
    local task_line=$("$rusk_cmd" list 2>/dev/null | grep -E '[•✔]' | grep -E "^\s+[•✔]\s+$task_id\s+")
    if [ -n "$task_line" ]; then
        # Extract text after ID and date
        # Format: status (•/✔), ID, date (optional), text
        # Use sed to remove leading whitespace, status, ID, and date
        echo "$task_line" | sed -E 's/^\s+[•✔]\s+[0-9]+\s+([0-9]{2}-[0-9]{2}-[0-9]{4}\s+)?//' | sed 's/^[[:space:]]*//;s/[[:space:]]*$//'
    fi
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
    
    # Complete subcommands with aliases
    case "$cmd" in
        add|a)
            # Complete --date flag with date options
            if [[ "$prev" == "--date" ]] || [[ "$prev" == "-d" ]]; then
                # Get all date options: today, tomorrow, week ahead, two weeks ahead
                local today=$(date +%d-%m-%Y 2>/dev/null)
                local tomorrow=$(date -d '+1 day' +%d-%m-%Y 2>/dev/null || date -v+1d +%d-%m-%Y 2>/dev/null || date +%d-%m-%Y)
                local week_ahead=$(date -d '+1 week' +%d-%m-%Y 2>/dev/null || date -v+1w +%d-%m-%Y 2>/dev/null || date +%d-%m-%Y)
                local two_weeks_ahead=$(date -d '+2 weeks' +%d-%m-%Y 2>/dev/null || date -v+2w +%d-%m-%Y 2>/dev/null || date +%d-%m-%Y)
                COMPREPLY=($(compgen -W "$today $tomorrow $week_ahead $two_weeks_ahead" -- "$cur"))
                return 0
            fi
            # Complete flags
            if [[ "$cur" == -* ]]; then
                COMPREPLY=($(compgen -W "--date -d --help -h" -- "$cur"))
                return 0
            fi
            COMPREPLY=()
            ;;
            
        edit|e)
            # If previous word is an ID and current is empty, suggest task text first
            # This handles "rusk e 1 <tab>" case (with space after ID)
            # But only if there's a single ID (not multiple IDs)
            if [[ "$prev" =~ ^[0-9]+$ ]] && [[ -z "$cur" ]]; then
                # Count how many IDs have been entered
                local id_count=0
                local i
                for ((i=2; i<COMP_CWORD; i++)); do
                    if [[ "${COMP_WORDS[i]}" =~ ^[0-9]+$ ]]; then
                        ((id_count++))
                    fi
                done
                # Only suggest task text if there's exactly one ID
                if [ $id_count -eq 1 ]; then
                    local task_text=$(_rusk_get_task_text "$prev")
                    if [ -n "$task_text" ]; then
                        # Return task text as-is (bash will handle it)
                        # Don't escape with printf '%q' as it makes the text unreadable
                        COMPREPLY=("$task_text")
                        return 0
                    fi
                fi
            fi
            
            # Complete task IDs (when at command or when typing ID)
            # Only suggest IDs if we're at the command or typing a number
            if [[ "$prev" == "edit" ]] || [[ "$prev" == "e" ]] || [[ "$cur" =~ ^[0-9]+$ ]]; then
                local ids=$(_rusk_get_task_ids)
                if [ -n "$ids" ]; then
                    # Exclude already entered IDs from suggestions
                    local entered_ids=""
                    local i
                    for ((i=2; i<COMP_CWORD; i++)); do
                        if [[ "${COMP_WORDS[i]}" =~ ^[0-9]+$ ]]; then
                            entered_ids="$entered_ids ${COMP_WORDS[i]}"
                        fi
                    done
                    # Filter out entered IDs
                    if [ -n "$entered_ids" ]; then
                        local filtered_ids=""
                        for id in $ids; do
                            if [[ ! "$entered_ids" =~ (^|[[:space:]])"$id"([[:space:]]|$) ]]; then
                                filtered_ids="$filtered_ids $id"
                            fi
                        done
                        ids=$(echo "$filtered_ids" | sed 's/^[[:space:]]*//;s/[[:space:]]*$//')
                    fi
                    if [ -n "$ids" ]; then
                        COMPREPLY=($(compgen -W "$ids" -- "$cur"))
                        return 0
                    fi
                fi
            fi
            
            # Complete --date flag with date options
            if [[ "$prev" == "--date" ]] || [[ "$prev" == "-d" ]]; then
                # Get all date options: today, tomorrow, week ahead, two weeks ahead
                local today=$(date +%d-%m-%Y 2>/dev/null)
                local tomorrow=$(date -d '+1 day' +%d-%m-%Y 2>/dev/null || date -v+1d +%d-%m-%Y 2>/dev/null || date +%d-%m-%Y)
                local week_ahead=$(date -d '+1 week' +%d-%m-%Y 2>/dev/null || date -v+1w +%d-%m-%Y 2>/dev/null || date +%d-%m-%Y)
                local two_weeks_ahead=$(date -d '+2 weeks' +%d-%m-%Y 2>/dev/null || date -v+2w +%d-%m-%Y 2>/dev/null || date +%d-%m-%Y)
                COMPREPLY=($(compgen -W "$today $tomorrow $week_ahead $two_weeks_ahead" -- "$cur"))
                return 0
            fi
            
            # Complete flags
            if [[ "$cur" == -* ]]; then
                COMPREPLY=($(compgen -W "--date -d --help -h" -- "$cur"))
                return 0
            fi
            
            COMPREPLY=()
            ;;
            
        mark|m|del|d)
            # Complete task IDs
            if [[ "$cur" =~ ^[0-9]*$ ]] || [[ "$prev" == "$cmd" ]]; then
                local ids=$(_rusk_get_task_ids)
                if [ -n "$ids" ]; then
                    # Exclude already entered IDs from suggestions
                    # Get all words after the command
                    local entered_ids=""
                    local i
                    for ((i=2; i<COMP_CWORD; i++)); do
                        if [[ "${COMP_WORDS[i]}" =~ ^[0-9]+$ ]]; then
                            entered_ids="$entered_ids ${COMP_WORDS[i]}"
                        fi
                    done
                    # Filter out entered IDs
                    if [ -n "$entered_ids" ]; then
                        local filtered_ids=""
                        for id in $ids; do
                            if [[ ! "$entered_ids" =~ (^|[[:space:]])"$id"([[:space:]]|$) ]]; then
                                filtered_ids="$filtered_ids $id"
                            fi
                        done
                        ids=$(echo "$filtered_ids" | sed 's/^[[:space:]]*//;s/[[:space:]]*$//')
                    fi
                    if [ -n "$ids" ]; then
                        COMPREPLY=($(compgen -W "$ids" -- "$cur"))
                        return 0
                    fi
                fi
            fi
            
            # For del, complete --done flag
            if [[ "$cmd" == "del" ]] || [[ "$cmd" == "d" ]]; then
                if [[ "$cur" == -* ]]; then
                    COMPREPLY=($(compgen -W "--done --help -h" -- "$cur"))
                    return 0
                fi
            fi
            
            COMPREPLY=()
            ;;
            
        list|l|restore|r)
            # These commands don't take arguments
            COMPREPLY=()
            ;;
            
        completions)
            # Complete completions subcommands
            if [[ "$prev" == "completions" ]]; then
                COMPREPLY=($(compgen -W "install show" -- "$cur"))
                return 0
            fi
            if [[ "$prev" == "install" ]] || [[ "$prev" == "show" ]]; then
                COMPREPLY=($(compgen -W "bash zsh fish nu powershell" -- "$cur"))
                return 0
            fi
            COMPREPLY=()
            ;;
            
        *)
            COMPREPLY=()
            ;;
    esac
}

complete -F _rusk_completion rusk

