<h1 align="center">rusk</h1>
<p align="center">A terminal todo manager</p>
<p align="center">
    <a href="https://github.com/tagirov/rusk/releases"><img alt="GitHub Release" src="https://img.shields.io/github/v/release/tagirov/rusk?logo=github&labelColor=blue"></a>
</p>

<p align="center"><img src="rusk.png" alt="demonstration of rusk in a battle"></p>

## Install
#### Manually
```
git clone https://github.com/tagirov/rusk && cd rusk
cargo build --release
sudo install ./target/release/rusk /usr/bin
```
#### Arch User Repository (AUR)
```
yay -S rusk
```

## Usage

##### Add a task
```bash
rusk add buy groceries
rusk add buy groceries --date 2024-07-01 // or --date 2024-07-01 buy groceries
```

##### List all tasks
```bash
rusk list
```

##### Mark task as done
```bash
rusk mark 3
```

##### Edit a task
```bash
rusk edit 3 --text "new text" --date 2024-07-01
```

##### Delete a task
```bash
rusk del 3
```

##### Delete all done tasks
```bash
rusk del --all
```

##### Help
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
