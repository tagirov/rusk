#[derive(Debug, PartialEq, Eq)]
pub enum AppError {
    SkipTask,
}

impl std::fmt::Display for AppError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AppError::SkipTask => write!(f, "task skipped by user"),
        }
    }
}

impl std::error::Error for AppError {}
