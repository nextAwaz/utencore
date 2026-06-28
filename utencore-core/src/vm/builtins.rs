//! utencore.* — low-level native built-ins.
//!
//! Provides the runtime primitives that higher-level libraries (utenstd, py2uc)
//! call into via the Import mechanism or Unsafe access.
//!
//! Architecture:
//!   utenstd (bytecode, UCSL) ──import──► utencore (native Rust)
//!                                           ├── Unsafe  (raw memory, FFI)
//!                                           ├── Gc      (GC control)
//!                                           ├── Ns      (namespace alias)
//!                                           ├── Math    (sqrt, sin, cos…)
//!                                           ├── Io      (read_file, write_file…)
//!                                           └── Sys     (clock, sleep, args…)
//!
//! utenstd is NOT implemented here — it lives as UCSL .uclib bytecode modules.
//! This file only provides the native utencore.* primitives.

use std::sync::Arc;
use std::io::{self, Write, BufRead};
use crate::error::{UtenError, UtenResult};
use utencore_types::*;
use super::{Vm, VmNativeFn};

// ═══════════════════════════════════════════════════════════════
// Registration helpers
// ═══════════════════════════════════════════════════════════════

fn n<F>(f: F) -> VmNativeFn
where F: Fn(&mut Vm, &[UValue]) -> UtenResult<UValue> + Send + Sync + 'static
{
    VmNativeFn(Arc::new(f))
}

/// Collect all built-in native functions.
/// Returns Vec of (namespace, name, func, n_params).
pub fn register_all(vm: &mut Vm) -> Vec<(String, String, NativeFuncIdx, u16)> {
    let mut funcs: Vec<(String, String, VmNativeFn, u16)> = vec![];

    // ── utencore.* — top-level functions ──
    // print (without newline), println (with newline)
    funcs.push(("utencore".into(), "print".into(),    n(builtin_print), 1));
    funcs.push(("utencore".into(), "println".into(),  n(builtin_println), 1));
    funcs.push(("utencore".into(), "input".into(),    n(builtin_input), 0));
    funcs.push(("utencore".into(), "exit".into(),     n(builtin_exit), 1));
    funcs.push(("utencore".into(), "assert".into(),   n(builtin_assert), 2));

    // ── utencore.Math.* ──
    funcs.push(("utencore.Math".into(), "sqrt".into(),  n(builtin_sqrt), 1));
    funcs.push(("utencore.Math".into(), "sin".into(),   n(builtin_sin), 1));
    funcs.push(("utencore.Math".into(), "cos".into(),   n(builtin_cos), 1));
    funcs.push(("utencore.Math".into(), "tan".into(),   n(builtin_tan), 1));
    funcs.push(("utencore.Math".into(), "floor".into(), n(builtin_floor), 1));
    funcs.push(("utencore.Math".into(), "ceil".into(),  n(builtin_ceil), 1));
    funcs.push(("utencore.Math".into(), "round".into(), n(builtin_round), 1));
    funcs.push(("utencore.Math".into(), "abs".into(),   n(builtin_abs), 1));
    funcs.push(("utencore.Math".into(), "pow".into(),   n(builtin_pow), 2));
    funcs.push(("utencore.Math".into(), "pi".into(),    n(builtin_pi), 0));
    funcs.push(("utencore.Math".into(), "e".into(),     n(builtin_e), 0));

    // ── utencore.Io.* ──
    funcs.push(("utencore.Io".into(), "read_file".into(),  n(builtin_read_file), 1));
    funcs.push(("utencore.Io".into(), "write_file".into(), n(builtin_write_file), 2));
    funcs.push(("utencore.Io".into(), "read_line".into(),  n(builtin_read_line), 0));

    // ── utencore.Sys.* ──
    funcs.push(("utencore.Sys".into(), "clock_ms".into(), n(builtin_clock_ms), 0));
    funcs.push(("utencore.Sys".into(), "sleep".into(),    n(builtin_sleep), 1));

    // Register each function and collect results
    funcs.into_iter().map(|(ns, name, func, n_params)| {
        let idx = vm.register_native_func_named(&format!("{ns}.{name}"), func);
        (ns, name, idx, n_params)
    }).collect()
}

// ═══════════════════════════════════════════════════════════════
// utencore.* implementations
// ═══════════════════════════════════════════════════════════════

fn builtin_print(vm: &mut Vm, args: &[UValue]) -> UtenResult<UValue> {
    let s = args.first().map(|v| vm.value_to_string(v)).unwrap_or_default();
    print!("{s}");
    Ok(UValue::Nil)
}

fn builtin_println(vm: &mut Vm, args: &[UValue]) -> UtenResult<UValue> {
    let s = args.first().map(|v| vm.value_to_string(v)).unwrap_or_default();
    println!("{s}");
    Ok(UValue::Nil)
}

fn builtin_input(vm: &mut Vm, _args: &[UValue]) -> UtenResult<UValue> {
    let mut line = String::new();
    match io::stdin().read_line(&mut line) {
        Ok(_) => {
            let trimmed = line.trim_end_matches('\n').trim_end_matches('\r').to_string();
            Ok(vm.alloc_heapstring(trimmed))
        }
        Err(_) => Ok(UValue::Nil),
    }
}

fn builtin_exit(_vm: &mut Vm, args: &[UValue]) -> UtenResult<UValue> {
    let code = args.first().and_then(|v| match v { UValue::Int32(c) => Some(*c), _ => None }).unwrap_or(0);
    std::process::exit(code);
}

fn builtin_assert(_vm: &mut Vm, args: &[UValue]) -> UtenResult<UValue> {
    let cond = args.first().map(|v| v.truthy()).unwrap_or(false);
    if !cond {
        let msg = args.get(1).map(|v| format!("{:?}", v)).unwrap_or_else(|| "assertion failed".into());
        return Err(UtenError::Vm(msg));
    }
    Ok(UValue::Nil)
}

// ═══════════════════════════════════════════════════════════════
// utencore.math.* implementations
// ═══════════════════════════════════════════════════════════════

fn as_f64(vm: &Vm, args: &[UValue], idx: usize) -> Option<f64> {
    let v = args.get(idx)?;
    match v {
        UValue::Float64(f) => Some(*f),
        UValue::Float32(f) => Some(*f as f64),
        UValue::Int64(i) => Some(*i as f64),
        UValue::Int32(i) => Some(*i as f64),
        _ => None,
    }
}

fn builtin_sqrt(_vm: &mut Vm, args: &[UValue]) -> UtenResult<UValue> {
    let v = as_f64(_vm, args, 0).ok_or(UtenError::TypeError { expected: "numeric", actual: format!("{:?}", args.first().map(|a| a.tag()).unwrap_or(ValueTag::Nil)) })?;
    Ok(UValue::Float64(v.sqrt()))
}

fn builtin_sin(_vm: &mut Vm, args: &[UValue]) -> UtenResult<UValue> {
    let v = as_f64(_vm, args, 0).ok_or_else(|| UtenError::TypeError { expected: "numeric".into(), actual: "?".into() })?;
    Ok(UValue::Float64(v.sin()))
}

fn builtin_cos(_vm: &mut Vm, args: &[UValue]) -> UtenResult<UValue> {
    let v = as_f64(_vm, args, 0).ok_or_else(|| UtenError::TypeError { expected: "numeric".into(), actual: "?".into() })?;
    Ok(UValue::Float64(v.cos()))
}

fn builtin_tan(_vm: &mut Vm, args: &[UValue]) -> UtenResult<UValue> {
    let v = as_f64(_vm, args, 0).ok_or_else(|| UtenError::TypeError { expected: "numeric".into(), actual: "?".into() })?;
    Ok(UValue::Float64(v.tan()))
}

fn builtin_floor(_vm: &mut Vm, args: &[UValue]) -> UtenResult<UValue> {
    let v = as_f64(_vm, args, 0).ok_or_else(|| UtenError::TypeError { expected: "numeric".into(), actual: "?".into() })?;
    Ok(UValue::Float64(v.floor()))
}

fn builtin_ceil(_vm: &mut Vm, args: &[UValue]) -> UtenResult<UValue> {
    let v = as_f64(_vm, args, 0).ok_or_else(|| UtenError::TypeError { expected: "numeric".into(), actual: "?".into() })?;
    Ok(UValue::Float64(v.ceil()))
}

fn builtin_round(_vm: &mut Vm, args: &[UValue]) -> UtenResult<UValue> {
    let v = as_f64(_vm, args, 0).ok_or_else(|| UtenError::TypeError { expected: "numeric".into(), actual: "?".into() })?;
    Ok(UValue::Float64(v.round()))
}

fn builtin_abs(_vm: &mut Vm, args: &[UValue]) -> UtenResult<UValue> {
    let v = as_f64(_vm, args, 0).ok_or_else(|| UtenError::TypeError { expected: "numeric".into(), actual: "?".into() })?;
    Ok(UValue::Float64(v.abs()))
}

fn builtin_pow(_vm: &mut Vm, args: &[UValue]) -> UtenResult<UValue> {
    let a = as_f64(_vm, args, 0).ok_or_else(|| UtenError::TypeError { expected: "numeric".into(), actual: "?".into() })?;
    let b = as_f64(_vm, args, 1).ok_or_else(|| UtenError::TypeError { expected: "numeric".into(), actual: "?".into() })?;
    Ok(UValue::Float64(a.powf(b)))
}

fn builtin_pi(_vm: &mut Vm, _args: &[UValue]) -> UtenResult<UValue> {
    Ok(UValue::Float64(std::f64::consts::PI))
}

fn builtin_e(_vm: &mut Vm, _args: &[UValue]) -> UtenResult<UValue> {
    Ok(UValue::Float64(std::f64::consts::E))
}

// ═══════════════════════════════════════════════════════════════
// utencore.io.* implementations
// ═══════════════════════════════════════════════════════════════

fn builtin_read_file(_vm: &mut Vm, args: &[UValue]) -> UtenResult<UValue> {
    let path = args.first().map(|v| format!("{:?}", v)).unwrap_or_default();
    match std::fs::read_to_string(&path) {
        Ok(content) => {
            // Store as heap string via vm
            Ok(_vm.alloc_heapstring(content))
        }
        Err(_) => Ok(UValue::Nil),
    }
}

fn builtin_write_file(_vm: &mut Vm, args: &[UValue]) -> UtenResult<UValue> {
    let path = args.first().map(|v| format!("{:?}", v)).unwrap_or_default();
    let content = args.get(1).map(|v| format!("{:?}", v)).unwrap_or_default();
    match std::fs::write(&path, &content) {
        Ok(_) => Ok(UValue::Bool(true)),
        Err(_) => Ok(UValue::Bool(false)),
    }
}

fn builtin_read_line(_vm: &mut Vm, _args: &[UValue]) -> UtenResult<UValue> {
    let mut line = String::new();
    match io::stdin().read_line(&mut line) {
        Ok(0) => Ok(UValue::Nil),
        Ok(_) => {
            let trimmed = line.trim_end_matches('\n').trim_end_matches('\r').to_string();
            Ok(_vm.alloc_heapstring(trimmed))
        }
        Err(_) => Ok(UValue::Nil),
    }
}

// ═══════════════════════════════════════════════════════════════
// utencore.sys.* implementations
// ═══════════════════════════════════════════════════════════════

fn builtin_clock_ms(_vm: &mut Vm, _args: &[UValue]) -> UtenResult<UValue> {
    let duration = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    Ok(UValue::Int64(duration.as_millis() as i64))
}

fn builtin_sleep(_vm: &mut Vm, args: &[UValue]) -> UtenResult<UValue> {
    let ms = args.first().and_then(|v| match v { UValue::Int32(m) => Some(*m as u64), UValue::Int64(m) => Some(*m as u64), _ => None }).unwrap_or(0);
    std::thread::sleep(std::time::Duration::from_millis(ms));
    Ok(UValue::Nil)
}
