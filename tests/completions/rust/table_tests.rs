//! The completion scripts of all five shells against one table of cases
//! (`tests/completions/cases.txt`, REVIEW №145): the shell suites next to
//! it checked little more than that a function exists. A shell that is not
//! installed is skipped (said on stderr: `--nocapture` shows it). The
//! scripts run in a [`common::Sandbox`] — its home, config and one-task
//! database, in a debug and a release build alike — with the `rusk` of
//! this build first in PATH.

use std::collections::{BTreeSet, HashMap};
use std::io::Read;
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use crate::common;

/// The shells of the table, by the name a case uses for them.
const SHELLS: &[&str] = &["bash", "zsh", "fish", "nu", "pwsh"];

/// How long one shell may take for the whole table.
const TIMEOUT: Duration = Duration::from_secs(120);

/// One line of the table.
struct Case {
    line: String,
    candidates: BTreeSet<String>,
}

/// The candidates of a case: separated by spaces; one that holds spaces is
/// written in double quotes.
fn candidates(field: &str) -> BTreeSet<String> {
    let mut all = BTreeSet::new();
    let mut rest = field.trim();
    while !rest.is_empty() {
        let (candidate, after) = match rest.strip_prefix('"') {
            Some(quoted) => quoted.split_once('"').expect("an unclosed quote in the table"),
            None => rest.split_once(' ').unwrap_or((rest, "")),
        };
        all.insert(candidate.to_string());
        rest = after.trim_start();
    }
    all
}

/// The cases for `shell`: the general ones, with the ones that name it in
/// their place, and the ones only it has. A shell a row names must be one
/// of [`SHELLS`] (`powershell` is `pwsh`): a misspelled one would leave the
/// row out in silence.
fn cases_for(shell: &str) -> Vec<Case> {
    let table = include_str!("../cases.txt");
    let mut general: Vec<Case> = Vec::new();
    let mut special: Vec<Case> = Vec::new();
    for (n, row) in table.lines().enumerate() {
        if row.starts_with('#') || row.trim().is_empty() {
            continue;
        }
        let mut fields = row.split('|');
        let line = fields.next().unwrap().to_string();
        let case = Case {
            candidates: candidates(fields.next().expect("a case needs its candidates")),
            line,
        };
        let into = match fields.next() {
            Some(shells) => {
                let names: Vec<&str> = shells
                    .split(',')
                    .map(|name| match name.trim() {
                        "powershell" => "pwsh",
                        name => name,
                    })
                    .collect();
                for name in &names {
                    assert!(SHELLS.contains(name), "cases.txt:{}: no shell {name:?}", n + 1);
                }
                if !names.contains(&shell) {
                    continue;
                }
                &mut special
            }
            None => &mut general,
        };
        assert!(!into.iter().any(|c| c.line == case.line), "cases.txt:{}: {:?} twice", n + 1, case.line);
        into.push(case);
    }
    for case in special {
        match general.iter_mut().find(|c| c.line == case.line) {
            Some(general) => general.candidates = case.candidates,
            None => general.push(case),
        }
    }
    general
}

/// Whether `program` runs here at all.
fn installed(program: &str, arg: &str) -> bool {
    Command::new(program)
        .arg(arg)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|status| status.success())
}

/// The words of a command line; a line that ends with a space ends with an
/// empty word.
fn words(line: &str) -> Vec<&str> {
    line.split(' ').collect()
}

fn single_quoted(word: &str) -> String {
    format!("'{}'", word.replace('\'', r"'\''"))
}

/// A script that prints, for every case, `<index>\t<candidates, each
/// followed by \x1f>` on a line of its own. What else a script prints —
/// on stdout or stderr — reaches the terminal of whoever presses Tab, so
/// nothing of it is thrown away here.
fn batch_script(shell: &str, script: &Path, cases: &[Case]) -> String {
    let script = script.display();
    let mut out = String::new();
    match shell {
        "bash" => {
            out.push_str(&format!("source '{script}'\n"));
            for (i, case) in cases.iter().enumerate() {
                let w = words(&case.line);
                let quoted: Vec<String> = w.iter().map(|w| single_quoted(w)).collect();
                out.push_str(&format!(
                    "COMP_WORDS=({}); COMP_CWORD={}; COMP_LINE={}; COMP_POINT=${{#COMP_LINE}}; COMPREPLY=(); \
                     _rusk_completion; printf '{i}\\t'; printf '%s\\x1f' \"${{COMPREPLY[@]}}\"; echo\n",
                    quoted.join(" "),
                    w.len() - 1,
                    single_quoted(&case.line)
                ));
            }
        }
        "zsh" => {
            // `compadd` works only inside the completion system. What the
            // script hands to it is collected and filtered the way compadd
            // filters: by `$PREFIX`, the word under the cursor as the
            // shell has it — not by `words`, which a script may change —
            // unless `-U` says not to. Options that take an argument skip
            // it; `-a` names arrays.
            // Sourced to define the functions only, not to run once.
            out.push_str(&format!("_RUSK_ZSH_SKIP_ENTRY=1 source '{script}'\n"));
            out.push_str(
                "compadd() { local unfiltered=0 arrays=0 c; local -a got; \
                   while (( $# )); do case $1 in \
                     --) shift; break;; \
                     -[PSpsIiWdJVXxrRMFEOAD]) shift 2;; \
                     -[PSpsIiWdJVXxrRMFEOAD]*) shift;; \
                     -U) unfiltered=1; shift;; \
                     -a) arrays=1; shift;; \
                     -[QfeqnlkC12]) shift;; \
                     *) break;; \
                   esac; done; \
                   if (( arrays )); then for c in \"$@\"; do got+=(\"${(@P)c}\"); done; else got=(\"$@\"); fi; \
                   for c in \"${got[@]}\"; do \
                     if (( unfiltered )) || [[ $c == ${PREFIX}* ]]; then candidates+=(\"$c\"); fi; \
                   done; }\n",
            );
            for (i, case) in cases.iter().enumerate() {
                let w = words(&case.line);
                let quoted: Vec<String> = w.iter().map(|w| single_quoted(w)).collect();
                out.push_str(&format!(
                    "() {{ local -a words candidates; words=({}); local CURRENT=${{#words}}; \
                     local PREFIX=${{words[CURRENT]}} SUFFIX=''; local LBUFFER={}; \
                     _rusk; printf '{i}\\t'; printf '%s\\x1f' \"${{candidates[@]}}\"; echo }}\n",
                    quoted.join(" "),
                    single_quoted(&case.line)
                ));
            }
        }
        "fish" => {
            out.push_str(&format!("source '{script}'\n"));
            for (i, case) in cases.iter().enumerate() {
                out.push_str(&format!(
                    "printf '{i}\\t'; for c in (complete -C {}); printf '%s\\x1f' (string split -f1 \\t -- $c); end; echo\n",
                    single_quoted(&case.line)
                ));
            }
        }
        "nu" => {
            out.push_str(&format!("use '{script}' *\n["));
            for case in cases {
                let spans: Vec<String> = words(&case.line)
                    .iter()
                    .map(|w| serde_json::to_string(w).unwrap())
                    .collect();
                out.push_str(&format!("[{}] ", spans.join(" ")));
            }
            out.push_str(
                "] | enumerate | each {|case| $\"($case.index)\\t(rusk-completions-main $case.item | get value | each {|v| $v + (char -u '1f')} | str join)\" } | str join \"\\n\" | print\n",
            );
        }
        "pwsh" => {
            out.push_str(&format!(". '{script}'\n"));
            for (i, case) in cases.iter().enumerate() {
                let line = case.line.replace('\'', "''");
                // Where a script offers nothing, PowerShell offers the
                // file names of the current directory; those are its own,
                // not the script's, and are left out.
                out.push_str(&format!(
                    "$r = TabExpansion2 -inputScript '{line}' -cursorColumn {}; \
                     $m = $r.CompletionMatches | Where-Object {{ $_.ResultType -notin 'ProviderItem', 'ProviderContainer' }}; \
                     '{i}' + \"`t\" + (($m.CompletionText | ForEach-Object {{ $_ + [char]0x1f }}) -join '')\n",
                    case.line.len()
                ));
            }
        }
        _ => unreachable!(),
    }
    out
}

/// `command`'s output, or `None` when it does not end within [`TIMEOUT`]
/// (it is killed then): a script that waits for input must not hang the
/// test run.
fn output_within(mut command: Command) -> Option<std::process::Output> {
    let mut child = command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let mut stdout = child.stdout.take().unwrap();
    let mut stderr = child.stderr.take().unwrap();
    let out = std::thread::spawn(move || {
        let mut bytes = Vec::new();
        stdout.read_to_end(&mut bytes).map(|_| bytes)
    });
    let err = std::thread::spawn(move || {
        let mut bytes = Vec::new();
        stderr.read_to_end(&mut bytes).map(|_| bytes)
    });
    let deadline = Instant::now() + TIMEOUT;
    let status = loop {
        if let Some(status) = child.try_wait().unwrap() {
            break status;
        }
        if Instant::now() > deadline {
            let _ = child.kill();
            let _ = child.wait();
            return None;
        }
        std::thread::sleep(Duration::from_millis(20));
    };
    Some(std::process::Output {
        status,
        stdout: out.join().unwrap().unwrap(),
        stderr: err.join().unwrap().unwrap(),
    })
}

/// Runs the cases in `shell`, when it is installed; what is wrong.
fn run_table(shell: &str, program: &str, version_arg: &str, args: &[&str], script: &str) -> Vec<String> {
    if !installed(program, version_arg) {
        eprintln!("{shell} is not installed: its completion table is skipped");
        return Vec::new();
    }
    let cases = cases_for(shell);
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    // What `rusk list --for-completion-lines` gives the scripts: the tasks
    // of the sandbox's database, whichever build answers.
    let sb = common::Sandbox::with_db(r#"[{"id":1,"text":"buy milk"}]"#);
    let rusk = common::require_rusk_bin().unwrap();
    let path = std::env::join_paths(
        std::iter::once(rusk.parent().unwrap().to_path_buf())
            .chain(std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default())),
    )
    .unwrap();
    let batch = batch_script(shell, &root.join("completions").join(script), &cases);
    // `pwsh -File` takes a file only by its extension on Windows.
    let file = sb.path().join(format!("cases.{}", if shell == "pwsh" { "ps1" } else { shell }));
    std::fs::write(&file, batch).unwrap();

    let template = sb.cmd();
    let mut command = Command::new(program);
    for (key, value) in template.get_envs() {
        match value {
            Some(value) => command.env(key, value),
            None => command.env_remove(key),
        };
    }
    command.args(args).arg(&file).env("PATH", path).current_dir(sb.path());
    let Some(output) = output_within(command) else {
        return vec![format!("{shell}: the table did not finish within {TIMEOUT:?}")];
    };

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let mut wrong = Vec::new();
    if !stderr.trim().is_empty() {
        wrong.push(format!("{shell}: printed on stderr: {}", stderr.trim()));
    }
    let mut got: HashMap<usize, Vec<String>> = HashMap::new();
    for row in stdout.lines() {
        match row.split_once('\t').and_then(|(index, candidates)| Some((index.parse::<usize>().ok()?, candidates))) {
            Some((index, candidates)) => {
                let offered = candidates.split('\x1f').filter(|c| !c.is_empty()).map(str::to_string).collect();
                got.insert(index, offered);
            }
            None => wrong.push(format!("{shell}: printed {row:?} (on the user's terminal)")),
        }
    }
    for (i, case) in cases.iter().enumerate() {
        let Some(offered) = got.get(&i) else {
            wrong.push(format!("{shell}: {:?} gave no answer", case.line));
            continue;
        };
        let set: BTreeSet<String> = offered.iter().cloned().collect();
        if set.len() != offered.len() {
            wrong.push(format!("{shell}: {:?} offered {offered:?}, some twice", case.line));
        } else if set != case.candidates {
            wrong.push(format!("{shell}: {:?} offered {set:?}, expected {:?}", case.line, case.candidates));
        }
    }
    wrong
}

#[test]
#[cfg(unix)]
fn bash_completes_every_case_of_the_table() {
    let wrong = run_table("bash", "bash", "--version", &["--norc", "--noprofile"], "rusk.bash");
    assert!(wrong.is_empty(), "{}", wrong.join("\n"));
}

#[test]
#[cfg(unix)]
fn zsh_completes_every_case_of_the_table() {
    let wrong = run_table("zsh", "zsh", "--version", &["-f"], "rusk.zsh");
    assert!(wrong.is_empty(), "{}", wrong.join("\n"));
}

#[test]
#[cfg(unix)]
fn fish_completes_every_case_of_the_table() {
    let wrong = run_table("fish", "fish", "--version", &["-N"], "rusk.fish");
    assert!(wrong.is_empty(), "{}", wrong.join("\n"));
}

#[test]
fn nu_completes_every_case_of_the_table() {
    let wrong = run_table("nu", "nu", "--version", &["--no-config-file"], "rusk.nu");
    assert!(wrong.is_empty(), "{}", wrong.join("\n"));
}

#[test]
fn powershell_completes_every_case_of_the_table() {
    let wrong = run_table("pwsh", "pwsh", "-Version", &["-NoProfile", "-NonInteractive", "-File"], "rusk.ps1");
    assert!(wrong.is_empty(), "{}", wrong.join("\n"));
}
