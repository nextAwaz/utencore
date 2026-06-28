//! utencore.Unsafe — raw memory, FFI, and low-level operations.
//!
//! Only classes inheriting from `Unsafe` can access these.
//! Capabilities:
//! - Raw memory allocation and access (malloc/free/read/write)
//! - CIB/FFI (dlopen, dlsym)
//! - Type introspection and casting
//! - Raw object construction

use std::collections::HashMap;
use std::sync::Arc;
use crate::error::{UtenError, UtenResult};
use utencore_types::*;
use super::{Vm, VmNativeFn};

pub fn register_all(vm: &mut Vm) -> Vec<(&'static str, NativeFuncIdx, u16)> {
    fn n<F>(f: F) -> VmNativeFn
    where F: Fn(&mut Vm, &[UValue]) -> UtenResult<UValue> + Send + Sync + 'static
    {
        VmNativeFn(Arc::new(f))
    }

    let mut funcs: Vec<(&'static str, VmNativeFn, u16)> = vec![];

    funcs.push(("alloc",      n(unsafe_alloc), 1));
    funcs.push(("free",       n(unsafe_free), 1));
    funcs.push(("read_byte",  n(unsafe_read_byte), 1));
    funcs.push(("write_byte", n(unsafe_write_byte), 2));
    funcs.push(("read_i32",   n(unsafe_read_i32), 1));
    funcs.push(("write_i32",  n(unsafe_write_i32), 2));
    funcs.push(("memcpy",     n(unsafe_memcpy), 3));
    funcs.push(("memset",     n(unsafe_memset), 3));
    funcs.push(("dlopen",     n(unsafe_dlopen), 1));
    funcs.push(("dlsym",      n(unsafe_dlsym), 2));
    funcs.push(("type_of",    n(unsafe_type_of), 1));
    funcs.push(("cast",       n(unsafe_cast), 2));
    funcs.push(("alloc_obj",  n(unsafe_alloc_obj), 1));

    let mut result = Vec::new();
    for (name, func, n_params) in funcs {
        let idx = vm.register_native_func(func);
        result.push((name, idx, n_params));
    }
    result
}

// ═══════════════════════════════════════════════════════
// Memory
// ═══════════════════════════════════════════════════════

fn unsafe_alloc(_vm: &mut Vm, args: &[UValue]) -> UtenResult<UValue> {
    let size = get_int(args, 0)?;
    if size <= 0 {
        return Err(UtenError::Vm("Unsafe.alloc: size must be positive".into()));
    }
    let layout = std::alloc::Layout::from_size_align(size as usize, 1)
        .map_err(|e| UtenError::Vm(format!("Unsafe.alloc: {e}")))?;
    let ptr = unsafe { std::alloc::alloc(layout) };
    if ptr.is_null() {
        return Err(UtenError::Vm("Unsafe.alloc: out of memory".into()));
    }
    Ok(UValue::Int64(ptr as i64))
}

fn unsafe_free(_vm: &mut Vm, args: &[UValue]) -> UtenResult<UValue> {
    let addr = get_int(args, 0)?;
    if addr == 0 {
        return Err(UtenError::Vm("Unsafe.free: null pointer".into()));
    }
    unsafe {
        std::alloc::dealloc(
            addr as *mut u8,
            std::alloc::Layout::from_size_align_unchecked(1, 1),
        );
    }
    Ok(UValue::Nil)
}

fn unsafe_read_byte(_vm: &mut Vm, args: &[UValue]) -> UtenResult<UValue> {
    let ptr = get_int(args, 0)? as *const u8;
    if ptr.is_null() { return Err(UtenError::Vm("Unsafe.read_byte: null pointer".into())); }
    unsafe { Ok(UValue::Int32(*ptr as i32)) }
}

fn unsafe_write_byte(_vm: &mut Vm, args: &[UValue]) -> UtenResult<UValue> {
    let ptr = get_int(args, 0)? as *mut u8;
    let val = get_int(args, 1)? as u8;
    if ptr.is_null() { return Err(UtenError::Vm("Unsafe.write_byte: null pointer".into())); }
    unsafe { *ptr = val; }
    Ok(UValue::Nil)
}

fn unsafe_read_i32(_vm: &mut Vm, args: &[UValue]) -> UtenResult<UValue> {
    let ptr = get_int(args, 0)? as *const i32;
    if ptr.is_null() { return Err(UtenError::Vm("Unsafe.read_i32: null pointer".into())); }
    unsafe { Ok(UValue::Int32(*ptr)) }
}

fn unsafe_write_i32(_vm: &mut Vm, args: &[UValue]) -> UtenResult<UValue> {
    let ptr = get_int(args, 0)? as *mut i32;
    let val = get_int(args, 1)? as i32;
    if ptr.is_null() { return Err(UtenError::Vm("Unsafe.write_i32: null pointer".into())); }
    unsafe { *ptr = val; }
    Ok(UValue::Nil)
}

fn unsafe_memcpy(_vm: &mut Vm, args: &[UValue]) -> UtenResult<UValue> {
    let dst = get_int(args, 0)? as *mut u8;
    let src = get_int(args, 1)? as *const u8;
    let n = get_int(args, 2)? as usize;
    if dst.is_null() || src.is_null() {
        return Err(UtenError::Vm("Unsafe.memcpy: null pointer".into()));
    }
    if n == 0 { return Ok(UValue::Nil); }
    unsafe { std::ptr::copy_nonoverlapping(src, dst, n); }
    Ok(UValue::Nil)
}

fn unsafe_memset(_vm: &mut Vm, args: &[UValue]) -> UtenResult<UValue> {
    let ptr = get_int(args, 0)? as *mut u8;
    let val = get_int(args, 1)? as u8;
    let n = get_int(args, 2)? as usize;
    if ptr.is_null() { return Err(UtenError::Vm("Unsafe.memset: null pointer".into())); }
    if n == 0 { return Ok(UValue::Nil); }
    unsafe { std::ptr::write_bytes(ptr, val, n); }
    Ok(UValue::Nil)
}

// ═══════════════════════════════════════════════════════
// CIB / FFI (using libloading)
// ═══════════════════════════════════════════════════════

fn unsafe_dlopen(_vm: &mut Vm, args: &[UValue]) -> UtenResult<UValue> {
    let path = get_str(args, 0)?;
    unsafe {
        match libloading::Library::new(&path) {
            Ok(lib) => {
                let ptr = Box::into_raw(Box::new(lib)) as i64;
                Ok(UValue::Int64(ptr))
            }
            Err(e) => Err(UtenError::Vm(format!("Unsafe.dlopen('{path}'): {e}"))),
        }
    }
}

fn unsafe_dlsym(_vm: &mut Vm, args: &[UValue]) -> UtenResult<UValue> {
    let lib_ptr = get_int(args, 0)? as *const libloading::Library;
    let name = get_str(args, 1)?;
    unsafe {
        match (*lib_ptr).get::<*mut std::os::raw::c_void>(name.as_bytes()) {
            Ok(sym) => Ok(UValue::Int64(*sym as i64)),
            Err(e) => Err(UtenError::Vm(format!("Unsafe.dlsym: {e}"))),
        }
    }
}

// ═══════════════════════════════════════════════════════
// Type system
// ═══════════════════════════════════════════════════════

fn unsafe_type_of(_vm: &mut Vm, args: &[UValue]) -> UtenResult<UValue> {
    let val = args.first().ok_or(UtenError::Vm("Unsafe.type_of: no args".into()))?;
    Ok(UValue::Int32(val.tag() as i32))
}

fn unsafe_cast(vm: &mut Vm, args: &[UValue]) -> UtenResult<UValue> {
    let val = args.first().ok_or(UtenError::Vm("Unsafe.cast: no args".into()))?;
    let target_tag = get_int(args, 1)? as u8;
    match (val.tag() as u8, target_tag) {
        (a, b) if a == b => Ok(val.clone()),
        (2..=5, 2..=5) => {
            let i = get_int(args, 0)?;
            let f = get_float(args, 0).unwrap_or(i as f64);
            match target_tag {
                2 => Ok(UValue::Int32(i as i32)),
                3 => Ok(UValue::Int64(i)),
                4 => Ok(UValue::Float32(f as f32)),
                5 => Ok(UValue::Float64(f)),
                _ => unreachable!(),
            }
        }
        (1, 2) => Ok(UValue::Int32(if matches!(val, UValue::Bool(true)) { 1 } else { 0 })),
        (1, 3) => Ok(UValue::Int64(if matches!(val, UValue::Bool(true)) { 1 } else { 0 })),
        _ => Err(UtenError::Vm(format!(
            "Unsafe.cast: cannot cast {:?} to tag {target_tag}", val.tag()))),
    }
}

fn unsafe_alloc_obj(vm: &mut Vm, args: &[UValue]) -> UtenResult<UValue> {
    let tag_val = get_int(args, 0)? as u8;
    let (tag, obj) = match tag_val {
        10 => (ValueTag::Array, HeapObject::Array(vec![])),
        11 => (ValueTag::Map, HeapObject::Map(HashMap::new())),
        12 => (ValueTag::Closure, HeapObject::Closure { func: 0, captures: vec![], module_id: 0 }),
        13 => (ValueTag::Struct, HeapObject::Struct(vec![])),
        20 => (ValueTag::Namespace, HeapObject::Namespace { name: 0, members: vec![], module_id: 0 }),
        21 => (ValueTag::Class, HeapObject::Class { name: 0, methods: vec![], fields: vec![], parent: None, constructor: None }),
        22 => (ValueTag::Object, HeapObject::Object { class_handle: 0, fields: vec![], proto: None }),
        30 => (ValueTag::Pair, HeapObject::Pair { car: Box::new(UValue::Nil), cdr: Box::new(UValue::Nil) }),
        31 => (ValueTag::Tuple, HeapObject::Tuple(vec![])),
        35 => (ValueTag::Set, HeapObject::Set(std::collections::HashSet::new())),
        7  => (ValueTag::HeapString, HeapObject::HeapString(String::new())),
        37 => (ValueTag::Bytes, HeapObject::Bytes(vec![])),
        38 => (ValueTag::ByteArray, HeapObject::ByteArray(vec![])),
        _  => (ValueTag::Struct, HeapObject::Struct(vec![])),
    };
    let h = vm.gc.alloc(obj);
    Ok(UValue::Gc(h, tag))
}

// ═══════════════════════════════════════════════════════
// Helpers
// ═══════════════════════════════════════════════════════

fn get_int(args: &[UValue], idx: usize) -> UtenResult<i64> {
    let v = args.get(idx).ok_or(UtenError::Vm(format!("arg {idx} out of bounds")))?;
    match v {
        UValue::Int32(i) => Ok(*i as i64),
        UValue::Int64(i) => Ok(*i),
        _ => Err(UtenError::TypeError { expected: "numeric".into(), actual: format!("{:?}", v.tag()) }),
    }
}

fn get_float(args: &[UValue], idx: usize) -> Option<f64> {
    args.get(idx).and_then(|v| match v {
        UValue::Float32(f) => Some(*f as f64),
        UValue::Float64(f) => Some(*f),
        _ => None,
    })
}

fn get_str(args: &[UValue], idx: usize) -> UtenResult<String> {
    let v = args.get(idx).ok_or(UtenError::Vm(format!("arg {idx} out of bounds")))?;
    match v {
        UValue::String(sid) => Ok(format!("<str#{sid}>")),
        UValue::Int32(i) => Ok(format!("{i}")),
        UValue::Int64(i) => Ok(format!("{i}")),
        UValue::Bool(b) => Ok(format!("{b}")),
        UValue::Nil => Ok("None".into()),
        _ => Err(UtenError::TypeError { expected: "string".into(), actual: format!("{:?}", v.tag()) }),
    }
}
