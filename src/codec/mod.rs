//! On-disk database formats. JSON stays the default; the database file
//! extension (`RUSK_DB` / `rusk_db`) picks the format:
//!
//! - `.json` (or anything unrecognized) — pretty-printed JSON, the default;
//! - `.csv` — RFC 4180 with a fixed `id,text,date,done,priority,after` schema
//!   for spreadsheet interop (LibreOffice/Excel/Google Sheets);
//! - `.md` / `.markdown` — GitHub-style task list (feature `fmt-markdown`);
//! - `.txt` — todo.txt (feature `fmt-todotxt`);
//! - `.ndjson` / `.jsonl` — one compact JSON task per line (feature `fmt-ndjson`);
//! - `.ics` — iCalendar VTODO (feature `fmt-ics`).
//!
//! Every format encodes the whole database and decodes it back; formats
//! whose feature is disabled are still recognized by extension so the error
//! names the missing feature instead of mis-parsing the file as JSON.
//!
//! A decoder maps the file to records and nothing more: an item without an
//! id comes back with id 0, one without text with an empty text, and ids
//! and dependencies are taken as written. Making the records a valid task
//! list is one step for every format and backend, after the decoding (see
//! [`crate::model::normalize`]).

pub mod csv;
#[cfg(feature = "fmt-ics")]
pub mod ics;
#[cfg(feature = "fmt-markdown")]
pub mod markdown;
#[cfg(feature = "fmt-ndjson")]
pub mod ndjson;
#[cfg(feature = "fmt-todotxt")]
pub mod todotxt;

use crate::model::Task;
use anyhow::{Context, Result};
use std::path::Path;

/// A database file as its decoder sees it. A UTF-8 BOM belongs to no
/// format: Windows editors and spreadsheets put one in front of the file
/// they save, and the database they saved is still a database. Stripped
/// here, once, for every format — decoders never see one.
fn content(data: &str) -> &str {
    data.trim_start_matches('\u{feff}')
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DbFormat {
    Json,
    Csv,
    #[cfg(feature = "fmt-markdown")]
    Markdown,
    #[cfg(feature = "fmt-todotxt")]
    TodoTxt,
    #[cfg(feature = "fmt-ndjson")]
    Ndjson,
    #[cfg(feature = "fmt-ics")]
    Ics,
}

/// Errors uniformly when `path` selects a format whose feature is compiled
/// out. The `$feature` literal is repeated because `cfg` cannot expand from
/// a variable.
macro_rules! gated {
    ($path:expr, $feature:literal, $variant:ident) => {{
        #[cfg(feature = $feature)]
        {
            return Ok(DbFormat::$variant);
        }
        #[cfg(not(feature = $feature))]
        {
            anyhow::bail!(
                "'{}' selects a database format that this rusk build does not include; \
                 rebuild with `--features {}`",
                $path.display(),
                $feature
            );
        }
    }};
}

/// The extension that names a database's format, lower-cased: the last
/// one — `notes.txt.json` is JSON, whatever the `.txt` in the middle says —
/// once the suffixes of the copies rusk keeps beside a database are taken
/// off, so that `tasks.csv.backup` and `tasks.db.before_restore.2` are what
/// their database is. Empty when there is none.
pub(crate) fn format_extension(path: &Path) -> String {
    // Lossy: a name that is not UTF-8 still has an ASCII extension.
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy())
        .unwrap_or_default()
        .to_ascii_lowercase();
    let mut base = name.as_str();
    // `.before_restore.N`: the numbered ones that do not replace an older copy.
    if let Some((stem, n)) = base.rsplit_once('.')
        && !n.is_empty()
        && n.bytes().all(|b| b.is_ascii_digit())
        && stem.ends_with(".before_restore")
    {
        base = stem;
    }
    for aux in [".backup", ".before_restore"] {
        if let Some(stem) = base.strip_suffix(aux) {
            base = stem;
            break;
        }
    }
    Path::new(base)
        .extension()
        .and_then(|ext| ext.to_str())
        .unwrap_or("")
        .to_string()
}

impl DbFormat {
    /// Detects the format from the database file name (see
    /// [`format_extension`]); no extension, or one that names no format, is
    /// JSON.
    pub fn from_path(path: &Path) -> Result<Self> {
        let ext = format_extension(path);
        let has = |name: &str| ext == name;

        if has("csv") {
            return Ok(DbFormat::Csv);
        }
        if has("md") || has("markdown") {
            gated!(path, "fmt-markdown", Markdown);
        }
        if has("txt") {
            gated!(path, "fmt-todotxt", TodoTxt);
        }
        if has("ndjson") || has("jsonl") {
            gated!(path, "fmt-ndjson", Ndjson);
        }
        if has("ics") {
            gated!(path, "fmt-ics", Ics);
        }
        Ok(DbFormat::Json)
    }

    /// Human name for error messages ("Failed to parse the … database file").
    pub fn name(self) -> &'static str {
        match self {
            DbFormat::Json => "JSON",
            DbFormat::Csv => "CSV",
            #[cfg(feature = "fmt-markdown")]
            DbFormat::Markdown => "Markdown",
            #[cfg(feature = "fmt-todotxt")]
            DbFormat::TodoTxt => "todo.txt",
            #[cfg(feature = "fmt-ndjson")]
            DbFormat::Ndjson => "NDJSON",
            #[cfg(feature = "fmt-ics")]
            DbFormat::Ics => "iCalendar",
        }
    }

    pub fn encode(self, tasks: &[Task]) -> Result<String> {
        match self {
            DbFormat::Json => {
                serde_json::to_string_pretty(tasks).context("Failed to serialize tasks")
            }
            DbFormat::Csv => Ok(csv::to_csv(tasks)),
            #[cfg(feature = "fmt-markdown")]
            DbFormat::Markdown => Ok(markdown::encode(tasks)),
            #[cfg(feature = "fmt-todotxt")]
            DbFormat::TodoTxt => Ok(todotxt::encode(tasks)),
            #[cfg(feature = "fmt-ndjson")]
            DbFormat::Ndjson => ndjson::encode(tasks),
            #[cfg(feature = "fmt-ics")]
            DbFormat::Ics => Ok(ics::encode(tasks)),
        }
    }

    /// True when `data`, which decoded to zero tasks, really is an empty
    /// database of this format, the way rusk itself writes one. A file of
    /// nothing but blanks is one for the formats that write an empty
    /// database as an empty file (Markdown, todo.txt, NDJSON — the answer
    /// is taken from the format itself, below) and for no other: elsewhere
    /// it is what an interrupted write or a full disk leaves behind, never
    /// a state rusk stored. CSV decodes an empty file, or one of blank
    /// rows, to nothing, but rusk always writes the header; Markdown and
    /// iCalendar skip whatever they do not recognize, so for them "zero
    /// tasks" is also what a file that is not a task list at all decodes to
    /// — somebody's calendar of appointments, which a save would replace.
    pub fn is_empty_database(self, data: &str) -> bool {
        let content = content(data);
        if content.trim().is_empty() {
            // Nothing is a database only where nothing is what rusk writes
            // for one: the formats that are a list of lines and no more
            // (Markdown, todo.txt, NDJSON) have no wrapper to put around
            // zero tasks, `[]` and the CSV header are one.
            return self.encode(&[]).is_ok_and(|empty| empty.trim().is_empty());
        }
        match self {
            // Decoded, so a record that is not blank is the header.
            DbFormat::Csv => csv::has_records(content),
            #[cfg(feature = "fmt-markdown")]
            DbFormat::Markdown => false,
            // A calendar rusk wrote and emptied: the wrapper, closed, and
            // nothing inside it. A calendar of appointments decodes to zero
            // tasks as well, and rusk's own save would drop every one of
            // them — that is a warning, not an empty database. So does a
            // write that stopped after the header.
            #[cfg(feature = "fmt-ics")]
            DbFormat::Ics => {
                let components: Vec<String> = content
                    .lines()
                    .filter_map(|line| {
                        let line = line.trim().to_ascii_uppercase();
                        (line.starts_with("BEGIN:") || line.starts_with("END:")).then_some(line)
                    })
                    .collect();
                components == ["BEGIN:VCALENDAR", "END:VCALENDAR"]
            }
            // JSON and NDJSON reject foreign content while decoding, and
            // every non-blank todo.txt line is a task: whatever got here
            // with records of none was an empty list written out.
            _ => true,
        }
    }

    /// Decodes a whole database into records as written — id 0 for an item
    /// without one, not yet a valid task list: that is
    /// [`crate::model::normalize`]'s job (see the module docs). JSON errors
    /// are returned raw (the file backend downcasts them for line-context
    /// reporting).
    pub fn decode(self, data: &str) -> Result<Vec<Task>> {
        let data = content(data);
        // Nothing but blanks holds no records in any format; whether that
        // is a database rusk wrote is `is_empty_database`'s call, and it is
        // not serde's business to refuse the file first.
        if data.trim().is_empty() {
            return Ok(Vec::new());
        }
        match self {
            DbFormat::Json => serde_json::from_str(data).map_err(Into::into),
            DbFormat::Csv => csv::from_csv(data),
            #[cfg(feature = "fmt-markdown")]
            DbFormat::Markdown => markdown::decode(data),
            #[cfg(feature = "fmt-todotxt")]
            DbFormat::TodoTxt => todotxt::decode(data),
            #[cfg(feature = "fmt-ndjson")]
            DbFormat::Ndjson => ndjson::decode(data),
            #[cfg(feature = "fmt-ics")]
            DbFormat::Ics => ics::decode(data),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn fmt(name: &str) -> DbFormat {
        DbFormat::from_path(&PathBuf::from(name)).unwrap()
    }

    #[test]
    fn format_detection() {
        assert_eq!(fmt("/a/tasks.json"), DbFormat::Json);
        assert_eq!(fmt("/a/tasks"), DbFormat::Json);
        assert_eq!(fmt("/a/tasks.csv"), DbFormat::Csv);
        assert_eq!(fmt("/a/Tasks.CSV"), DbFormat::Csv);
        // Only the last extension counts, after the suffix of a copy.
        assert_eq!(fmt("/a/tasks.csv.backup"), DbFormat::Csv);
        assert_eq!(fmt("/a/tasks.csv.before_restore"), DbFormat::Csv);
        assert_eq!(fmt("/a/tasks.csv.before_restore.3"), DbFormat::Csv);
        assert_eq!(fmt("/a/tasks.csv.3"), DbFormat::Json);
        assert_eq!(fmt("/a/tasks.backup"), DbFormat::Json);
        assert_eq!(fmt("/a/notes.txt.json"), DbFormat::Json);
        assert_eq!(fmt("/a/data.md.csv"), DbFormat::Csv);
        assert_eq!(fmt("/a/tasks.db.json"), DbFormat::Json);
        assert_eq!(fmt("/a/.csv"), DbFormat::Json);
    }

    #[cfg(feature = "fmt-markdown")]
    #[test]
    fn markdown_detection() {
        assert_eq!(fmt("/a/tasks.md"), DbFormat::Markdown);
        assert_eq!(fmt("/a/tasks.markdown"), DbFormat::Markdown);
    }

    #[cfg(feature = "fmt-todotxt")]
    #[test]
    fn todotxt_detection() {
        assert_eq!(fmt("/a/todo.txt"), DbFormat::TodoTxt);
    }

    #[cfg(feature = "fmt-ndjson")]
    #[test]
    fn ndjson_detection() {
        assert_eq!(fmt("/a/tasks.ndjson"), DbFormat::Ndjson);
        assert_eq!(fmt("/a/tasks.jsonl"), DbFormat::Ndjson);
    }

    #[cfg(feature = "fmt-ics")]
    #[test]
    fn ics_detection() {
        assert_eq!(fmt("/a/tasks.ics"), DbFormat::Ics);
    }

    #[test]
    fn json_decodes_nothing_but_a_real_empty_database() {
        assert!(DbFormat::Json.is_empty_database("[ ]"));
        // A file with nothing in it is no `[]`: what an interrupted write
        // or a full disk leaves behind must not pass as a stored state.
        assert!(!DbFormat::Json.is_empty_database(""));
        assert!(!DbFormat::Json.is_empty_database(" \n\t"));
    }

    /// A UTF-8 BOM is stripped for every format, in one place, before any
    /// decoder sees the text (REVIEW №17: a BOM made JSON "corrupted").
    #[test]
    fn a_byte_order_mark_belongs_to_no_format() {
        let task = Task {
            id: 1,
            text: "bom".into(),
            date: None,
            done: false,
            priority: false,
            after: Vec::new(),
        };
        for format in [
            DbFormat::Json,
            DbFormat::Csv,
            #[cfg(feature = "fmt-markdown")]
            DbFormat::Markdown,
            #[cfg(feature = "fmt-todotxt")]
            DbFormat::TodoTxt,
            #[cfg(feature = "fmt-ndjson")]
            DbFormat::Ndjson,
            #[cfg(feature = "fmt-ics")]
            DbFormat::Ics,
        ] {
            let encoded = format.encode(std::slice::from_ref(&task)).unwrap();
            let with_bom = format!("\u{feff}{encoded}");
            let decoded = format
                .decode(&with_bom)
                .unwrap_or_else(|e| panic!("{}: {e:#}", format.name()));
            assert_eq!(decoded.len(), 1, "{}", format.name());
            assert_eq!(decoded[0].text, "bom", "{}", format.name());
            assert!(format.is_empty_database(&format!(
                "\u{feff}{}",
                format.encode(&[]).unwrap()
            )));
        }
    }

    /// Blanks decode to no records whatever the format, so that "cannot be
    /// read" and "empty database" stay one question for one place to
    /// answer (`is_empty_database`), the same for a local file and a
    /// remote one.
    #[test]
    fn blank_content_holds_no_records_in_any_format() {
        for format in [
            DbFormat::Json,
            DbFormat::Csv,
            #[cfg(feature = "fmt-markdown")]
            DbFormat::Markdown,
            #[cfg(feature = "fmt-todotxt")]
            DbFormat::TodoTxt,
            #[cfg(feature = "fmt-ndjson")]
            DbFormat::Ndjson,
            #[cfg(feature = "fmt-ics")]
            DbFormat::Ics,
        ] {
            for data in ["", " \n \t\n", "\u{feff}"] {
                let decoded = format
                    .decode(data)
                    .unwrap_or_else(|e| panic!("{} ({data:?}): {e:#}", format.name()));
                assert!(decoded.is_empty(), "{}", format.name());
                // Blanks are a stored state exactly where rusk writes an
                // empty database as an empty file, and nowhere else.
                let writes_nothing = format.encode(&[]).unwrap().trim().is_empty();
                assert_eq!(
                    format.is_empty_database(data),
                    writes_nothing,
                    "{} ({data:?})",
                    format.name()
                );
            }
        }
    }

    #[test]
    fn csv_without_tasks_is_empty_only_with_its_header() {
        assert!(DbFormat::Csv.is_empty_database(&DbFormat::Csv.encode(&[]).unwrap()));
        assert!(DbFormat::Csv.is_empty_database("id,text,date,done,priority\n"));
        // rusk never writes a zero-length CSV: a crash leftover, not a state.
        assert!(!DbFormat::Csv.is_empty_database(""));
        assert!(!DbFormat::Csv.is_empty_database("\n"));
        // Nor one of blank rows: what a spreadsheet leaves of a cleared sheet.
        assert!(!DbFormat::Csv.is_empty_database(",,,,,\r\n,,,,,\r\n"));
    }

    #[cfg(feature = "fmt-markdown")]
    #[test]
    fn markdown_without_tasks_is_empty_only_when_blank() {
        assert!(DbFormat::Markdown.is_empty_database(""));
        assert!(DbFormat::Markdown.is_empty_database("\u{feff}\n  \n"));
        assert!(DbFormat::Markdown.is_empty_database(&DbFormat::Markdown.encode(&[]).unwrap()));
        assert!(!DbFormat::Markdown.is_empty_database("this is not a task list\n"));
    }

    #[cfg(feature = "fmt-ics")]
    #[test]
    fn ics_without_tasks_is_empty_only_as_an_emptied_calendar() {
        assert!(DbFormat::Ics.is_empty_database(&DbFormat::Ics.encode(&[]).unwrap()));
        assert!(DbFormat::Ics.is_empty_database("begin:vcalendar\r\nEND:VCALENDAR\r\n"));
        assert!(!DbFormat::Ics.is_empty_database("this is not a task list\n"));
        assert!(!DbFormat::Ics.is_empty_database(""));
        // Somebody's calendar: no task in it, but a save would drop the
        // appointments, so it is no empty database.
        assert!(!DbFormat::Ics.is_empty_database(
            "BEGIN:VCALENDAR\r\nVERSION:2.0\r\nBEGIN:VEVENT\r\nUID:e1\r\nEND:VEVENT\r\nEND:VCALENDAR\r\n"
        ));
        // A write that stopped after the header is no state either.
        assert!(!DbFormat::Ics.is_empty_database("BEGIN:VCALENDAR\r\nVERSION:2.0\r\n"));
    }
}
