use crate::db;

use rand::prelude::*;
use serde_derive::{Deserialize, Serialize};
use std::error::Error;
use std::fs::File;
use std::io::{self, prelude::*};

pub fn create_task(date: String, mut query: String) -> Result<(), Box<dyn Error>> {
    let date: Vec<&str> = date.split('/').collect();
    let (year, month, day) = (date[0], date[1], date[2]);

    let date = Date::new(year.to_string(), month.to_string(), day.to_string());

    if query == "" {
        println!("\nEmpty task description\n");

        io::stdin().read_line(&mut query).unwrap();
    }

    let query = query.trim().to_string(); // cutoff \n

    add_task(Task::new(query, date))
}

pub fn add_task(task: Task) -> Result<(), Box<dyn Error>> {
    let serialize_task = serde_json::to_string(&task)?;

    writeln!(db::db(), "{}", serialize_task)?;
    println!("\nNew task successfully added");

    Ok(())
}

pub fn delete_task(id: u8) -> Result<(), Box<dyn Error>> {
    let mut db_buf = String::new();
    db::db().read_to_string(&mut db_buf)?;

    let mut array_db: Vec<&str> = db_buf.split("\n").collect();
    array_db.retain(|&x| !x.contains(&id.to_string()));
    array_db.pop(); // trim the last empty line

    let file_path = format!("{}/{}", &crate::CFG_PATH, "rusk/db.json");

    let mut db_file = File::with_options()
        .write(true)
        .truncate(true)
        .open(&file_path)?;

    for task in array_db {
        writeln!(db_file, "{}", &task)?;
    }

    Ok(())
}

fn generate_unique_id() -> u8 {
    let mut db_buf = String::new();
    db::db()
        .read_to_string(&mut db_buf)
        .expect("Problem with generate task id");

    let mut id: u8 = thread_rng().gen_range(100, 255);

    while id.to_string().contains("id") {
        id = thread_rng().gen_range(100, 255);
    }
    id
}

#[derive(Serialize, Deserialize, Debug)]
pub struct Task {
    pub id: u8,
    pub content: String,
    pub date: Date,
}

impl Task {
    fn new(content: String, date: Date) -> Self {
        let id = generate_unique_id();

        Task { id, content, date }
    }
}

#[derive(Serialize, Deserialize, Debug)]
pub struct Date {
    pub year: String,
    pub month: String,
    pub day: String,
}

impl Date {
    fn new(year: String, month: String, day: String) -> Self {
        Date { year, month, day }
    }
}
