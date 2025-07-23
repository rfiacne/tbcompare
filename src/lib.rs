//! # tbcompare
//! 
//! A tool for comparing text files with specific naming conventions.
//! 
//! This library provides functionality to compare pairs of files that follow a specific naming pattern,
//! detect file encodings, and report differences between them.

pub mod file_utils;
pub mod comparison;

pub use file_utils::{detect_encoding, read_and_process_file};
pub use comparison::{compare_files, generate_file_pairs};