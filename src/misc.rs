use std::error::Error;

pub fn version() -> Result<(), Box<dyn Error>> {
    println!("rusk 0.1.0 (2020-02-08)");

    Ok(())
}

pub fn help() -> Result<(), Box<dyn Error>> {
    println!(
        "
a    | Add a new task with or without a specific date (you must specify
                             the date according to the following pattern: 20yy.mm.dd)

l    | List of all tasks:  |id|, date (if specified), task
d    | Delete task by specified id

-h, --help       | Help menu
-v, --version    | Print version and date of release


 Examples:
 
rusk a 2020/01/31 Take a shower
rusk a Take a shower
rusk d 123"
    );

    Ok(())
}
