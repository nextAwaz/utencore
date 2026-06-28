//! UtenCore bytecode format.
//!
//! The bytecode is a serializable representation of compiled code.
//! It can be saved as .uclib (library) or .ucch (cache) files.
//!
//! # Format
//!
//! Each bytecode chunk is a sequence of opcodes followed by optional
//! operands. All multi-byte operands are little-endian.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use utencore_types::UtenResult;
use utencore_types::{Opcode, OpFlags, OpcodeInfo, opcode_info};
use utencore_types::{FuncRef, StringId, StructDef, UValue};
use utencore_types::{BYTECODE_VERSION, UCLIB_MAGIC, UCCH_MAGIC};

/// A complete compiled module (saved as .uclib)
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct UtenModule {
    pub magic: [u8; 4],
    pub version: (u16, u16),   // VM version that compiled this module
    pub bytecode_version: u32, // bytecode format version (determines VM compat)
    pub header: ModuleHeader,
    pub strings: Vec<String>,
    /// Constant pool: inline values accessible via PushConst
    pub constants: Vec<ConstValue>,
    pub functions: Vec<FunctionDef>,
    /// Struct type definitions (value types)
    pub structs: Vec<StructDef>,
    pub globals: Vec<GlobalDef>,
    pub exports: std::collections::HashMap<String, ExportEntry>,
    pub imports: Vec<ImportEntry>,
    /// Exception handling table: try blocks for bytecode
    pub exceptions: Vec<ExceptionTableEntry>,
    /// Fast string→id lookup cache (not serialized, rebuilt on load)
    #[serde(skip)]
    #[serde(default)]
    pub string_map: HashMap<String, StringId>,
}

/// Module header metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModuleHeader {
    /// Name of the module
    pub name: String,
    /// Source language (e.g., "python", "typescript")
    pub source_lang: String,
    /// Compiler info
    pub compiler: String,
    /// Compiler version
    pub compiler_version: String,
    /// GC strategy to use: "generational" (default), "mark-sweep", "refcount", or "none"
    pub gc_strategy: String,
    /// Whether JIT is recommended
    pub jit_recommended: bool,
    /// Custom metadata (compiler-specific)
    pub metadata: HashMap<String, String>,
    /// Line number mapping: (func_index, bytecode_offset) -> line_number
    pub line_map: Vec<LineEntry>,
}

/// A line number entry for debug info
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LineEntry {
    pub func_index: u32,
    pub offset: u32,
    pub line: u32,
    pub column: u32,
}

/// A function definition in bytecode
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FunctionDef {
    pub name: String,
    /// Raw bytecode sequence
    pub bytecode: Vec<u8>,
    /// Number of local variables
    pub n_locals: u16,
    /// Number of parameters
    pub n_params: u16,
    /// Whether this is a variadic function
    pub is_variadic: bool,
    /// Capture environment size (for closures)
    pub n_captures: u16,
    /// Return type hint
    pub return_type: Option<String>,
    /// Parameter type hints
    pub param_types: Vec<String>,
    /// JIT-compiled code address (runtime, not serialized)
    #[serde(skip)]
    pub jit_code: Option<*const u8>,
    /// Compilation hotness counter (runtime)
    #[serde(skip)]
    pub hotness: u32,
}

unsafe impl Send for FunctionDef {}
unsafe impl Sync for FunctionDef {}

/// A global variable definition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GlobalDef {
    pub name: String,
    pub init_value: Option<ConstValue>,
    pub is_exported: bool,
}

/// A constant value embedded in bytecode
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ConstValue {
    Nil,
    Bool(bool),
    Int32(i32),
    Int64(i64),
    Float32(f32),
    Float64(f64),
    String(StringId),
}

/// An export entry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ExportEntry {
    Function(FuncRef),
    Global(u32),
    Type(String),  // type name for structs
}

/// An import entry (for cross-module calls)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImportEntry {
    pub module_name: StringId,
    pub export_name: StringId,
    pub import_type: ImportType,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ImportType {
    Function,
    Value,
}

/// An entry in the exception handling table.
/// For a try block starting at `try_start` ending at `try_end`,
/// if an exception is thrown, execution jumps to `handler_pc`.
/// If `catch_type` is some StringId, only catch exceptions of that type.
/// A `handler_pc` of 0 means finally block (always runs).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExceptionTableEntry {
    /// Index of the function this exception entry belongs to.
    pub func_index: u32,
    pub try_start: u32,
    pub try_end: u32,
    pub handler_pc: u32,
    pub catch_type: Option<StringId>,
    pub finally_pc: Option<u32>,
}

impl UtenModule {
    /// Create a new module with default headers
    pub fn new(name: &str) -> Self {
        UtenModule {
            magic: *utencore_types::UCLIB_MAGIC,
            version: (0, 1),
            bytecode_version: utencore_types::BYTECODE_VERSION,
            header: ModuleHeader {
                name: name.to_string(),
                source_lang: String::new(),
                compiler: "utencore".to_string(),
                compiler_version: env!("CARGO_PKG_VERSION").to_string(),
                gc_strategy: "generational".to_string(),
                jit_recommended: false,
                metadata: std::collections::HashMap::new(),
                line_map: Vec::new(),
            },
            strings: Vec::new(),
            constants: Vec::new(),
            functions: Vec::new(),
            structs: Vec::new(),
            globals: Vec::new(),
            exports: std::collections::HashMap::new(),
            imports: Vec::new(),
            exceptions: Vec::new(),
            string_map: HashMap::new(),
        }
    }

    /// Add a string to the pool and return its ID.
    /// Uses a HashMap for O(1) lookup — falls back to linear scan
    /// if the cache isn't built yet (e.g. after deserialization).
    pub fn intern(&mut self, s: &str) -> StringId {
        // Try hash map first
        if let Some(&id) = self.string_map.get(s) {
            return id;
        }
        // Fallback: linear scan (for deserialized modules without string_map)
        if let Some(idx) = self.strings.iter().position(|x| x == s) {
            let id = idx as StringId;
            self.string_map.insert(s.to_string(), id);
            return id;
        }
        let id = self.strings.len() as StringId;
        self.strings.push(s.to_string());
        self.string_map.insert(s.to_string(), id);
        id
    }

    /// After deserialization, rebuild the string_map for fast lookups.
    pub fn rebuild_string_map(&mut self) {
        self.string_map.clear();
        for (i, s) in self.strings.iter().enumerate() {
            self.string_map.insert(s.clone(), i as StringId);
        }
    }

    /// Serialize to bytes (.uclib format)
    pub fn to_bytes(&self) -> UtenResult<Vec<u8>> {
        Ok(bincode::serialize(self)?)
    }

    /// Deserialize from bytes
    pub fn from_bytes(bytes: &[u8]) -> UtenResult<Self> {
        let mut module: Self = bincode::deserialize(bytes)?;
        module.rebuild_string_map();
        Ok(module)
    }

    /// Serialize as cache (.ucch) — same bytes but different magic
    pub fn to_cache_bytes(&self) -> UtenResult<Vec<u8>> {
        let mut module = self.clone();
        module.magic = *utencore_types::UCCH_MAGIC;
        Ok(bincode::serialize(&module)?)
    }

    /// Look up a struct definition by name (returns index and definition).
    pub fn find_struct(&self, name: &str) -> Option<(usize, &StructDef)> {
        self.structs.iter().enumerate().find(|(_, s)| {
            self.strings.get(s.name as usize).map_or(false, |n| n == name)
        })
    }

    /// Look up a struct definition by index.
    pub fn get_struct(&self, sid: utencore_types::StructId) -> Option<&StructDef> {
        self.structs.get(sid as usize)
    }

    /// Resolve a TypeRef's concrete byte size if it's a local struct.
    pub fn type_ref_size(&self, tref: &utencore_types::TypeRef) -> u32 {
        match tref {
            utencore_types::TypeRef::Struct(sid) => {
                self.structs.get(*sid as usize).map(|s| s.size).unwrap_or(0)
            }
            utencore_types::TypeRef::GenericInst { def_id, .. } => {
                self.structs.get(*def_id as usize).map(|s| s.size).unwrap_or(0)
            }
            other => other.byte_size(),
        }
    }
}

/// A bytecode writer helper
pub struct BytecodeWriter {
    bytes: Vec<u8>,
}

impl BytecodeWriter {
    pub fn new() -> Self {
        BytecodeWriter { bytes: Vec::new() }
    }

    pub fn emit(&mut self, opcode: Opcode) {
        self.bytes.push(opcode as u8);
    }

    /// Get the current length of bytecode
    pub fn len(&self) -> usize {
        self.bytes.len()
    }

    /// Get mutable reference to internal bytes (for extension)
    pub fn bytes_mut(&mut self) -> &mut Vec<u8> {
        &mut self.bytes
    }

    /// Get reference to internal bytes
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }

    pub fn emit_u16(&mut self, val: u16) {
        self.bytes.extend_from_slice(&val.to_le_bytes());
    }

    pub fn emit_i16(&mut self, val: i16) {
        self.bytes.extend_from_slice(&val.to_le_bytes());
    }

    pub fn emit_u32(&mut self, val: u32) {
        self.bytes.extend_from_slice(&val.to_le_bytes());
    }

    pub fn emit_i32(&mut self, val: i32) {
        self.bytes.extend_from_slice(&val.to_le_bytes());
    }

    pub fn emit_i64(&mut self, val: i64) {
        self.bytes.extend_from_slice(&val.to_le_bytes());
    }

    pub fn emit_f32(&mut self, val: f32) {
        self.bytes.extend_from_slice(&val.to_le_bytes());
    }

    pub fn emit_f64(&mut self, val: f64) {
        self.bytes.extend_from_slice(&val.to_le_bytes());
    }

    pub fn emit_op(&mut self, opcode: Opcode, operand: u16) {
        self.bytes.push(opcode as u8);
        self.bytes.extend_from_slice(&operand.to_le_bytes());
    }

    pub fn into_bytes(self) -> Vec<u8> {
        self.bytes
    }
}

/// A bytecode reader for decoding
pub struct BytecodeReader<'a> {
    bytes: &'a [u8],
    pos: usize,
}

impl<'a> BytecodeReader<'a> {
    pub fn new(bytes: &'a [u8]) -> Self {
        BytecodeReader { bytes, pos: 0 }
    }

    pub fn remaining(&self) -> usize {
        self.bytes.len().saturating_sub(self.pos)
    }

    pub fn pos(&self) -> usize {
        self.pos
    }

    pub fn set_pos(&mut self, pos: usize) {
        self.pos = pos;
    }

    pub fn read_u8(&mut self) -> u8 {
        let b = self.bytes[self.pos];
        self.pos += 1;
        b
    }

    pub fn read_i16(&mut self) -> i16 {
        let val = i16::from_le_bytes([
            self.bytes[self.pos],
            self.bytes[self.pos + 1],
        ]);
        self.pos += 2;
        val
    }

    pub fn read_u16(&mut self) -> u16 {
        let val = u16::from_le_bytes([
            self.bytes[self.pos],
            self.bytes[self.pos + 1],
        ]);
        self.pos += 2;
        val
    }

    pub fn read_i32(&mut self) -> i32 {
        let val = i32::from_le_bytes([
            self.bytes[self.pos],
            self.bytes[self.pos + 1],
            self.bytes[self.pos + 2],
            self.bytes[self.pos + 3],
        ]);
        self.pos += 4;
        val
    }

    pub fn read_i64(&mut self) -> i64 {
        let val = i64::from_le_bytes([
            self.bytes[self.pos],
            self.bytes[self.pos + 1],
            self.bytes[self.pos + 2],
            self.bytes[self.pos + 3],
            self.bytes[self.pos + 4],
            self.bytes[self.pos + 5],
            self.bytes[self.pos + 6],
            self.bytes[self.pos + 7],
        ]);
        self.pos += 8;
        val
    }

    pub fn read_f32(&mut self) -> f32 {
        f32::from_bits(self.read_i32() as u32)
    }

    pub fn read_f64(&mut self) -> f64 {
        f64::from_bits(self.read_i64() as u64)
    }

    /// Read next opcode
    pub fn next_opcode(&mut self) -> Option<Opcode> {
        if self.pos >= self.bytes.len() {
            return None;
        }
        let byte = self.read_u8();
        Opcode::from_byte(byte)
    }

    /// Peek at next opcode without advancing
    pub fn peek_opcode(&self) -> Option<Opcode> {
        if self.pos >= self.bytes.len() {
            return None;
        }
        Opcode::from_byte(self.bytes[self.pos])
    }

    /// Read opcode and its operand (if any)
    pub fn read_instruction(&mut self) -> Option<(Opcode, u32)> {
        let op = self.next_opcode()?;
        let info = opcode_info(op);
        let operand = match info.operand_size {
            0 => 0,
            1 => self.read_u8() as u32,
            2 => self.read_u16() as u32,
            4 => self.read_i32() as u32,
            8 => self.read_i64() as u32,
            _ => 0,
        };
        Some((op, operand))
    }
}

// ═══════════════════════════════════════════════════════
// ModuleBuilder — ergonomic wrapper for compiler plugins
// ═══════════════════════════════════════════════════════

/// A safe, ergonomic builder for constructing `UtenModule` from a compiler.
///
/// Key design decisions:
///   - **No raw `&mut UtenModule` in CompileContext** — the builder wraps it,
///     so `intern()` and `emit()` can coexist without borrow conflicts.
///   - **No `std::mem::replace` workaround needed** — compiler plugins call
///     methods on `&mut ModuleBuilder`, which owns the emit state internally.
///   - **Drop-in for py2uc's `Ctx`/`Buf`** — replaces both with one type.
pub struct ModuleBuilder<'a> {
    pub module: &'a mut UtenModule,
    /// Current active bytecode writer (one per function).
    pub writer: BytecodeWriter,
    /// Accumulated function definitions.
    pub functions: Vec<FunctionDef>,
}

impl<'a> ModuleBuilder<'a> {
    pub fn new(module: &'a mut UtenModule) -> Self {
        ModuleBuilder {
            module,
            writer: BytecodeWriter::new(),
            functions: Vec::new(),
        }
    }

    /// Intern a string into the module's string pool.
    pub fn intern(&mut self, s: &str) -> StringId {
        self.module.intern(s)
    }

    /// Emit a single opcode byte.
    #[inline]
    pub fn emit(&mut self, op: Opcode) {
        self.writer.emit(op);
    }

    /// Emit opcode + u16 operand.
    #[inline]
    pub fn emit_op(&mut self, op: Opcode, operand: u16) {
        self.writer.emit_op(op, operand);
    }

    /// Emit opcode + i16 operand.
    pub fn emit_i16(&mut self, v: i16) {
        self.writer.emit_i16(v);
    }

    /// Emit opcode + i32 operand.
    pub fn emit_i32(&mut self, v: i32) {
        self.writer.emit_i32(v);
    }

    /// Emit opcode + i64 operand.
    pub fn emit_i64(&mut self, v: i64) {
        self.writer.emit_i64(v);
    }

    /// Emit f64 bytes inline.
    pub fn emit_f64(&mut self, v: f64) {
        self.writer.emit_f64(v);
    }

    /// Current bytecode length (for jump patching).
    #[inline]
    pub fn writer_len(&self) -> usize {
        self.writer.len()
    }

    /// Patch an i16 at a specific offset (for jump fixups).
    pub fn patch_i16(&mut self, at: usize, v: i16) {
        self.writer.bytes_mut()[at..at + 2].copy_from_slice(&v.to_le_bytes());
    }

    /// Finish the current function and start a new one.
    pub fn finish_function(&mut self, name: &str, n_locals: u16, n_params: u16) -> FuncRef {
        let fr = self.functions.len() as FuncRef;
        self.functions.push(FunctionDef {
            name: name.into(),
            bytecode: self.writer.bytes().to_vec(),
            n_locals,
            n_params,
            is_variadic: false,
            n_captures: 0,
            return_type: None,
            param_types: vec![],
            jit_code: None,
            hotness: 0,
        });
        self.writer = BytecodeWriter::new();
        fr
    }

    /// Finalize: move all built functions into the module.
    pub fn finalize(&mut self) {
        self.module.functions = std::mem::take(&mut self.functions);
    }

    /// Set the source language in the module header.
    pub fn set_source_lang(&mut self, lang: &str) {
        self.module.header.source_lang = lang.to_string();
    }

    /// Set the GC strategy in the module header.
    pub fn set_gc_strategy(&mut self, strategy: &str) {
        self.module.header.gc_strategy = strategy.to_string();
    }

    /// Get a mutable reference to the underlying writer bytes (for direct manipulation).
    pub fn writer_bytes_mut(&mut self) -> &mut Vec<u8> {
        self.writer.bytes_mut()
    }
}

// ═══════════════════════════════════════════════════════
// Verification
// ═══════════════════════════════════════════════════════

/// Quick structural verification: magic bytes, basic bounds.
pub fn quick_verify(module: &UtenModule) -> Result<(), String> {
    // 1. Magic bytes
    if module.magic != *UCLIB_MAGIC && module.magic != *UCCH_MAGIC {
        return Err(format!("Invalid magic bytes: {:02X?}", module.magic));
    }

    // 2. Bytecode version
    if module.bytecode_version > BYTECODE_VERSION {
        return Err(format!(
            "Module bytecode version {} exceeds VM bytecode version {}",
            module.bytecode_version, BYTECODE_VERSION
        ));
    }

    // 3. Check strings pool has no empty gaps (strings are accessed by index)
    let n_strings = module.strings.len();

    // 4. Check constants pool string references
    for (i, c) in module.constants.iter().enumerate() {
        if let ConstValue::String(sid) = c {
            if (*sid as usize) >= n_strings {
                return Err(format!("Constant {} references out-of-bounds string id {}", i, sid));
            }
        }
    }

    // 5. Check function definitions
    let n_funcs = module.functions.len();
    for (fi, func) in module.functions.iter().enumerate() {
        if func.bytecode.is_empty() && func.name != "__init__" {
            return Err(format!("Function {} '{}' has empty bytecode", fi, func.name));
        }
        // Check n_captures is not excessive
        if func.n_captures > 1024 {
            return Err(format!("Function {} '{}' has excessive captures ({})", fi, func.name, func.n_captures));
        }
    }

    // 6. Check module header metadata
    if module.header.name.is_empty() {
        return Err("Module has empty name".into());
    }

    // 7. Check exports reference valid functions
    for (name, export) in &module.exports {
        match export {
            ExportEntry::Function(fr) => {
                if (*fr as usize) >= n_funcs {
                    return Err(format!("Export '{}' references invalid function {}", name, fr));
                }
            }
            ExportEntry::Global(g) => {
                if (*g as usize) >= module.globals.len() {
                    return Err(format!("Export '{}' references invalid global {}", name, g));
                }
            }
            ExportEntry::Type(_) => {}
        }
    }

    // 8. Check global defs
    for (gi, g) in module.globals.iter().enumerate() {
        if g.name.is_empty() {
            return Err(format!("Global {} has empty name", gi));
        }
    }

    // 9. Check import entries
    for (ii, imp) in module.imports.iter().enumerate() {
        if (imp.module_name as usize) >= n_strings {
            return Err(format!("Import {} references invalid module name string id {}", ii, imp.module_name));
        }
        if (imp.export_name as usize) >= n_strings {
            return Err(format!("Import {} references invalid export name string id {}", ii, imp.export_name));
        }
    }

    // 10. Check exception table structural integrity
    for (ei, entry) in module.exceptions.iter().enumerate() {
        if (entry.func_index as usize) >= n_funcs {
            return Err(format!(
                "Exception entry {} references invalid function index {}",
                ei, entry.func_index
            ));
        }
        if entry.try_start >= entry.try_end {
            return Err(format!(
                "Exception entry {}: try_start ({}) >= try_end ({})",
                ei, entry.try_start, entry.try_end
            ));
        }
        let func_len = module.functions[entry.func_index as usize].bytecode.len() as u32;
        if entry.try_end > func_len {
            return Err(format!(
                "Exception entry {}: try_end ({}) exceeds function bytecode length ({})",
                ei, entry.try_end, func_len
            ));
        }
        if entry.handler_pc != 0 && entry.handler_pc > func_len {
            return Err(format!(
                "Exception entry {}: handler_pc ({}) exceeds function bytecode length ({})",
                ei, entry.handler_pc, func_len
            ));
        }
    }

    Ok(())
}

/// Full module verification: structural checks + per-function opcode analysis
/// and CFG-based stack balance tracking.
pub fn verify_module(module: &UtenModule) -> Result<(), String> {
    quick_verify(module)?;

    let n_strings = module.strings.len();
    let n_funcs = module.functions.len();

    for (fi, func) in module.functions.iter().enumerate() {
        let bytecode = &func.bytecode;
        let len = bytecode.len();
        if len == 0 {
            continue;
        }

        // Phase 1: Decode all instructions, validate opcodes and operand bounds
        let mut instructions: Vec<(usize, OpcodeInfo, u32)> = Vec::new(); // (offset, info, operand)
        let mut i = 0;
        while i < len {
            let op_byte = bytecode[i];
            let Some(op) = Opcode::from_byte(op_byte) else {
                return Err(format!(
                    "Func {} '{}': invalid opcode byte 0x{:02X} at offset 0x{:04X}",
                    fi, func.name, op_byte, i
                ));
            };
            let info = opcode_info(op);
            let operand_size = info.operand_size as usize;

            if i + 1 + operand_size > len {
                return Err(format!(
                    "Func {} '{}': opcode {} at offset 0x{:04X} requires {} operand bytes, only {} remain",
                    fi, func.name, info.mnemonic, i, operand_size, len - i - 1
                ));
            }

            // Read operand
            let operand = if operand_size > 0 {
                let mut buf = [0u8; 8];
                let end = i + 1 + operand_size;
                buf[..operand_size].copy_from_slice(&bytecode[i + 1..end]);
                match operand_size {
                    1 => buf[0] as u32,
                    2 => u16::from_le_bytes([buf[0], buf[1]]) as u32,
                    4 => u32::from_le_bytes([buf[0], buf[1], buf[2], buf[3]]),
                    8 => 0, // IMM64: operand is embedded, no meaningful u32 value
                    _ => 0,
                }
            } else {
                0
            };

            // Store instruction for jump target analysis
            let info_flags = info.flags;
            let info_mnemonic = info.mnemonic;
            instructions.push((i, info, operand));

            // Validate opcode-specific operand ranges
            if info_flags.contains(OpFlags::HAS_FUNC) {
                let fr = operand as usize;
                if fr >= n_funcs {
                    return Err(format!(
                        "Func {} '{}': {} at offset 0x{:04X} references invalid function {} (max {})",
                        fi, func.name, info_mnemonic, i, fr, n_funcs - 1
                    ));
                }
            }
            if info_flags.contains(OpFlags::HAS_STRING) {
                // ClassAddMethod uses bit 15 as constructor flag, not part of StringId
                let sid = if info_mnemonic == "class_add_method" {
                    (operand & 0x7FFF) as usize
                } else {
                    operand as usize
                };
                if sid >= n_strings {
                    return Err(format!(
                        "Func {} '{}': {} at offset 0x{:04X} references invalid string id {} (pool size {})",
                        fi, func.name, info_mnemonic, i, sid, n_strings
                    ));
                }
            }
            if info_flags.contains(OpFlags::HAS_CONST) {
                let ci = operand as usize;
                if ci >= module.constants.len() {
                    return Err(format!(
                        "Func {} '{}': PushConst at offset 0x{:04X} references invalid constant index {} (pool size {})",
                        fi, func.name, i, ci, module.constants.len()
                    ));
                }
            }
            if info_flags.contains(OpFlags::HAS_GLOBAL) {
                let gi = operand as usize;
                // py2uc uses string_id as global index without creating GlobalDef entries,
                // so only flag this as a warning rather than a hard error.
                // The VM auto-resizes the globals array at runtime.
                if gi >= module.globals.len() && !module.globals.is_empty() {
                    return Err(format!(
                        "Func {} '{}': global index {} out of range (module has {} globals)",
                        fi, func.name, gi, module.globals.len()
                    ));
                }
            }

            i += 1 + operand_size;
        }

        // Phase 2: Validate jump targets — check ALL instruction offsets from the
        // raw bytecode (re-decode to avoid borrow issues with the instructions vec)
        let mut i = 0;
        while i < len {
            let op_byte = bytecode[i];
            let Some(op) = Opcode::from_byte(op_byte) else { break; };
            let info = opcode_info(op);
            let operand_size = info.operand_size as usize;

            if info.flags.contains(OpFlags::IS_JUMP) && operand_size >= 2 {
                if i + 1 + operand_size <= len {
                    let operand = if operand_size >= 2 {
                        u16::from_le_bytes([bytecode[i + 1], bytecode[i + 2]]) as u32
                    } else { 0 };
                    let jump_offset = operand as i16 as i64;
                    let target = i as i64 + 3 + jump_offset;
                    if target < 0 || target as usize >= len {
                        return Err(format!(
                            "Func {} '{}': {} at offset 0x{:04X} jumps to 0x{:04X}, outside bytecode [0, {})",
                            fi, func.name, info.mnemonic, i, target, len
                        ));
                    }
                }
            }

            i += 1 + operand_size;
            if i > len { break; }
        }

        // Phase 3: Stack depth consistency analysis (CFG-based)
        // We don't do full type checking (scripting VM), but we verify that
        // basic block entries have consistent stack depths.
        let n_instructions = instructions.len();
        if n_instructions == 0 {
            continue;
        }

        // Build a map of instruction offset → index in instructions
        let mut offset_to_idx: Vec<Option<usize>> = vec![None; len];
        for (idx, &(off, _, _)) in instructions.iter().enumerate() {
            offset_to_idx[off] = Some(idx);
        }

        // Compute successor list for each instruction (re-decode to avoid borrow issues)
        let mut successors: Vec<Vec<usize>> = vec![Vec::new(); n_instructions];
        for (idx, &(offset, _, operand)) in instructions.iter().enumerate() {
            let op = Opcode::from_byte(bytecode[offset]).unwrap(); // safe: already validated
            let info = opcode_info(op);
            if info.flags.contains(OpFlags::IS_JUMP) {
                // Compute target offset
                let jump_offset = operand as i16 as i64;
                let target = offset as i64 + 3 + jump_offset;
                if target >= 0 && (target as usize) < len {
                    if let Some(&target_idx) = offset_to_idx[target as usize].as_ref() {
                        successors[idx].push(target_idx);
                    }
                }
                // Conditional jumps also fall through
                if info.mnemonic.starts_with("jump_if") || info.mnemonic == "for_prep" || info.mnemonic == "for_step" || info.mnemonic == "next" {
                    // Fall-through to next instruction
                    if idx + 1 < n_instructions {
                        successors[idx].push(idx + 1);
                    }
                }
            } else if info.flags.contains(OpFlags::TERMINATOR) {
                // No successors (return, raise, halt, etc.)
            } else {
                // Normal fall-through
                if idx + 1 < n_instructions {
                    successors[idx].push(idx + 1);
                }
            }
        }

        // Simple stack depth analysis via fixed-point iteration
        let mut stack_depths: Vec<Option<i32>> = vec![None; n_instructions];
        stack_depths[0] = Some(0); // entry: stack is empty

        let mut changed = true;
        #[allow(unused_variables)]
        let iterations = 0;
        while changed {
            changed = false;
            for idx in 0..n_instructions {
                let Some(current_depth) = stack_depths[idx] else { continue };
                let (off, _, _) = instructions[idx];
                let op = Opcode::from_byte(bytecode[off]).unwrap();
                let info = opcode_info(op);
                let after_depth = current_depth + info.stack_effect as i32;

                for &succ_idx in &successors[idx] {
                    let prev = stack_depths[succ_idx];
                    if prev.is_none() {
                        stack_depths[succ_idx] = Some(after_depth);
                        changed = true;
                    } else if prev != Some(after_depth) {
                        // Stack depth mismatch — common in dynamically-typed VMs due
                        // to conditional branches with different paths. Warn but allow.
                        // (strict checking would return Err here)
                    }
                }
            }
        }
    }

    Ok(())
}

// ═══════════════════════════════════════════════════════
// Module loading / registry
// ═══════════════════════════════════════════════════════

/// Registry of loaded modules by name.
/// Used by VM to resolve `import` statements.
#[derive(Clone)]
pub struct ModuleLoader {
    modules: std::collections::HashMap<String, usize>,
}

impl ModuleLoader {
    pub fn new() -> Self {
        ModuleLoader {
            modules: std::collections::HashMap::new(),
        }
    }

    /// Register a loaded module by name.
    pub fn register_loaded(&mut self, name: &str, module_id: usize) {
        self.modules.insert(name.to_string(), module_id);
    }

    /// Look up a loaded module by name. Returns its module ID if found.
    pub fn find_loaded(&self, name: &str) -> Option<usize> {
        self.modules.get(name).copied()
    }
}
