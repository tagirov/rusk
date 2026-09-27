#[cfg(feature = "interactive")]
mod dialogs;
mod editor;
mod formatter;
mod handlers;

pub struct HandlerCLI;

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
