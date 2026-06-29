//! UtenCore Stack Virtual Machine.

use std::collections::HashMap;
use std::sync::Arc;
use crate::bytecode::{BytecodeReader, ExceptionTableEntry, UtenModule};
use crate::cib::CibEngine;
use crate::error::{UtenError, UtenResult};
use utencore_gc::{GcEngine, GcHandle, TraceRoots};
use utencore_bytecode::ModuleLoader;
use crate::opcodes::{Opcode, OpFlags, opcode_info};
use crate::plugin::PluginManager;
use utencore_types::*;
use utencore_types::BYTECODE_VERSION;

/// Embedded UCSL standard library modules (compiled into the VM binary).
/// These are registered at VM startup so `import math` etc. work out of the box
/// without needing separate .uclib files in the filesystem.
const EMBEDDED_UCSL: &[(&str, &[u8])] = &[
    // Embedded stdlib modules are built from src/stdlib/*.uclib
    // These are minimal re-export stubs that map to utencore.* native functions.
    // Real utenstd will be separate.
    ("math", include_bytes!("../../../src/stdlib/math.uclib")),
    ("io",   include_bytes!("../../../src/stdlib/io.uclib")),
    ("sys",  include_bytes!("../../../src/stdlib/sys.uclib")),
];

/// A VM-native function — Rust closure callable from bytecode.
/// Uses a newtype struct so the compiler doesn't confuse the type with bare Arc.
#[derive(Clone)]
pub struct VmNativeFn(pub Arc<dyn Fn(&mut Vm, &[UValue]) -> UtenResult<UValue> + Send + Sync>);

/// Sub-modules (split for maintainability)
/// Files use PascalCase; mod names stay snake_case per Rust convention.
#[path = "Dispatch.rs"]  pub mod dispatch;
#[path = "Call.rs"]      pub mod call;
#[path = "Gc.rs"]        pub mod gc;
#[path = "Namespace.rs"] pub mod ns;
#[path = "Unsafe.rs"]    pub mod unsafe_;
#[path = "Builtins.rs"]  pub mod builtins;

#[derive(Debug, Clone)]
pub struct VmConfig {
    pub stack_size: usize,
    pub frame_size: usize,
    pub jit_enabled: bool,
    pub jit_threshold: u32,
    pub gc_interval: u32,
    pub max_recursion: u32,
}

impl Default for VmConfig {
    fn default() -> Self {
        VmConfig {
            stack_size: 1024 * 1024,
            frame_size: 1024,
            jit_enabled: true,
            jit_threshold: 100,
            gc_interval: 10000,
            max_recursion: 1000,
        }
    }
}

pub struct Vm {
    pub(crate) config: VmConfig,
    pub(crate) stack: Vec<UValue>,
    pub(crate) frames: Vec<CallFrame>,
    pc: usize,
    modules: Vec<LoadedModule>,
    pub(crate) gc: Box<dyn GcEngine>,
    pub(crate) cib: CibEngine,
    pub(crate) plugin_mgr: PluginManager,
    pub(crate) loader: ModuleLoader,
    running: bool,
    allocation_count: u64,
    pub(crate) handlers: Vec<ExceptionHandler>,
    /// Registry of native (Rust) functions callable from bytecode
    pub(crate) native_funcs: Vec<VmNativeFn>,
    /// Name → index mapping for native functions (used by LoadNative opcode)
    pub(crate) native_func_names: HashMap<String, NativeFuncIdx>,
    /// Namespace aliases: alias → target (e.g. "myns.ttt" → "foo.frr")
    pub(crate) ns_aliases: HashMap<String, String>,
    // Current bytecode being executed — borrows from current function.
    // NOT cloned per iteration. Stored as raw pointer + length.
    current_bytecode_ptr: *const u8,
    current_bytecode_len: usize,
    // Recursion depth counter (checked against config.max_recursion)
    call_depth: u32,
    /// UCSL shared-library registry (lazily initialized on first import)
    ucs_reg: Option<crate::ucsl::UcslRegistry>,
}

#[derive(Clone)]
pub(crate) struct LoadedModule {
    module: UtenModule,
    import_resolutions: Vec<ImportResolution>,
    pub(crate) globals: Vec<UValue>,
    export_values: HashMap<String, UValue>,
}

#[derive(Clone)]
pub(crate) struct ImportResolution {
    module_id: ModuleId,
    func_id: FuncRef,
}

#[derive(Clone)]
pub struct CallFrame {
    pub func_ref: FuncRef,
    pub module_id: ModuleId,
    pub return_pc: usize,
    pub stack_base: usize,
    pub locals: Vec<UValue>,
    pub captures: Vec<UValue>,
}

#[derive(Clone)]
pub struct ExceptionHandler {
    pub frame_depth: usize,
    pub handler_pc: usize,
    pub table_entry: Option<ExceptionTableEntry>,
}

impl Vm {
    pub fn new() -> Self {
        let mut vm = Vm::with_config(VmConfig::default());
        vm.init_unsafe_module();
        vm.init_embedded_stdlib();
        vm
    }

    pub fn with_config(config: VmConfig) -> Self {
        let mut vm = Vm {
            config,
            stack: Vec::with_capacity(1024),
            frames: Vec::new(),
            pc: 0,
            modules: Vec::new(),
            gc: Box::new(utencore_gc::memory::GenerationalGc::new()),
            cib: CibEngine::new(),
            plugin_mgr: PluginManager::new(),
            loader: ModuleLoader::new(),
            running: false,
            allocation_count: 0,
            native_funcs: Vec::new(),
            native_func_names: HashMap::new(),
            ns_aliases: HashMap::new(),
            current_bytecode_ptr: std::ptr::null(),
            current_bytecode_len: 0,
            handlers: Vec::new(),
            call_depth: 0,
            ucs_reg: None,
        };
        vm.init_unsafe_module();
        vm
    }

    pub(crate) fn check_recursion(&self) -> UtenResult<()> {
        if self.call_depth >= self.config.max_recursion {
            return Err(UtenError::Vm(format!(
                "maximum recursion depth exceeded ({} >= {})",
                self.call_depth, self.config.max_recursion
            )));
        }
        Ok(())
    }

    pub fn load_module(&mut self, module: UtenModule) -> UtenResult<ModuleId> {
        if module.bytecode_version > BYTECODE_VERSION {
            return Err(UtenError::Vm(format!(
                "bytecode version {} exceeds VM bytecode version {}",
                module.bytecode_version, BYTECODE_VERSION
            )));
        }
        // Full module verification
        utencore_bytecode::bytecode::verify_module(&module).map_err(|e| {
            UtenError::Vm(format!("Module verification failed: {e}"))
        })?;
        let global_count = module.globals.len().max(16);
        // Apply GC strategy from module header
        self.apply_gc_strategy(&module.header.gc_strategy);
        self.cib.register_interfaces_from_module(
            &module.header.metadata, None);
        let loaded = LoadedModule {
            globals: vec![UValue::Nil; global_count],
            import_resolutions: Vec::new(),
            export_values: HashMap::new(),
            module,
        };
        let id = self.modules.len() as ModuleId;
        self.modules.push(loaded);
        Ok(id)
    }

    pub(crate) fn apply_gc_strategy(&mut self, strategy: &str) {
        // If GC already matches strategy, keep it.
        // Only swap if module explicitly requires something different.
        let current_strategy = self.gc.strategy_name();
        if current_strategy != strategy {
            match strategy {
                "mark-sweep" => {
                    self.gc = Box::new(crate::memory::MarkSweepGc::new());
                }
                "refcount" => {
                    self.gc = Box::new(crate::memory::RefCountGc::new());
                }
                "none" => { /* no GC — runtime disables allocation tracking */ }
                _ => {} // keep current (generational is default)
            }
        }
    }

    pub fn execute(&mut self, module_id: ModuleId, func_ref: FuncRef, args: Vec<UValue>)
        -> UtenResult<UValue>
    {
        // Save any existing state (supports re-entrant execute)
        let saved_stack = self.stack.clone();
        let saved_frames = self.frames.clone();
        let saved_pc = self.pc;
        let saved_handlers = self.handlers.clone();
        let saved_depth = self.call_depth;
        let saved_running = self.running;

        let frame = CallFrame {
            func_ref,
            module_id,
            return_pc: 0,
            stack_base: 0,
            locals: vec![UValue::Nil; 32],
            captures: Vec::new(),
        };
        for arg in args { self.stack.push(arg); }
        self.frames.push(frame);
        self.pc = 0;
        self.running = true;
        self.call_depth = 0;
        let result = self.run_loop();

        // Restore saved state (keeps stack residue)
        self.stack = saved_stack;
        self.frames = saved_frames;
        self.pc = saved_pc;
        self.handlers = saved_handlers;
        self.call_depth = saved_depth;
        self.running = saved_running;
        self.current_bytecode_ptr = std::ptr::null();
        self.current_bytecode_len = 0;

        result
    }

    /// Execute a module's init function.
    ///
    /// After init completes, non-Nil globals are automatically synced to
    /// export_values so that ImportFunc/ImportValue can discover them.
    /// This means compilers that use StoreGlobal (like py2uc) don't need
    /// to emit explicit Export opcodes — though Export still works and
    /// takes precedence.
    pub(crate) fn run_module_init(&mut self, module_id: ModuleId) -> UtenResult<()> {
        let n_funcs = self.modules[module_id as usize].module.functions.len();
        if n_funcs == 0 { return Ok(()); }
        let init_func = (n_funcs - 1) as FuncRef;

        let saved = self.save_state();
        self.frames.clear();
        self.frames.push(CallFrame {
            func_ref: init_func,
            module_id,
            return_pc: 0,
            stack_base: 0,
            locals: vec![UValue::Nil; 32],
            captures: Vec::new(),
        });
        self.pc = 0;
        self.running = true;
        self.call_depth = 0;
        self.run_loop().ok();
        self.restore_state(saved);

        // Language-agnostic: sync StoreGlobal assignments to export_values
        // so ImportFunc/ImportValue can find them without explicit Export opcodes.
        self.sync_globals_to_exports(module_id);

        Ok(())
    }

    pub(crate) fn save_state(&self) -> VmState {
        VmState {
            frames: self.frames.clone(),
            pc: self.pc,
            stack_len: self.stack.len(),
            handlers: self.handlers.clone(),
            call_depth: self.call_depth,
            running: self.running,
        }
    }

    pub(crate) fn restore_state(&mut self, state: VmState) {
        self.frames = state.frames;
        self.pc = state.pc;
        self.handlers = state.handlers;
        self.call_depth = state.call_depth;
        self.running = state.running;
        self.current_bytecode_ptr = std::ptr::null();
        self.current_bytecode_len = 0;
    }

    pub(crate) fn run_loop(&mut self) -> UtenResult<UValue> {
        while self.running {
            // Check recursion limit
            if self.call_depth > self.config.max_recursion {
                return Err(UtenError::Vm("Max recursion depth exceeded".into()));
            }

            let mid = self.current_module_id();
            let func_ref = self.current_func_ref();
            // SAFETY: func_ref should always be valid in a loaded module
            if func_ref as usize >= self.modules[mid].module.functions.len() {
                return Err(UtenError::Vm("Invalid function ref".into()));
            }
            let func = &self.modules[mid].module.functions[func_ref as usize];
            let bytecode = func.bytecode.as_slice();

            // Set the bytecode slice pointer — NO CLONE
            self.current_bytecode_ptr = bytecode.as_ptr();
            self.current_bytecode_len = bytecode.len();

            if self.pc >= bytecode.len() { break; }

            // Read opcode directly from the slice
            let op_byte = bytecode[self.pc];
            self.pc += 1;
            let Some(op) = Opcode::from_byte(op_byte) else {
                return Err(UtenError::UnknownOpcode(op_byte));
            };
            let info = opcode_info(op);

            // Read operand (if any)
            // Compute operand WITHOUT holding a borrow so dispatch can borrow self
            let operand = match info.operand_size {
                0 => 0u32,
                1 => if self.pc < bytecode.len() { let v = bytecode[self.pc] as u32; self.pc += 1; v } else { 0 },
                2 => if self.pc + 2 <= bytecode.len() {
                        let v = u16::from_le_bytes([bytecode[self.pc], bytecode[self.pc + 1]]) as u32;
                        self.pc += 2; v
                    } else { 0 },
                4 => if self.pc + 4 <= bytecode.len() {
                        let v = i32::from_le_bytes([bytecode[self.pc], bytecode[self.pc+1], bytecode[self.pc+2], bytecode[self.pc+3]]) as u32;
                        self.pc += 4; v
                    } else { 0 },
                8 => if self.pc + 8 <= bytecode.len() {
                        self.pc += 8; // Read_bytes handled inline for f64
                        0
                    } else { 0 },
                _ => 0u32,
            };

            // Drop the bytecode reference before calling dispatch (which mutates self)
            // PushF64 reads from self.current_bytecode_ptr/length instead
            drop(bytecode);

            if let Err(e) = self.dispatch(op, operand) {
                if self.raise_exception(e.to_string()).is_err() {
                    // Unhandled: error (first line) + JVM-style stack trace
                    eprintln!("{e}");
                    self.print_stack_trace();
                    return Err(e);
                }
            }
            self.allocation_count += 1;

            // GC check (respect config.gc_interval)
            if self.allocation_count % self.config.gc_interval as u64 == 0 {
                {
                // GC collect: temporarily detach gc reference
                #[allow(unused_unsafe)]
                let gc_ptr: *mut Box<dyn GcEngine> = &mut self.gc;
                unsafe { (*gc_ptr).collect(self); }
            }
            }
        }
        Ok(self.stack.pop().unwrap_or(UValue::Nil))
    }
}

pub(crate) struct VmState {
    frames: Vec<CallFrame>,
    pc: usize,
    stack_len: usize,
    handlers: Vec<ExceptionHandler>,
    call_depth: u32,
    running: bool,
}

impl Vm {
    pub(crate) fn current_module_id(&self) -> usize {
        self.frames.last().map(|f| f.module_id as usize).unwrap_or(0)
    }

    pub(crate) fn current_func_ref(&self) -> FuncRef {
        self.frames.last().map(|f| f.func_ref).unwrap_or(0)
    }

    pub(crate) fn value_as_int(&self, v: &UValue) -> UtenResult<i64> {
        match v {
            UValue::Int32(i) => Ok(*i as i64),
            UValue::Int64(i) => Ok(*i),
            UValue::Bool(true) => Ok(1),
            UValue::Bool(false) => Ok(0),
            UValue::Float32(f) => Ok(*f as i64),
            UValue::Float64(f) => Ok(*f as i64),
            UValue::Complex { real, .. } => Ok(*real as i64),
            UValue::Gc(h, tag) => match tag {
                ValueTag::BigInt => {
                    if let HeapObject::BigInt(bi) = self.gc.get(*h) {
                        // Convert BigInt to i64 (returns 0 if out of range)
                        use std::convert::TryFrom;
                        Ok(i64::try_from(bi).unwrap_or(0))
                    } else { Err(UtenError::TypeError { expected: "numeric".into(), actual: format!("{:?}", tag) }) }
                }
                _ => Err(UtenError::TypeError { expected: "numeric".into(), actual: format!("{:?}", tag) }),
            },
            _ => Err(UtenError::TypeError { expected: "numeric".into(), actual: format!("{:?}", v.tag()) }),
        }
    }

    pub(crate) fn value_as_uint(&self, v: &UValue) -> UtenResult<u64> {
        match v {
            UValue::Int32(i) => Ok(*i as u64),
            UValue::Int64(i) => Ok(*i as u64),
            UValue::Bool(true) => Ok(1),
            UValue::Bool(false) => Ok(0),
            UValue::Float32(f) => Ok(*f as u64),
            UValue::Float64(f) => Ok(*f as u64),
            UValue::Complex { real, .. } => Ok(*real as u64),
            UValue::Gc(h, tag) => match tag {
                ValueTag::BigInt => {
                    if let HeapObject::BigInt(bi) = self.gc.get(*h) {
                        use std::convert::TryFrom;
                        Ok(u64::try_from(bi).unwrap_or(0))
                    } else { Err(UtenError::TypeError { expected: "unsigned numeric".into(), actual: format!("{:?}", tag) }) }
                }
                _ => Err(UtenError::TypeError { expected: "unsigned numeric".into(), actual: format!("{:?}", tag) }),
            },
            _ => Err(UtenError::TypeError { expected: "unsigned numeric".into(), actual: format!("{:?}", v.tag()) }),
        }
    }

    pub(crate) fn value_to_string(&self, v: &UValue) -> String {
        let mid = self.current_module_id();
        match v {
            UValue::String(sid) => self.modules[mid].module.strings[*sid as usize].clone(),
            UValue::Int32(v) => format!("{}", v),
            UValue::Int64(v) => format!("{}", v),
            UValue::Float32(v) => format!("{}", v),
            UValue::Float64(v) => format!("{}", v),
            UValue::Bool(v) => format!("{}", v),
            UValue::Nil => "None".to_string(),
            UValue::Complex { real, imag } => format!("{}+{}j", real, imag),
            UValue::Gc(h, tag) => match tag {
                ValueTag::HeapString => {
                    if let HeapObject::HeapString(s) = self.gc.get(*h) {
                        s.clone()
                    } else { format!("<obj#{}>", h) }
                }
                ValueTag::BigInt => {
                    if let HeapObject::BigInt(bi) = self.gc.get(*h) {
                        format!("{}", bi)
                    } else { format!("<obj#{}>", h) }
                }
                ValueTag::Bytes => {
                    if let HeapObject::Bytes(b) = self.gc.get(*h) {
                        format!("<bytes len={}>", b.len())
                    } else { format!("<obj#{}>", h) }
                }
                ValueTag::ByteArray => {
                    if let HeapObject::ByteArray(b) = self.gc.get(*h) {
                        format!("<bytearray len={}>", b.len())
                    } else { format!("<obj#{}>", h) }
                }
                ValueTag::Array => {
                    let arr_clone = if let HeapObject::Array(arr) = self.gc.get(*h) { arr.clone() } else { vec![] };
                    let items: Vec<String> = arr_clone.iter().map(|v| self.value_to_string(v)).collect();
                    format!("[{}]", items.join(", "))
                }
                ValueTag::Map => {
                    let map_clone = if let HeapObject::Map(m) = self.gc.get(*h) { m.clone() } else { std::collections::HashMap::new() };
                    let items: Vec<String> = map_clone.iter().map(|(k, v)| {
                        format!("{}: {}", self.value_to_string(k), self.value_to_string(v))
                    }).collect();
                    format!("{{{}}}", items.join(", "))
                }
                ValueTag::Set => {
                    let set_clone = if let HeapObject::Set(s) = self.gc.get(*h) { s.clone() } else { std::collections::HashSet::new() };
                    let items: Vec<String> = set_clone.iter().map(|v| self.value_to_string(v)).collect();
                    format!("{{{}}}", items.join(", "))
                }
                ValueTag::Tuple => {
                    let tup_clone = if let HeapObject::Tuple(t) = self.gc.get(*h) { t.clone() } else { vec![] };
                    let items: Vec<String> = tup_clone.iter().map(|v| self.value_to_string(v)).collect();
                    if items.len() == 1 { format!("({},)", items[0]) }
                    else { format!("({})", items.join(", ")) }
                }
                ValueTag::Range => {
                    let range_clone = if let HeapObject::Range { start, end, step, exclusive } = self.gc.get(*h) {
                        Some((start.clone(), end.clone(), step.clone(), *exclusive))
                    } else { None };
                    if let Some((s, e, st, excl)) = range_clone {
                        let ss = self.value_to_string(&s);
                        let es = self.value_to_string(&e);
                        if excl { format!("{ss}..{es}") }
                        else { format!("{ss}..={es}") }
                    } else { format!("<obj#{}>", h) }
                }
                ValueTag::Lambda => format!("<lambda {}>", h),
                _ => format!("<obj#{}>", h),
            },
            UValue::NativeFunc(_) => "<native_fn>".to_string(),
            UValue::NativeFn(_) => "<native>".to_string(),
            UValue::StructInline(sid, _) => format!("<struct#{}>", sid),
            UValue::BoxedStruct(sid, _) => format!("<boxed_struct#{}>", sid),
        }
    }

    pub(crate) fn peek(&self, depth: usize) -> UtenResult<&UValue> {
        let len = self.stack.len();
        if depth >= len {
            return Err(UtenError::StackUnderflow { needed: depth + 1, actual: len });
        }
        Ok(&self.stack[len - 1 - depth])
    }

    pub(crate) fn pop(&mut self) -> UtenResult<UValue> {
        self.stack.pop().ok_or(UtenError::StackUnderflow { needed: 1, actual: 0 })
    }

    pub(crate) fn pop_int(&mut self) -> UtenResult<i64> {
        let v = self.pop()?;
        match v {
            UValue::Int32(i) => Ok(i as i64),
            UValue::Int64(i) => Ok(i),
            UValue::Bool(true) => Ok(1),
            UValue::Bool(false) => Ok(0),
            UValue::Float32(f) => Ok(f as i64),
            UValue::Float64(f) => Ok(f as i64),
            UValue::Complex { real, .. } => Ok(real as i64),
            UValue::Gc(h, tag) if tag == ValueTag::BigInt => {
                if let HeapObject::BigInt(bi) = self.gc.get(h) {
                    use std::convert::TryFrom;
                    Ok(i64::try_from(bi).unwrap_or(0))
                } else { Err(UtenError::TypeError { expected: "numeric".into(), actual: "BigInt".into() }) }
            }
            v => Err(UtenError::TypeError { expected: "numeric".into(), actual: format!("{:?}", v.tag()) }),
        }
    }

    pub(crate) fn pop_uint(&mut self) -> UtenResult<u64> {
        match self.pop()? {
            UValue::Int32(i) => Ok(i as u64),
            UValue::Int64(i) => Ok(i as u64),
            UValue::Bool(true) => Ok(1),
            UValue::Bool(false) => Ok(0),
            UValue::Float32(f) => Ok(f as u64),
            UValue::Float64(f) => Ok(f as u64),
            UValue::Complex { real, .. } => Ok(real as u64),
            v => Err(UtenError::TypeError { expected: "unsigned".into(), actual: format!("{:?}", v.tag()) }),
        }
    }

    pub(crate) fn pop_float(&mut self) -> UtenResult<f64> {
        match self.pop()? {
            UValue::Float32(f) => Ok(f as f64),
            UValue::Float64(f) => Ok(f),
            UValue::Int32(i) => Ok(i as f64),
            UValue::Int64(i) => Ok(i as f64),
            UValue::Bool(true) => Ok(1.0),
            UValue::Bool(false) => Ok(0.0),
            UValue::Complex { real, .. } => Ok(real),
            v => Err(UtenError::TypeError { expected: "numeric".into(), actual: format!("{:?}", v.tag()) }),
        }
    }

    pub(crate) fn pop_gc(&mut self, expected: ValueTag) -> UtenResult<GcHandle> {
        match self.pop()? {
            UValue::Gc(h, tag) if tag == expected => Ok(h),
            UValue::Gc(_, tag) => Err(UtenError::TypeError {
                expected: "Gc object of specific type",
                actual: format!("{:?} (expected {:?})", tag, expected),
            }),
            v => Err(UtenError::TypeError { expected: "gc object".into(), actual: format!("{:?}", v.tag()) }),
        }
    }

    /// Read a field value from raw bytes based on TypeRef.
    /// Used by GetField dispatch for value type structs.
    pub(crate) fn read_field_from_bytes(
        bytes: &[u8],
        type_ref: &utencore_types::TypeRef,
        _mid: usize,
        _modules: &[LoadedModule],
    ) -> Option<UValue> {
        match type_ref {
            utencore_types::TypeRef::Bool => {
                Some(UValue::Bool(bytes[0] != 0))
            }
            utencore_types::TypeRef::I8 => {
                Some(UValue::Int32(bytes[0] as i8 as i32))
            }
            utencore_types::TypeRef::I16 => {
                Some(UValue::Int32(i16::from_le_bytes(bytes[..2].try_into().ok()?) as i32))
            }
            utencore_types::TypeRef::I32 => {
                Some(UValue::Int32(i32::from_le_bytes(bytes[..4].try_into().ok()?)))
            }
            utencore_types::TypeRef::I64 => {
                Some(UValue::Int64(i64::from_le_bytes(bytes[..8].try_into().ok()?)))
            }
            utencore_types::TypeRef::U8 => {
                Some(UValue::Int32(bytes[0] as i32))
            }
            utencore_types::TypeRef::U16 => {
                Some(UValue::Int32(u16::from_le_bytes(bytes[..2].try_into().ok()?) as i32))
            }
            utencore_types::TypeRef::U32 => {
                Some(UValue::Int64(u32::from_le_bytes(bytes[..4].try_into().ok()?) as i64))
            }
            utencore_types::TypeRef::U64 => {
                Some(UValue::Int64(i64::from_le_bytes(bytes[..8].try_into().ok()?)))
            }
            utencore_types::TypeRef::F32 => {
                Some(UValue::Float32(f32::from_le_bytes(bytes[..4].try_into().ok()?)))
            }
            utencore_types::TypeRef::F64 => {
                Some(UValue::Float64(f64::from_le_bytes(bytes[..8].try_into().ok()?)))
            }
            utencore_types::TypeRef::String => {
                // StringId (u32) in the struct bytes
                let sid = u32::from_le_bytes(bytes[..4].try_into().ok()?);
                Some(UValue::String(sid))
            }
            // Struct-in-struct: recurse for now (returns nil as placeholder)
            // Full recursive struct-typed-field support would need the struct def
            _ => Some(UValue::Nil),
        }
    }

    /// Write a field value into raw bytes based on TypeRef.
    /// Used by SetField dispatch for value type structs.
    pub(crate) fn write_field_to_bytes(
        bytes: &mut [u8],
        type_ref: &utencore_types::TypeRef,
        val: &UValue,
    ) {
        match type_ref {
            utencore_types::TypeRef::Bool => {
                let b = val.as_bool().unwrap_or(false);
                bytes[0] = if b { 1 } else { 0 };
            }
            utencore_types::TypeRef::I8 | utencore_types::TypeRef::U8 => {
                let v = val.as_i32().unwrap_or(0) as u8;
                bytes[0] = v;
            }
            utencore_types::TypeRef::I16 | utencore_types::TypeRef::U16 => {
                let v = val.as_i32().unwrap_or(0) as i16;
                let le = v.to_le_bytes();
                if bytes.len() >= 2 { bytes[..2].copy_from_slice(&le); }
            }
            utencore_types::TypeRef::I32 | utencore_types::TypeRef::U32 | utencore_types::TypeRef::F32 => {
                let v = match type_ref {
                    utencore_types::TypeRef::F32 => {
                        match val {
                            UValue::Float32(f) => f.to_bits() as i32,
                            _ => val.as_i32().unwrap_or(0),
                        }
                    }
                    _ => val.as_i32().unwrap_or(0),
                };
                let le = v.to_le_bytes();
                if bytes.len() >= 4 { bytes[..4].copy_from_slice(&le); }
            }
            utencore_types::TypeRef::I64 | utencore_types::TypeRef::U64 | utencore_types::TypeRef::F64 => {
                let v = match type_ref {
                    utencore_types::TypeRef::F64 => {
                        match val {
                            UValue::Float64(f) => f.to_bits() as i64,
                            _ => val.as_i64().unwrap_or(0),
                        }
                    }
                    _ => val.as_i64().unwrap_or(0),
                };
                let le = v.to_le_bytes();
                if bytes.len() >= 8 { bytes[..8].copy_from_slice(&le); }
            }
            utencore_types::TypeRef::String => {
                let sid = match val {
                    UValue::String(s) => *s,
                    _ => 0,
                };
                let le = sid.to_le_bytes();
                if bytes.len() >= 4 { bytes[..4].copy_from_slice(&le); }
            }
            _ => {}
        }
    }

    fn binary_int_op<F>(&mut self, f: F) -> UtenResult<i64>
    where F: Fn(i64, i64) -> i64 {
        let b = self.pop_int()?;
        let a = self.pop_int()?;
        Ok(f(a, b))
    }

    fn binary_float_op<F>(&mut self, f: F) -> UtenResult<f64>
    where F: Fn(f64, f64) -> f64 {
        let b = self.pop_float()?;
        let a = self.pop_float()?;
        Ok(f(a, b))
    }

    /// Try to convert a value to BigInt, or None if not possible
    pub(crate) fn as_bigint(&self, v: &UValue) -> Option<num_bigint::BigInt> {
        match v {
            UValue::Gc(h, tag) if *tag == ValueTag::BigInt => {
                if let HeapObject::BigInt(bi) = self.gc.get(*h) {
                    Some(bi.clone())
                } else { None }
            }
            UValue::Int32(i) => Some(num_bigint::BigInt::from(*i as i64)),
            UValue::Int64(i) => Some(num_bigint::BigInt::from(*i)),
            UValue::Bool(b) => Some(num_bigint::BigInt::from(if *b { 1i64 } else { 0 })),
            _ => None,
        }
    }

    /// Try a BigInt binary operation. Returns Some(UValue) if both operands are BigInt-compatible,
    /// None if neither is (caller should fall through to i64/float logic).
    pub(crate) fn try_bigint_binop<F>(&mut self, f: F) -> UtenResult<Option<UValue>>
    where F: Fn(num_bigint::BigInt, num_bigint::BigInt) -> num_bigint::BigInt
    {
        let b = self.peek(0)?.clone();
        let a = self.peek(1)?.clone();
        let ba = self.as_bigint(&a);
        let bb = self.as_bigint(&b);
        if let (Some(ba), Some(bb)) = (ba, bb) {
            self.pop()?;
            self.pop()?;
            let result = HeapObject::BigInt(f(ba, bb));
            Ok(Some(UValue::Gc(self.gc.alloc(result), ValueTag::BigInt)))
        } else {
            Ok(None) // not a BigInt op, let caller handle
        }
    }

    /// Try a BigInt unary operation. Returns Some(UValue) if the operand is BigInt-compatible.
    pub(crate) fn try_bigint_unop<F>(&mut self, f: F) -> UtenResult<Option<UValue>>
    where F: Fn(num_bigint::BigInt) -> num_bigint::BigInt
    {
        let v = self.peek(0)?.clone();
        if let Some(bv) = self.as_bigint(&v) {
            self.pop()?;
            let result = HeapObject::BigInt(f(bv));
            Ok(Some(UValue::Gc(self.gc.alloc(result), ValueTag::BigInt)))
        } else {
            Ok(None)
        }
    }

    /// Create a HeapString from a string value
    pub(crate) fn alloc_heapstring(&mut self, s: String) -> UValue {
        UValue::Gc(self.gc.alloc(HeapObject::HeapString(s)), ValueTag::HeapString)
    }

    /// Register a native (Rust) function and return its index.
    pub(crate) fn register_native_func(&mut self, f: VmNativeFn) -> NativeFuncIdx {
        let idx = self.native_funcs.len() as NativeFuncIdx;
        self.native_funcs.push(f);
        idx
    }

    /// Register a native function with a name (makes it accessible via LoadNative opcode).
    pub(crate) fn register_native_func_named(&mut self, name: &str, f: VmNativeFn) -> NativeFuncIdx {
        let idx = self.register_native_func(f);
        self.native_func_names.insert(name.to_string(), idx);
        idx
    }

    /// Notify the GC of a pointer store into a heap object (write barrier).
    /// Call this after storing a GC-tracked value into a heap container.
    /// `container` is the handle of the heap object being mutated,
    /// `stored_value` is the value being stored (may or may not be GC-tracked).
    #[inline]
    pub(crate) fn gc_write_barrier(&mut self, container: GcHandle, stored_value: &UValue) {
        if let UValue::Gc(child, _) = stored_value {
            self.gc.write_barrier(container, *child);
        }
    }

    /// Initialize the utencore module with Unsafe native functions.
    /// Called once at VM start after construction.
    pub fn init_unsafe_module(&mut self) {
        use crate::bytecode::{FunctionDef, UtenModule};

        let funcs = unsafe_::register_all(self);
        let gc_funcs = gc::register_all(self);
        let ns_funcs = ns::register_all(self);
        let mut m = UtenModule::new("utencore");
        m.header.metadata.insert("type".into(), "builtin".into());

        // Create an empty __init__ function (needed for module to load)
        let init_func = FunctionDef {
            name: "__init__".into(),
            bytecode: vec![crate::opcodes::Opcode::Return as u8],
            n_locals: 0, n_params: 0, is_variadic: false, n_captures: 0,
            return_type: None, param_types: vec![],
            jit_code: None, hotness: 0,
        };
        m.functions.push(init_func);

        // Load the module into VM
        if let Ok(mid) = self.load_module(m) {
            // Register so bytecode can `import utencore`
            self.loader.register_loaded("utencore", mid as usize);

            // Create Unsafe namespace
            {
                let mut members = Vec::new();
                for (name, idx, _) in &funcs {
                    let name_sid = self.modules[mid as usize].module.intern(name);
                    members.push((name_sid, UValue::NativeFunc(*idx)));
                }
                let ns_handle = self.gc.alloc(HeapObject::Namespace { name: 0, members, module_id: mid });
                self.modules[mid as usize].export_values.insert(
                    "Unsafe".into(),
                    UValue::Gc(ns_handle, ValueTag::Namespace),
                );
            }

            // Create Gc namespace
            if !gc_funcs.is_empty() {
                let mut members = Vec::new();
                for (name, idx, _) in &gc_funcs {
                    let name_sid = self.modules[mid as usize].module.intern(name);
                    members.push((name_sid, UValue::NativeFunc(*idx)));
                }
                let ns_handle = self.gc.alloc(HeapObject::Namespace { name: 0, members, module_id: mid });
                self.modules[mid as usize].export_values.insert(
                    "Gc".into(),
                    UValue::Gc(ns_handle, ValueTag::Namespace),
                );
            }

            // Create Ns namespace
            if !ns_funcs.is_empty() {
                let mut members = Vec::new();
                for (name, idx, _) in &ns_funcs {
                    let name_sid = self.modules[mid as usize].module.intern(name);
                    members.push((name_sid, UValue::NativeFunc(*idx)));
                }
                let ns_handle = self.gc.alloc(HeapObject::Namespace { name: 0, members, module_id: mid });
                self.modules[mid as usize].export_values.insert(
                    "Ns".into(),
                    UValue::Gc(ns_handle, ValueTag::Namespace),
                );
            }

            // ── Register native utencore.* functions ──
            let builtin_funcs = builtins::register_all(self);
            let mid_usize = mid as usize;
            let mut sub_ns: std::collections::HashMap<String, Vec<(StringId, UValue)>> =
                std::collections::HashMap::new();
            for (ns_name, func_name, idx, _n_params) in &builtin_funcs {
                let func_name_sid = self.modules[mid_usize].module.intern(func_name);
                if ns_name == "utencore" {
                    // Export directly: utencore.print, utencore.input, etc.
                    self.modules[mid_usize].export_values.insert(
                        func_name.clone(),
                        UValue::NativeFunc(*idx),
                    );
                } else {
                    // Group into sub-namespace: utencore.Math.sqrt, utencore.Io.read_file
                    sub_ns.entry(ns_name.clone()).or_default()
                        .push((func_name_sid, UValue::NativeFunc(*idx)));
                }
            }
            // Create sub-namespace objects for Math, Io, Sys
            for (ns_path, members) in sub_ns {
                let short_name = ns_path.rsplit('.').next().unwrap_or(&ns_path).to_string();
                let ns_handle = self.gc.alloc(HeapObject::Namespace {
                    name: self.modules[mid_usize].module.intern(&ns_path),
                    members, module_id: mid,
                });
                self.modules[mid_usize].export_values.insert(
                    short_name,
                    UValue::Gc(ns_handle, ValueTag::Namespace),
                );
            }
        }
    }

    /// Load embedded UCSL standard library modules into the VM at startup.
    /// Each module is deserialized from its compiled-in `.uclib` bytes, loaded,
    /// initialized, and registered by name so `import math` etc. always work
    /// without requiring separate files in the filesystem.
    pub fn init_embedded_stdlib(&mut self) {
        for (name, data) in EMBEDDED_UCSL {
            match crate::bytecode::UtenModule::from_bytes(data) {
                Ok(module) => {
                    let bytecode_ver = module.bytecode_version;
                    match self.load_module(module) {
                        Ok(mid) => {
                            self.run_module_init(mid).ok();
                            self.loader.register_loaded(name, mid as usize);
                            log::info!("embedded stdlib '{}' (v{}) loaded OK", name, bytecode_ver);
                        }
                        Err(e) => log::warn!("embedded stdlib '{}' load failed: {e}", name),
                    }
                }
                Err(e) => log::warn!("embedded stdlib '{}' deserialize failed: {e}", name),
            }
        }
    }

    /// Resolve a StringId to its string by searching ALL loaded modules.
    /// This handles cross-module string lookups where a Namespace was created
    /// in one module (e.g. utencore) but accessed from another (e.g. user script).
    pub(crate) fn resolve_string_across_modules(&self, sid: StringId) -> String {
        // Search current module first (most likely to have the string)
        let cur_mid = self.current_module_id();
        if let Some(s) = self.modules.get(cur_mid)
            .and_then(|m| m.module.strings.get(sid as usize))
        {
            return s.clone();
        }
        // Fallback: search all loaded modules
        for module in &self.modules {
            if let Some(s) = module.module.strings.get(sid as usize) {
                return s.clone();
            }
        }
        String::new()
    }

    /// Get or initialize the UCSL registry.
    pub(crate) fn get_ucsl_registry(&mut self) -> &mut crate::ucsl::UcslRegistry {
        if self.ucs_reg.is_none() {
            let mut reg = crate::ucsl::UcslRegistry::new();
            reg.discover();
            self.ucs_reg = Some(reg);
        }
        self.ucs_reg.as_mut().unwrap()
    }

    /// Resolve a module name and import it.
    /// Returns the module_id if already loaded or successfully loaded.
    pub(crate) fn import_module_by_name(&mut self, name: &str) -> Option<ModuleId> {
        // 1. Check already-loaded modules
        if let Some(mid) = self.loader.find_loaded(name) {
            return Some(mid as ModuleId);
        }

        // 2. Resolve via UCSL registry
        let path = {
            let reg = self.get_ucsl_registry();
            reg.resolve(name)
        };

        if let Some(path) = path {
            // 3. Read file
            let bytes = std::fs::read(&path).ok()?;
            // 4. Deserialize
            let module = crate::bytecode::UtenModule::from_bytes(&bytes).ok()?;
            // 5. Load into VM
            let mid = self.load_module(module).ok()?;
            // 6. Register by name
            self.loader.register_loaded(name, mid as usize);
            // 7. Run module init
            self.run_module_init(mid).ok();
            Some(mid)
        } else {
            // 3b. Try dotted name → filesystem path: "utenstd.math" → "ucsl/utenstd/math.uclib"
            let fs_path = std::path::Path::new("ucsl").join(
                name.replace('.', "/") + ".uclib"
            );
            if fs_path.exists() {
                let bytes = std::fs::read(&fs_path).ok()?;
                let module = crate::bytecode::UtenModule::from_bytes(&bytes).ok()?;
                let mid = self.load_module(module).ok()?;
                self.loader.register_loaded(name, mid as usize);
                self.run_module_init(mid).ok();
                Some(mid)
            } else {
                None
            }
        }
    }

    /// Resolve a namespace name through aliases.
    /// E.g., if alias "myns.ttt" → "foo.frr" exists,
    ///   resolve_ns_alias("myns.ttt.xxx") → "foo.frr.xxx"
    pub(crate) fn resolve_ns_alias(&self, name: &str) -> String {
        // Check exact match first
        if let Some(target) = self.ns_aliases.get(name) {
            return target.clone();
        }
        // Check prefix matches: "myns.ttt.xxx.YYY" → "myns.ttt" is prefix
        for (alias, target) in &self.ns_aliases {
            if name.starts_with(&format!("{alias}.")) {
                let suffix = &name[alias.len() + 1..];
                return format!("{}.{}", target, suffix);
            }
        }
        name.to_string()
    }

    /// Walk the prototype chain of a value looking for an operator handler.
    /// Returns Some(handler_function) if found, None otherwise.
    /// This enables __add__, __eq__, etc. metatable dispatch.
    /// Walk the prototype chain of a value looking for an operator handler.
    /// Returns Some(handler_function) if found, None otherwise.
    /// This enables __add__, __eq__, etc. metatable dispatch.
    ///
    /// Search order:
    ///   1. Object's own class methods → parent class chain
    ///   2. Object's proto chain (each proto's own class methods)
    pub(crate) fn get_operator_handler(&mut self, val: &UValue, op_name: &str) -> Option<UValue> {
        let (h, start_tag) = match val {
            UValue::Gc(h, tag) => (*h, *tag),
            _ => return None,
        };

        let mid = self.current_module_id();
        if mid >= self.modules.len() {
            return None;
        }

        // Intern the operator name once
        let op_name_id = self.modules[mid].module.intern(op_name);

        // Helper: search a Class's method list by name, return (func_ref, module_id)
        let find_in_class = |gc: &dyn GcEngine, class_h: GcHandle| -> Option<(FuncRef, ModuleId)> {
            if let HeapObject::Class { methods, .. } = gc.get(class_h) {
                methods.iter()
                    .find(|(sid, _)| *sid == op_name_id)
                    .map(|(_, fr)| (*fr, mid as ModuleId))
            } else {
                None
            }
        };

        // Helper: walk a Class and its parent chain
        let find_in_class_chain = |gc: &dyn GcEngine, mut class_h: GcHandle| -> Option<(FuncRef, ModuleId)> {
            loop {
                if let Some(result) = find_in_class(gc, class_h) {
                    return Some(result);
                }
                // Walk parent chain
                if let HeapObject::Class { parent, .. } = gc.get(class_h) {
                    match parent {
                        Some(p) => class_h = *p,
                        None => return None,
                    }
                } else {
                    return None;
                }
            }
        };

        // Start: try the object's own class (if it's an Object)
        if start_tag == ValueTag::Object {
            if let HeapObject::Object { class_handle, proto, .. } = self.gc.get(h) {
                // Check class chain first
                if let Some((fr, module_id)) = find_in_class_chain(self.gc.as_ref(), *class_handle) {
                    let closure = HeapObject::Closure {
                        func: fr,
                        captures: vec![],
                        module_id,
                    };
                    return Some(UValue::Gc(self.gc.alloc(closure), ValueTag::Closure));
                }

                // Walk proto chain
                let mut proto_current = *proto;
                while let Some(ph) = proto_current {
                    match self.gc.get(ph) {
                        HeapObject::Object { class_handle: pc, proto: pp, .. } => {
                            if let Some((fr, module_id)) = find_in_class_chain(self.gc.as_ref(), *pc) {
                                let closure = HeapObject::Closure {
                                    func: fr,
                                    captures: vec![],
                                    module_id,
                                };
                                return Some(UValue::Gc(self.gc.alloc(closure), ValueTag::Closure));
                            }
                            proto_current = *pp;
                        }
                        HeapObject::Class { .. } => {
                            if let Some((fr, module_id)) = find_in_class_chain(self.gc.as_ref(), ph) {
                                let closure = HeapObject::Closure {
                                    func: fr,
                                    captures: vec![],
                                    module_id,
                                };
                                return Some(UValue::Gc(self.gc.alloc(closure), ValueTag::Closure));
                            }
                            break;
                        }
                        _ => break,
                    }
                }
            }
        }

        // For Class values, check their method chain directly
        if start_tag == ValueTag::Class {
            if let Some((fr, module_id)) = find_in_class_chain(self.gc.as_ref(), h) {
                let closure = HeapObject::Closure {
                    func: fr,
                    captures: vec![],
                    module_id,
                };
                return Some(UValue::Gc(self.gc.alloc(closure), ValueTag::Closure));
            }
        }

        None
    }

    pub(crate) fn call_operator_handler(&mut self, handler: UValue, args: Vec<UValue>) -> UtenResult<()> {
        self.call_depth += 1;
        match handler {
            UValue::Gc(h, tag) if tag == ValueTag::Closure || tag == ValueTag::Lambda => {
                let (func, module_id) = match self.gc.get(h) {
                    HeapObject::Closure { func, module_id, .. } => (*func, *module_id),
                    HeapObject::Lambda { func, module_id, .. } => (*func, *module_id),
                    _ => return Err(UtenError::Vm("bad operator handler".into())),
                };
                self.call_function_with_args(module_id as usize, func, args)
            }
            UValue::NativeFunc(idx) => {
                let func_arc = self.native_funcs.get(idx as usize)
                    .ok_or(UtenError::Vm(format!("Invalid native func index {idx}")))?
                    .0.clone();
                let result = (func_arc)(self, &args)?;
                self.stack.push(result);
                Ok(())
            }
            _ => {
                self.stack.push(UValue::Nil);
                Ok(())
            }
        }
    }

    /// Print a stack trace for the current execution state.
    /// Walks all frames from top (current) to bottom (entry point),
    /// showing function name, bytecode offset, and source line if available.
    pub(crate) fn print_stack_trace(&self) {
        // JVM-style stack trace
        for (depth, frame) in self.frames.iter().enumerate().rev() {
            let mid = frame.module_id as usize;
            let func_name = if mid < self.modules.len() {
                let fi = frame.func_ref as usize;
                if fi < self.modules[mid].module.functions.len() {
                    self.modules[mid].module.functions[fi].name.clone()
                } else { format!("func#{}", fi) }
            } else { format!("mod#{}", mid) };

            let pc = if depth == self.frames.len() - 1 {
                self.pc
            } else {
                frame.return_pc
            };

            // Look up source line from line_map
            let source_line = if mid < self.modules.len() {
                let fi = frame.func_ref;
                self.modules[mid].module.header.line_map.iter()
                    .find(|e| e.func_index == fi && e.offset <= pc as u32 && (pc as u32) < e.offset + 16)
                    .map(|e| e.line)
            } else { None };

            let source_loc = if mid < self.modules.len() {
                format!("{}", self.modules[mid].module.header.name)
            } else { "?".into() };

            if let Some(line) = source_line {
                eprintln!("\tat {func_name}({source_loc}:{line})");
            } else {
                eprintln!("\tat {func_name}({source_loc}) + {pc:#x}");
            }
        }
    }

    pub(crate) fn raise_exception(&mut self, msg: String) -> UtenResult<()> {
        // Intern the error string upfront to avoid mutable borrow conflict inside the loop
        let err_sid = {
            let mid = self.current_module_id();
            self.modules[mid].module.intern(&msg)
        };

        // Walk frames from top to bottom, checking each module's exception table
        let frame_count = self.frames.len();
        for depth in (0..frame_count).rev() {
            let (module_id, func_ref, pc) = {
                let frame = &self.frames[depth];
                let pc = if depth == frame_count - 1 {
                    // Current (top) frame: use current PC
                    self.pc as u32
                } else {
                    // Caller frame: use return_pc (= instruction after the call,
                    // still within the try block if the call was made in one)
                    frame.return_pc as u32
                };
                (frame.module_id as usize, frame.func_ref as u32, pc)
            };

            if module_id >= self.modules.len() {
                continue;
            }

            // Clone exceptions table to avoid borrow conflict during unwind
            let exceptions = self.modules[module_id].module.exceptions.clone();

            // Search the module's exception table for matching entries
            for entry in &exceptions {
                if entry.func_index != func_ref {
                    continue; // entry belongs to a different function
                }
                if pc >= entry.try_start && pc < entry.try_end {
                    // Found a matching handler
                    if entry.handler_pc != 0 {
                        // Unwind frames above this depth
                        while self.frames.len() > depth + 1 {
                            self.frames.pop();
                        }
                        // Push the exception string so the handler can pop it
                        self.stack.push(UValue::String(err_sid));
                        // Jump to handler
                        self.pc = entry.handler_pc as usize;
                        return Ok(());
                    }
                    // handler_pc == 0: finally block — for now just continue
                    // searching (simple fallthrough)
                }
            }
        }

        // No handler found — propagate error
        Err(UtenError::Vm(format!("Unhandled exception: {msg}")))
    }

    // ── Namespace / Module management ──

    /// Build a HeapObject::Namespace wrapping a loaded module's exports.
    ///
    /// This is the language-agnostic representation of an imported module:
    /// ts2uc, py2uc, lu2uc all get the same Namespace handle back from
    /// the Import opcode. GetField/GetAttr on the namespace resolve to
    /// the module's exported symbols.
    ///
    /// `resolved_name` is the post-alias-resolution module name (used as
    /// the namespace display name).
    pub(crate) fn build_module_namespace(&mut self, module_id: ModuleId, resolved_name: &str) -> GcHandle {
        let mid = self.current_module_id();
        let name_sid = self.modules[mid].module.intern(resolved_name);

        // Collect export entries as namespace members.
        // Clone export_values first to avoid borrow conflict with module.intern().
        let exports_snapshot: Vec<(String, UValue)> = {
            let target_mid = module_id as usize;
            if target_mid < self.modules.len() {
                self.modules[target_mid].export_values.iter()
                    .map(|(k, v)| (k.clone(), v.clone()))
                    .collect()
            } else {
                Vec::new()
            }
        };

        let target_mid = module_id as usize;
        let mut members: Vec<(StringId, UValue)> = Vec::new();
        for (export_name, value) in &exports_snapshot {
            let member_sid = self.modules[target_mid].module.intern(export_name);
            members.push((member_sid, value.clone()));
        }

        self.gc.alloc(HeapObject::Namespace {
            name: name_sid,
            members,
            module_id,
        })
    }

    /// Sync non-Nil global variables to export_values after module init runs.
    ///
    /// This bridges the gap for compilers (like py2uc) that use StoreGlobal
    /// instead of the Export opcode. After module init executes, any global
    /// that was assigned a non-Nil value is automatically discoverable via
    /// ImportFunc/ImportValue.
    ///
    /// Compilers that emit explicit Export opcodes don't need this, but it
    /// is harmless — Export always wins (it writes after init, overwriting).
    pub(crate) fn sync_globals_to_exports(&mut self, module_id: ModuleId) {
        let mid = module_id as usize;
        if mid >= self.modules.len() {
            return;
        }

        // Collect names and corresponding values from globals, plus function
        // names for the function-index fallback. All done in one scope so the
        // immutable borrow on self.modules is released before we mutate.
        let (to_export, func_names, globals_snapshot) = {
            let module = &self.modules[mid];
            let to_export: Vec<(String, UValue)> = module.module.globals.iter()
                .enumerate()
                .filter_map(|(i, gdef)| {
                    let val = module.globals.get(i).cloned().unwrap_or(UValue::Nil);
                    if matches!(val, UValue::Nil) {
                        None
                    } else {
                        Some((gdef.name.clone(), val))
                    }
                })
                .collect();
            let func_names: Vec<String> = module.module.functions.iter()
                .map(|f| f.name.clone())
                .collect();
            let globals_snapshot: Vec<UValue> = module.globals.clone();
            (to_export, func_names, globals_snapshot)
        };

        // Insert all collected exports (now the immutable borrow is released)
        for (name, val) in &to_export {
            self.modules[mid].export_values.entry(name.clone()).or_insert_with(|| val.clone());
        }

        // Check each global position: if it holds a non-Nil value and the
        // position corresponds to a function index, export it by function name.
        for (idx, val) in globals_snapshot.iter().enumerate() {
            if matches!(val, UValue::Nil) {
                continue;
            }
            if idx < func_names.len() && !func_names[idx].starts_with('<') {
                let name = func_names[idx].clone();
                self.modules[mid].export_values.entry(name).or_insert_with(|| val.clone());
            }
        }
    }
}

impl utencore_gc::TraceRoots for Vm {
    fn trace_roots(&mut self, tracer: &mut dyn FnMut(utencore_types::GcHandle)) {
        // Trace the VM stack
        for val in &self.stack {
            if let utencore_types::UValue::Gc(h, _) = val {
                tracer(*h);
            }
        }

        // Trace frame locals and captures
        for frame in &self.frames {
            for local in &frame.locals {
                if let utencore_types::UValue::Gc(h, _) = local {
                    tracer(*h);
                }
            }
            for cap in &frame.captures {
                if let utencore_types::UValue::Gc(h, _) = cap {
                    tracer(*h);
                }
            }
        }
        // Trace module globals
        for module in &self.modules {
            for global in &module.globals {
                if let utencore_types::UValue::Gc(h, _) = global {
                    tracer(*h);
                }
            }
        }
    }
}

// ── Namespace management tests ──

#[cfg(test)]
mod tests {
    use crate::bytecode::{BytecodeWriter, FunctionDef, UtenModule};
    use crate::opcodes::Opcode;
    use crate::vm::Vm;
    use utencore_types::*;

    /// Helper: build a module with one function, load it, run init, return mid.
    fn load_and_init(vm: &mut Vm, name: &str, bytecode: Vec<u8>) -> ModuleId {
        let mut m = UtenModule::new(name);
        m.functions.push(FunctionDef {
            name: "<main>".into(),
            bytecode,
            n_locals: 4, n_params: 0, is_variadic: false,
            n_captures: 0, return_type: None, param_types: vec![],
            jit_code: None, hotness: 0,
        });
        let mid = vm.load_module(m).unwrap();
        vm.run_module_init(mid).ok();
        mid
    }

    #[test]
    fn test_export_opcode_registers_value() {
        let mut vm = Vm::new();
        let mut w = BytecodeWriter::new();
        w.emit(Opcode::PushI32); w.emit_i32(42);
        let answer_sid = 0u16; // first interned string will be "answer"
        w.emit(Opcode::Export); w.emit_u16(answer_sid);
        w.emit(Opcode::Return);

        // We need "answer" in the string pool. Pre-intern it so sid == 0.
        let mut m = UtenModule::new("test_export");
        assert_eq!(m.intern("answer"), 0); // confirm sid 0
        m.functions.push(FunctionDef {
            name: "<main>".into(), bytecode: w.into_bytes(),
            n_locals: 4, n_params: 0, is_variadic: false,
            n_captures: 0, return_type: None, param_types: vec![],
            jit_code: None, hotness: 0,
        });
        let mid = vm.load_module(m).unwrap();
        vm.run_module_init(mid).unwrap();

        assert!(vm.modules[mid as usize].export_values.contains_key("answer"),
            "Export opcode should register 'answer' in export_values");
    }

    #[test]
    fn test_build_module_namespace() {
        let mut vm = Vm::new();

        // Create a module that exports "greet"
        let mut m = UtenModule::new("mylib");
        let greet_sid = m.intern("greet");
        m.functions.push(FunctionDef {
            name: "greet".into(),
            bytecode: {
                let mut w = BytecodeWriter::new();
                w.emit(Opcode::PushI32); w.emit_i32(99);
                w.emit(Opcode::ReturnValue);
                w.into_bytes()
            },
            n_locals: 0, n_params: 0, is_variadic: false,
            n_captures: 0, return_type: None, param_types: vec![],
            jit_code: None, hotness: 0,
        });
        // Init function that exports greet
        m.functions.push(FunctionDef {
            name: "<main>".into(),
            bytecode: {
                let mut w = BytecodeWriter::new();
                w.emit(Opcode::PushI32); w.emit_i32(99);
                w.emit(Opcode::Export); w.emit_u16(greet_sid as u16);
                w.emit(Opcode::Return);
                w.into_bytes()
            },
            n_locals: 4, n_params: 0, is_variadic: false,
            n_captures: 0, return_type: None, param_types: vec![],
            jit_code: None, hotness: 0,
        });
        let mid = vm.load_module(m).unwrap();
        vm.run_module_init(mid).unwrap();

        // Register for import resolution
        vm.loader.register_loaded("mylib", mid as usize);

        // Build namespace and verify it contains "greet"
        let ns_handle = vm.build_module_namespace(mid, "mylib");
        if let HeapObject::Namespace { members, .. } = vm.gc.get(ns_handle) {
            assert!(!members.is_empty(), "Namespace should have members");
            // members contain (StringId keyed to mylib's string pool, UValue)
        } else {
            panic!("Expected Namespace");
        }
    }

    #[test]
    fn test_cross_module_import_via_opcodes() {
        let mut vm = Vm::new();

        // ── Module "mylib": exports "answer" = 42 ──
        let mut lib = UtenModule::new("mylib");
        let answer_sid = lib.intern("answer") as u16; // sid 0
        assert_eq!(answer_sid, 0);
        lib.functions.push(FunctionDef {
            name: "<main>".into(),
            bytecode: {
                let mut w = BytecodeWriter::new();
                w.emit(Opcode::PushI32); w.emit_i32(42);
                w.emit(Opcode::Export); w.emit_u16(answer_sid);
                w.emit(Opcode::Return);
                w.into_bytes()
            },
            n_locals: 4, n_params: 0, is_variadic: false,
            n_captures: 0, return_type: None, param_types: vec![],
            jit_code: None, hotness: 0,
        });
        let lib_mid = vm.load_module(lib).unwrap();
        vm.run_module_init(lib_mid).unwrap();
        vm.loader.register_loaded("mylib", lib_mid as usize);

        // ── Module "main": import mylib; mylib.answer() ──
        let mut main = UtenModule::new("main");
        let mylib_sid = main.intern("mylib") as u16;
        let answer_sid_main = main.intern("answer") as u16; // sid 1
        main.functions.push(FunctionDef {
            name: "<main>".into(),
            bytecode: {
                let mut w = BytecodeWriter::new();
                // import mylib → pushes Namespace
                w.emit(Opcode::Import); w.emit_u16(mylib_sid);
                // get answer from namespace
                w.emit(Opcode::GetField); w.emit_u16(answer_sid_main);
                // call it (answer is a value 42, CallValue on non-callable may fail,
                // but we're just testing the import→namespace→GetField chain)
                // Actually, let's just return the value directly
                w.emit(Opcode::ReturnValue);
                w.into_bytes()
            },
            n_locals: 4, n_params: 0, is_variadic: false,
            n_captures: 0, return_type: None, param_types: vec![],
            jit_code: None, hotness: 0,
        });
        let main_mid = vm.load_module(main).unwrap();

        let result = vm.execute(main_mid, 0, vec![]).unwrap();
        // Should get 42 from the exported value
        assert_eq!(format!("{result}"), "42",
            "Import→GetField on namespace should return the exported value");
    }

    #[test]
    fn test_importfunc_with_namespace_handle() {
        let mut vm = Vm::new();

        // Module "calc" with a function "double" and Export in init
        let mut calc = UtenModule::new("calc");
        let double_sid = calc.intern("double") as u16;
        // The double function
        calc.functions.push(FunctionDef {
            name: "double".into(),
            bytecode: {
                let mut w = BytecodeWriter::new();
                w.emit(Opcode::LoadLocal); w.emit_u16(0); // load param
                w.emit(Opcode::PushI32); w.emit_i32(2);
                w.emit(Opcode::Mul);
                w.emit(Opcode::ReturnValue);
                w.into_bytes()
            },
            n_locals: 1, n_params: 1, is_variadic: false,
            n_captures: 0, return_type: None, param_types: vec![],
            jit_code: None, hotness: 0,
        });
        // Init: create closure for double, export it
        calc.functions.push(FunctionDef {
            name: "<main>".into(),
            bytecode: {
                let mut w = BytecodeWriter::new();
                w.emit(Opcode::MakeClosure); w.emit_u16(0); // func 0
                w.emit(Opcode::Export); w.emit_u16(double_sid);
                w.emit(Opcode::Return);
                w.into_bytes()
            },
            n_locals: 4, n_params: 0, is_variadic: false,
            n_captures: 0, return_type: None, param_types: vec![],
            jit_code: None, hotness: 0,
        });
        let calc_mid = vm.load_module(calc).unwrap();
        vm.run_module_init(calc_mid).unwrap();
        vm.loader.register_loaded("calc", calc_mid as usize);

        // Main: import calc; ImportFunc double; call with arg 5
        let mut main = UtenModule::new("main");
        let calc_sid = main.intern("calc") as u16;
        let double_sid_main = main.intern("double") as u16;
        main.functions.push(FunctionDef {
            name: "<main>".into(),
            bytecode: {
                let mut w = BytecodeWriter::new();
                // Push arg and arg_count FIRST (go below the function on stack)
                w.emit(Opcode::PushI32); w.emit_i32(5);   // arg
                w.emit(Opcode::PushI32); w.emit_i32(1);   // arg_count
                // import calc → pushes Namespace
                w.emit(Opcode::Import); w.emit_u16(calc_sid);
                // ImportFunc double → pops namespace, pushes "double" closure
                // Now stack is: [arg=5, arg_count=1, closure] — CallValue ready
                w.emit(Opcode::ImportFunc); w.emit_u16(double_sid_main);
                w.emit(Opcode::CallValue);
                w.emit(Opcode::ReturnValue);
                w.into_bytes()
            },
            n_locals: 4, n_params: 0, is_variadic: false,
            n_captures: 0, return_type: None, param_types: vec![],
            jit_code: None, hotness: 0,
        });
        let main_mid = vm.load_module(main).unwrap();

        let result = vm.execute(main_mid, 0, vec![]).unwrap();
        assert_eq!(format!("{result}"), "10",
            "Import→ImportFunc→CallValue(5) on double should return 10");
    }

    #[test]
    fn test_sync_globals_to_exports() {
        let mut vm = Vm::new();

        // Module that uses StoreGlobal (not Export) — like py2uc output
        let mut m = UtenModule::new("pymod");
        m.globals.push(crate::bytecode::GlobalDef {
            name: "my_func".into(),
            init_value: None,
            is_exported: false,
        });
        m.functions.push(FunctionDef {
            name: "<main>".into(),
            bytecode: {
                let mut w = BytecodeWriter::new();
                w.emit(Opcode::PushI32); w.emit_i32(77);
                w.emit(Opcode::StoreGlobal); w.emit_u16(0); // global index 0 = "my_func"
                w.emit(Opcode::Return);
                w.into_bytes()
            },
            n_locals: 4, n_params: 0, is_variadic: false,
            n_captures: 0, return_type: None, param_types: vec![],
            jit_code: None, hotness: 0,
        });
        let mid = vm.load_module(m).unwrap();
        vm.run_module_init(mid).unwrap();

        // run_module_init now calls sync_globals_to_exports automatically...
        // Let's check: was "my_func" synced?
        // (If not, we need to call it explicitly)
        assert!(vm.modules[mid as usize].export_values.contains_key("my_func"),
            "sync_globals_to_exports should export StoreGlobal-assigned functions");
    }
}
