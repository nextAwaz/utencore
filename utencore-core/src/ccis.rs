//! CCIS — Common Compiler Interface Specification.
//!
//! Standardized plugin manifest and ABI for compiler plugins.
//! Every compiler plugin ships a CCIS manifest (JSON) that declares:
//! - Identity (name, version, language)
//! - Supported source file extensions
//! - Default GC strategy (written to .uclib header)
//! - Required CIB interfaces for C-library calls
//! - Plugin metadata (compiler-specific config)
//!
//! # ABI Stability
//!
//! `CCIS_ABI_VERSION` 1 uses C-compatible FFI: plugins are .so/.dll
//! exposing `ccis_init()` which returns the manifest as a JSON string.
//! Compilation is dispatched via `ccis_compile()`.

use std::collections::HashMap;
use serde::{Deserialize, Serialize};

/// Current CCIS ABI version.
pub const CCIS_ABI_VERSION: u32 = 1;

/// Current CCIS manifest format version.
pub const CCIS_MANIFEST_VERSION: u32 = 1;

// ── CCIS Manifest ──

/// A compiler plugin manifest (JSON-serializable), **deprecated**.
///
/// ⚠️ This type is kept for C ABI compatibility (`ccis_init()` JSON output).
/// For new code, use `PluginManifest` from `plugin.rs` instead — it includes
/// bytecode version compatibility checking and is the canonical format.
///
/// `CcisManifest` is automatically converted to `PluginManifest` internally
/// when passed to `PluginManager::register_compiler()`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[deprecated(since = "0.0.5", note = "use plugin::PluginManifest instead")]
pub struct CcisManifest {
    /// CCIS manifest format version (must == CCIS_MANIFEST_VERSION)
    pub ccis_version: u32,
    /// Plugin name (e.g. "py2uc", "ts2uc")
    pub name: String,
    /// Plugin version string
    pub version: String,
    /// Human-readable description
    pub description: String,
    /// Source language name (e.g. "Python 3", "TypeScript")
    pub language: String,
    /// File extensions this compiler handles (e.g. ["py"])
    pub extensions: Vec<String>,
    /// Default GC strategy written to .uclib headers.
    /// One of: "generational", "mark-sweep", "refcount", "none"
    #[serde(default = "default_gc")]
    pub default_gc: String,
    /// Whether JIT is recommended for this language
    #[serde(default)]
    pub jit_recommended: bool,
    /// CIB interface names this compiler needs (auto-loaded on registration)
    #[serde(default)]
    pub cib_interfaces: Vec<String>,
    /// Plugin ABI version (for dynamic loading compatibility)
    pub abi_version: u32,
    /// Compiler-specific key-value metadata
    #[serde(default)]
    pub metadata: HashMap<String, String>,
}

fn default_gc() -> String { "generational".into() }

impl CcisManifest {
    pub fn new(name: &str, language: &str, extensions: &[&str]) -> Self {
        CcisManifest {
            ccis_version: CCIS_MANIFEST_VERSION,
            name: name.to_string(),
            version: env!("CARGO_PKG_VERSION").to_string(),
            description: String::new(),
            language: language.to_string(),
            extensions: extensions.iter().map(|s| s.to_string()).collect(),
            default_gc: "generational".to_string(),
            jit_recommended: false,
            cib_interfaces: Vec::new(),
            abi_version: CCIS_ABI_VERSION,
            metadata: HashMap::new(),
        }
    }

    /// Validate the manifest for required fields.
    pub fn validate(&self) -> Result<(), Vec<String>> {
        let mut errs = Vec::new();
        if self.ccis_version != CCIS_MANIFEST_VERSION {
            errs.push(format!("Unsupported CCIS manifest version: {}", self.ccis_version));
        }
        if self.name.is_empty() {
            errs.push("Plugin name is empty".into());
        }
        if self.extensions.is_empty() {
            errs.push("No source extensions declared".into());
        }
        if self.abi_version != CCIS_ABI_VERSION {
            errs.push(format!("Unsupported CCIS ABI version: {}", self.abi_version));
        }
        match self.default_gc.as_str() {
            "generational" | "mark-sweep" | "refcount" | "none" => {}
            _ => errs.push(format!("Unknown GC strategy: {}", self.default_gc)),
        }
        if errs.is_empty() { Ok(()) } else { Err(errs) }
    }

    /// Serialize to JSON string.
    pub fn to_json(&self) -> String {
        serde_json::to_string(self).unwrap_or_default()
    }

    /// Deserialize from JSON string.
    pub fn from_json(json: &str) -> Result<Self, String> {
        serde_json::from_str(json).map_err(|e| format!("CCIS manifest parse: {e}"))
    }
}

// ── CCIS Plugin ABI (C FFI) ──

/// Compilation context: what a compiler receives.
/// The compiler fills in the module via the `builder` — no serialization round-trip,
/// no `std::mem::replace` workaround needed.
pub struct CompileContext<'a, 'b> {
    /// Source code to compile
    pub source: &'a str,
    /// Source filename (for error reporting and module name)
    pub filename: &'a str,
    /// Module builder — provides intern()/emit()/finish_function() in one place.
    /// Uses a separate lifetime 'b from the context to allow the builder to be
    /// borrowed independently (e.g., finalized) after the context is dropped.
    pub builder: &'b mut crate::bytecode::ModuleBuilder<'a>,
    /// Compilation options
    pub options: &'a CompilerOptions,
}

/// Compiler options that affect code generation.
///
/// GC strategy is set by the plugin manifest (PluginManifest::ccis.default_gc),
/// NOT here. This struct only contains per-invocation options.
#[derive(Debug, Clone)]
pub struct CompilerOptions {
    /// Optimization level (0 = none, 1 = basic, 2 = aggressive)
    pub optimize: u8,
    /// Whether to emit debug info (line numbers, etc.)
    pub emit_debug: bool,
    /// Whether to emit line number mappings in the module header
    pub emit_line_map: bool,
}

impl Default for CompilerOptions {
    fn default() -> Self {
        CompilerOptions {
            optimize: 0,
            emit_debug: false,
            emit_line_map: false,
        }
    }
}

/// A structured compilation error with source location.
#[derive(Debug, Clone)]
pub struct CompileError {
    pub message: String,
    pub file: String,
    pub line: usize,
    pub col: usize,
    pub level: ErrorLevel,
}

/// Severity level of a compilation diagnostic.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorLevel {
    Error,
    Warning,
    Note,
}

impl CompileError {
    pub fn error(msg: &str, file: &str, line: usize, col: usize) -> Self {
        CompileError {
            message: msg.to_string(),
            file: file.to_string(),
            line, col,
            level: ErrorLevel::Error,
        }
    }

    pub fn warning(msg: &str, file: &str, line: usize, col: usize) -> Self {
        CompileError {
            message: msg.to_string(),
            file: file.to_string(),
            line, col,
            level: ErrorLevel::Warning,
        }
    }
}

impl std::fmt::Display for CompileError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let tag = match self.level {
            ErrorLevel::Error => "error",
            ErrorLevel::Warning => "warning",
            ErrorLevel::Note => "note",
        };
        write!(f, "{}:{}:{}: {}: {}", self.file, self.line, self.col, tag, self.message)
    }
}

/// Result of a CCIS compile call.
#[repr(C)]
pub struct CcisCompileResult {
    pub data: *const u8,
    pub len: usize,
    pub error: *const std::os::raw::c_char,
}

/// CCIS plugin ABI functions (exported by dynamic plugins).
///
/// Required exports:
/// - `ccis_init()`           → *const c_char (JSON manifest)
/// - `ccis_compile(...)`     → CcisCompileResult
/// - `ccis_free_result(r)`   → ()
/// - `ccis_get_default_gc()` → *const c_char (GC strategy string)
#[no_mangle]
pub extern "C" fn ccis_alloc(len: usize) -> *mut u8 {
    let mut v = Vec::with_capacity(len);
    let ptr = v.as_mut_ptr();
    std::mem::forget(v);
    ptr
}

#[no_mangle]
pub extern "C" fn ccis_free(ptr: *mut u8, len: usize) {
    unsafe { let _ = Vec::from_raw_parts(ptr, len, len); }
}

// ── Borrowed string helpers for marshal ──

/// A zero-copy string reference backed by a module's string table.
/// Used when passing strings across the FFI boundary without allocation.
#[derive(Debug, Clone)]
pub struct BorrowedStr {
    pub module_id: u16,
    pub string_id: u32,
}
