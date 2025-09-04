use rusk::Task;
use chrono::NaiveDate;

// Helper function to create test tasks
pub fn create_test_task(id: u8, text: &str, done: bool) -> Task {
    Task {
        id,
        text: text.to_string(),
        date: None,
        done,
    }
}

// Helper function to create test tasks with date
#[allow(dead_code)]
pub fn create_test_task_with_date(id: u8, text: &str, done: bool, date: &str) -> Task {
    Task {
        id,
        text: text.to_string(),
        date: NaiveDate::parse_from_str(date, "%Y-%m-%d").ok(),
        done,
    }
}
