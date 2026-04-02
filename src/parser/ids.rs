use super::date::is_cli_date_help_value;

/// Parse comma-separated IDs from a single string segment.
/// Shared logic used by both `parse_flexible_ids` and `parse_edit_args`.
fn parse_comma_ids(s: &str) -> Vec<u8> {
    s.split(',')
        .filter_map(|part| {
            let trimmed = part.trim();
            if trimmed.is_empty() {
                None
            } else {
                trimmed.parse::<u8>().ok()
            }
        })
        .collect()
}

/// Parse ID list for `del` / `mark`: use comma-separated tokens (e.g. `1,2,3`).
/// If no argument contains a comma, only the first bare `u8` token is kept; extra argv words are ignored.
pub fn parse_flexible_ids(args: &[String]) -> Vec<u8> {
    let mut ids = Vec::new();

    if args.is_empty() {
        return ids;
    }

    let has_comma_args = args.iter().any(|a| {
        let t = a.trim();
        t.contains(',') || t.starts_with(',')
    });

    for arg in args {
        let trimmed_arg = arg.trim();
        if trimmed_arg.contains(',') || trimmed_arg.starts_with(',') {
            ids.extend(parse_comma_ids(trimmed_arg));
        } else if !has_comma_args && let Ok(id) = trimmed_arg.parse::<u8>() {
            if ids.is_empty() {
                ids.push(id);
            }
        }
    }

    ids
}

pub type EditArgs = (Vec<u8>, Option<Vec<String>>);

/// Parse edit command arguments to separate IDs and text
pub fn parse_edit_args(args: Vec<String>) -> EditArgs {
    let mut ids = Vec::new();
    let mut text_parts = Vec::new();
    let mut parsing_ids = true;

    let mut i = 0;
    while i < args.len() {
        let arg = &args[i];

        if arg == "-d" || arg == "--date" {
            if i + 1 < args.len() {
                let next = &args[i + 1];
                if is_cli_date_help_value(next) {
                    i += 2;
                    continue;
                }
                if !next.starts_with('-') {
                    i += 2;
                    continue;
                }
            }
            i += 1;
            continue;
        }

        if parsing_ids {
            let trimmed_arg = arg.trim();
            if trimmed_arg.contains(',') || trimmed_arg.starts_with(',') {
                let parsed = parse_comma_ids(trimmed_arg);
                if parsed.is_empty() {
                    parsing_ids = false;
                    text_parts.push(arg.clone());
                } else {
                    ids.extend(parsed);
                }
            } else if let Ok(id) = trimmed_arg.parse::<u8>() {
                if ids.is_empty() {
                    ids.push(id);
                } else {
                    parsing_ids = false;
                    text_parts.push(arg.clone());
                }
            } else {
                parsing_ids = false;
                text_parts.push(arg.clone());
            }
        } else {
            text_parts.push(arg.clone());
        }

        i += 1;
    }

    let text_option = if text_parts.is_empty() {
        None
    } else {
        Some(text_parts)
    };
    (ids, text_option)
}
