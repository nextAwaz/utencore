// Bytecode codegen — using utencore::bytecode::ModuleBuilder.

use utencore::bytecode::{FunctionDef, ModuleBuilder};
use utencore::opcodes::Opcode;
use utencore::FuncRef;
use crate::ast::*;

// ── Old-style Ctx (internal to codegen) ──

struct Buf(Vec<u8>);
impl Buf {
    fn new() -> Self { Buf(vec![]) }
    fn push(&mut self, b: u8) { self.0.push(b); }
    fn push_op(&mut self, op: Opcode) { self.0.push(op as u8); }
    fn push_u16(&mut self, v: u16) { self.0.extend_from_slice(&v.to_le_bytes()); }
    fn push_op16(&mut self, op: Opcode, v: u16) { self.push_op(op); self.push_u16(v); }
    fn push_i16(&mut self, v: i16) { self.0.extend_from_slice(&v.to_le_bytes()); }
    fn push_i32(&mut self, v: i32) { self.0.extend_from_slice(&v.to_le_bytes()); }
    fn push_f64(&mut self, v: f64) { self.0.extend_from_slice(&v.to_le_bytes()); }
    fn len(&self) -> usize { self.0.len() }
    fn patch_i16(&mut self, at: usize, v: i16) { self.0[at..at+2].copy_from_slice(&v.to_le_bytes()); }
    fn take(&mut self) -> Vec<u8> { std::mem::take(&mut self.0) }
}

struct Ctx {
    module: utencore::bytecode::UtenModule,
    funcs: Vec<FunctionDef>,
    cur: Buf,
    fc: u32,
}
impl Ctx {
    fn new(name: &str) -> Self {
        Ctx { module: utencore::bytecode::UtenModule::new(name), funcs: vec![], cur: Buf::new(), fc: 0 }
    }
    fn nf(&self) -> u16 { self.funcs.len() as u16 }
    fn push_fc(&mut self) { self.fc += 1; }
}

fn intern(module: &mut utencore::bytecode::UtenModule, s: &str) -> u16 { module.intern(s) as u16 }

// ── Public API ──

/// Compile into a ModuleBuilder (modern path — no std::mem::replace).
///
/// Uses the existing Ctx-based codegen internally, then moves results into
/// `b.functions`. The caller (`PluginManager::compile`) calls `b.finalize()`
/// to move functions into `b.module.functions`.
pub fn compile_into_builder(p: &Program, name: &str, b: &mut ModuleBuilder) -> Result<(), String> {
    let mut c = Ctx::new(name);
    c.module.header.source_lang = "python".into();
    compile_into_ctx(p, &mut c);
    // Fill builder's functions and string pool
    b.functions = c.funcs;
    b.module.strings = c.module.strings;
    b.module.string_map = c.module.string_map;
    b.module.header = c.module.header;
    Ok(()) // note: no b.finalize() here; caller does it
}

/// Legacy: compile to bytes (deprecated).
pub fn compile(p: &Program, name: &str) -> Result<Vec<u8>, String> {
    let mut c = Ctx::new(name);
    compile_into_ctx(p, &mut c);
    c.module.functions = c.funcs;
    c.module.to_bytes().map_err(|e| format!("Serialize: {e}"))
}

/// Compile into a pre-existing module (backward compat).
pub fn compile_into(p: &Program, name: &str, module: &mut utencore::bytecode::UtenModule) -> Result<(), String> {
    let mut b = ModuleBuilder::new(module);
    compile_into_builder(p, name, &mut b)
}

// ── Internal codegen (unchanged from original Ctx-based logic) ──

fn compile_into_ctx(p: &Program, c: &mut Ctx) {
    for s in &p.stmts { stmt(s, c); }
    c.cur.push_op(Opcode::Return);
    c.funcs.push(FunctionDef {
        name: "<module>".into(), bytecode: c.cur.take(),
        n_locals: 64, n_params: 0, is_variadic: false, n_captures: 0,
        return_type: None, param_types: vec![], jit_code: None, hotness: 0,
    });
}

fn opb(op: Opcode) -> u8 { op as u8 }

fn stmt(s: &Stmt, c: &mut Ctx) {
    match s {
        Stmt::Expr(e) => {
            let is_print = matches!(e, Expr::Call { func, .. } if matches!(&**func, Expr::Name(n) if n == "print"));
            expr(e, c);
            if !is_print { c.cur.push_op(Opcode::Pop); }
        }
        Stmt::Assign { targets, value, aug: None } => {
            for t in targets {
                match t {
                    Expr::Name(n) => {
                        expr(value, c);
                        c.cur.push_op16(Opcode::StoreGlobal, intern(&mut c.module, n));
                    }
                    Expr::Attribute { value: obj, attr } => {
                        expr(obj, c);
                        expr(value, c);
                        c.cur.push_op16(Opcode::SetAttr, intern(&mut c.module, attr));
                    }
                    _ => {
                        expr(value, c); c.cur.push_op(Opcode::Pop);
                    }
                }
            }
        }
        Stmt::Assign { targets, value, aug: Some(op) } => {
            for t in targets {
                if let Expr::Name(n) = t {
                    let sid = intern(&mut c.module, n);
                    c.cur.push_op16(Opcode::LoadGlobal, sid);
                    expr(value, c);
                    c.cur.push(bop(*op));
                    c.cur.push_op16(Opcode::StoreGlobal, sid);
                }
            }
        }
        Stmt::AnnAssign { target, value, .. } => {
            if let (Expr::Name(n), Some(v)) = (target, value) {
                expr(v, c);
                c.cur.push_op16(Opcode::StoreGlobal, intern(&mut c.module, n));
            }
        }
        Stmt::If { test, body, orelse } => {
            expr(test, c);
            let j1 = c.cur.len(); c.cur.push_op16(Opcode::JumpIfFalse, 0);
            for s in body { stmt(s, c); }
            let j2 = c.cur.len(); c.cur.push_op16(Opcode::Jump, 0);
            let t = c.cur.len() as i16 - j1 as i16 - 3;
            c.cur.patch_i16(j1 + 1, t);
            for s in orelse { stmt(s, c); }
            c.cur.patch_i16(j2 + 1, c.cur.len() as i16 - j2 as i16 - 3);
        }
        Stmt::While { test, body, orelse: _ } => {
            let ls = c.cur.len();
            expr(test, c);
            let ex = c.cur.len(); c.cur.push_op16(Opcode::JumpIfFalse, 0);
            for s in body { stmt(s, c); }
            c.cur.push_op16(Opcode::Jump, (ls as i16 - c.cur.len() as i16 - 3) as u16);
            c.cur.patch_i16(ex + 1, c.cur.len() as i16 - ex as i16 - 3);
        }
        Stmt::For { target, iter, body, orelse: _ } => {
            let id = c.fc; c.push_fc();
            let its = intern(&mut c.module, &format!("__it_{id}__"));
            let iis = intern(&mut c.module, &format!("__i_{id}__"));
            expr(iter, c);
            c.cur.push_op16(Opcode::StoreGlobal, its);
            c.cur.push_op(Opcode::PushI32); c.cur.push_i32(0);
            c.cur.push_op16(Opcode::StoreGlobal, iis);
            let ls = c.cur.len();
            c.cur.push_op16(Opcode::LoadGlobal, iis);
            c.cur.push_op16(Opcode::LoadGlobal, its);
            c.cur.push_op(Opcode::ArrayLen);
            c.cur.push_op(Opcode::Lt);
            let ex = c.cur.len(); c.cur.push_op16(Opcode::JumpIfFalse, 0);
            c.cur.push_op16(Opcode::LoadGlobal, its);
            c.cur.push_op16(Opcode::LoadGlobal, iis);
            c.cur.push_op(Opcode::ArrayGet);
            if let Expr::Name(v) = target {
                c.cur.push_op16(Opcode::StoreGlobal, intern(&mut c.module, v));
            }
            for s in body { stmt(s, c); }
            c.cur.push_op16(Opcode::LoadGlobal, iis);
            c.cur.push_op(Opcode::PushI32); c.cur.push_i32(1);
            c.cur.push_op(Opcode::Add);
            c.cur.push_op16(Opcode::StoreGlobal, iis);
            c.cur.push_op16(Opcode::Jump, (ls as i16 - c.cur.len() as i16 - 3) as u16);
            c.cur.patch_i16(ex + 1, c.cur.len() as i16 - ex as i16 - 3);
        }
        Stmt::Return(Some(e)) => { expr(e, c); c.cur.push_op(Opcode::ReturnValue); }
        Stmt::Return(None) => { c.cur.push_op(Opcode::Return); }
        Stmt::FuncDef { name, args, body, .. } => {
            let saved = c.cur.take();
            c.cur.push_op16(Opcode::AllocFrame, 16);
            for (i, a) in args.args.iter().enumerate() {
                c.cur.push_op(Opcode::LoadLocal); c.cur.push_u16(i as u16);
                c.cur.push_op16(Opcode::StoreGlobal, intern(&mut c.module, &a.arg));
            }
            for s in body { stmt(s, c); }
            c.cur.push_op(Opcode::Return);
            let inner_bc = c.cur.take();
            let fi = c.nf();
            c.funcs.push(FunctionDef {
                name: name.clone(), bytecode: inner_bc,
                n_locals: args.args.len() as u16 + 16,
                n_params: args.args.len() as u16, is_variadic: false, n_captures: 0,
                return_type: None, param_types: vec![], jit_code: None, hotness: 0,
            });
            c.cur = Buf(saved);
            c.cur.push_op16(Opcode::MakeClosure, fi);
            c.cur.push_op16(Opcode::StoreGlobal, intern(&mut c.module, name));
        }
        Stmt::ClassDef { name, body, .. } => {
            // Phase 1: compile all methods, collect func_refs
            let mut methods: Vec<(String, FuncRef)> = Vec::new();
            for s in body {
                if let Stmt::FuncDef { name: fn_name, args, body: fn_body, decorators: _, returns: _ } = s {
                    let saved = c.cur.take();
                    c.cur.push_op16(Opcode::AllocFrame, 16);
                    // First param is self (index 0)
                    for (i, a) in args.args.iter().enumerate() {
                        c.cur.push_op(Opcode::LoadLocal); c.cur.push_u16(i as u16);
                        c.cur.push_op16(Opcode::StoreGlobal, intern(&mut c.module, &a.arg));
                    }
                    for s in fn_body { stmt(s, c); }
                    c.cur.push_op(Opcode::Return);
                    let inner_bc = c.cur.take();
                    let fi = c.nf();
                    c.funcs.push(FunctionDef {
                        name: fn_name.clone(), bytecode: inner_bc,
                        n_locals: args.args.len() as u16 + 17,
                        n_params: args.args.len() as u16, is_variadic: false,
                        n_captures: 0, return_type: None, param_types: vec![],
                        jit_code: None, hotness: 0,
                    });
                    c.cur = Buf(saved);
                    methods.push((fn_name.clone(), fi as FuncRef));
                }
            }

            // Phase 2: extract field names from method bodies (self.x = ...)
            fn collect_fields(stmts: &[Stmt], fields: &mut Vec<String>) {
                for s in stmts {
                    match s {
                        Stmt::Assign { ref targets, .. } => {
                            for t in targets {
                                if let Expr::Attribute { value: obj, attr } = t {
                                    if let Expr::Name(ref n) = &**obj {
                                        if n == "self" && !fields.contains(attr) {
                                            fields.push(attr.clone());
                                        }
                                    }
                                }
                            }
                        }
                        Stmt::If { body, orelse, .. } => {
                            collect_fields(body, fields);
                            collect_fields(orelse, fields);
                        }
                        Stmt::While { body, .. } => collect_fields(body, fields),
                        Stmt::For { body, .. } => collect_fields(body, fields),
                        Stmt::With { body, .. } => collect_fields(body, fields),
                        Stmt::Try { body, orelse, .. } => {
                            collect_fields(body, fields);
                            collect_fields(orelse, fields);
                        }
                        _ => {}
                    }
                }
            }
            let mut fields: Vec<String> = Vec::new();
            for s in body {
                if let Stmt::FuncDef { body: fn_body, .. } = s {
                    collect_fields(fn_body, &mut fields);
                }
            }

            // Phase 3: emit class construction bytecode
            let name_sid = intern(&mut c.module, name);
            c.cur.push_op16(Opcode::NewNamespace, name_sid);
            c.cur.push_op(Opcode::NewClass);

            // Add fields to the class (Dup class handle for each)
            for field_name in &fields {
                c.cur.push_op(Opcode::Dup);
                c.cur.push_op16(Opcode::ClassAddField, intern(&mut c.module, field_name));
            }

            // Add methods to the class
            for (method_name, fi) in &methods {
                c.cur.push_op(Opcode::Dup); // dup class handle for the method
                c.cur.push_op16(Opcode::MakeClosure, *fi as u16);
                // Bit 15 = constructor flag (VM checks this, not magic name)
                // py2uc marks __init__ as constructor; other compilers set their own
                let is_init = method_name == "__init__";
                let name_sid = intern(&mut c.module, method_name);
                let operand = if is_init { name_sid | 0x8000 } else { name_sid };
                // Emit ClassAddMethod with possible constructor flag in bit 15
                c.cur.push(Opcode::ClassAddMethod as u8);
                c.cur.push_u16(operand);
            }

            c.cur.push_op16(Opcode::StoreGlobal, name_sid);
        }
        Stmt::Import { names } => {
            for alias in names {
                let sid = intern(&mut c.module, &alias.name);
                c.cur.push_op16(Opcode::Import, sid);
                // Store module handle as a global so user can access it by name
                c.cur.push_op16(Opcode::StoreGlobal, intern(&mut c.module, &alias.name));
            }
        }
        Stmt::ImportFrom { module: Some(module_name), names, level: _ } => {
            let mod_sid = intern(&mut c.module, module_name);
            c.cur.push_op16(Opcode::Import, mod_sid);
            let mod_id_local = c.fc; c.push_fc();
            let local_name = format!("__mod_{mod_id_local}__");
            let local_sid = intern(&mut c.module, &local_name);
            c.cur.push_op16(Opcode::StoreGlobal, local_sid);
            for alias in names {
                c.cur.push_op16(Opcode::LoadGlobal, local_sid);
                let name_sid = intern(&mut c.module, &alias.name);
                c.cur.push_op16(Opcode::ImportFunc, name_sid);
                let store_name = alias.asname.as_ref().unwrap_or(&alias.name);
                c.cur.push_op16(Opcode::StoreGlobal, intern(&mut c.module, store_name));
            }
        }
        Stmt::ImportFrom { module: None, .. } => {}
        Stmt::Pass => {}
        Stmt::Break => { c.cur.push_op16(Opcode::Jump, 0); }
        Stmt::Continue => { c.cur.push_op16(Opcode::Jump, 0); }
        Stmt::With { items, body } => {
            for wi in items { expr(&wi.context_expr, c); c.cur.push_op(Opcode::Pop); }
            for s in body { stmt(s, c); }
        }
        Stmt::Try { body, .. } => { for s in body { stmt(s, c); } }
        _ => {}
    }
}

fn expr(e: &Expr, c: &mut Ctx) {
    match e {
        Expr::Name(n) => { let sid = intern(&mut c.module, n); c.cur.push_op16(Opcode::LoadGlobal, sid); }
        Expr::Int(v) => { c.cur.push_op(Opcode::PushI32); c.cur.push_i32(*v as i32); }
        Expr::Float(v) => { c.cur.push_op(Opcode::PushF64); c.cur.push_f64(*v); }
        Expr::Str(s) => { let sid = intern(&mut c.module, s); c.cur.push_op16(Opcode::PushString, sid); }
        Expr::Bool(b) => { c.cur.push_op(if *b { Opcode::PushTrue } else { Opcode::PushFalse }); }
        Expr::None_ => { c.cur.push_op(Opcode::PushNil); }
        Expr::Lambda { args, body } => {
            let saved = c.cur.take();
            let n_args = args.args.len() as u16;
            for (i, a) in args.args.iter().enumerate() {
                c.cur.push_op(Opcode::LoadLocal); c.cur.push_u16(i as u16);
                c.cur.push_op16(Opcode::StoreGlobal, intern(&mut c.module, &a.arg));
            }
            expr(body, c);
            c.cur.push_op(Opcode::ReturnValue);
            let inner_bc = c.cur.take();
            let fi = c.nf();
            c.funcs.push(FunctionDef {
                name: "<lambda>".into(), bytecode: inner_bc,
                n_locals: n_args + 16, n_params: n_args,
                is_variadic: false, n_captures: 0,
                return_type: None, param_types: vec![], jit_code: None, hotness: 0,
            });
            c.cur = Buf(saved);
            c.cur.push_op16(Opcode::MakeClosure, fi);
        }
        Expr::List(items) | Expr::Tuple(items) | Expr::Set(items) => {
            for item in items { expr(item, c); }
            c.cur.push_op16(Opcode::NewArray, items.len() as u16);
        }
        Expr::Dict(items) => {
            for (k, v) in items { expr(k, c); expr(v, c); }
            c.cur.push_op(Opcode::NewMap);
        }
        Expr::BinOp { left, op, right } => { expr(left, c); expr(right, c); c.cur.push(bop(*op)); }
        Expr::UnaryOp { op, operand } => {
            expr(operand, c);
            c.cur.push(match op { UnaryOp::USub => opb(Opcode::Neg), UnaryOp::Not => opb(Opcode::Not), _ => 0 });
        }
        Expr::Compare { left, ops, comparators } => {
            expr(left, c);
            for (i, op) in ops.iter().enumerate() { expr(&comparators[i], c); c.cur.push(cop(*op)); }
        }
        Expr::BoolOp { op, values } => {
            expr(&values[0], c);
            for v in &values[1..] { expr(v, c); c.cur.push(if *op == BoolOp::And { opb(Opcode::And) } else { opb(Opcode::Or) }); }
        }
        Expr::Call { func, args, keywords: kw, .. } => {
            if let Expr::Name(n) = &**func {
                match n.as_str() {
                    // print removed from opcodes — use utencore.println native function
                    "len" => { for a in args.iter() { expr(a, c); } c.cur.push_op(Opcode::ArrayLen); return; }
                    "int" => { for a in args.iter() { expr(a, c); } c.cur.push_op(Opcode::ToI32); c.cur.push_op(Opcode::ToI64); return; }
                    "str" => { for a in args.iter() { expr(a, c); } c.cur.push_op(Opcode::ToString); return; }
                    "range" => {
                        for a in args.iter() { expr(a, c); }
                        c.cur.push_op16(Opcode::NewArray, args.len() as u16);
                        return;
                    }
                    "enumerate" | "zip" | "list" | "sorted" | "reversed" | "map" | "filter" => {
                        for a in args.iter() { expr(a, c); }
                        for _ in 0..args.len() { c.cur.push_op(Opcode::Pop); }
                        c.cur.push_op16(Opcode::NewArray, 0);
                        return;
                    }
                    _ => {}
                }
            }
            if let Expr::Attribute { value: obj, attr } = &**func {
                // Compile-time handling for known type methods
                match attr.as_str() {
                    // String methods removed from opcodes — use library calls
                    // "upper", "lower", "strip", "trim" fall through to attribute access
                    "len" => { expr(obj, c); c.cur.push_op(Opcode::StrLen); return; }
                    _ => {}
                }
                // Generic attribute call: push args, push arg count (inc self), push method, CallValue
                for a in args.iter().rev() { expr(a, c); }
                for k in kw { expr(&k.value, c); }
                c.cur.push_op(Opcode::PushI32); c.cur.push_i32((args.len() + kw.len()) as i32);
                expr(obj, c);
                c.cur.push_op16(Opcode::GetField, intern(&mut c.module, attr));
                c.cur.push_op(Opcode::CallValue);
                return;
            }
            for a in args.iter().rev() { expr(a, c); }
            for k in kw { expr(&k.value, c); }
            c.cur.push_op(Opcode::PushI32); c.cur.push_i32((args.len() + kw.len()) as i32);
            expr(func, c);
            c.cur.push_op(Opcode::CallValue);
        }
        Expr::Attribute { value, attr } => {
            expr(value, c);
            c.cur.push_op16(Opcode::GetField, intern(&mut c.module, attr));
        }
        Expr::Subscript { value, slice } => {
            expr(value, c);
            match &**slice {
                Slice::Index(idx) => { expr(idx, c); c.cur.push_op(Opcode::ArrayGet); }
                _ => { c.cur.push_op(Opcode::PushI32); c.cur.push_i32(0); c.cur.push_op(Opcode::ArrayGet); }
            }
        }
        Expr::FString(parts) => {
            let n = parts.len();
            if n == 0 { c.cur.push_op16(Opcode::PushString, intern(&mut c.module, "")); return; }
            for p in parts {
                match p {
                    FStringPart::Literal(s) => { let sid = intern(&mut c.module, s); c.cur.push_op16(Opcode::PushString, sid); }
                    FStringPart::Expr(e) => { expr(e, c); c.cur.push_op(Opcode::ToString); }
                }
            }
            for _ in 1..n { c.cur.push_op(Opcode::Add); }
        }
        Expr::IfExpr { test, body, orelse } => {
            expr(test, c);
            let j1 = c.cur.len(); c.cur.push_op16(Opcode::JumpIfFalse, 0);
            expr(body, c);
            let j2 = c.cur.len(); c.cur.push_op16(Opcode::Jump, 0);
            c.cur.patch_i16(j1 + 1, c.cur.len() as i16 - j1 as i16 - 3);
            expr(orelse, c);
            c.cur.patch_i16(j2 + 1, c.cur.len() as i16 - j2 as i16 - 3);
        }
        Expr::NamedExpr { target, value } => {
            expr(value, c);
            c.cur.push_op(Opcode::Dup);
            if let Expr::Name(n) = &**target {
                let sid = intern(&mut c.module, n);
                c.cur.push_op16(Opcode::StoreGlobal, sid);
            }
        }
        _ => { c.cur.push_op(Opcode::PushNil); }
    }
}

fn bop(o: BinOp) -> u8 {
    match o { BinOp::Add=>opb(Opcode::Add), BinOp::Sub=>opb(Opcode::Sub), BinOp::Mul=>opb(Opcode::Mul),
        BinOp::Div=>opb(Opcode::Div), BinOp::Mod=>opb(Opcode::Mod), BinOp::FloorDiv=>opb(Opcode::Div),
        BinOp::Pow=>opb(Opcode::Mul), _=>opb(Opcode::Add) }
}
fn cop(o: CmpOp) -> u8 {
    match o { CmpOp::Eq=>opb(Opcode::Eq), CmpOp::NotEq=>opb(Opcode::Ne), CmpOp::Lt=>opb(Opcode::Lt),
        CmpOp::LtE=>opb(Opcode::Le), CmpOp::Gt=>opb(Opcode::Gt), CmpOp::GtE=>opb(Opcode::Ge), _=>opb(Opcode::Eq) }
}
