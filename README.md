# rusk
Extremely simple TODO-manager for CLI


## Usage


Add a task:
`
cargo run a task
`
If we want concrete date:
`
cargo run a 2020/02/08 task
`
Let's see what we need to do:
`
cargo run l
`
In this list we can see the identifiers of all the tasks that we have

We will use it to remove a task from the list:
`
cargo run d 456
`
#
At the first start (cargo run **a**(add) or **l**(list)) , Rusk creates a database file in `XDG_CONFIG_HOME/rusk/db.json`

You can also copy compiled file in `/bin/` directory and run it next time by `rusk` command in console:
```
sudo install /rusk/target/release/rusk /usr/bin/    
```
Quick help is available by the commands `-h` or `--help`
