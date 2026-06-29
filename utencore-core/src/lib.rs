//! Uten Core — A general-purpose stack-based language virtual machine.
//!
//! Universal stack machine VM for scripting languages. Provides ~120 opcodes,
//! pluggable GC, LLVM JIT, CIB FFI bridge, and a plugin compiler system.

pub mod ir;
pub mod jit;
pub mod vm;
pub mod cib;
pub mod plugin;
pub mod ccis;
pub mod ucsl;

// Re-exports from sub-crates
pub use utencore_types::*;
pub use utencore_bytecode::*;
pub use utencore_gc::*;

// Re-exports for convenience
pub use vm::*;
