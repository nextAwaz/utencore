//! utencore.Gc — garbage collection control.
//!
//! Provides fine-grained GC control: collect (force), suggest, pin/unpin.
//! These are exposed as native functions in the `utencore.Gc` namespace.

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

    funcs.push(("collect",  n(gc_collect), 0));   // force full GC
    funcs.push(("suggest",  n(gc_suggest), 0));   // suggest GC (may or may not run)
    funcs.push(("pin",      n(gc_pin), 1));        // pin object from GC
    funcs.push(("unpin",    n(gc_unpin), 1));      // unpin object

    let mut result = Vec::new();
    for (name, func, n_params) in funcs {
        let idx = vm.register_native_func(func);
        result.push((name, idx, n_params));
    }
    result
}

/// Force a full garbage collection cycle.
fn gc_collect(vm: &mut Vm, _args: &[UValue]) -> UtenResult<UValue> {
    let gc_ptr: *mut Box<dyn crate::memory::GcEngine> = &mut vm.gc;
    unsafe { (*gc_ptr).collect(vm); }
    Ok(UValue::Nil)
}

/// Suggest that the GC might want to run.
/// The GC may or may not collect depending on internal heuristics.
fn gc_suggest(_vm: &mut Vm, _args: &[UValue]) -> UtenResult<UValue> {
    // TODO: implement heuristic-based GC trigger
    // For now, this is a no-op hint
    Ok(UValue::Nil)
}

/// Pin a GC handle to prevent it from being collected.
fn gc_pin(vm: &mut Vm, args: &[UValue]) -> UtenResult<UValue> {
    let h = get_int(args, 0)? as u32;
    vm.gc.pin(h);
    Ok(UValue::Nil)
}

/// Unpin a previously pinned GC handle.
fn gc_unpin(vm: &mut Vm, args: &[UValue]) -> UtenResult<UValue> {
    let h = get_int(args, 0)? as u32;
    vm.gc.unpin(h);
    Ok(UValue::Nil)
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
