//! UtenCore VM comprehensive test suite.

#[cfg(test)]
mod tests {
    use crate::bytecode::{BytecodeWriter, FunctionDef, UtenModule};
    use crate::opcodes::Opcode;
    use crate::vm::{Vm, VmConfig};
    use utencore_types::*;

    fn exec_simple(bytecode: Vec<u8>) -> (Vm, UValue) {
        let mut vm = Vm::new();
        let mut m = UtenModule::new("test");
        m.functions.push(FunctionDef {
            name: "<main>".into(), bytecode,
            n_locals: 4, n_params: 0, is_variadic: false,
            n_captures: 0, return_type: None, param_types: vec![],
            jit_code: None, hotness: 0,
        });
        let mid = vm.load_module(m.clone()).unwrap();
        vm.run_module_init(mid).ok();
        let result = vm.execute(mid, 0, vec![]).unwrap();
        (vm, result)
    }

    fn numeric_str(vm: &Vm, val: &UValue) -> String {
        match val {
            UValue::Int32(i) => i.to_string(),
            UValue::Int64(i) => i.to_string(),
            UValue::Gc(h, ValueTag::BigInt) => {
                match vm.gc.get(*h) {
                    HeapObject::BigInt(bi) => format!("{bi}"),
                    _ => "?".to_string(),
                }
            }
            other => format!("{other}"),
        }
    }

    fn exec_int_binop(a: i32, b: i32, op: Opcode) -> String {
        let mut w = BytecodeWriter::new();
        w.emit(Opcode::PushI32); w.emit_i32(a);
        w.emit(Opcode::PushI32); w.emit_i32(b);
        w.emit(op);
        w.emit(Opcode::ReturnValue);
        let (vm, r) = exec_simple(w.into_bytes());
        numeric_str(&vm, &r)
    }

    fn exec_int_unop(v: i32, op: Opcode) -> String {
        let mut w = BytecodeWriter::new();
        w.emit(Opcode::PushI32); w.emit_i32(v);
        w.emit(op);
        w.emit(Opcode::ReturnValue);
        let (vm, r) = exec_simple(w.into_bytes());
        numeric_str(&vm, &r)
    }

    // -- 1. Stack Operations --

    #[test]
    fn test_push_nil() {
        let mut w = BytecodeWriter::new();
        w.emit(Opcode::PushNil);
        w.emit(Opcode::ReturnValue);
        let (_, r) = exec_simple(w.into_bytes());
        assert_eq!(r, UValue::Nil);
    }

    #[test]
    fn test_push_bool() {
        let mut w = BytecodeWriter::new();
        w.emit(Opcode::PushTrue);
        w.emit(Opcode::ReturnValue);
        let (_, r) = exec_simple(w.into_bytes());
        assert_eq!(r, UValue::Bool(true));
    }

    #[test]
    fn test_push_i32() {
        let mut w = BytecodeWriter::new();
        w.emit(Opcode::PushI32); w.emit_i32(42);
        w.emit(Opcode::ReturnValue);
        let (_, r) = exec_simple(w.into_bytes());
        assert_eq!(r, UValue::Int32(42));
    }

    #[test]
    fn test_pop() {
        let mut w = BytecodeWriter::new();
        w.emit(Opcode::PushI32); w.emit_i32(1);
        w.emit(Opcode::PushI32); w.emit_i32(2);
        w.emit(Opcode::Pop);
        w.emit(Opcode::ReturnValue);
        let (_, r) = exec_simple(w.into_bytes());
        assert_eq!(r, UValue::Int32(1));
    }

    #[test]
    fn test_popn() {
        let mut w = BytecodeWriter::new();
        w.emit(Opcode::PushI32); w.emit_i32(10);
        w.emit(Opcode::PushI32); w.emit_i32(20);
        w.emit(Opcode::PushI32); w.emit_i32(30);
        w.emit(Opcode::PopN); w.emit_u16(2);
        w.emit(Opcode::ReturnValue);
        let (_, r) = exec_simple(w.into_bytes());
        assert_eq!(r, UValue::Int32(10));
    }

    #[test]
    fn test_rot() {
        let mut w = BytecodeWriter::new();
        w.emit(Opcode::PushI32); w.emit_i32(1);
        w.emit(Opcode::PushI32); w.emit_i32(2);
        w.emit(Opcode::PushI32); w.emit_i32(3);
        w.emit(Opcode::Rot);
        w.emit(Opcode::ReturnValue);
        let (_, r) = exec_simple(w.into_bytes());
        assert_eq!(r, UValue::Int32(1));
    }

    // -- 2. Integer Arithmetic --

    #[test]
    fn test_add() { assert_eq!(exec_int_binop(10, 20, Opcode::Add), "30"); }

    #[test]
    fn test_sub() { assert_eq!(exec_int_binop(100, 30, Opcode::Sub), "70"); }

    #[test]
    fn test_mul() { assert_eq!(exec_int_binop(7, 8, Opcode::Mul), "56"); }

    #[test]
    fn test_div() { assert_eq!(exec_int_binop(100, 3, Opcode::Div), "33"); }

    #[test]
    fn test_div_negative() { assert_eq!(exec_int_binop(-10, 3, Opcode::Div), "-3"); }

    #[test]
    fn test_mod() { assert_eq!(exec_int_binop(10, 3, Opcode::Mod), "1"); }

    #[test]
    fn test_neg() { assert_eq!(exec_int_unop(42, Opcode::Neg), "-42"); }

    #[test]
    fn test_abs() { assert_eq!(exec_int_unop(-42, Opcode::Abs), "42"); }

    #[test]
    fn test_pow() { assert_eq!(exec_int_binop(2, 10, Opcode::Pow), "1024"); }

    #[test]
    fn test_checked_add() { assert_eq!(exec_int_binop(100, 200, Opcode::CheckedAdd), "300"); }

    #[test]
    fn test_wrapping_add() {
        let mut w = BytecodeWriter::new();
        w.emit(Opcode::PushI32); w.emit_i32(i32::MAX);
        w.emit(Opcode::PushI32); w.emit_i32(1);
        w.emit(Opcode::WrappingAdd);
        w.emit(Opcode::ReturnValue);
        let (vm, r) = exec_simple(w.into_bytes());
        assert_eq!(numeric_str(&vm, &r), (i64::from(i32::MAX) + 1i64).to_string());
    }

    // -- 3. Float Arithmetic --

    #[test]
    fn test_fadd() {
        let mut w = BytecodeWriter::new();
        w.emit(Opcode::PushF64); w.emit_f64(1.0);
        w.emit(Opcode::PushF64); w.emit_f64(2.0);
        w.emit(Opcode::FAdd);
        w.emit(Opcode::ReturnValue);
        let (_, r) = exec_simple(w.into_bytes());
        match r { UValue::Float64(v) => assert!((v - 3.0).abs() < 1e-10), _ => panic!("not float"), }
    }

    #[test]
    fn test_fneg() {
        let mut w = BytecodeWriter::new();
        w.emit(Opcode::PushF64); w.emit_f64(3.14);
        w.emit(Opcode::FNeg);
        w.emit(Opcode::ReturnValue);
        let (_, r) = exec_simple(w.into_bytes());
        match r { UValue::Float64(v) => assert!((v - (-3.14)).abs() < 1e-10), _ => panic!("not float"), }
    }

    // -- 4. Bitwise --

    #[test]
    fn test_bitand() { assert_eq!(exec_int_binop(0b1100, 0b1010, Opcode::BitAnd), "8"); }
    #[test]
    fn test_bitor() { assert_eq!(exec_int_binop(0b1100, 0b1010, Opcode::BitOr), "14"); }
    #[test]
    fn test_bitxor() { assert_eq!(exec_int_binop(0b1100, 0b1010, Opcode::BitXor), "6"); }
    #[test]
    fn test_bitnot() { assert_eq!(exec_int_unop(0, Opcode::BitNot), "-1"); }
    #[test]
    fn test_shl() { assert_eq!(exec_int_binop(1, 8, Opcode::Shl), "256"); }
    #[test]
    fn test_shr() { assert_eq!(exec_int_binop(256, 8, Opcode::Shr), "1"); }

    // -- 5. Comparison --

    #[test]
    fn test_eq() { assert_eq!(exec_int_binop(5, 5, Opcode::Eq), "true"); }
    #[test]
    fn test_ne() { assert_eq!(exec_int_binop(5, 6, Opcode::Ne), "true"); }
    #[test]
    fn test_lt() { assert_eq!(exec_int_binop(3, 5, Opcode::Lt), "true"); }
    #[test]
    fn test_gt() { assert_eq!(exec_int_binop(7, 5, Opcode::Gt), "true"); }

    #[test]
    fn test_and_or_not() {
        let mut w = BytecodeWriter::new();
        w.emit(Opcode::PushTrue);
        w.emit(Opcode::PushFalse);
        w.emit(Opcode::And);
        w.emit(Opcode::ReturnValue);
        let (_, r) = exec_simple(w.into_bytes());
        assert_eq!(r, UValue::Bool(false));
    }

    // -- 6. Type & Conversion --

    #[test]
    fn test_typeof() {
        let mut w = BytecodeWriter::new();
        w.emit(Opcode::PushI32); w.emit_i32(42);
        w.emit(Opcode::TypeOf);
        w.emit(Opcode::ReturnValue);
        let (_, r) = exec_simple(w.into_bytes());
        assert_eq!(r, UValue::Int32(ValueTag::Int32 as i32));
    }

    #[test]
    fn test_enum_create_match() {
        let mut w = BytecodeWriter::new();
        w.emit(Opcode::PushI32); w.emit_i32(42);
        w.emit(Opcode::PushI32); w.emit_i32(1);
        w.emit(Opcode::EnumCreate);
        w.emit(Opcode::EnumMatch);
        w.emit(Opcode::Pop);
        w.emit(Opcode::ReturnValue);
        let (_, r) = exec_simple(w.into_bytes());
        assert_eq!(r, UValue::Int32(1));
    }

    // -- 7. Control Flow --

    #[test]
    fn test_jump_forward() {
        let mut w = BytecodeWriter::new();
        w.emit(Opcode::PushI32); w.emit_i32(10);
        w.emit(Opcode::Jump); w.emit_i16(5);
        w.emit(Opcode::PushI32); w.emit_i32(42);
        w.emit(Opcode::PushI32); w.emit_i32(99);
        w.emit(Opcode::ReturnValue);
        let (vm, r) = exec_simple(w.into_bytes());
        assert_eq!(numeric_str(&vm, &r), "99");
    }

    #[test]
    fn test_jump_if_false() {
        let mut w = BytecodeWriter::new();
        w.emit(Opcode::PushFalse);
        w.emit(Opcode::JumpIfFalse); w.emit_i16(5);
        w.emit(Opcode::PushI32); w.emit_i32(42);
        w.emit(Opcode::PushI32); w.emit_i32(7);
        w.emit(Opcode::ReturnValue);
        let (vm, r) = exec_simple(w.into_bytes());
        assert_eq!(numeric_str(&vm, &r), "7");
    }

    // -- 9. Variables --

    #[test]
    fn test_load_store_local() {
        let mut w = BytecodeWriter::new();
        w.emit(Opcode::AllocFrame); w.emit_u16(2);
        w.emit(Opcode::PushI32); w.emit_i32(42);
        w.emit(Opcode::StoreLocal); w.emit_u16(0);
        w.emit(Opcode::LoadLocal); w.emit_u16(0);
        w.emit(Opcode::ReturnValue);
        let (_, r) = exec_simple(w.into_bytes());
        assert_eq!(r, UValue::Int32(42));
    }

    #[test]
    fn test_load_store_global() {
        let mut w = BytecodeWriter::new();
        w.emit(Opcode::PushI32); w.emit_i32(77);
        w.emit(Opcode::StoreGlobal); w.emit_u16(0);
        w.emit(Opcode::LoadGlobal); w.emit_u16(0);
        w.emit(Opcode::ReturnValue);
        let (_, r) = exec_simple(w.into_bytes());
        assert_eq!(r, UValue::Int32(77));
    }

    // -- 10. Array --

    #[test]
    fn test_new_array_len() {
        let mut w = BytecodeWriter::new();
        w.emit(Opcode::PushI32); w.emit_i32(1);
        w.emit(Opcode::PushI32); w.emit_i32(2);
        w.emit(Opcode::NewArray); w.emit_u16(2);
        w.emit(Opcode::ArrayLen);
        w.emit(Opcode::ReturnValue);
        let (_, r) = exec_simple(w.into_bytes());
        assert_eq!(r, UValue::Int64(2));
    }

    #[test]
    fn test_array_get() {
        let mut w = BytecodeWriter::new();
        w.emit(Opcode::PushI32); w.emit_i32(10);
        w.emit(Opcode::PushI32); w.emit_i32(20);
        w.emit(Opcode::PushI32); w.emit_i32(30);
        w.emit(Opcode::NewArray); w.emit_u16(3);
        w.emit(Opcode::PushI32); w.emit_i32(1);
        w.emit(Opcode::ArrayGet);
        w.emit(Opcode::ReturnValue);
        let (_, r) = exec_simple(w.into_bytes());
        assert_eq!(r, UValue::Int32(20));
    }

    #[test]
    fn test_array_push_pop() {
        let mut w = BytecodeWriter::new();
        w.emit(Opcode::NewArray); w.emit_u16(0);
        w.emit(Opcode::Dup);
        w.emit(Opcode::PushI32); w.emit_i32(99);
        w.emit(Opcode::ArrayPush);
        w.emit(Opcode::ArrayPop);
        w.emit(Opcode::ReturnValue);
        let (_, r) = exec_simple(w.into_bytes());
        assert_eq!(r, UValue::Int32(99));
    }

    #[test]
    fn test_array_concat() {
        let mut w = BytecodeWriter::new();
        w.emit(Opcode::PushI32); w.emit_i32(1);
        w.emit(Opcode::PushI32); w.emit_i32(2);
        w.emit(Opcode::NewArray); w.emit_u16(2);
        w.emit(Opcode::PushI32); w.emit_i32(3);
        w.emit(Opcode::PushI32); w.emit_i32(4);
        w.emit(Opcode::NewArray); w.emit_u16(2);
        w.emit(Opcode::ArrayConcat);
        w.emit(Opcode::ArrayLen);
        w.emit(Opcode::ReturnValue);
        let (_, r) = exec_simple(w.into_bytes());
        assert_eq!(r, UValue::Int64(4));
    }

    // -- 11. Map --

    #[test]
    fn test_new_map_len() {
        let mut w = BytecodeWriter::new();
        w.emit(Opcode::NewMap);
        w.emit(Opcode::MapLen);
        w.emit(Opcode::ReturnValue);
        let (_, r) = exec_simple(w.into_bytes());
        assert_eq!(r, UValue::Int64(0));
    }

    #[test]
    fn test_map_set_get() {
        let mut w = BytecodeWriter::new();
        w.emit(Opcode::NewMap);
        w.emit(Opcode::Dup);
        w.emit(Opcode::PushI32); w.emit_i32(42);
        w.emit(Opcode::PushI32); w.emit_i32(99);
        w.emit(Opcode::MapSet);
        w.emit(Opcode::PushI32); w.emit_i32(42);
        w.emit(Opcode::MapGet);
        w.emit(Opcode::ReturnValue);
        let (_, r) = exec_simple(w.into_bytes());
        assert_eq!(r, UValue::Int32(99));
    }

    // -- 12. Set --

    #[test]
    fn test_set_add_contains() {
        let mut w = BytecodeWriter::new();
        w.emit(Opcode::NewSet);
        w.emit(Opcode::Dup);
        w.emit(Opcode::PushI32); w.emit_i32(42);
        w.emit(Opcode::SetAdd);
        w.emit(Opcode::PushI32); w.emit_i32(42);
        w.emit(Opcode::SetContains);
        w.emit(Opcode::ReturnValue);
        let (_, r) = exec_simple(w.into_bytes());
        assert_eq!(r, UValue::Bool(true));
    }

    #[test]
    fn test_tuple() {
        let mut w = BytecodeWriter::new();
        w.emit(Opcode::PushI32); w.emit_i32(1);
        w.emit(Opcode::PushI32); w.emit_i32(2);
        w.emit(Opcode::PushI32); w.emit_i32(3);
        w.emit(Opcode::Tuple); w.emit_u16(3);
        w.emit(Opcode::ArrayLen);
        w.emit(Opcode::ReturnValue);
        let (_, r) = exec_simple(w.into_bytes());
        assert_eq!(r, UValue::Int64(3));
    }

    // -- 13. String --

    fn exec_str_mod(strings: &[&str], bytecode: Vec<u8>) -> (Vm, ModuleId, UValue) {
        let mut m = UtenModule::new("test");
        m.intern("");
        for s in strings { m.intern(s); }
        m.functions.push(FunctionDef {
            name: "<main>".into(), bytecode,
            n_locals: 4, n_params: 0, is_variadic: false,
            n_captures: 0, return_type: None, param_types: vec![],
            jit_code: None, hotness: 0,
        });
        let mut vm = Vm::new();
        let mid = vm.load_module(m).unwrap();
        vm.run_module_init(mid).ok();
        let r = vm.execute(mid, 0, vec![]).unwrap();
        (vm, mid, r)
    }

    fn get_str(vm: &Vm, mid: ModuleId, sid: StringId) -> String {
        if (mid as usize) < vm.modules.len() {
            vm.modules[mid as usize].module.strings.get(sid as usize).cloned().unwrap_or_default()
        } else { String::new() }
    }

    #[test]
    fn test_string_len() {
        let (_, _, r) = exec_str_mod(&["", "hello"], {
            let mut w = BytecodeWriter::new();
            w.emit(Opcode::PushString); w.emit_u16(1);
            w.emit(Opcode::StrLen);
            w.emit(Opcode::ReturnValue);
            w.into_bytes()
        });
        assert_eq!(r, UValue::Int64(5));
    }

    #[test]
    fn test_string_concat() {
        let (vm, mid, r) = exec_str_mod(&["", "Hello", "World"], {
            let mut w = BytecodeWriter::new();
            w.emit(Opcode::PushString); w.emit_u16(1);
            w.emit(Opcode::PushString); w.emit_u16(2);
            w.emit(Opcode::StrConcat);
            w.emit(Opcode::ReturnValue);
            w.into_bytes()
        });
        match r { UValue::String(sid) => assert_eq!(get_str(&vm, mid, sid), "HelloWorld"), _ => panic!("not string") }
    }

    #[test]
    fn test_string_substring() {
        let (vm, mid, r) = exec_str_mod(&["", "hello"], {
            let mut w = BytecodeWriter::new();
            w.emit(Opcode::PushString); w.emit_u16(1);
            w.emit(Opcode::PushI32); w.emit_i32(1);
            w.emit(Opcode::PushI32); w.emit_i32(4);
            w.emit(Opcode::StrSub);
            w.emit(Opcode::ReturnValue);
            w.into_bytes()
        });
        match r { UValue::String(sid) => assert_eq!(get_str(&vm, mid, sid), "ell"), _ => panic!("not string") }
    }

    #[test]
    fn test_string_split() {
        let (_, _, r) = exec_str_mod(&["", "a,b,c", ","], {
            let mut w = BytecodeWriter::new();
            w.emit(Opcode::PushString); w.emit_u16(1);
            w.emit(Opcode::PushString); w.emit_u16(2);
            w.emit(Opcode::StrSplit);
            w.emit(Opcode::ArrayLen);
            w.emit(Opcode::ReturnValue);
            w.into_bytes()
        });
        assert_eq!(r, UValue::Int64(3));
    }

    #[test]
    fn test_string_replace() {
        let (vm, mid, r) = exec_str_mod(&["", "hello world", "world", "Rust"], {
            let mut w = BytecodeWriter::new();
            w.emit(Opcode::PushString); w.emit_u16(1);
            w.emit(Opcode::PushString); w.emit_u16(2);
            w.emit(Opcode::PushString); w.emit_u16(3);
            w.emit(Opcode::StrReplace);
            w.emit(Opcode::ReturnValue);
            w.into_bytes()
        });
        match r { UValue::String(sid) => assert_eq!(get_str(&vm, mid, sid), "hello Rust"), _ => panic!("not string") }
    }

    #[test]
    fn test_string_compare() {
        let (_, _, r) = exec_str_mod(&["", "abc", "def"], {
            let mut w = BytecodeWriter::new();
            w.emit(Opcode::PushString); w.emit_u16(1);
            w.emit(Opcode::PushString); w.emit_u16(2);
            w.emit(Opcode::StrCmp);
            w.emit(Opcode::ReturnValue);
            w.into_bytes()
        });
        assert_eq!(r, UValue::Int32(-1));
    }

    // -- 14. OOP --

    #[test]
    fn test_new_class_and_object() {
        let (vm, _mid, r) = exec_str_mod(&["", "ns"], {
            let mut w = BytecodeWriter::new();
            w.emit(Opcode::NewNamespace); w.emit_u16(1);
            w.emit(Opcode::NewClass);
            w.emit(Opcode::NewObject);
            w.emit(Opcode::TypeOf);
            w.emit(Opcode::ReturnValue);
            w.into_bytes()
        });
        assert_eq!(r, UValue::Int32(ValueTag::Object as i32));
        drop(vm);
    }

    // -- 16. Module --

    #[test]
    fn test_export_opcode() {
        let mut vm = Vm::new();
        let mut m = UtenModule::new("test_export");
        assert_eq!(m.intern("answer"), 0);
        let mut w = BytecodeWriter::new();
        w.emit(Opcode::PushI32); w.emit_i32(42);
        w.emit(Opcode::Export); w.emit_u16(0);
        w.emit(Opcode::Return);
        m.functions.push(FunctionDef {
            name: "<main>".into(), bytecode: w.into_bytes(),
            n_locals: 4, n_params: 0, is_variadic: false,
            n_captures: 0, return_type: None, param_types: vec![],
            jit_code: None, hotness: 0,
        });
        let mid = vm.load_module(m).unwrap();
        vm.run_module_init(mid).unwrap();
        assert!(vm.modules[mid as usize].export_values.contains_key("answer"));
    }

    #[test]
    fn test_cross_module_import() {
        let mut vm = Vm::new();
        let mut lib = UtenModule::new("mylib");
        lib.intern("answer");
        lib.functions.push(FunctionDef {
            name: "<main>".into(), bytecode: {
                let mut w = BytecodeWriter::new();
                w.emit(Opcode::PushI32); w.emit_i32(42);
                w.emit(Opcode::Export); w.emit_u16(0);
                w.emit(Opcode::Return); w.into_bytes()
            },
            n_locals: 4, n_params: 0, is_variadic: false,
            n_captures: 0, return_type: None, param_types: vec![],
            jit_code: None, hotness: 0,
        });
        let lib_mid = vm.load_module(lib).unwrap();
        vm.run_module_init(lib_mid).unwrap();
        vm.loader.register_loaded("mylib", lib_mid as usize);

        let mut main = UtenModule::new("main");
        main.intern("mylib");
        main.intern("answer");
        main.functions.push(FunctionDef {
            name: "<main>".into(), bytecode: {
                let mut w = BytecodeWriter::new();
                w.emit(Opcode::Import); w.emit_u16(0);
                w.emit(Opcode::GetField); w.emit_u16(1);
                w.emit(Opcode::ReturnValue); w.into_bytes()
            },
            n_locals: 4, n_params: 0, is_variadic: false,
            n_captures: 0, return_type: None, param_types: vec![],
            jit_code: None, hotness: 0,
        });
        let main_mid = vm.load_module(main).unwrap();
        let r = vm.execute(main_mid, 0, vec![]).unwrap();
        assert_eq!(numeric_str(&vm, &r), "42");
    }

    #[test]
    fn test_sync_globals() {
        let mut vm = Vm::new();
        let mut m = UtenModule::new("pymod");
        m.globals.push(crate::bytecode::GlobalDef {
            name: "my_func".into(), init_value: None, is_exported: false,
        });
        m.functions.push(FunctionDef {
            name: "<main>".into(), bytecode: {
                let mut w = BytecodeWriter::new();
                w.emit(Opcode::PushI32); w.emit_i32(77);
                w.emit(Opcode::StoreGlobal); w.emit_u16(0);
                w.emit(Opcode::Return); w.into_bytes()
            },
            n_locals: 4, n_params: 0, is_variadic: false,
            n_captures: 0, return_type: None, param_types: vec![],
            jit_code: None, hotness: 0,
        });
        let mid = vm.load_module(m).unwrap();
        vm.run_module_init(mid).unwrap();
        assert!(vm.modules[mid as usize].export_values.contains_key("my_func"));
    }

    // -- 17. Errors --

    fn expect_exec_error(bytecode: Vec<u8>) {
        let mut m = UtenModule::new("test");
        m.functions.push(FunctionDef {
            name: "<main>".into(), bytecode,
            n_locals: 4, n_params: 0, is_variadic: false,
            n_captures: 0, return_type: None, param_types: vec![],
            jit_code: None, hotness: 0,
        });
        let mut vm = Vm::new();
        let mid = vm.load_module(m).unwrap();
        vm.run_module_init(mid).ok();
        assert!(vm.execute(mid, 0, vec![]).is_err());
    }

    #[test]
    fn test_division_by_zero() {
        let mut w = BytecodeWriter::new();
        w.emit(Opcode::PushI32); w.emit_i32(1);
        w.emit(Opcode::PushI32); w.emit_i32(0);
        w.emit(Opcode::Div);
        w.emit(Opcode::ReturnValue);
        expect_exec_error(w.into_bytes());
    }

    #[test]
    fn test_empty_stack_pop() {
        let mut w = BytecodeWriter::new();
        w.emit(Opcode::Pop);
        w.emit(Opcode::ReturnValue);
        expect_exec_error(w.into_bytes());
    }

    // -- 18. Functional --

    #[test]
    fn test_cons_car_cdr() {
        let mut w = BytecodeWriter::new();
        w.emit(Opcode::PushI32); w.emit_i32(1);
        w.emit(Opcode::PushI32); w.emit_i32(2);
        w.emit(Opcode::Cons);
        w.emit(Opcode::Car);
        w.emit(Opcode::ReturnValue);
        let (_, r) = exec_simple(w.into_bytes());
        assert_eq!(r, UValue::Int32(1));
    }

    // -- 19. Builtins --

    #[test]
    fn test_builtin_functions_registered() {
        let vm = Vm::new();
        assert!(vm.native_func_names.contains_key("utencore.print"));
        assert!(vm.native_func_names.contains_key("utencore.Math.sqrt"));
    }

    // -- 20. Stress --

    #[test]
    fn test_large_array_stress() {
        let mut w = BytecodeWriter::new();
        for i in 0..100 { w.emit(Opcode::PushI32); w.emit_i32(i); }
        w.emit(Opcode::NewArray); w.emit_u16(100);
        w.emit(Opcode::ArrayLen);
        w.emit(Opcode::ReturnValue);
        let (_, r) = exec_simple(w.into_bytes());
        assert_eq!(r, UValue::Int64(100));
    }

    #[test]
    fn test_gc_pressure_stress() {
        let mut w = BytecodeWriter::new();
        for _ in 0..200 {
            w.emit(Opcode::NewArray); w.emit_u16(0);
            w.emit(Opcode::Pop);
        }
        w.emit(Opcode::PushI32); w.emit_i32(1);
        w.emit(Opcode::ReturnValue);
        let (_, r) = exec_simple(w.into_bytes());
        assert_eq!(r, UValue::Int32(1));
    }

    #[test]
    fn test_deep_stack_stress() {
        let mut w = BytecodeWriter::new();
        for i in 0..300 { w.emit(Opcode::PushI32); w.emit_i32(i); }
        w.emit(Opcode::PopN); w.emit_u16(299);
        w.emit(Opcode::ReturnValue);
        let (_, r) = exec_simple(w.into_bytes());
        assert_eq!(r, UValue::Int32(0));
    }

    #[test]
    fn test_nested_arrays_stress() {
        let mut w = BytecodeWriter::new();
        for batch in 0..30 {
            for i in 0..10 { w.emit(Opcode::PushI32); w.emit_i32(batch * 100 + i); }
            w.emit(Opcode::NewArray); w.emit_u16(10);
        }
        w.emit(Opcode::NewArray); w.emit_u16(30);
        w.emit(Opcode::ArrayLen);
        w.emit(Opcode::ReturnValue);
        let (_, r) = exec_simple(w.into_bytes());
        assert_eq!(r, UValue::Int64(30));
    }

    #[test]
    fn test_multiple_modules_stress() {
        let mut vm = Vm::new();
        for idx in 0..10 {
            let mut m = UtenModule::new(&format!("mod_{idx}"));
            m.functions.push(FunctionDef {
                name: "<main>".into(), bytecode: {
                    let mut w = BytecodeWriter::new();
                    w.emit(Opcode::PushI32); w.emit_i32(idx);
                    w.emit(Opcode::ReturnValue);
                    w.into_bytes()
                },
                n_locals: 4, n_params: 0, is_variadic: false,
                n_captures: 0, return_type: None, param_types: vec![],
                jit_code: None, hotness: 0,
            });
            let mid = vm.load_module(m).unwrap();
            vm.run_module_init(mid).ok();
            let r = vm.execute(mid, 0, vec![]).unwrap();
            assert_eq!(r, UValue::Int32(idx));
        }
    }

    #[test]
    fn test_large_map_stress() {
        let mut w = BytecodeWriter::new();
        w.emit(Opcode::NewMap);
        for i in 0..20 {
            w.emit(Opcode::Dup);
            w.emit(Opcode::PushI32); w.emit_i32(i);
            w.emit(Opcode::PushI32); w.emit_i32(i * 10);
            w.emit(Opcode::MapSet);
        }
        w.emit(Opcode::MapLen);
        w.emit(Opcode::ReturnValue);
        let (_, r) = exec_simple(w.into_bytes());
        assert_eq!(r, UValue::Int64(20));
    }

    // -- 21. Serialization --

    #[test]
    fn test_module_roundtrip() {
        let mut m = UtenModule::new("test_ser");
        m.intern("hello");
        m.functions.push(FunctionDef {
            name: "<main>".into(),
            bytecode: vec![Opcode::PushNil as u8, Opcode::Return as u8],
            n_locals: 0, n_params: 0, is_variadic: false,
            n_captures: 0, return_type: None, param_types: vec![],
            jit_code: None, hotness: 0,
        });
        let bytes = m.to_bytes().unwrap();
        let restored = UtenModule::from_bytes(&bytes).unwrap();
        assert_eq!(restored.header.name, "test_ser");
        assert_eq!(restored.strings.len(), 1);
    }

    #[test]
    fn test_cache_bytes_magic() {
        let m = UtenModule::new("test_cache");
        assert_eq!(&m.to_cache_bytes().unwrap()[..4], b"UCCH");
        assert_eq!(&m.to_bytes().unwrap()[..4], b"UCLB");
    }

    #[test]
    fn test_string_intern() {
        let mut m = UtenModule::new("test");
        let s1 = m.intern("hello");
        let s2 = m.intern("hello");
        let s3 = m.intern("world");
        assert_eq!(s1, s2);
        assert_ne!(s1, s3);
    }

    #[test]
    fn test_bytecode_writer() {
        let mut w = BytecodeWriter::new();
        w.emit(Opcode::PushI32); w.emit_i32(42);
        w.emit(Opcode::ReturnValue);
        let bytes = w.into_bytes();
        assert_eq!(bytes[0], Opcode::PushI32 as u8);
        assert_eq!(bytes[5], Opcode::ReturnValue as u8);
    }

    #[test]
    fn test_bytecode_reader() {
        let mut w = BytecodeWriter::new();
        w.emit(Opcode::PushI32); w.emit_i32(12345);
        w.emit(Opcode::PushTrue);
        w.emit(Opcode::ReturnValue);
        let bytes = w.into_bytes();
        let mut reader = crate::bytecode::BytecodeReader::new(&bytes);
        let (op1, _) = reader.read_instruction().unwrap();
        assert_eq!(op1, Opcode::PushI32);
        assert!(reader.read_instruction().is_some());
    }

    #[test]
    fn test_bytecode_version_rejected() {
        let mut m = UtenModule::new("old");
        m.bytecode_version = 999;
        assert!(Vm::new().load_module(m).is_err());
    }

    // -- 22. Opcode Enum --

    #[test]
    fn test_opcode_from_byte() {
        assert_eq!(Opcode::from_byte(0x00), Some(Opcode::Nop));
        assert_eq!(Opcode::from_byte(0x10), Some(Opcode::Add));
        assert_eq!(Opcode::from_byte(0x78), Some(Opcode::Return));
        assert_eq!(Opcode::from_byte(0xFE), Some(Opcode::Halt));
        assert_eq!(Opcode::from_byte(0xFF), Some(Opcode::Raise));
        assert_eq!(Opcode::from_byte(0x4E), None);
    }

    // -- 23. VM Config --

    #[test]
    fn test_vm_config_defaults() {
        let c = VmConfig::default();
        assert_eq!(c.max_recursion, 1000);
        assert!(c.jit_enabled);
    }

    #[test]
    fn test_vm_with_custom_config() {
        let config = VmConfig { max_recursion: 50, ..Default::default() };
        let mut vm = Vm::with_config(config);
        let mut m = UtenModule::new("test");
        m.functions.push(FunctionDef {
            name: "<main>".into(), bytecode: {
                let mut w = BytecodeWriter::new();
                w.emit(Opcode::PushI32); w.emit_i32(42);
                w.emit(Opcode::ReturnValue);
                w.into_bytes()
            },
            n_locals: 4, n_params: 0, is_variadic: false,
            n_captures: 0, return_type: None, param_types: vec![],
            jit_code: None, hotness: 0,
        });
        let mid = vm.load_module(m).unwrap();
        vm.run_module_init(mid).ok();
        let r = vm.execute(mid, 0, vec![]).unwrap();
        assert_eq!(r, UValue::Int32(42));
    }

    // -- 24. Unsafe module --

    #[test]
    fn test_unsafe_module_loaded() {
        let vm = Vm::new();
        assert!(vm.modules[0].export_values.contains_key("Unsafe"));
        assert!(vm.modules[0].export_values.contains_key("Math"));
    }
}
