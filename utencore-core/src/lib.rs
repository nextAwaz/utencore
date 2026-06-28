//! Uten Core — A general-purpose stack-based language virtual machine.
//!
//! Universal stack machine VM for scripting languages. Provides ~130 opcodes,
//! pluggable GC, LLVM JIT, CIB FFI bridge, and a plugin compiler system.

pub mod ir;
pub mod vm;
pub mod cib;
pub mod jit;
pub mod plugin;
// Re-exports from sub-crates
pub use utencore_types::*;
pub use utencore_bytecode::*;
pub use utencore_gc::*;

pub mod ccis;
pub mod ucsl;

// Re-exports for convenience
pub use vm::*;


