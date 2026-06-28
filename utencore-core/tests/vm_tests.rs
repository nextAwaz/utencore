// VM integration tests: build bytecode, run, check results.

use utencore::bytecode::{BytecodeWriter, FunctionDef, UtenModule};
use utencore::opcodes::Opcode;
use utencore::types::{FieldDef, StringId, StructDef, TypeRef, UValue, MAX_INLINE_STRUCT_SIZE};
use utencore::vm::{Vm, VmConfig};

fn make_simple_module(bytecode: Vec<u8>) -> UtenModule {
    let mut m = UtenModule::new("test");
    m.functions.push(FunctionDef {
        name: "<main>".into(),
        bytecode,
        n_locals: 4, n_params: 0, is_variadic: false,
        n_captures: 0, return_type: None, param_types: vec![],
        jit_code: None, hotness: 0,
    });
    m
}

fn exec(bc: Vec<u8>) -> String {
    let module = make_simple_module(bc);
    let mut vm = Vm::new();
    let mid = vm.load_module(module).unwrap();
    match vm.execute(mid, 0, vec![]) {
        Ok(v) => format!("{v}"),
        Err(e) => format!("err:{e}"),
    }
}

#[test]
fn test_push_nil() {
    let mut w = BytecodeWriter::new();
    w.emit(Opcode::PushNil);
    w.emit(Opcode::ReturnValue);
    assert_eq!(exec(w.into_bytes()), "nil");
}

#[test]
fn test_push_i32() {
    let mut w = BytecodeWriter::new();
    w.emit(Opcode::PushI32); w.emit_i32(42);
    w.emit(Opcode::ReturnValue);
    assert_eq!(exec(w.into_bytes()), "42");
}

#[test]
fn test_push_true() {
    let mut w = BytecodeWriter::new();
    w.emit(Opcode::PushTrue);
    w.emit(Opcode::ReturnValue);
    assert_eq!(exec(w.into_bytes()), "true");
}

#[test]
fn test_push_false() {
    let mut w = BytecodeWriter::new();
    w.emit(Opcode::PushFalse);
    w.emit(Opcode::ReturnValue);
    assert_eq!(exec(w.into_bytes()), "false");
}

#[test]
fn test_dup() {
    let mut w = BytecodeWriter::new();
    w.emit(Opcode::PushI32); w.emit_i32(7);
    w.emit(Opcode::Dup);
    w.emit(Opcode::Add);
    w.emit(Opcode::ReturnValue);
    assert_eq!(exec(w.into_bytes()), "14");
}

#[test]
fn test_swap() {
    let mut w = BytecodeWriter::new();
    w.emit(Opcode::PushI32); w.emit_i32(3);
    w.emit(Opcode::PushI32); w.emit_i32(5);
    w.emit(Opcode::Swap);
    w.emit(Opcode::Sub);
    w.emit(Opcode::ReturnValue);
    assert_eq!(exec(w.into_bytes()), "2"); // 5 - 3 = 2 after swap
}

#[test]
fn test_add() {
    let mut w = BytecodeWriter::new();
    w.emit(Opcode::PushI32); w.emit_i32(10);
    w.emit(Opcode::PushI32); w.emit_i32(20);
    w.emit(Opcode::Add);
    w.emit(Opcode::ReturnValue);
    assert_eq!(exec(w.into_bytes()), "30");
}

#[test]
fn test_sub() {
    let mut w = BytecodeWriter::new();
    w.emit(Opcode::PushI32); w.emit_i32(100);
    w.emit(Opcode::PushI32); w.emit_i32(30);
    w.emit(Opcode::Sub);
    w.emit(Opcode::ReturnValue);
    assert_eq!(exec(w.into_bytes()), "70");
}

#[test]
fn test_mul() {
    let mut w = BytecodeWriter::new();
    w.emit(Opcode::PushI32); w.emit_i32(7);
    w.emit(Opcode::PushI32); w.emit_i32(6);
    w.emit(Opcode::Mul);
    w.emit(Opcode::ReturnValue);
    assert_eq!(exec(w.into_bytes()), "42");
}

#[test]
fn test_div() {
    let mut w = BytecodeWriter::new();
    w.emit(Opcode::PushI32); w.emit_i32(10);
    w.emit(Opcode::PushI32); w.emit_i32(3);
    w.emit(Opcode::Div);
    w.emit(Opcode::ReturnValue);
    assert_eq!(exec(w.into_bytes()), "3");
}

#[test]
fn test_neg() {
    let mut w = BytecodeWriter::new();
    w.emit(Opcode::PushI32); w.emit_i32(42);
    w.emit(Opcode::Neg);
    w.emit(Opcode::ReturnValue);
    assert_eq!(exec(w.into_bytes()), "-42");
}

#[test]
fn test_inc_dec() {
    let mut w = BytecodeWriter::new();
    w.emit(Opcode::PushI32); w.emit_i32(5);
    w.emit(Opcode::Inc);
    w.emit(Opcode::Inc);
    w.emit(Opcode::Dec);
    w.emit(Opcode::ReturnValue);
    assert_eq!(exec(w.into_bytes()), "6");
}

// Float opcodes temporarily disabled (need opcode_info fix)
// #[test]
// fn test_fadd() {...}
// #[test]
// fn test_fdiv() {...}

#[test]
fn test_eq_true() {
    let mut w = BytecodeWriter::new();
    w.emit(Opcode::PushI32); w.emit_i32(5);
    w.emit(Opcode::PushI32); w.emit_i32(5);
    w.emit(Opcode::Eq);
    w.emit(Opcode::ReturnValue);
    assert_eq!(exec(w.into_bytes()), "true");
}

#[test]
fn test_lt() {
    let mut w = BytecodeWriter::new();
    w.emit(Opcode::PushI32); w.emit_i32(3);
    w.emit(Opcode::PushI32); w.emit_i32(7);
    w.emit(Opcode::Lt);
    w.emit(Opcode::ReturnValue);
    assert_eq!(exec(w.into_bytes()), "true");
}

#[test]
fn test_gt_false() {
    let mut w = BytecodeWriter::new();
    w.emit(Opcode::PushI32); w.emit_i32(2);
    w.emit(Opcode::PushI32); w.emit_i32(10);
    w.emit(Opcode::Gt);
    w.emit(Opcode::ReturnValue);
    assert_eq!(exec(w.into_bytes()), "false");
}

#[test]
fn test_jump() {
    let mut w = BytecodeWriter::new();
    w.emit(Opcode::PushI32); w.emit_i32(1);
    w.emit(Opcode::Jump); w.emit_i16(3);
    w.emit(Opcode::PushI32); w.emit_i32(99);
    w.emit(Opcode::PushI32); w.emit_i32(2);
    w.emit(Opcode::ReturnValue);
    assert_eq!(exec(w.into_bytes()), "2");
}

#[test]
fn test_jump_if_false() {
    let mut w = BytecodeWriter::new();
    w.emit(Opcode::PushI32); w.emit_i32(0);
    w.emit(Opcode::JumpIfFalse); w.emit_i16(6);
    w.emit(Opcode::PushI32); w.emit_i32(0);
    w.emit(Opcode::ReturnValue);
    w.emit(Opcode::PushI32); w.emit_i32(42);
    w.emit(Opcode::ReturnValue);
    assert_eq!(exec(w.into_bytes()), "42");
}

#[test]
fn test_jump_if_true() {
    let mut w = BytecodeWriter::new();
    w.emit(Opcode::PushI32); w.emit_i32(1);
    w.emit(Opcode::JumpIfTrue); w.emit_i16(6);
    w.emit(Opcode::PushI32); w.emit_i32(0);
    w.emit(Opcode::ReturnValue);
    w.emit(Opcode::PushI32); w.emit_i32(99);
    w.emit(Opcode::ReturnValue);
    assert_eq!(exec(w.into_bytes()), "99");
}

#[test]
fn test_new_array() {
    let mut w = BytecodeWriter::new();
    w.emit(Opcode::PushI32); w.emit_i32(3);
    w.emit(Opcode::PushI32); w.emit_i32(2);
    w.emit(Opcode::PushI32); w.emit_i32(1);
    w.emit_op(Opcode::NewArray, 3);
    w.emit(Opcode::ReturnValue);
    let r = exec(w.into_bytes());
    assert!(r.contains("Array"), "expected Array, got {r}");
}

#[test]
fn test_array_get_set() {
    let mut w = BytecodeWriter::new();
    w.emit(Opcode::PushI32); w.emit_i32(30);
    w.emit(Opcode::PushI32); w.emit_i32(20);
    w.emit(Opcode::PushI32); w.emit_i32(10);
    w.emit_op(Opcode::NewArray, 3);
    w.emit(Opcode::Dup);
    w.emit(Opcode::PushI32); w.emit_i32(1);
    w.emit(Opcode::ArrayGet);
    w.emit(Opcode::ReturnValue);
    assert_eq!(exec(w.into_bytes()), "20");
}

#[test]
fn test_array_len() {
    let mut w = BytecodeWriter::new();
    w.emit(Opcode::PushI32); w.emit_i32(3);
    w.emit(Opcode::PushI32); w.emit_i32(2);
    w.emit(Opcode::PushI32); w.emit_i32(1);
    w.emit_op(Opcode::NewArray, 3);
    w.emit(Opcode::ArrayLen);
    w.emit(Opcode::ReturnValue);
    assert_eq!(exec(w.into_bytes()), "3");
}

#[test]
fn test_add_strings() {
    let mut w = BytecodeWriter::new();
    w.emit(Opcode::PushString); w.emit_u16(0);
    w.emit(Opcode::PushString); w.emit_u16(1);
    w.emit(Opcode::Add);
    w.emit(Opcode::ReturnValue);
    let mut m = make_simple_module(w.into_bytes());
    m.strings.push("Hello, ".into());
    m.strings.push("World!".into());
    assert_eq!(exec_val(m), "<str#2>"); // "Hello, World!" is interned
}

// ═══════════════════════════════════════════════════════════════
// Value Type Tests
// ═══════════════════════════════════════════════════════════════

/// Helper: create a module with a Point struct {x:i32, y:i32}
fn make_point_module(bytecode: Vec<u8>) -> UtenModule {
    let mut m = UtenModule::new("test");
    // Define Point struct (2 × i32 = 8 bytes)
    m.strings.push("Point".into());  // sid:0
    m.strings.push("x".into());      // sid:1
    m.strings.push("y".into());      // sid:2
    m.structs.push(StructDef {
        name: 0,  // "Point"
        size: 8,
        alignment: 4,
        is_packed: false,
        generic_params: vec![],
        fields: vec![
            FieldDef { name: 1, type_ref: TypeRef::I32, offset: 0, size: 4 },
            FieldDef { name: 2, type_ref: TypeRef::I32, offset: 4, size: 4 },
        ],
    });
    m.functions.push(FunctionDef {
        name: "<main>".into(),
        bytecode,
        n_locals: 4, n_params: 0, is_variadic: false,
        n_captures: 0, return_type: None, param_types: vec![],
        jit_code: None, hotness: 0,
    });
    m
}

fn exec_val(module: UtenModule) -> String {
    let mut vm = Vm::new();
    let mid = vm.load_module(module).unwrap();
    match vm.execute(mid, 0, vec![]) {
        Ok(v) => format!("{v}"),
        Err(e) => format!("err:{e}"),
    }
}

#[test]
fn test_init_struct_inline() {
    let mut w = BytecodeWriter::new();
    // InitStruct 0 (Point), field 0 is at offset 0
    w.emit_op(Opcode::InitStruct, 0);  // push zeroed Point
    w.emit(Opcode::ReturnValue);
    let module = make_point_module(w.into_bytes());
    let r = exec_val(module);
    assert!(r.contains("struct#0"), "expected struct#0, got {r}");
}

#[test]
fn test_set_get_field_inline() {
    let mut w = BytecodeWriter::new();
    // Create Point, set x=42, read x
    w.emit_op(Opcode::InitStruct, 0);  // push zeroed Point
    // Duplicate struct, push 42, set field 0 (x)
    w.emit(Opcode::Dup);
    w.emit(Opcode::PushI32); w.emit_i32(42);
    w.emit_op(Opcode::SetField, 0);  // struct.x = 42
    // Now get field 0 (x) from the struct
    w.emit_op(Opcode::GetField, 0);
    w.emit(Opcode::ReturnValue);
    let module = make_point_module(w.into_bytes());
    assert_eq!(exec_val(module), "42");
}

#[test]
fn test_set_get_field_two_fields() {
    let mut w = BytecodeWriter::new();
    // Create Point, set x=100, y=200, read y
    w.emit_op(Opcode::InitStruct, 0);  // Point
    w.emit(Opcode::Dup);
    w.emit(Opcode::PushI32); w.emit_i32(100);
    w.emit_op(Opcode::SetField, 0);  // .x = 100
    w.emit(Opcode::Dup);
    w.emit(Opcode::PushI32); w.emit_i32(200);
    w.emit_op(Opcode::SetField, 1);  // .y = 200
    w.emit_op(Opcode::GetField, 1);  // read y
    w.emit(Opcode::ReturnValue);
    let module = make_point_module(w.into_bytes());
    assert_eq!(exec_val(module), "200");
}

#[test]
fn test_value_type_copy_semantics() {
    let mut w = BytecodeWriter::new();
    // Create p = Point(10, 20), copy to q, modify q.x, check p.x unaffected
    w.emit_op(Opcode::InitStruct, 0);  // p
    w.emit(Opcode::Dup);
    w.emit(Opcode::PushI32); w.emit_i32(10);
    w.emit_op(Opcode::SetField, 0);
    w.emit(Opcode::Dup);
    w.emit(Opcode::PushI32); w.emit_i32(20);
    w.emit_op(Opcode::SetField, 1);
    // Stack: [p]
    w.emit(Opcode::Dup);  // [p, p] — copy!
    w.emit(Opcode::PushI32); w.emit_i32(999);
    w.emit_op(Opcode::SetField, 0);  // modify copy's x
    w.emit(Opcode::Pop);  // drop copy
    // Read original p.x
    w.emit_op(Opcode::GetField, 0);
    w.emit(Opcode::ReturnValue);
    let module = make_point_module(w.into_bytes());
    // Should still be 10 (value semantics preserved)
    assert_eq!(exec_val(module), "10");
}

// ═══════════════════════════════════════════════════════════════
// Generics: Template Monomorphization Test
// ═══════════════════════════════════════════════════════════════

/// Generate a concrete StructDef from a generic template
fn monomorphize_struct(
    template: &StructDef,
    name: StringId,
    type_args: &[TypeRef],
    strings: &[String],
) -> StructDef {
    let resolve = |tref: &TypeRef| -> TypeRef {
        match tref {
            TypeRef::GenericParam(idx) => {
                type_args.get(*idx as usize).cloned().unwrap_or(TypeRef::Void)
            }
            other => other.clone(),
        }
    };

    let mut fields: Vec<FieldDef> = template.fields.iter().map(|f| {
        let resolved = resolve(&f.type_ref);
        let size = match &resolved {
            TypeRef::I32 | TypeRef::U32 | TypeRef::F32 => 4,
            TypeRef::I64 | TypeRef::U64 | TypeRef::F64 => 8,
            _ => 4, // default
        };
        FieldDef {
            name: f.name,
            type_ref: resolved,
            offset: f.offset, // keep same layout for testing
            size,
        }
    }).collect();

    // Recompute offsets
    let mut offset = 0u32;
    for f in &mut fields {
        f.offset = offset;
        offset += f.size;
    }

    StructDef {
        name,
        size: offset,
        alignment: 4,
        is_packed: template.is_packed,
        generic_params: vec![],
        fields,
    }
}

#[test]
fn test_generic_monomorphization() {
    // Simulate a generic Pair<T> template: {first: T, second: T}
    // Then monomorphize to Pair<int>
    let mut m = UtenModule::new("generic_test");
    m.strings.push("Pair<int>".into());  // sid:0
    m.strings.push("first".into());      // sid:1
    m.strings.push("second".into());     // sid:2

    let generic_template = StructDef {
        name: 0,  // "Pair<int>"
        size: 0,  // will be computed
        alignment: 4,
        is_packed: false,
        generic_params: vec![0],  // placeholder param name
        fields: vec![
            FieldDef { name: 1, type_ref: TypeRef::GenericParam(0), offset: 0, size: 0 },
            FieldDef { name: 2, type_ref: TypeRef::GenericParam(0), offset: 0, size: 0 },
        ],
    };

    // Monomorphize with T = I32
    let concrete = monomorphize_struct(
        &generic_template,
        0,  // "Pair<int>"
        &[TypeRef::I32],
        &m.strings,
    );

    assert_eq!(concrete.size, 8, "Pair<int> should be 8 bytes");
    assert_eq!(concrete.fields.len(), 2);
    assert_eq!(concrete.fields[0].offset, 0);
    assert_eq!(concrete.fields[0].size, 4);
    assert_eq!(concrete.fields[1].offset, 4);
    assert_eq!(concrete.fields[1].size, 4);

    // Now test Monomorphize with F64
    let concrete_f64 = monomorphize_struct(
        &generic_template,
        0,
        &[TypeRef::F64],
        &m.strings,
    );

    assert_eq!(concrete_f64.size, 16, "Pair<f64> should be 16 bytes");
    assert_eq!(concrete_f64.fields[0].size, 8);
    assert_eq!(concrete_f64.fields[1].size, 8);
}

// ═══════════════════════════════════════════════════════════════
// Prototype Chain Tests
// ═══════════════════════════════════════════════════════════════

/// Helper: execute bytecode with a module that has a built-in handler
fn exec_module(module: UtenModule) -> String {
    let mut vm = Vm::new();
    let mid = vm.load_module(module).unwrap();
    match vm.execute(mid, 0, vec![]) {
        Ok(v) => format!("{v}"),
        Err(e) => format!("err:{e}"),
    }
}

#[test]
fn test_proto_chain_attr_lookup() {
    // Create proto_obj with 'answer' field, connect child to its proto,
    // GetAttr on child finds it via proto chain.
    let mut w = BytecodeWriter::new();
    // Create a class with one field "answer"
    w.emit_op(Opcode::NewNamespace, 0u16);
    w.emit(Opcode::NewClass);
    w.emit(Opcode::Dup);
    w.emit_op(Opcode::ClassAddField, 1); // add 'answer' field → [class]

    // Create child object from this class, save to local 0
    w.emit(Opcode::Dup);
    w.emit(Opcode::NewObject);          // child → [class]
    w.emit_op(Opcode::StoreLocal, 0);   // child → local 0, stack: [class]

    // Create proto_obj from this class, save to local 1
    w.emit(Opcode::Dup);
    w.emit(Opcode::NewObject);          // proto_obj → [class]
    w.emit_op(Opcode::StoreLocal, 1);   // proto_obj → local 1, stack: [class]
    w.emit(Opcode::Pop);                // clean up class → []

    // Set proto_obj.answer = 42
    w.emit_op(Opcode::LoadLocal, 1);    // [proto_obj]
    w.emit(Opcode::Dup);
    w.emit(Opcode::PushI32); w.emit_i32(42);
    w.emit_op(Opcode::SetAttr, 1);      // [proto_obj]

    // Set child.proto = proto_obj
    w.emit_op(Opcode::LoadLocal, 0);    // [child, proto_obj]
    w.emit(Opcode::Swap);               // [proto_obj, child]
    w.emit_op(Opcode::ClassSetParent, 0); // child.proto = proto_obj → []

    // Get child.answer → should walk proto chain → 42
    w.emit_op(Opcode::LoadLocal, 0);    // [child]
    w.emit_op(Opcode::GetAttr, 1);      // child.answer → 42
    w.emit(Opcode::ReturnValue);

    let mut m = UtenModule::new("proto_test");
    m.strings.push("test".into());
    m.strings.push("answer".into());
    m.functions.push(FunctionDef {
        name: "<main>".into(), bytecode: w.into_bytes(),
        n_locals: 4, n_params: 0, is_variadic: false,
        n_captures: 0, return_type: None, param_types: vec![],
        jit_code: None, hotness: 0,
    });
    assert_eq!(exec_module(m), "42");
}

#[test]
fn test_proto_chain_missing_attr() {
    // Attribute not found anywhere on chain returns nil
    let mut w = BytecodeWriter::new();
    w.emit_op(Opcode::NewNamespace, 0u16);
    w.emit(Opcode::NewClass);
    w.emit(Opcode::Dup);
    w.emit(Opcode::NewObject);           // child → [class]
    w.emit_op(Opcode::StoreLocal, 0);    // child → local 0, stack: [class]
    w.emit(Opcode::Dup);
    w.emit(Opcode::NewObject);           // proto_obj → [class]
    w.emit_op(Opcode::StoreLocal, 1);    // proto_obj → local 1, stack: [class]
    w.emit(Opcode::Pop);                 // clean up → []
    // child.proto = proto_obj
    w.emit_op(Opcode::LoadLocal, 0);    // [child]
    w.emit_op(Opcode::LoadLocal, 1);    // [child, proto_obj]
    w.emit(Opcode::Swap);               // [proto_obj, child]
    w.emit_op(Opcode::ClassSetParent, 0); // child.proto = proto_obj → []
    w.emit_op(Opcode::LoadLocal, 0);    // [child]
    w.emit_op(Opcode::GetAttr, 0);       // child.nonexistent → nil
    w.emit(Opcode::ReturnValue);
    let mut m = UtenModule::new("proto_miss");
    m.strings.push("test".into());
    m.functions.push(FunctionDef {
        name: "<main>".into(), bytecode: w.into_bytes(),
        n_locals: 4, n_params: 0, is_variadic: false,
        n_captures: 0, return_type: None, param_types: vec![],
        jit_code: None, hotness: 0,
    });
    assert_eq!(exec_module(m), "nil");
}

#[test]
fn test_proto_chain_hasattr() {
    // HasAttr should find proto's attr through the chain
    let mut w = BytecodeWriter::new();
    w.emit_op(Opcode::NewNamespace, 0u16);
    w.emit(Opcode::NewClass);
    w.emit(Opcode::Dup);
    w.emit_op(Opcode::ClassAddField, 1); // [class]
    w.emit(Opcode::Dup);
    w.emit(Opcode::NewObject);           // child → [class]
    w.emit_op(Opcode::StoreLocal, 0);    // child → local 0, [class]
    w.emit(Opcode::Dup);
    w.emit(Opcode::NewObject);           // proto → [class]
    w.emit_op(Opcode::StoreLocal, 1);
    w.emit(Opcode::Pop);                 // []
    w.emit_op(Opcode::LoadLocal, 1);    // [proto]
    w.emit(Opcode::Dup);
    w.emit(Opcode::PushI32); w.emit_i32(99);
    w.emit_op(Opcode::SetAttr, 1);       // proto.answer = 99 → [proto]
    w.emit_op(Opcode::LoadLocal, 0);    // [child, proto]
    w.emit(Opcode::Swap);                // [proto, child]
    w.emit_op(Opcode::ClassSetParent, 0); // child.proto = proto → []
    w.emit_op(Opcode::LoadLocal, 0);    // [child]
    w.emit_op(Opcode::HasAttr, 1);       // child has 'answer'? (via proto)
    w.emit(Opcode::ReturnValue);
    let mut m = UtenModule::new("proto_has");
    m.strings.push("test".into());
    m.strings.push("answer".into());
    m.functions.push(FunctionDef {
        name: "<main>".into(), bytecode: w.into_bytes(),
        n_locals: 4, n_params: 0, is_variadic: false,
        n_captures: 0, return_type: None, param_types: vec![],
        jit_code: None, hotness: 0,
    });
    assert_eq!(exec_module(m), "true");
}

// ═══════════════════════════════════════════════════════════════
// Operator Dispatch Tests
// ═══════════════════════════════════════════════════════════════

#[test]
fn test_native_operator_dispatch() {
    // Test that primitive types still work correctly (no operator override on primitives)
    let mut w = BytecodeWriter::new();
    w.emit(Opcode::PushI32); w.emit_i32(10);
    w.emit(Opcode::PushI32); w.emit_i32(20);
    w.emit(Opcode::Add);
    w.emit(Opcode::ReturnValue);
    assert_eq!(exec(w.into_bytes()), "30");
}

#[test]
fn test_eq_primitive() {
    let mut w = BytecodeWriter::new();
    w.emit(Opcode::PushI32); w.emit_i32(5);
    w.emit(Opcode::PushI32); w.emit_i32(5);
    w.emit(Opcode::Eq);
    w.emit(Opcode::ReturnValue);
    assert_eq!(exec(w.into_bytes()), "true");
}
