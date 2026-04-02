use chrono::NaiveDate;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct Task {
    pub id: u8,
    pub text: String,
    pub date: Option<NaiveDate>,
    pub done: bool,
}
