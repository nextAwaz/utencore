//! Uten Core — type system foundation.
//!
//! This crate provides the fundamental types used by all other UtenCore crates:
//! - `UValue` — the universal value type (stack values)
//! - `HeapObject` — GC heap objects
//! - `Opcode` — all supported opcodes
//! - `UtenError` — error types

pub mod types;
pub mod error;
pub mod opcodes;
pub mod tests;

// Re-exports for convenience
pub use types::*;
pub use error::*;
pub use opcodes::*;

/// Current version of the UtenCore VM
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Bytecode format version (independent of VM version).
pub const BYTECODE_VERSION: u32 = 3;

/// Magic bytes for .uclib files: "UCLB" (UtenCore Library)
pub const UCLIB_MAGIC: &[u8; 4] = b"UCLB";

/// Magic bytes for .ucch files: "UCCH" (UtenCore Cache)
pub const UCCH_MAGIC: &[u8; 4] = b"UCCH";

/// Magic bytes for .ucir files: "UCIR" (UtenCore IR)
pub const UCIR_MAGIC: &[u8; 4] = b"UCIR";

/// Magic bytes for .ucif files: "UCIF" (UtenCore Interface)
pub const UCIF_MAGIC: &[u8; 4] = b"UCIF";

// End of constants
