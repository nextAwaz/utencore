//! CCIS-based Plugin System — UtenCore Common Compiler Interface Specification.
//!
//! Split into:
//!   Manifest.rs — PluginManifest, PluginInfo, PluginType, type aliases
//!   Manager.rs — PluginManager (registration, compilation dispatch, dynamic loading)
//!   mod.rs    — Re-exports + CCIS ABI helper functions

#[path = "Manifest.rs"] pub mod manifest;
#[path = "Manager.rs"]  pub mod manager;

pub use manifest::*;
pub use manager::PluginManager;

// ── CCIS ABI helpers (exported by the VM for dynamic plugins) ──

use crate::ccis::{CcisCompileResult, CCIS_ABI_VERSION};

#[no_mangle]
pub extern "C" fn ccis_plugin_version() -> u32 {
    CCIS_ABI_VERSION
}

/// CCIS compile function stub for dynamic plugins.
#[no_mangle]
pub extern "C" fn ccis_compile(
    source: *const std::os::raw::c_char,
    filename: *const std::os::raw::c_char,
    options_json: *const std::os::raw::c_char,
) -> CcisCompileResult {
    let _source = if source.is_null() { String::new() } else { unsafe { std::ffi::CStr::from_ptr(source) }.to_string_lossy().into_owned() };
    let _filename = if filename.is_null() { String::new() } else { unsafe { std::ffi::CStr::from_ptr(filename) }.to_string_lossy().into_owned() };
    let err_msg = if options_json.is_null() { "options_json is null" } else { "Dynamic ccis_compile not implemented" };
    CcisCompileResult {
        data: std::ptr::null(),
        len: 0,
        error: std::ffi::CString::new(err_msg).unwrap().into_raw(),
    }
}

/// CCIS free result stub.
#[no_mangle]
pub extern "C" fn ccis_free_result(result: CcisCompileResult) {
    if !result.data.is_null() {
        unsafe { let _ = Vec::from_raw_parts(result.data as *mut u8, result.len, result.len); }
    }
    if !result.error.is_null() {
        unsafe { let _ = std::ffi::CString::from_raw(result.error as *mut std::os::raw::c_char); }
    }
}
