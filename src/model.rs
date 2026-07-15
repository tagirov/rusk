use chrono::NaiveDate;
use serde::{Deserialize, Serialize};

/// Task identifier. u32 keeps ids compact in every storage format and
/// converts losslessly to SQLite's i64 column type.
pub type TaskId = u32;

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct Task {
    pub id: TaskId,
    pub text: String,
    pub date: Option<NaiveDate>,
    pub done: bool,
    #[serde(default)]
    pub priority: bool,
    /// Ids of tasks this one depends on (`--after`): it should not be
    /// completed before all of them are done. Skipped in JSON when empty so
    /// existing databases stay byte-identical until the feature is used.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub after: Vec<TaskId>,
}
