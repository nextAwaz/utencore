//! UtenCore Intermediate Representation (IR).
//!
//! The IR is a higher-level representation between source-language ASTs
//! and final bytecode. Compilers (e.g., py2uc) produce IR, which is then
//! lowered to bytecode by the IR-to-bytecode pass.
//!
//! IR is structured as a control-flow graph (CFG) of basic blocks,
//! each containing a sequence of IR instructions.

use serde::{Deserialize, Serialize};

use crate::types::{FuncRef, StringId};

/// An IR program: set of functions with their CFGs.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IrProgram {
    pub version: u8,
    pub functions: Vec<IrFunction>,
    pub strings: Vec<String>,
    pub name: String,
}

/// A function in IR form
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IrFunction {
    pub name: String,
    pub params: Vec<IrParam>,
    pub return_type: Option<String>,
    pub blocks: Vec<IrBlock>,
    pub n_locals: u16,
    pub is_variadic: bool,
}

/// A function parameter
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IrParam {
    pub name: String,
    pub type_hint: Option<String>,
}

/// A basic block in the CFG
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IrBlock {
    pub name: String,
    pub instructions: Vec<IrInst>,
    pub terminator: IrTerminator,
    /// Phi nodes for SSA (optional)
    pub phis: Vec<IrPhi>,
}

/// An IR instruction (three-address code style)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum IrInst {
    // ── Literals ──
    ConstNil,
    ConstBool(bool),
    ConstI32(i32),
    ConstI64(i64),
    ConstF32(f32),
    ConstF64(f64),
    ConstStr(StringId),

    // ── Stack/Data movement ──
    Copy { dest: IrValue, src: IrValue },
    LoadLocal { dest: IrValue, idx: u16 },
    StoreLocal { val: IrValue, idx: u16 },
    LoadCapture { dest: IrValue, idx: u16 },
    StoreCapture { val: IrValue, idx: u16 },
    LoadGlobal { dest: IrValue, idx: u16 },
    StoreGlobal { val: IrValue, idx: u16 },

    // ── Arithmetic ──
    Add { dest: IrValue, left: IrValue, right: IrValue },
    Sub { dest: IrValue, left: IrValue, right: IrValue },
    Mul { dest: IrValue, left: IrValue, right: IrValue },
    Div { dest: IrValue, left: IrValue, right: IrValue },
    Mod { dest: IrValue, left: IrValue, right: IrValue },
    Neg { dest: IrValue, val: IrValue },
    FAdd { dest: IrValue, left: IrValue, right: IrValue },
    FSub { dest: IrValue, left: IrValue, right: IrValue },
    FMul { dest: IrValue, left: IrValue, right: IrValue },
    FDiv { dest: IrValue, left: IrValue, right: IrValue },
    Pow { dest: IrValue, base: IrValue, exp: IrValue },

    // ── Bitwise ──
    BitAnd { dest: IrValue, left: IrValue, right: IrValue },
    BitOr { dest: IrValue, left: IrValue, right: IrValue },
    BitXor { dest: IrValue, left: IrValue, right: IrValue },
    BitNot { dest: IrValue, val: IrValue },
    Shl { dest: IrValue, left: IrValue, right: IrValue },
    Shr { dest: IrValue, left: IrValue, right: IrValue },

    // ── Comparison ──
    Eq { dest: IrValue, left: IrValue, right: IrValue },
    Ne { dest: IrValue, left: IrValue, right: IrValue },
    Lt { dest: IrValue, left: IrValue, right: IrValue },
    Le { dest: IrValue, left: IrValue, right: IrValue },
    Gt { dest: IrValue, left: IrValue, right: IrValue },
    Ge { dest: IrValue, left: IrValue, right: IrValue },

    // ── Conversion ──
    ToI32 { dest: IrValue, val: IrValue },
    ToI64 { dest: IrValue, val: IrValue },
    ToF32 { dest: IrValue, val: IrValue },
    ToF64 { dest: IrValue, val: IrValue },
    ToBool { dest: IrValue, val: IrValue },
    TypeOf { dest: IrValue, val: IrValue },

    // ── Objects ──
    NewArray { dest: IrValue, elements: Vec<IrValue> },
    ArrayLen { dest: IrValue, arr: IrValue },
    ArrayGet { dest: IrValue, arr: IrValue, idx: IrValue },
    ArraySet { arr: IrValue, idx: IrValue, val: IrValue },
    ArrayPush { arr: IrValue, val: IrValue },
    ArrayPop { dest: IrValue, arr: IrValue },
    ArraySlice { dest: IrValue, arr: IrValue, start: IrValue, end: IrValue },

    NewMap { dest: IrValue },
    MapGet { dest: IrValue, map: IrValue, key: IrValue },
    MapSet { map: IrValue, key: IrValue, val: IrValue },
    MapDel { map: IrValue, key: IrValue },

    // ── Struct/Field ──
    GetField { dest: IrValue, obj: IrValue, field: StringId },
    SetField { obj: IrValue, field: StringId, val: IrValue },

    // ── Strings ──
    StrConcat { dest: IrValue, left: IrValue, right: IrValue },
    StrLen { dest: IrValue, val: IrValue },
    StrSub { dest: IrValue, val: IrValue, start: IrValue, end: IrValue },

    // ── Function / Call ──
    Call { dest: IrValue, func: FuncRef, args: Vec<IrValue> },
    CallValue { dest: IrValue, func: IrValue, args: Vec<IrValue> },
    MakeClosure { dest: IrValue, func: FuncRef, captures: Vec<IrValue> },
    Invoke { dest: IrValue, obj: IrValue, method: StringId, args: Vec<IrValue> },

    // ── CIB ──
    CibLoad { dest: IrValue, lib_name: StringId },
    CibSym { dest: IrValue, sym_name: StringId },
    CibCall { dest: IrValue, func: IrValue, args: Vec<IrValue> },

    // ── Module ──
    Import { dest: IrValue, module: StringId },
    ImportFunc { dest: IrValue, module: StringId, name: StringId },
    Export { name: StringId, val: IrValue },

    // ── Other ──
    Print { val: IrValue },
    Line { line: u32 },
    Debug { msg: String },
}

/// IR terminator (end of basic block)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum IrTerminator {
    Fallthrough,
    Jump(String),
    Branch { cond: IrValue, true_block: String, false_block: String },
    Return(Option<IrValue>),
    Halt,
}

/// A phi node in SSA form
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IrPhi {
    pub dest: IrValue,
    pub incoming: Vec<(IrValue, String)>,
}

/// An IR value reference
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum IrValue {
    VReg(u32),
    Local(u16),
    Const(IrConst),
    Param(u16),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum IrConst {
    Nil,
    Bool(bool),
    I32(i32),
    I64(i64),
    F32(f32),
    F64(f64),
    Str(StringId),
}

// ── IR Lowering to Bytecode ──

/// Lower an IR program to bytecode
pub fn ir_to_bytecode(program: IrProgram) -> crate::bytecode::UtenModule {
    let mut module = crate::bytecode::UtenModule::new(&program.name);

    for s in &program.strings {
        module.intern(s);
    }

    for ir_func in &program.functions {
        let mut writer = crate::bytecode::BytecodeWriter::new();
        let mut line_map = Vec::new();

        for (block_idx, block) in ir_func.blocks.iter().enumerate() {
            if block_idx > 0 {
                // Line opcode removed — block info in module.header.line_map
            }

            for inst in &block.instructions {
                let offset_before = writer.len() as u32;
                if let Err(e) = ir_inst_to_bytecode(inst, &mut writer, &module) {
                    log::warn!("IR lowering error: {}", e);
                }
                if let IrInst::Line { line } = inst {
                    line_map.push(crate::bytecode::LineEntry {
                        func_index: module.functions.len() as u32,
                        offset: offset_before,
                        line: *line,
                        column: 0,
                    });
                }
            }

            ir_terminator_to_bytecode(&block.terminator, &mut writer);
        }

        let func_def = crate::bytecode::FunctionDef {
            name: ir_func.name.clone(),
            bytecode: writer.into_bytes(),
            n_locals: ir_func.n_locals,
            n_params: ir_func.params.len() as u16,
            is_variadic: ir_func.is_variadic,
            n_captures: 0,
            return_type: ir_func.return_type.clone(),
            param_types: ir_func.params.iter()
                .map(|p| p.type_hint.clone().unwrap_or_default())
                .collect(),
            jit_code: None,
            hotness: 0,
        };

        module.functions.push(func_def);
        module.header.line_map.extend(line_map);
    }

    module
}

fn ir_inst_to_bytecode(
    inst: &IrInst,
    writer: &mut crate::bytecode::BytecodeWriter,
    _module: &crate::bytecode::UtenModule,
) -> Result<(), String> {
    use IrInst::*;
    match inst {
        ConstNil => { writer.emit(crate::Opcode::PushNil); }
        ConstBool(true) => { writer.emit(crate::Opcode::PushTrue); }
        ConstBool(false) => { writer.emit(crate::Opcode::PushFalse); }
        ConstI32(v) => { writer.emit(crate::Opcode::PushI32); writer.emit_i32(*v); }
        ConstI64(v) => { writer.emit(crate::Opcode::PushI64); writer.emit_i64(*v); }
        ConstF32(v) => { writer.emit(crate::Opcode::PushF32); writer.emit_f32(*v); }
        ConstF64(v) => { writer.emit(crate::Opcode::PushF64); writer.emit_f64(*v); }
        ConstStr(sid) => { writer.emit_op(crate::Opcode::PushString, *sid as u16); }

        LoadLocal { idx, .. } => { writer.emit_op(crate::Opcode::LoadLocal, *idx); }
        StoreLocal { idx, .. } => { writer.emit_op(crate::Opcode::StoreLocal, *idx); }
        LoadGlobal { idx, .. } => { writer.emit_op(crate::Opcode::LoadGlobal, *idx); }
        StoreGlobal { idx, .. } => { writer.emit_op(crate::Opcode::StoreGlobal, *idx); }
        // LoadCapture/StoreCapture removed — closures use MakeClosure captures
        LoadCapture { .. } => {}
        StoreCapture { .. } => {}

        Add { .. } => writer.emit(crate::Opcode::Add),
        Sub { .. } => writer.emit(crate::Opcode::Sub),
        Mul { .. } => writer.emit(crate::Opcode::Mul),
        Div { .. } => writer.emit(crate::Opcode::Div),
        Mod { .. } => writer.emit(crate::Opcode::Mod),
        Neg { .. } => writer.emit(crate::Opcode::Neg),
        FAdd { .. } => writer.emit(crate::Opcode::FAdd),
        FSub { .. } => writer.emit(crate::Opcode::FSub),
        FMul { .. } => writer.emit(crate::Opcode::FMul),
        FDiv { .. } => writer.emit(crate::Opcode::FDiv),
        // Pow removed — use Multiply loop or library
        Pow { .. } => {}

        BitAnd { .. } => writer.emit(crate::Opcode::BitAnd),
        BitOr { .. } => writer.emit(crate::Opcode::BitOr),
        BitXor { .. } => writer.emit(crate::Opcode::BitXor),
        BitNot { .. } => writer.emit(crate::Opcode::BitNot),
        Shl { .. } => writer.emit(crate::Opcode::Shl),
        Shr { .. } => writer.emit(crate::Opcode::Shr),

        Eq { .. } => writer.emit(crate::Opcode::Eq),
        Ne { .. } => writer.emit(crate::Opcode::Ne),
        Lt { .. } => writer.emit(crate::Opcode::Lt),
        Le { .. } => writer.emit(crate::Opcode::Le),
        Gt { .. } => writer.emit(crate::Opcode::Gt),
        Ge { .. } => writer.emit(crate::Opcode::Ge),

        ToI32 { .. } => writer.emit(crate::Opcode::ToI32),
        ToI64 { .. } => writer.emit(crate::Opcode::ToI64),
        ToF32 { .. } => writer.emit(crate::Opcode::ToF32),
        ToF64 { .. } => writer.emit(crate::Opcode::ToF64),
        ToBool { .. } => writer.emit(crate::Opcode::ToBool),
        TypeOf { .. } => writer.emit(crate::Opcode::TypeOf),

        NewArray { .. } => writer.emit_op(crate::Opcode::NewArray, 0),
        ArrayLen { .. } => writer.emit(crate::Opcode::ArrayLen),
        ArrayGet { .. } => writer.emit(crate::Opcode::ArrayGet),
        ArraySet { .. } => writer.emit(crate::Opcode::ArraySet),
        ArrayPush { .. } => writer.emit(crate::Opcode::ArrayPush),
        ArrayPop { .. } => writer.emit(crate::Opcode::ArrayPop),
        // ArraySlice removed — use library call
        ArraySlice { .. } => {}

        NewMap { .. } => writer.emit(crate::Opcode::NewMap),
        MapGet { .. } => writer.emit(crate::Opcode::MapGet),
        MapSet { .. } => writer.emit(crate::Opcode::MapSet),
        MapDel { .. } => writer.emit(crate::Opcode::MapDel),

        GetField { field, .. } => writer.emit_op(crate::Opcode::GetField, *field as u16),
        SetField { field, .. } => writer.emit_op(crate::Opcode::SetField, *field as u16),

        // StrConcat removed — use library call
        StrConcat { .. } => {}
        StrLen { .. } => writer.emit(crate::Opcode::StrLen),
        StrSub { .. } => writer.emit(crate::Opcode::StrSub),

        Call { func, .. } => writer.emit_op(crate::Opcode::Call, *func as u16),
        CallValue { .. } => writer.emit(crate::Opcode::CallValue),
        MakeClosure { func, .. } => writer.emit_op(crate::Opcode::MakeClosure, *func as u16),
        // Invoke removed — use CallMethod + GetAttr
        Invoke { method, .. } => writer.emit_op(crate::Opcode::CallMethod, *method as u16),

        CibLoad { lib_name, .. } => writer.emit_op(crate::Opcode::CibLoad, *lib_name as u16),
        CibSym { .. } => writer.emit(crate::Opcode::CibSym),
        CibCall { .. } => writer.emit(crate::Opcode::CibCall),

        Import { module, .. } => writer.emit_op(crate::Opcode::Import, *module as u16),
        ImportFunc { module, name, .. } => {
            writer.emit_op(crate::Opcode::ImportFunc, *module as u16);
            writer.emit_u16(*name as u16);
        }
        Export { name, .. } => writer.emit_op(crate::Opcode::Export, *name as u16),

        // Print removed — use utencore.println native function
        Print { .. } => {}
        Line { .. } => {}
        Debug { msg } => log::debug!("IR debug: {msg}"),

        Copy { .. } => writer.emit(crate::Opcode::Dup),
    }
    Ok(())
}

fn ir_terminator_to_bytecode(
    term: &IrTerminator,
    writer: &mut crate::bytecode::BytecodeWriter,
) {
    use IrTerminator::*;
    match term {
        Fallthrough => {}
        Jump(_) => {
            writer.emit(crate::Opcode::Jump);
            writer.emit_i16(0);
        }
        Branch { .. } => {
            writer.emit(crate::Opcode::JumpIfFalse);
            writer.emit_i16(0);
        }
        Return(None) => writer.emit(crate::Opcode::Return),
        Return(Some(_)) => writer.emit(crate::Opcode::ReturnValue),
        Halt => writer.emit(crate::Opcode::Halt),
    }
}
