#![feature(with_options)]

use std::error::Error;
use std::fs::{self, File};
use std::io::{self, prelude::*};
use std::path::Path;

use rand::prelude::*;
use serde_derive::{Deserialize, Serialize};

const CFG_PATH: &str = env!("HOME");

struct Args {
    mode: String,
    date: String,
    query: String,
    id: u8,
}

impl Args {
    fn parse(mut args: std::env::Args) -> Result<Args, Box<dyn Error>> {
        args.next();

        let mode = match args.next() {
            Some(arg) => arg,
            None => String::from("--help")
        };

        let mut date = String::from("///");
        let mut id: u8 = 0;
        let mut query: Vec<String> = Vec::new();

        if mode == "a" {
            if let Some(d) = args.next() {
                if d.starts_with("20") {
                    date = d;
                } else {
                    query.push(d);
                }
            }
        }

        if mode == "d" {
            if let Some(n) = args.next() {
                id = n.parse()?;
            }
        }

        for item in args {
            query.push(item);
        }

        let query: String = query.join(" ");

      Ok( Args {
            mode,
            date,
            query,
            id,
        })
    }
}

#[derive(Serialize, Deserialize, Debug)]
struct Task {
    id: u8,
    content: String,
    date: Date,
}

impl Task {
    fn new(content: String, date: Date) -> Result<Self, Box<dyn Error>> {
        let id = generate_unique_id()?;

        Ok(Task { id, content, date })
    }
}

#[derive(Serialize, Deserialize, Debug)]
struct Date {
    year: String,
    month: String,
    day: String,
}

impl Date {
    fn new(year: String, month: String, day: String) -> Self {
        Date { year, month, day }
    }
}

fn main() {
    let args = Args::parse(std::env::args()).unwrap_or_else(|e| {
			eprintln!("Error when parsing arguments: {}", e);
			std::process::exit(1);
		});

		run(args).unwrap_or_else(|e| {
			eprintln!("Runtime error: {}", e);
			std::process::exit(1);
		});
}

fn run(args: Args) -> Result<(), Box<dyn Error>> {
    match args.mode.as_str() { 
        "a" | "--add" => create_task(args.date, args.query),
        "l" | "--list" => show_task_list(),
        "d" | "--delete" => delete_task(args.id),
        "h" | "--help" => help(),
        "v" | "--version" => version(),
        _ => help(),
    }?;
	Ok(())
}

fn db() -> std::io::Result<File> {
    let cfg_dir = format!("{}/{}", &CFG_PATH, "rusk");
    let db_file = format!("{}/{}", &cfg_dir, "db.json");

    if !Path::new(&cfg_dir).exists() {
        fs::create_dir_all(cfg_dir)?;

        println!("\nDatabase created");
    }

    let db_file = File::with_options()
        .read(true)
        .write(true)
        .append(true)
        .create(true)
        .open(&db_file)?;

    Ok(db_file)
}

fn create_task(date: String, mut query: String) -> Result<(), Box<dyn Error>> {
    let date: Vec<&str> = date.split('/').collect();
    let (year, month, day) = (date[0], date[1], date[2]);

    let date = Date::new(year.to_string(), month.to_string(), day.to_string());

    if query == "" {
        println!("\nEmpty task description\n");

        io::stdin().read_line(&mut query)?;
    }

    let query = query.trim().to_string(); // cutoff \n

    Ok(add_task(Task::new(query, date)?)?)
}

fn add_task(task: Task) -> Result<(), Box<dyn Error>> {
    let serialize_task = serde_json::to_string(&task)?;
		
		let mut db = db()?;

    writeln!(db, "{}", serialize_task)?;
    println!("\nNew task added");

		Ok(())
}

fn delete_task(id: u8) -> Result<(), Box<dyn Error>> {
    let mut db_buf = String::new();
		let mut db = db()?;
    db.read_to_string(&mut db_buf)?;

    let mut array_db: Vec<&str> = db_buf.split("\n").collect();
    array_db.retain(|&x| !x.contains(&id.to_string()));
    array_db.pop(); // cutoff last empty line

    let db_file_path = format!("{}/{}", CFG_PATH, "rusk/db.json");

    let mut db_file = File::with_options()
        .write(true)
        .truncate(true)
        .open(&db_file_path)?;

    for task in array_db {
        writeln!(db_file, "{}", &task)?;
    }

		Ok(())
}

fn show_task_list() -> Result<(), Box<dyn Error>>{
    let mut db_buf = String::new();
		let mut db = db()?;
    db.read_to_string(&mut db_buf)?;

    let mut db_array: Vec<&str> = db_buf.split('\n').collect();

    db_array.pop(); // trim empty line at the end
    println!();

    for line in &db_array {
        let task: Task = serde_json::from_str(&line)?;

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

fn generate_unique_id() -> Result<u8, Box<dyn Error>> {
    let mut db_buf = String::new();
		let mut db = db()?;
    db.read_to_string(&mut db_buf)?;

    let mut id: u8 = thread_rng().gen_range(100, 255);

    while id.to_string().contains("id") {
        id = thread_rng().gen_range(100, 255);
    }
    Ok(id)
}

fn help() -> Result<(), Box<dyn Error>> {
    println!(
        "

╔═════╦════════════╦════════════════════════════════════════════════════════════════════╗
║  a  ║  --add     ║  Add new task with or without specific date (you must specify      ║
║     ║            ║  the date according to the following pattern: 20yy.mm.dd )         ║
╠═════╬════════════╬════════════════════════════════════════════════════════════════════╣
║  l  ║  --list    ║  List of all tasks:  id, date (if specified), task                 ║
╠═════╬════════════╬════════════════════════════════════════════════════════════════════╣
║  d  ║  --delete  ║  Delete task by id                                                 ║
╠═════╬════════════╬════════════════════════════════════════════════════════════════════╣
║  h  ║  --help    ║  This menu                                                         ║
╠═════╬════════════╬════════════════════════════════════════════════════════════════════╣
║  v  ║  --version ║  Print version and date of release                                 ║
╚═════╩════════════╩════════════════════════════════════════════════════════════════════╝

     ------------
     | Examples |
     ------------ 
 
     rusk a 2020/01/31 Take a shower
     rusk a Take a shower
     rusk d 123

"
    );

	Ok(())
}

fn version() -> Result<(), Box<dyn Error>> {
    println!("rusk 0.2.0 (2021-11-07)");
	
	Ok(())
}
