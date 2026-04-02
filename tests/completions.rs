#![cfg(feature = "completions")]

#[path = "completions/rust/completion_tests.rs"]
mod completion_tests;

#[path = "completions/rust/completions_install_tests.rs"]
mod completions_install_tests;

#[path = "completions/rust/nu_completion_tests.rs"]
mod nu_completion_tests;
