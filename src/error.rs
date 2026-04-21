#[derive(Debug, PartialEq, Eq)]
pub enum AppError {
    /// User pressed Esc in the interactive editor and there is another task
    /// after the current one to edit. Handled silently by the batch edit loop.
    SkipTask,
    /// User cancelled the editor session (e.g. Esc on the last task, or a
    /// discarded change confirmation). Maps to a clean `exit 0` in `main`.
    UserCancel,
    /// User pressed Ctrl+C inside an interactive prompt. Maps to `exit 130`
    /// in `main` (the conventional SIGINT exit code).
    UserAbort,
}

impl std::fmt::Display for AppError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AppError::SkipTask => write!(f, "task skipped by user"),
            AppError::UserCancel => write!(f, "cancelled by user"),
            AppError::UserAbort => write!(f, "aborted by user"),
        }
    }
}

impl std::error::Error for AppError {}
