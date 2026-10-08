# The demo recording for README.md, made with vhs
# (https://github.com/charmbracelet/vhs): `cargo build --release && vhs rusk.tape`
# from the repository root. The database is a throwaway file and the config is
# off, so the recording never touches your tasks.
Output rusk.gif

Set Shell "bash"
Set Theme "GitHub Dark"
Set Width 860
Set Height 540
Set FontSize 20
Set Padding 20
Set Framerate 20
Set TypingSpeed 40ms

Hide
Type "export PATH=$PWD/target/release:$PATH RUSK_DB=$(mktemp -d)/tasks.json RUSK_CONFIG= && clear"
Enter
Show

Sleep 500ms
Type "rusk add Buy groceries"
Sleep 300ms
Enter
Sleep 1s
Type "rusk add Call the dentist -d 3d"
Sleep 300ms
Enter
Sleep 1s
Type "rusk add Finish the report -d 2w"
Sleep 300ms
Enter
Sleep 1s
Type "rusk add Deploy the release -a 2,3"
Sleep 300ms
Enter
Sleep 1s
Type "rusk add FIXME flaky test on CI"
Sleep 300ms
Enter
Sleep 1.5s

Hide
Type "clear"
Enter
Show

Type "rusk mark 1"
Sleep 300ms
Enter
Sleep 1s
Type "rusk mark 3 -p"
Sleep 300ms
Enter
Sleep 1s
Type "rusk"
Sleep 300ms
Enter
Sleep 3s

Hide
Type "clear"
Enter
Show

Type "rusk search dentist"
Sleep 300ms
Enter
Sleep 2s
Type "rusk edit 2 -d +1w"
Sleep 300ms
Enter
Sleep 2.5s

Hide
Type "clear"
Enter
Show

Type "rusk del 1"
Sleep 300ms
Enter
Sleep 1s
Type "y"
Sleep 1s
Type "rusk"
Sleep 300ms
Enter
Sleep 3s

Hide
Type "clear"
Enter
Show

Type "rusk edit 3"
Sleep 300ms
Enter
Sleep 1.5s
Right 30
Sleep 500ms
Type " and send it"
Sleep 1s
Ctrl+S
Sleep 4s
