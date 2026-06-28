//! Uten Core — garbage collection engine.
//!
//! Pluggable GC with mark-sweep and reference-counting implementations.

pub mod memory;

pub use memory::GcEngine;
pub use memory::GcStats;
pub use memory::TraceRoots;
// GcHandle is re-exported from utencore-types (included via utencore_gc::GcHandle)
pub use utencore_types::GcHandle;
