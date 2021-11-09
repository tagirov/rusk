#![feature(with_options)]

use std::fs::{self, File};
use std::io::{self, prelude::*};
use std::path::Path;

use rand::prelude::*;
use serde_derive::{Deserialize, Serialize};

const CFG_PATH: &str = env!("HOME");

type GenError = Box<dyn std::error::Error>;
type GenResult<T> = Result<T, GenError>;

struct Args {
    mode: String,
    date: String,
    query: String,
    id: u8,
}

impl Args {
    fn parse(mut args: std::env::Args) -> GenResult<Args> {
        args.next();

        let mode = match args.next() {
            Some(arg) => arg,
            None => String::from("--list"),
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

        Ok(Args {
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
    fn new(content: String, date: Date) -> io::Result<Self> {
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

fn run(args: Args) -> GenResult<()> {
    match args.mode.as_str() {
        "a" | "--add" => create_task(args.date, args.query)?,
        "l" | "--list" => show_task_list()?,
        "d" | "--delete" => delete_task(args.id)?,
        "h" | "--help" => help(),
        "v" | "--version" => version(),
        _ => help(),
    };
    Ok(())
}

fn db() -> io::Result<File> {
    let db_dir = format!("{}/{}", &CFG_PATH, ".rusk");
    let db_file = format!("{}/{}", &db_dir, "db.json");

    if !Path::new(&db_dir).exists() {
        fs::create_dir_all(db_dir)?;

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

fn create_task(date: String, mut query: String) -> GenResult<()> {
    let date: Vec<&str> = date.split('/').collect();
    let (year, month, day) = (date[0], date[1], date[2]);

    let date = Date::new(year.to_string(), month.to_string(), day.to_string());

    if query == "" {
        println!("\nEmpty task description\n");

        io::stdin().read_line(&mut query)?;
    }

    let query = query.trim().to_string(); // trim \n

    Ok(add_task(Task::new(query, date)?)?)
}

fn add_task(task: Task) -> GenResult<()> {
    let serialized_task = serde_json::to_string(&task)?;

    writeln!(db()?, "{}", serialized_task)?;
    println!("\nNew task added ({})", task.id);

    Ok(())
}

fn delete_task(id: u8) -> io::Result<()> {
    let mut db_buf = String::new();
    db()?.read_to_string(&mut db_buf)?;

    let mut db_into_vec: Vec<&str> = db_buf.split("\n").collect();
    db_into_vec.retain(|&x| !x.contains(&id.to_string()));
    db_into_vec.pop(); // trim empty line at the end

    let db_file_path = format!("{}/{}", CFG_PATH, ".rusk/db.json");

    let mut db_file = File::with_options()
        .write(true)
        .truncate(true)
        .open(&db_file_path)?;

    for task in db_into_vec {
        writeln!(db_file, "{}", &task)?;
    }

    Ok(())
}

fn show_task_list() -> GenResult<()> {
    let mut db_buf = String::new();
    db()?.read_to_string(&mut db_buf)?;

    let mut db_into_vec: Vec<&str> = db_buf.split('\n').collect();

    db_into_vec.pop(); // trim empty line at the end
    println!();

    for line in &db_into_vec {
        let task: Task = serde_json::from_str(&line)?;

        if task.date.year != "" {
            println!(
                "{}| --- {}/{}/{} --- | {}",
                task.id, task.date.year, task.date.month, task.date.day, task.content,
            );
        } else {
            println!("{}| ------------------ | {}", task.id, task.content);
        }
    }
    Ok(())
}

fn generate_unique_id() -> io::Result<u8> {
    let mut db_buf = String::new();
    db()?.read_to_string(&mut db_buf)?;

    let mut id: u8 = thread_rng().gen_range(100, 255);

    let pattern = format!("{}:{}", "\"id\"", id);

    while db_buf.contains(&pattern) {
        id = thread_rng().gen_range(100, 255);
    }
    Ok(id)
}

fn help() {
    println!(
        "

╔═════╦════════════╦════════════════════════════════════════════════════════════════════╗
║  a  ║  --add     ║  Add a new task with or without a specific date (you must specify  ║
║     ║            ║  date according to the following pattern: 20yy.mm.dd )             ║
╠═════╬════════════╬════════════════════════════════════════════════════════════════════╣
║  l  ║  --list    ║  List of all tasks:  id, date (if specified), task                 ║
╠═════╬════════════╬════════════════════════════════════════════════════════════════════╣
║  d  ║  --delete  ║  Delete task by id                                                 ║
╠═════╬════════════╬════════════════════════════════════════════════════════════════════╣
║  h  ║  --help    ║  This menu                                                         ║
╠═════╬════════════╬════════════════════════════════════════════════════════════════════╣
║  v  ║  --version ║  Print version and release date                                    ║
╚═════╩════════════╩════════════════════════════════════════════════════════════════════╝

     ------------
     | Examples |
     ------------ 
 
     rusk a 2020/01/31 Take a shower
     rusk a Take a shower
     rusk d 123

"
    );
}

fn version() {
    println!("rusk 0.4.0 (2021-11-09)");
}

// fn sort_by_date() {
// 		TODO   //
// }

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
