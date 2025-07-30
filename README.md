# Rusk - A minimal CLI todo manager

![rusk](rusk.png)

## Usage

### Add a task
```bash
rusk add buy groceries
rusk add buy groceries --date 2024-07-01 // or --date 2024-07-01 buy groceries
```

### List all tasks
```bash
rusk list
```

### Mark task as done
```bash
rusk mark 3
```

### Edit a task
```bash
rusk edit 3 --text "new text" --date 2024-07-01
```

### Delete a task
```bash
rusk del 3
```

### Delete all done tasks
```bash
rusk del --all
```

### Help
```bash
rusk --help
```


## Aliases
```bash
rusk a (add)
rusk l (list)
rusk m (mark)
rusk e (edit)
rusk d (del)

-t (--text)
-d (--date)
-h (--help)

```
