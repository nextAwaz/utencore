//! Uten Core — A general-purpose stack-based language virtual machine.
//!
//! Universal stack machine VM for scripting languages. Provides ~130 opcodes,
//! pluggable GC, LLVM JIT, CIB FFI bridge, and a plugin compiler system.

#[path = "Ir.rs"]   pub mod ir;
#[path = "Jit.rs"]  pub mod jit;
pub mod vm;
pub mod cib;
#[path = "Plugin.rs"] pub mod plugin;
#[path = "Ccis.rs"] pub mod ccis;
#[path = "Ucsl.rs"] pub mod ucsl;

// Re-exports from sub-crates
pub use utencore_types::*;
pub use utencore_bytecode::*;
pub use utencore_gc::*;

// Re-exports for convenience
pub use vm::*;
