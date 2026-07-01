//! Uten Core — bytecode format and module management.
//!
//! This crate defines:
//! - `UtenModule` — the bytecode module format
//! - `FunctionDef` — function definitions
//! - Verification and loading utilities

pub mod bytecode;
pub mod tests;

pub use bytecode::*;
