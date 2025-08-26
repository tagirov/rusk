<h1 align="center">rusk</h1>
<p align="center">A minimal terminal task manager</p>
<p align="center">
    <a href="https://github.com/tagirov/rusk/releases"><img alt="GitHub Release" src="https://img.shields.io/github/v/release/tagirov/rusk?logo=github&labelColor=blue"></a>
</p>

<p align="center"><img src="rusk.png" alt="demonstration of rusk in a battle"></p>

# Install
#### Manually
```
git clone https://github.com/tagirov/rusk && cd rusk
cargo build --release
sudo install ./target/release/rusk /usr/bin
```
#### Arch Linux (AUR)
```
paru -S rusk
```

#### Cargo via github
```
cargo install --git https://github.com/tagirov/rusk
```

> The binary will be installed to ~/.cargo/bin/rusk. To use it globally, you must either create a symlink in /usr/bin or add ~/.cargo/bin to your $PATH.

# Usage

#### Add a task
```bash
rusk add buy groceries
rusk add buy groceries --date 2024-07-01
```

#### List all tasks. These commands are all the same
```bash
rusk list
rusk l
rusk
```

#### Mark or unmark a task as done
```bash
rusk mark 3
```

#### Edit a task
```bash
rusk edit 3 --text new text --date 2024-07-01
```

#### Delete a task
```bash
rusk del 3
```

#### Delete all done tasks
```bash
rusk del --all
```
#### Multiple tasks can be passed to the edit, mark, and del commands

```bash
rusk mark 1 2 5
rusk del {1..5}  ## 1 2 3 4 5 
rusk e 1 2 -t These tasks are hidden now -d 2000-1-1
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
-V (--version)

```
