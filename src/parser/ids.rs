/// `-d` / `--date` on `edit` with no value (a value is required; bare `-d` is not supported).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BareEditDateFlag;

/// Remove `-d` / `--date` and the following value from `rusk edit` argv (non-interactive date change).
/// Callers must run `args_have_date_then_help` (or equivalent) on the original argv first, so
/// `rusk edit 1 -d -h` is handled as help, not an error, before this runs.
/// Returns [`BareEditDateFlag`] when the flag has no value or the next token starts with `-` (a flag)
/// and is not a date string.
pub fn strip_edit_date_flag(args: Vec<String>) -> Result<(Vec<String>, Option<String>), BareEditDateFlag> {
    let mut out = Vec::with_capacity(args.len());
    let mut i = 0;
    let mut last_date: Option<String> = None;
    while i < args.len() {
        let a = &args[i];
        if a == "-d" || a == "--date" {
            if i + 1 < args.len() {
                let next = &args[i + 1];
                if !next.starts_with('-') {
                    last_date = Some(next.clone());
                    i += 2;
                    continue;
                }
            }
            return Err(BareEditDateFlag);
        }
        out.push(a.clone());
        i += 1;
    }
    Ok((out, last_date))
}

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
