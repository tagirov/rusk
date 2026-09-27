#![cfg(feature = "completions")]

// Shared test helpers, declared once so submodules can `use crate::common`.
mod common;

#[path = "completions/rust/completions_install_tests.rs"]
mod completions_install_tests;

#[path = "completions/rust/nu_completion_tests.rs"]
mod nu_completion_tests;

#[path = "completions/rust/table_tests.rs"]
mod table_tests;
