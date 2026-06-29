// ── Function calls ──

use crate::error::{UtenError, UtenResult};
use utencore_types::*;
use super::*;

impl Vm {
    pub(crate) fn call_function_with_args(&mut self, mid: usize, func_ref: FuncRef, args: Vec<UValue>) -> UtenResult<()> {
        // Bounds check
        if mid >= self.modules.len() {
            return Err(UtenError::Vm(format!("module {mid} not loaded")));
        }
        if (func_ref as usize) >= self.modules[mid].module.functions.len() {
            return Err(UtenError::Vm(format!("function {func_ref} not found in module {mid}")));
        }
        let (n_params, n_locals) = {
            let m = &self.modules[mid];
            let f = &m.module.functions[func_ref as usize];
            (f.n_params, f.n_locals)
        };
        let mut args = args;
        args.resize(n_params as usize, UValue::Nil);
        let mut locals = vec![UValue::Nil; n_locals as usize];
        for (i, arg) in args.into_iter().enumerate() {
            if i < locals.len() { locals[i] = arg; }
        }
        self.frames.push(CallFrame {
            func_ref, module_id: mid as ModuleId,
            return_pc: self.pc, stack_base: self.stack.len(),
            locals, captures: vec![],
        });
        self.pc = 0;
        Ok(())
    }

    pub(crate) fn call_function(&mut self, func_ref: FuncRef) -> UtenResult<()> {
        let mid = self.current_module_id();
        let n_params = {
            let m = &self.modules[mid];
            let f = &m.module.functions[func_ref as usize];
            f.n_params
        };
        let n_avail = (n_params as usize).min(self.stack.len());
        let mut args = Vec::with_capacity(n_avail);
        for _ in 0..n_avail { args.push(self.pop()?); }
        args.reverse();
        self.call_function_with_args(mid, func_ref, args)
    }

    pub(crate) fn do_return(&mut self, val: Option<UValue>) -> UtenResult<()> {
        let frame = self.frames.pop().ok_or(UtenError::Vm("no frame".into()))?;
        self.stack.truncate(frame.stack_base);
        self.stack.push(val.unwrap_or(UValue::Nil));
        if self.frames.is_empty() { self.running = false; }
        else { self.pc = frame.return_pc; }
        Ok(())
    }
}
