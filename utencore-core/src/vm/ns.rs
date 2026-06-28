//! utencore.Ns — namespace management.
//!
//! Provides namespace aliasing and resolution.
//! This is a VM-level namespace management feature, not an "unsafe" capability.

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

    funcs.push(("alias",    n(ns_alias), 2));
    funcs.push(("resolve",  n(ns_resolve), 1));

    let mut result = Vec::new();
    for (name, func, n_params) in funcs {
        let idx = vm.register_native_func(func);
        result.push((name, idx, n_params));
    }
    result
}

/// Create or update a namespace alias.
/// `Ns.alias("short.name", "very.long.namespace.path")`
fn ns_alias(vm: &mut Vm, args: &[UValue]) -> UtenResult<UValue> {
    let alias = get_str(args, 0)?;
    let target = get_str(args, 1)?;
    vm.ns_aliases.insert(alias, target);
    Ok(UValue::Nil)
}

/// Resolve a namespace name through aliases.
fn ns_resolve(vm: &mut Vm, args: &[UValue]) -> UtenResult<UValue> {
    let name = get_str(args, 0)?;
    let resolved = vm.resolve_ns_alias(&name);
    let mid = vm.current_module_id();
    let sid = vm.modules[mid].module.intern(&resolved);
    Ok(UValue::String(sid))
}

// ═══════════════════════════════════════════════════════
// Helpers (mirrored from unsafe_.rs)
// ═══════════════════════════════════════════════════════

fn get_int(args: &[UValue], idx: usize) -> UtenResult<i64> {
    let v = args.get(idx).ok_or(UtenError::Vm(format!("arg {idx} out of bounds")))?;
    match v {
        UValue::Int32(i) => Ok(*i as i64),
        UValue::Int64(i) => Ok(*i),
        _ => Err(UtenError::TypeError { expected: "numeric".into(), actual: format!("{:?}", v.tag()) }),
    }
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
