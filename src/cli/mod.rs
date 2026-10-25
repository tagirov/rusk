#[cfg(feature = "interactive")]
mod dialogs;
mod editor;
mod formatter;
mod handlers;

pub struct HandlerCLI;

/// A bound of `rusk rm done --older` / `--newer` (and `rm all`'s): as it was given (an age
/// or a date), for the messages, and the day it names.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Bound {
    pub text: String,
    pub day: chrono::NaiveDate,
}

/// What `rusk rm done` / `rusk rm all` was told to select: the tasks of
/// the scope with a day from `newer` to `older`, both inclusive, either
/// bound optional.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Bounds {
    pub scope: crate::storage::Scope,
    pub older: Option<Bound>,
    pub newer: Option<Bound>,
}

impl Bounds {
    pub fn selection(&self) -> crate::storage::Selection {
        crate::storage::Selection {
            scope: self.scope,
            older: self.older.as_ref().map(|b| b.day),
            newer: self.newer.as_ref().map(|b| b.day),
        }
    }

    /// The bounds in words: `older than 7d`, `newer than 01-09-2026`, or
    /// both joined with `and`; empty without a bound.
    pub fn describe(&self) -> String {
        let mut parts = Vec::new();
        if let Some(b) = &self.older {
            parts.push(format!("older than {}", b.text));
        }
        if let Some(b) = &self.newer {
            parts.push(format!("newer than {}", b.text));
        }
        parts.join(" and ")
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use std::sync::{Mutex, MutexGuard};

    static COLORS: Mutex<()> = Mutex::new(());

    /// Colors on for as long as it lives. The override is global to the
    /// process and the unit tests run on parallel threads: a test that
    /// depends on it holds the lock.
    pub(crate) struct ForcedColors {
        _lock: MutexGuard<'static, ()>,
    }

    impl Drop for ForcedColors {
        fn drop(&mut self) {
            colored::control::unset_override();
        }
    }

    pub(crate) fn force_colors() -> ForcedColors {
        let lock = COLORS.lock().unwrap_or_else(|e| e.into_inner());
        colored::control::set_override(true);
        ForcedColors { _lock: lock }
    }
}
