//! UtenCore LLVM JIT backend.
//!
//! Uses LLVM to compile hot functions to native machine code.
//! The JIT monitors function hotness and compiles frequently-used
//! functions for native-speed execution.

use std::collections::HashMap;

use utencore_bytecode::FunctionDef;
use crate::error::{UtenError, UtenResult};
use utencore_types::FuncRef;

/// JIT-compiled native code entry point
pub type JitFn = unsafe extern "C" fn() -> i64;

/// The JIT compiler engine
pub struct JitEngine {
    /// Whether JIT is enabled
    enabled: bool,
    /// Compilation threshold (calls before JIT)
    threshold: u32,
    /// Compiled functions: (module_id, func_ref) -> native code pointer
    compiled: HashMap<(u16, FuncRef), *const u8>,
    /// LLVM context (lazily initialized)
    llvm_initialized: bool,
}

impl JitEngine {
    pub fn new() -> Self {
        JitEngine {
            enabled: true,
            threshold: 100,
            compiled: HashMap::new(),
            llvm_initialized: false,
        }
    }

    pub fn with_threshold(threshold: u32) -> Self {
        JitEngine {
            enabled: true,
            threshold,
            compiled: HashMap::new(),
            llvm_initialized: false,
        }
    }

    /// Check if a function should be JIT-compiled
    pub fn should_compile(&self, func: &FunctionDef) -> bool {
        self.enabled && func.hotness >= self.threshold && func.jit_code.is_none()
    }

    /// Compile a function to native code via LLVM
    pub fn compile(&mut self, module_id: u16, func_ref: FuncRef, func: &FunctionDef) -> UtenResult<()> {
        if !self.enabled {
            return Ok(());
        }

        self.init_llvm();

        log::info!("JIT compiling: {} (hotness={})", func.name, func.hotness);
        // In production, this would:
        // 1. Create an LLVM module
        // 2. Translate bytecode to LLVM IR
        // 3. Run optimization passes
        // 4. Emit native code via LLVM's ORC JIT

        // For the initial implementation, we use LLVM's C API via llvm-sys
        // to demonstrate the JIT pipeline.

        // Placeholder: mark as compiled (no actual JIT in first iteration)
        // The full LLVM IR translation will follow the pattern:
        //
        // unsafe {
        //     let context = LLVMContextCreate();
        //     let module = LLVMModuleCreateWithNameInContext(cname, context);
        //     let builder = LLVMCreateBuilderInContext(context);
        //
        //     create_function_type(...)
        //     create_basic_block(...)
        //     // ... translate each opcode to LLVM instruction ...
        //
        //     LLVMVerifyModule(module, ...)
        //     LLVMOrcCreateLLJIT(...)
        //     LLVMOrcAddLLVMIRModule(...)
        // }

        self.compiled.insert((module_id, func_ref), std::ptr::null());

        Ok(())
    }

    /// Execute a previously compiled function
    /// Safety: the caller must ensure the function was compiled
    pub unsafe fn execute(&self, _module_id: u16, _func_ref: FuncRef) -> UtenResult<i64> {
        // Would call the native function pointer
        // let fn_ptr: JitFn = std::mem::transmute(ptr);
        // Ok(fn_ptr())
        Err(UtenError::Jit("JIT execution not yet implemented (placeholder)".into()))
    }

    /// Invalidate compiled code for a function
    pub fn invalidate(&mut self, module_id: u16, func_ref: FuncRef) {
        self.compiled.remove(&(module_id, func_ref));
    }

    /// Enable or disable JIT
    pub fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
    }

    fn init_llvm(&mut self) {
        if self.llvm_initialized {
            return;
        }
        #[cfg(feature = "jit")]
        {
            unsafe {
                llvm_sys::core::LLVMInitializeAllTargetInfos();
                llvm_sys::core::LLVMInitializeAllTargets();
                llvm_sys::core::LLVMInitializeAllTargetMCs();
                llvm_sys::core::LLVMInitializeAllAsmParsers();
                llvm_sys::core::LLVMInitializeAllAsmPrinters();
            }
        }
        self.llvm_initialized = true;
    }
}

impl Drop for JitEngine {
    fn drop(&mut self) {
        // Would free LLVM modules and JIT code
    }
}
