#![feature(with_options)]

mod app;
mod db;
mod display;
mod misc;
mod task;

use app::Args;
use std::env;
use std::process;

const CFG_PATH: &str = env!("XDG_CONFIG_HOME");

fn main() {
    let args = Args::parse(env::args()).unwrap_or_else(|err| {
        eprintln!("{}", err);
        process::exit(1);
    });

    if let Err(e) = app::run(args) {
        eprintln!("Application error: {}", e);
        process::exit(1);
    }
}
