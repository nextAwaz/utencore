//! CIB (Central Interface Bridge) — production FFI engine.
//!
//! Architecture:
//!   cib/ffi.rs     — raw libffi bindings
//!   cib/marshal.rs — UValue ↔ C type marshalling
//!   cib/structs.rs — struct layout computation
//!   cib/ucif.rs    — UCIF schema + validation
//!   cib/mod.rs     — CibEngine (this file)
//!
//! Thread safety: All mutable state is behind RwLock.
//! Cross-compiler: Compilers declare C deps via .ucif, CIB doesn't know
//!   about Python/TS/Kotlin — only C types and calling conventions.

#[path = "Ffi.rs"]     pub mod ffi;
#[path = "Marshal.rs"] pub mod marshal;
#[path = "Structs.rs"] pub mod structs;
#[path = "Ucif.rs"]    pub mod ucif;

use std::collections::HashMap;
use std::ffi::CString;
use std::mem;
use std::sync::{Arc, RwLock};

use crate::error::{UtenError, UtenResult};
use utencore_types::*;
pub use marshal::{CType, MarshalledValue};
pub use structs::CStructLayout;
pub use ucif::*;

// ── Public API types (shorthand for UtenCore) ──

/// Function signature for CIB lookups.
#[derive(Debug, Clone, PartialEq)]
pub struct FuncSignature {
    pub param_types: Vec<ValueTag>,
    pub return_type: ValueTag,
    pub is_variadic: bool,
}

// ── Engine ──

/// The CIB engine. Clone is cheap (Arc<RwLock<...>>).
#[derive(Clone)]
pub struct CibEngine {
    inner: Arc<RwLock<CibInner>>,
}

struct CibInner {
    /// Loaded libraries (name → handle)
    libraries: HashMap<String, *mut std::os::raw::c_void>,
    /// Loaded interfaces (name → index)
    interfaces: HashMap<String, usize>,
    /// All interface data
    interface_list: Vec<UcifInterface>,
    /// Resolved function addresses: (interface_idx, func_idx) → fn_ptr
    resolved_fns: HashMap<(usize, usize), usize>,
    /// Pre-computed libffi CIFs: (interface_idx, func_idx) → FfiCif
    /// Cached at load time so call_typed doesn't prepare_cif on every call.
    cached_cifs: HashMap<(usize, usize), crate::cib::ffi::FfiCif>,
    /// Struct layouts (name → computed layout)
    structs: HashMap<String, CStructLayout>,
    /// Constants (name → UValue)
    constants: HashMap<String, UValue>,
    /// Resolved errno locations
    errno_loc: Option<usize>,
}

impl CibEngine {
    pub fn new() -> Self {
        CibEngine {
            inner: Arc::new(RwLock::new(CibInner {
                libraries: HashMap::new(),
                interfaces: HashMap::new(),
                interface_list: Vec::new(),
                resolved_fns: HashMap::new(),
                cached_cifs: HashMap::new(),
                structs: HashMap::new(),
                constants: HashMap::new(),
                errno_loc: None,
            })),
        }
    }

    /// Load all CIB interfaces requested by registered compiler plugins.
    /// Called during VM initialization after all plugins are registered.
    pub fn load_plugin_interfaces(&self, plugin_mgr: &crate::plugin::PluginManager) -> UtenResult<()> {
        let mut errors = Vec::new();
        for iface_name in &plugin_mgr.cib_dependencies {
            // Try loading from standard paths
            let paths = [
                format!("ucif/{iface_name}.ucif"),
                format!("{iface_name}.ucif"),
                format!("interfaces/{iface_name}.ucif"),
            ];
            let mut loaded = false;
            for path in &paths {
                if let Ok(data) = std::fs::read(path) {
                    match self.load_interface_bytes(&data) {
                        Ok(_) => {
                            log::info!("CIB: auto-loaded interface '{}' from {}", iface_name, path);
                            loaded = true;
                            break;
                        }
                        Err(e) => {
                            log::warn!("CIB: failed to load interface '{}' from {}: {}", iface_name, path, e);
                        }
                    }
                }
            }
            if !loaded {
                errors.push(format!("CIB interface '{}' not found (searched: {:?})", iface_name, paths));
            }
        }
        if errors.is_empty() {
            Ok(())
        } else {
            Err(UtenError::Cib(errors.join("; ")))
        }
    }

    // ── Library management ──

    /// Load a shared library.
    pub fn load_library(&self, name: &str) -> UtenResult<()> {
        let mut inner = self.inner.write().map_err(|e| UtenError::Cib(format!("Lock: {e}")))?;

        if inner.libraries.contains_key(name) {
            return Ok(());
        }

        let lib_name = if name.contains('.') || name.contains('/') {
            name.to_string()
        } else {
            format!("lib{name}.so")
        };

        let lib = unsafe { libloading::Library::new(&lib_name)
            .map_err(|e| UtenError::Cib(format!("dlopen '{lib_name}': {e}")))? };

        let handle = Box::into_raw(Box::new(lib));
        let ptr = handle as *mut std::os::raw::c_void;
        inner.libraries.insert(name.to_string(), ptr);

        // Initialize errno location if not set
        if inner.errno_loc.is_none() {
            inner.errno_loc = Some(get_errno_location(ptr));
        }

        log::info!("CIB: loaded '{lib_name}'");
        Ok(())
    }

    /// Unload a library by name.
    pub fn unload_library(&self, name: &str) -> UtenResult<()> {
        let mut inner = self.inner.write().map_err(|e| UtenError::Cib(format!("Lock: {e}")))?;
        if let Some(ptr) = inner.libraries.remove(name) {
            unsafe { drop(Box::from_raw(ptr as *mut libloading::Library)); }
            log::info!("CIB: unloaded '{name}'");
        }
        Ok(())
    }

    /// Load a UCIF interface from a .ucif file path (no extension needed).
    pub fn load_interface_file(&self, name: &str) -> UtenResult<usize> {
        let paths = [
            format!("ucif/{name}.ucif"),
            format!("{name}.ucif"),
        ];
        for path in &paths {
            if let Ok(data) = std::fs::read(path) {
                return self.load_interface_bytes(&data);
            }
        }
        Err(UtenError::Cib(format!("Interface '{name}' not found (searched: {:?})", paths)))
    }

    /// Find a symbol address in loaded libraries.
    pub fn find_symbol(&self, name: &str) -> Option<usize> {
        let inner = self.inner.read().ok()?;
        let c_name = CString::new(name).ok()?;
        for (&_, &ptr) in &inner.libraries {
            let lib = unsafe { &*(ptr as *const libloading::Library) };
            let result = unsafe { lib.get::<*mut std::os::raw::c_void>(c_name.as_bytes()) };
            if let Ok(symbol) = result {
                return Some(*symbol as usize);
            }
        }
        None
    }

    // ── Interface loading ──

    /// Load a UCIF interface from bytes.
    pub fn load_interface_bytes(&self, bytes: &[u8]) -> UtenResult<usize> {
        let ucif = UcifInterface::from_bytes(bytes)
            .map_err(|e| UtenError::Cib(e))?;
        ucif.validate().map_err(|e| UtenError::Cib(e.join("; ")))?;
        self.load_interface(ucif)
    }

    /// Load a parsed UCIF interface.
    pub fn load_interface(&self, ucif: UcifInterface) -> UtenResult<usize> {
        let mut inner = self.inner.write().map_err(|e| UtenError::Cib(format!("Lock: {e}")))?;

        if inner.interfaces.contains_key(&ucif.name) {
            return Err(UtenError::Cib(format!("Interface '{}' already loaded", ucif.name)));
        }

        // Load libraries
        for lib in &ucif.libraries {
            let _ = self.load_library(lib);
        }

        // Compute struct layouts
        for s in &ucif.structs {
            let fields: Vec<_> = s.fields.iter()
                .map(|f| (f.name.clone(), f.ctype.clone()))
                .collect();
            let layout = CStructLayout::compute(&s.name, &fields);
            inner.structs.insert(s.name.clone(), layout);
        }

        // Resolve constants
        for c in &ucif.constants {
            let val = match c {
                ConstDef::Int(v) => UValue::Int64(*v),
                ConstDef::UInt(v) => UValue::Int64(*v as i64),
                ConstDef::Float(v) => UValue::Float64(*v),
                ConstDef::Str(s) => UValue::Int64(s.len() as i64),
                ConstDef::Ptr(name) => {
                    let addr = self.find_symbol(name).unwrap_or(0);
                    UValue::Int64(addr as i64)
                }
            };
            let name = match c {
                ConstDef::Int(_) => format!("int"),
                ConstDef::UInt(_) => format!("uint"),
                ConstDef::Float(_) => format!("float"),
                ConstDef::Str(s) => s.clone(),
                ConstDef::Ptr(s) => s.clone(),
            };
            inner.constants.insert(name, val);
        }

        // Resolve function symbols and pre-compute libffi CIFs
        let iface_idx = inner.interface_list.len();
        for (fi, f) in ucif.functions.iter().enumerate() {
            if let Some(addr) = self.find_symbol(&f.name) {
                inner.resolved_fns.insert((iface_idx, fi), addr);
            }

            // Pre-compute CIF for this function (cached for call_typed)
            let arg_ffi_types: Vec<*mut libffi_sys::ffi_type> = f.params.iter()
                .map(|p| crate::cib::ffi::ffi_type_for(&p.ctype))
                .collect();
            let ret_ffi_type = crate::cib::ffi::ffi_type_for(&f.ret);
            if let Ok(cif) = crate::cib::ffi::prepare_cif(
                crate::cib::ffi::FfiAbi::DefaultAbi,
                ret_ffi_type,
                &arg_ffi_types,
            ) {
                inner.cached_cifs.insert((iface_idx, fi), cif);
            }
        }

        let name = ucif.name.clone();
        inner.interface_list.push(ucif.clone());
        inner.interfaces.insert(ucif.name, iface_idx);
        log::info!("CIB: loaded interface '{name}' (idx={iface_idx})");
        Ok(iface_idx)
    }

    // ── Typed FFI calls ──

    /// Call a C function by interface and function index.
    pub fn call_typed(
        &self,
        interface_idx: usize,
        func_idx: usize,
        args: &[UValue],
    ) -> UtenResult<UValue> {
        let inner = self.inner.read().map_err(|e| UtenError::Cib(format!("Lock: {e}")))?;

        let ucif = inner.interface_list.get(interface_idx)
            .ok_or_else(|| UtenError::Cib(format!("Interface {interface_idx} not found")))?.clone();

        let proto = ucif.functions.get(func_idx)
            .ok_or_else(|| UtenError::Cib(format!("Function {func_idx} not found in '{}'", ucif.name)))?;

        // Validate arg count
        if !proto.variadic && args.len() != proto.params.len() {
            return Err(UtenError::Cib(format!(
                "Arg count mismatch for '{}': expected {}, got {}",
                proto.name, proto.params.len(), args.len()
            )));
        }

        let fn_ptr = *inner.resolved_fns.get(&(interface_idx, func_idx))
            .ok_or_else(|| UtenError::Cib(format!(
                "Symbol '{}' not resolved", proto.name
            )))?;

        // Use pre-computed CIF from cache (set up in load_interface)
        let cif = inner.cached_cifs.get(&(interface_idx, func_idx))
            .ok_or_else(|| UtenError::Cib(format!(
                "CIF for function {} not cached", proto.name
            )))?;

        // Marshal arguments
        let mut raw_args: Vec<MarshalledValue> = Vec::new();
        let mut arg_ptrs: Vec<*mut std::ffi::c_void> = Vec::new();

        for (i, arg) in args.iter().enumerate() {
            let ctype = if i < proto.params.len() {
                &proto.params[i].ctype
            } else {
                &CType::VarArg
            };
            let mv = marshal::marshal(arg, ctype)
                .map_err(|e| UtenError::Cib(e))?;
            raw_args.push(mv);
        }

        // Allocate stack slots for each argument and write marshalled values
        let mut arg_buffers: Vec<Vec<u8>> = Vec::new();
        for (i, mv) in raw_args.iter().enumerate() {
            let ctype = if i < proto.params.len() {
                &proto.params[i].ctype
            } else {
                &CType::VarArg
            };
            let size = ctype.size().max(8); // ensure minimum stack slot
            let mut buf = vec![0u8; size];
            mv.write_to(&mut buf[..ctype.size()]);
            arg_ptrs.push(buf.as_mut_ptr() as *mut std::ffi::c_void);
            arg_buffers.push(buf);
        }

        // Allocate return buffer
        let ret_size = proto.ret.size().max(8);
        let mut ret_buf = vec![0u8; ret_size];

        // ACTUAL FFI CALL (uses cached CIF — no prepare_cif overhead)
        #[cfg(not(feature = "no-cib"))]
        unsafe {
            ffi::call(cif, fn_ptr, ret_buf.as_mut_ptr() as *mut std::ffi::c_void, &mut arg_ptrs);
        }
        #[cfg(feature = "no-cib")]
        { /* CIB disabled — skip FFI call */ }

        // Unmarshal return value
        marshal::unmarshal(&ret_buf[..proto.ret.size()], &proto.ret)
            .map_err(|e| UtenError::Cib(e))
    }

    // ── Legacy C call (for NativeFnHandle-based calls from VM) ──

    /// Map a ValueTag to its default C type for FFI marshalling.
    fn value_tag_to_ctype(tag: &ValueTag) -> CType {
        match tag {
            ValueTag::Nil => CType::Void,
            ValueTag::Bool => CType::Bool,
            ValueTag::Int32 => CType::Int,
            ValueTag::Int64 => CType::LongLong,
            ValueTag::Float32 => CType::Float,
            ValueTag::Float64 => CType::Double,
            ValueTag::String | ValueTag::HeapString | ValueTag::Bytes | ValueTag::ByteArray => CType::Pointer(Box::new(CType::Char)),
            ValueTag::BigInt => CType::LongLong,
            // For opaque GC objects, pass as raw pointer-sized integer
            ValueTag::NativeFn => CType::Pointer(Box::new(CType::Void)),
            // Struct tags and other GC objects → void pointer
            _ => CType::Pointer(Box::new(CType::Void)),
        }
    }

    pub fn call(&self, func: &NativeFnHandle, args: &[UValue]) -> UtenResult<UValue> {
        log::debug!("CIB legacy call: {} @ 0x{:x}", func.name, func.ptr);

        // Map function signature ValueTags to CTypes
        let param_ctypes: Vec<CType> = func.signature.param_types.iter()
            .map(|t| Self::value_tag_to_ctype(t))
            .collect();
        let ret_ctype = Self::value_tag_to_ctype(&func.signature.return_type);

        // Marshal arguments
        let mut raw_args: Vec<MarshalledValue> = Vec::new();
        let mut arg_ptrs: Vec<*mut std::ffi::c_void> = Vec::new();

        for (i, arg) in args.iter().enumerate() {
            let ctype = if i < param_ctypes.len() {
                &param_ctypes[i]
            } else {
                &CType::VarArg
            };
            let mv = marshal::marshal(arg, ctype)
                .map_err(|e| UtenError::Cib(format!("marshal arg {i}: {e}")))?;
            raw_args.push(mv);
        }

        // Allocate stack slots and write marshalled values
        let mut arg_buffers: Vec<Vec<u8>> = Vec::new();
        for (i, mv) in raw_args.iter().enumerate() {
            let ctype = if i < param_ctypes.len() {
                &param_ctypes[i]
            } else {
                &CType::VarArg
            };
            let size = ctype.size().max(8);
            let mut buf = vec![0u8; size];
            mv.write_to(&mut buf[..ctype.size()]);
            arg_ptrs.push(buf.as_mut_ptr() as *mut std::ffi::c_void);
            arg_buffers.push(buf);
        }

        // Prepare FFI types for CIF
        let ret_ffi_type = crate::cib::ffi::ffi_type_for(&ret_ctype);
        let arg_ffi_types: Vec<*mut libffi_sys::ffi_type> = param_ctypes.iter()
            .map(|ct| crate::cib::ffi::ffi_type_for(ct))
            .collect();

        let cif = crate::cib::ffi::prepare_cif(
            crate::cib::ffi::FfiAbi::DefaultAbi,
            ret_ffi_type,
            &arg_ffi_types,
        ).map_err(|e| UtenError::Cib(format!("prepare_cif: {e}")))?;

        // Allocate return buffer
        let ret_size = ret_ctype.size().max(8);
        let mut ret_buf = vec![0u8; ret_size];

        // ACTUAL FFI CALL
        #[cfg(not(feature = "no-cib"))]
        unsafe {
            ffi::call(&cif, func.ptr, ret_buf.as_mut_ptr() as *mut std::ffi::c_void, &mut arg_ptrs);
        }
        #[cfg(feature = "no-cib")]
        { /* CIB disabled */ }

        // Unmarshal return value
        marshal::unmarshal(&ret_buf[..ret_ctype.size()], &ret_ctype)
            .map_err(|e| UtenError::Cib(format!("unmarshal return: {e}")))
    }

    // ── Struct operations ──

    pub fn get_struct(&self, name: &str) -> Option<CStructLayout> {
        self.inner.read().ok()?.structs.get(name).cloned()
    }

    pub fn pack_struct(&self, struct_name: &str, fields: &[(String, UValue)]) -> UtenResult<Vec<u8>> {
        let inner = self.inner.read().map_err(|e| UtenError::Cib(format!("Lock: {e}")))?;
        let layout = inner.structs.get(struct_name)
            .ok_or_else(|| UtenError::Cib(format!("Unknown struct '{struct_name}'")))?;
        layout.pack(fields).map_err(|e| UtenError::Cib(e))
    }

    pub fn unpack_struct(&self, struct_name: &str, data: &[u8]) -> UtenResult<Vec<(String, UValue)>> {
        let inner = self.inner.read().map_err(|e| UtenError::Cib(format!("Lock: {e}")))?;
        let layout = inner.structs.get(struct_name)
            .ok_or_else(|| UtenError::Cib(format!("Unknown struct '{struct_name}'")))?;
        layout.unpack(data).map_err(|e| UtenError::Cib(e))
    }

    // ── Constants ──

    pub fn get_constant(&self, name: &str) -> Option<UValue> {
        self.inner.read().ok()?.constants.get(name).cloned()
    }

    pub fn list_interfaces(&self) -> Vec<String> {
        self.inner.read()
            .map(|i| i.interfaces.keys().cloned().collect())
            .unwrap_or_default()
    }

    /// Register a module's UCIF interfaces.
    /// Compilers can embed .ucif in module metadata.
    pub fn register_interfaces_from_module(
        &self,
        _metadata: &HashMap<String, String>,
        _module_bytes: Option<&[u8]>,
    ) {
        // In production, read "ucif.interface" metadata key
        // and load the interface.
    }
}

// ── errno helper ──

fn get_errno_location(_lib_handle: *mut std::os::raw::c_void) -> usize {
    #[cfg(target_os = "linux")]
    unsafe { libc::__errno_location() as usize }

    #[cfg(target_os = "macos")]
    unsafe { libc::__error() as usize }

    #[cfg(target_os = "windows")]
    {
        // On Windows, errno is thread-local; return a pointer to it via MSVC runtime.
        // _errno() in MSVC returns a mutable pointer to the thread-local errno.
        extern "C" { fn _errno() -> *mut i32; }
        unsafe { _errno() as usize }
    }

    #[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
    { 0 }
}
