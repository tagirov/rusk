//! Task id lists on the command line: `rusk mark 1,2,3`, `rusk edit 1,2 new text`,
//! `rusk add -a 19,22`.
//!
//! An id list is one comma-separated word. The shell splits `1, 2, 3` into
//! three words, so words are glued back together across a comma boundary
//! (`1,` `2` or `1` `,2`); a word that follows the list without such a comma is
//! not an id — for `edit` it starts the text (`edit 3 1,000 units sold` keeps
//! task 1 alone), for `mark` / `del` it is an error. Nothing is dropped
//! silently: a part that is not a number is an error, and a repeated id is
//! applied once.

use std::fmt;

use crate::model::TaskId;

/// What is wrong with a task id list from the command line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IdListError {
    /// No id at all: no argument (`list` is empty) or only commas.
    Empty { list: String },
    /// `part` of `list` is not a task id (`abc`, `-1`, `+1`, `1-3`, out of range).
    NotAnId { part: String, list: String },
    /// A word after the id list that is not glued to it by a comma
    /// (`mark 1 2`, `mark 3 1,2`).
    Trailing { word: String, list: String },
    /// `edit`: everything after the id list reads as more ids (`edit 1,2 3`,
    /// `edit 1 2 3 -d 2w`). Taken as the new text, it would replace the
    /// texts of the tasks with those numbers.
    MoreIds { words: String, list: String },
}

impl fmt::Display for IdListError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty { list } if list.is_empty() => write!(f, "no task ids given"),
            Self::Empty { list } => write!(f, "no task id in '{list}'"),
            Self::NotAnId { part, list } if part == list => {
                write!(f, "'{part}' is not a task id")
            }
            Self::NotAnId { part, list } => write!(f, "'{part}' in '{list}' is not a task id"),
            Self::Trailing { word, list } => write!(
                f,
                "unexpected argument '{word}' after the ids '{list}' (task ids are one comma-separated list)"
            ),
            Self::MoreIds { words, list } => write!(
                f,
                "'{words}' after the ids '{list}' reads as more task ids, not as a new text: \
                 task ids are one comma-separated list, and a text made of numbers goes after `--` \
                 (`rusk edit {list} -- {words}`)"
            ),
        }
    }
}

impl std::error::Error for IdListError {}

/// Parses one comma-separated id list (`1,2,3`; blanks around the numbers and
/// empty parts are tolerated). Ids are returned in input order, each once.
pub fn parse_id_list(list: &str) -> Result<Vec<TaskId>, IdListError> {
    let mut ids: Vec<TaskId> = Vec::new();
    for part in list.split(',') {
        let part = part.trim();
        if part.is_empty() {
            continue;
        }
        // Digits only: `str::parse` would also take `+1`.
        let id = if part.bytes().all(|b| b.is_ascii_digit()) {
            part.parse::<TaskId>().ok()
        } else {
            None
        };
        let Some(id) = id else {
            return Err(IdListError::NotAnId {
                part: part.to_string(),
                list: list.trim().to_string(),
            });
        };
        if !ids.contains(&id) {
            ids.push(id);
        }
    }
    if ids.is_empty() {
        return Err(IdListError::Empty {
            list: list.trim().to_string(),
        });
    }
    Ok(ids)
}

/// The id list at the start of argv and the words after it.
///
/// The list is the first word plus every following word glued to it by a
/// comma (see the module doc); the ids come back parsed, the list as one
/// string (for messages). The first word must be an id list.
pub fn split_leading_ids(args: &[String]) -> Result<(Vec<TaskId>, String, &[String]), IdListError> {
    let Some(first) = args.first() else {
        return Err(IdListError::Empty {
            list: String::new(),
        });
    };
    let mut list = first.trim().to_string();
    let mut used = 1;
    while let Some(word) = args.get(used) {
        let word = word.trim();
        let glued = list.ends_with(',') || word.starts_with(',');
        let continues = word
            .chars()
            .next()
            .is_some_and(|c| c.is_ascii_digit() || c == ',');
        if !(glued && continues) {
            break;
        }
        list.push_str(word);
        used += 1;
    }
    let ids = parse_id_list(&list)?;
    Ok((ids, list, &args[used..]))
}

/// `mark` / `del`: argv is nothing but the id list.
pub fn parse_id_args(args: &[String]) -> Result<Vec<TaskId>, IdListError> {
    let (ids, list, rest) = split_leading_ids(args)?;
    if let Some(word) = rest.first() {
        return Err(IdListError::Trailing {
            word: word.clone(),
            list,
        });
    }
    Ok(ids)
}

/// `edit`: the id list, then the new text (`None` without text: the
/// interactive editor). `args` is argv up to `--`, `verbatim` what came
/// after it: text as it is, never ids.
pub fn parse_edit_args(
    args: &[String],
    verbatim: &[String],
) -> Result<(Vec<TaskId>, Option<Vec<String>>), IdListError> {
    let (ids, list, rest) = split_leading_ids(args)?;
    if !rest.is_empty() && rest.iter().all(|word| parse_id_list(word).is_ok()) {
        return Err(IdListError::MoreIds {
            words: rest.join(" "),
            list,
        });
    }
    let text: Vec<String> = rest.iter().chain(verbatim).cloned().collect();
    Ok((ids, (!text.is_empty()).then_some(text)))
}
