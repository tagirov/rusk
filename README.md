# rusk
TODO manager for CLI

## install

Clone the repo

```
git clone https://github.com/tagirov/rusk && cd rusk
```
Build binary

```
cargo build --release
```

Install compiled file system-wide
```
sudo install ./target/release/rusk /usr/bin/    
```

## usage

If the database file is missing, Rusk creates `$HOME/rusk/db.json` when you exec either `rusk a` or `rusk l`

Add task
```
rusk a task_name
```
Add task with the concrete date
```
rusk a 2020/02/08 task_name
```
Task list
```
rusk l
```
in the list above we can see identifiers of all tasks that we have

Remove the task from the list
```
rusk d task_id
```
