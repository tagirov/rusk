use std::fs::{self, File};
use std::path::Path;

pub fn db() -> File {
    let cfg_dir = format!("{}/{}", &crate::CFG_PATH, "rusk");
    let file_path = format!("{}/{}", &cfg_dir, "db.json");

    if !Path::new(&cfg_dir).exists() {
        fs::create_dir_all(cfg_dir).expect("Can't create the database");

        println!("\nDatabase file created successfully");
    }

    let db_file = File::with_options()
        .read(true)
        .write(true)
        .append(true)
        .create(true)
        .open(&file_path)
        .expect("Can't load the database");

    db_file
}
