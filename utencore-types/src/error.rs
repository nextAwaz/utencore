//! UtenCore error types.

use thiserror::Error;

/// All errors that can occur in the UtenCore VM.
#[derive(Debug, Error)]
pub enum UtenError {
    #[error("VM error: {0}")]
    Vm(String),

    #[error("Stack underflow: needed {needed}, had {actual}")]
    StackUnderflow { needed: usize, actual: usize },

    #[error("Type error: expected {expected:?}, got {actual:?}")]
    TypeError { expected: &'static str, actual: String },

    #[error("Unknown opcode: 0x{0:02x}")]
    UnknownOpcode(u8),

    #[error("Module error: {0}")]
    Module(String),

    #[error("CIB error: {0}")]
    Cib(String),

    #[error("JIT error: {0}")]
    Jit(String),

    #[error("GC error: {0}")]
    Gc(String),

    #[error("{0}")]
    Plugin(String),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Serialization error: {0}")]
    Serde(#[from] bincode::Error),

    #[error("Parse error: {0}")]
    Parse(String),

    #[error("Halt: {0}")]
    Halt(String),

    #[error("Verification error: {0}")]
    Verify(String),
}

pub type UtenResult<T> = Result<T, UtenError>;
