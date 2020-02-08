use crate::{display, misc, task};

use std::error::Error;

pub fn run(args: Args) -> Result<(), Box<dyn Error>> {
    match args.mode.as_ref() {
        "a" => task::create_task(args.date, args.query),
        "l" => display::show_task_list(),
        "d" => task::delete_task(args.id),
        "-h" | "--help" => misc::help(),
        "-v" | "--version" => misc::version(),
        _ => panic!("No command specified"),
    }
}

pub struct Args {
    mode: String,
    date: String,
    query: String,
    id: u8,
}

impl Args {
    pub fn parse(mut args: std::env::Args) -> Result<Args, &'static str> {
        args.next();

        let mode = match args.next() {
            Some(arg) => arg,
            None => return Err("\nNeed specify a command"),
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
                id = n.parse().unwrap();
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
