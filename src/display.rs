use crate::{db, task};

use std::error::Error;
use std::io::prelude::*;

pub fn show_task_list() -> Result<(), Box<dyn Error>> {
    let mut db_buf = String::new();
    db::db().read_to_string(&mut db_buf)?;

    let mut db_array: Vec<&str> = db_buf.split('\n').collect();

    db_array.pop(); // trim empty line at end
    println!();

    for line in &db_array {
        let task: task::Task = serde_json::from_str(&line)?;

        if task.date.year != "" {
            println!(
                "|{}|    {}/{}/{}   {}",
                task.id, task.date.year, task.date.month, task.date.day, task.content,
            );
        } else {
            println!("|{}|                 {}", task.id, task.content);
        }
    }
    Ok(())
}
