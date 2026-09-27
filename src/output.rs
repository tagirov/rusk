//! Standard output and standard error of the commands, and whether they
//! are colored.
//!
//! `println!` panics when a write fails: `rusk list | head -2` ended in
//! "failed printing to stdout: Broken pipe" and exit 101, and a full disk
//! the same way (REVIEW №40). What a command prints goes through [`out!`] and
//! [`outln!`] instead, which hand the failure back as an error: a reader
//! that stopped reading is [`Closed`], on which `main` ends the command
//! quietly (the work is done, only nobody wants to read the rest); any other
//! failure is reported like every error. A message on standard error never
//! fails the command: there is nowhere left to report that.

use std::ffi::OsString;
use std::fmt;
use std::io::{self, Write};
use std::sync::OnceLock;

/// Standard output was closed by its reader (`rusk list | head`).
#[derive(Debug)]
pub struct Closed;

impl fmt::Display for Closed {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("standard output was closed by its reader")
    }
}

impl std::error::Error for Closed {}

/// Writes to standard output, flushed: a prompt that does not end its line
/// is on the screen before the answer is read.
pub fn write_fmt(args: fmt::Arguments<'_>) -> anyhow::Result<()> {
    let mut stdout = io::stdout().lock();
    stdout
        .write_fmt(args)
        .and_then(|()| stdout.flush())
        .map_err(failed_write)
}

fn failed_write(error: io::Error) -> anyhow::Error {
    if error.kind() == io::ErrorKind::BrokenPipe {
        anyhow::Error::new(Closed)
    } else {
        anyhow::Error::new(error).context("Failed to write to standard output")
    }
}

/// Whether `error` comes from a standard output its reader has closed.
pub fn is_closed(error: &anyhow::Error) -> bool {
    error.chain().any(|cause| cause.is::<Closed>())
}

/// Writes to standard error; a failure is ignored.
pub fn ewrite_fmt(args: fmt::Arguments<'_>) {
    let mut stderr = io::stderr().lock();
    let _ = stderr.write_fmt(args).and_then(|()| stderr.flush());
}

/// `print!` that returns a failed write as an error (see [`write_fmt`]).
#[macro_export]
macro_rules! out {
    ($($arg:tt)*) => {
        $crate::output::write_fmt(format_args!($($arg)*))
    };
}

/// `println!` that returns a failed write as an error (see [`write_fmt`]).
#[macro_export]
macro_rules! outln {
    () => {
        $crate::output::write_fmt(format_args!("\n"))
    };
    ($($arg:tt)*) => {
        $crate::output::write_fmt(format_args!("{}\n", format_args!($($arg)*)))
    };
}

/// `eprintln!` that cannot panic (see [`ewrite_fmt`]).
#[macro_export]
macro_rules! errln {
    ($($arg:tt)*) => {
        $crate::output::ewrite_fmt(format_args!("{}\n", format_args!($($arg)*)))
    };
}

/// Whether output is colored, decided once at start-up.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Colors {
    /// All output of this process: rusk's own (the `colored` crate) and
    /// clap's `--help` and argument errors alike.
    pub on: bool,
    /// The user turned colors off.
    pub off_by_user: bool,
}

impl Colors {
    /// `RUSK_NO_COLOR`, a `NO_COLOR` that is set and not empty, or
    /// `no_color = true` in the config turn colors off, whatever else is
    /// set. no-color.org: an empty `NO_COLOR` means nothing, which is how
    /// `RUSK_NO_COLOR` has always read. Otherwise `CLICOLOR_FORCE` (not
    /// empty, not `0`) turns them on, `CLICOLOR=0` or `TERM=dumb` off, and a
    /// terminal on standard output decides. `var` reads the environment.
    pub fn decide(
        var: impl Fn(&str) -> Option<OsString>,
        no_color_config: bool,
        stdout_is_terminal: bool,
    ) -> Self {
        let set = |name: &str| var(name).is_some_and(|value| !value.is_empty());
        let off_by_user = set("RUSK_NO_COLOR") || set("NO_COLOR") || no_color_config;
        let on = !off_by_user
            && if var("CLICOLOR_FORCE").is_some_and(|v| !v.is_empty() && v != "0") {
                true
            } else if var("CLICOLOR").is_some_and(|v| v == "0")
                || var("TERM").is_some_and(|v| v == "dumb")
            {
                false
            } else {
                stdout_is_terminal
            };
        Self { on, off_by_user }
    }

    /// The same decision for clap, which would otherwise make its own per
    /// stream from rules of its own (`CLICOLOR_FORCE=0` forces there).
    pub fn clap(self) -> clap::ColorChoice {
        if self.on {
            clap::ColorChoice::Always
        } else {
            clap::ColorChoice::Never
        }
    }
}

static COLORS: OnceLock<Colors> = OnceLock::new();

/// Applies `colors` to all output of this process.
pub fn set_colors(colors: Colors) {
    colored::control::set_override(colors.on);
    let _ = COLORS.set(colors);
}

/// The decision [`set_colors`] applied; off when none was made.
pub fn colors() -> Colors {
    COLORS.get().copied().unwrap_or(Colors {
        on: false,
        off_by_user: false,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn decide(vars: &[(&str, &str)], config: bool, tty: bool) -> Colors {
        Colors::decide(
            |name| {
                vars.iter()
                    .find(|(key, _)| *key == name)
                    .map(|(_, value)| OsString::from(value))
            },
            config,
            tty,
        )
    }

    #[test]
    fn a_terminal_gets_colors_and_a_pipe_does_not() {
        assert!(decide(&[], false, true).on);
        assert!(!decide(&[], false, false).on);
        assert!(!decide(&[], false, true).off_by_user);
    }

    /// REVIEW №123: an empty NO_COLOR turned colors off (the `colored`
    /// crate reacts to its mere presence), against no-color.org.
    #[test]
    fn an_empty_no_color_means_nothing() {
        assert!(decide(&[("NO_COLOR", "")], false, true).on);
        assert!(decide(&[("RUSK_NO_COLOR", "")], false, true).on);
        for vars in [&[("NO_COLOR", "1")][..], &[("RUSK_NO_COLOR", "yes")]] {
            let colors = decide(vars, false, true);
            assert!(!colors.on && colors.off_by_user, "{vars:?}");
        }
    }

    #[test]
    fn the_config_turns_colors_off_and_nothing_turns_them_back_on() {
        let colors = decide(&[("CLICOLOR_FORCE", "1")], true, true);
        assert!(!colors.on && colors.off_by_user);
        assert_eq!(colors.clap(), clap::ColorChoice::Never);
        // A user who said "no colors" is not overruled by a forcing
        // variable meant for pipes.
        assert!(!decide(&[("NO_COLOR", "1"), ("CLICOLOR_FORCE", "1")], false, true).on);
        assert!(!decide(&[("RUSK_NO_COLOR", "1"), ("CLICOLOR_FORCE", "1")], false, false).on);
    }

    #[test]
    fn clicolor_force_and_clicolor_speak_for_the_terminal() {
        assert!(decide(&[("CLICOLOR_FORCE", "1")], false, false).on);
        assert!(!decide(&[("CLICOLOR_FORCE", "0")], false, false).on);
        assert!(!decide(&[("CLICOLOR_FORCE", "")], false, false).on);
        assert!(!decide(&[("CLICOLOR", "0")], false, true).on);
        assert!(decide(&[("CLICOLOR", "1")], false, true).on);
        assert!(!decide(&[("TERM", "dumb")], false, true).on);
        assert!(decide(&[("TERM", "dumb"), ("CLICOLOR_FORCE", "1")], false, false).on);
    }

    /// Review of R19: clap decided on its own (`CLICOLOR_FORCE=0` forced
    /// its colors, `TERM=dumb` took them away), apart from the rest.
    #[test]
    fn clap_gets_the_same_decision() {
        assert_eq!(decide(&[("CLICOLOR_FORCE", "0")], false, false).clap(), clap::ColorChoice::Never);
        assert_eq!(decide(&[], false, true).clap(), clap::ColorChoice::Always);
        assert_eq!(decide(&[("CLICOLOR", "0")], false, true).clap(), clap::ColorChoice::Never);
    }

    #[test]
    fn a_closed_pipe_is_told_from_other_failures() {
        let closed = failed_write(io::Error::from(io::ErrorKind::BrokenPipe));
        assert!(is_closed(&closed));
        assert!(is_closed(&closed.context("while listing")));
        let full = failed_write(io::Error::from_raw_os_error(28));
        assert!(!is_closed(&full));
        let report = format!("{full:#}");
        assert!(report.starts_with("Failed to write to standard output: "), "{report}");
    }
}
