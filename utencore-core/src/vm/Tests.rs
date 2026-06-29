//! VM unit tests.

#[cfg(test)]
mod tests {
    use crate::bytecode::{BytecodeWriter, FunctionDef, UtenModule};
    use crate::opcodes::Opcode;
    use crate::vm::Vm;
    use utencore_types::*;

    fn load_and_init(vm: &mut Vm, name: &str, bytecode: Vec<u8>) -> ModuleId {
        let mut m = UtenModule::new(name);
        m.functions.push(FunctionDef {
            name: "<main>".into(), bytecode,
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
        let answer_sid = 0u16;
        w.emit(Opcode::Export); w.emit_u16(answer_sid);
        w.emit(Opcode::Return);

        let mut m = UtenModule::new("test_export");
        assert_eq!(m.intern("answer"), 0);
        m.functions.push(FunctionDef {
            name: "<main>".into(), bytecode: w.into_bytes(),
            n_locals: 4, n_params: 0, is_variadic: false,
            n_captures: 0, return_type: None, param_types: vec![],
            jit_code: None, hotness: 0,
        });
        let mid = vm.load_module(m).unwrap();
        vm.run_module_init(mid).unwrap();
        assert!(vm.modules[mid as usize].export_values.contains_key("answer"),
            "Export should register 'answer' in export_values");
    }

    #[test]
    fn test_build_module_namespace() {
        let mut vm = Vm::new();
        let mut m = UtenModule::new("mylib");
        let greet_sid = m.intern("greet");
        m.functions.push(FunctionDef {
            name: "greet".into(),
            bytecode: {
                let mut w = BytecodeWriter::new();
                w.emit(Opcode::PushI32); w.emit_i32(99);
                w.emit(Opcode::ReturnValue); w.into_bytes()
            },
            n_locals: 0, n_params: 0, is_variadic: false,
            n_captures: 0, return_type: None, param_types: vec![],
            jit_code: None, hotness: 0,
        });
        m.functions.push(FunctionDef {
            name: "<main>".into(),
            bytecode: {
                let mut w = BytecodeWriter::new();
                w.emit(Opcode::PushI32); w.emit_i32(99);
                w.emit(Opcode::Export); w.emit_u16(greet_sid as u16);
                w.emit(Opcode::Return); w.into_bytes()
            },
            n_locals: 4, n_params: 0, is_variadic: false,
            n_captures: 0, return_type: None, param_types: vec![],
            jit_code: None, hotness: 0,
        });
        let mid = vm.load_module(m).unwrap();
        vm.run_module_init(mid).unwrap();
        vm.loader.register_loaded("mylib", mid as usize);
        let ns_handle = vm.build_module_namespace(mid, "mylib");
        if let HeapObject::Namespace { members, .. } = vm.gc.get(ns_handle) {
            assert!(!members.is_empty(), "Namespace should have members");
        } else {
            panic!("Expected Namespace");
        }
    }

    #[test]
    fn test_cross_module_import_via_opcodes() {
        let mut vm = Vm::new();
        let mut lib = UtenModule::new("mylib");
        let answer_sid = lib.intern("answer") as u16;
        assert_eq!(answer_sid, 0);
        lib.functions.push(FunctionDef {
            name: "<main>".into(),
            bytecode: {
                let mut w = BytecodeWriter::new();
                w.emit(Opcode::PushI32); w.emit_i32(42);
                w.emit(Opcode::Export); w.emit_u16(answer_sid);
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
        let mylib_sid = main.intern("mylib") as u16;
        let answer_sid_main = main.intern("answer") as u16;
        main.functions.push(FunctionDef {
            name: "<main>".into(),
            bytecode: {
                let mut w = BytecodeWriter::new();
                w.emit(Opcode::Import); w.emit_u16(mylib_sid);
                w.emit(Opcode::GetField); w.emit_u16(answer_sid_main);
                w.emit(Opcode::ReturnValue); w.into_bytes()
            },
            n_locals: 4, n_params: 0, is_variadic: false,
            n_captures: 0, return_type: None, param_types: vec![],
            jit_code: None, hotness: 0,
        });
        let main_mid = vm.load_module(main).unwrap();
        let result = vm.execute(main_mid, 0, vec![]).unwrap();
        assert_eq!(format!("{result}"), "42");
    }

    #[test]
    fn test_importfunc_with_namespace_handle() {
        let mut vm = Vm::new();
        let mut calc = UtenModule::new("calc");
        let double_sid = calc.intern("double") as u16;
        calc.functions.push(FunctionDef {
            name: "double".into(),
            bytecode: {
                let mut w = BytecodeWriter::new();
                w.emit(Opcode::LoadLocal); w.emit_u16(0);
                w.emit(Opcode::PushI32); w.emit_i32(2);
                w.emit(Opcode::Mul); w.emit(Opcode::ReturnValue); w.into_bytes()
            },
            n_locals: 1, n_params: 1, is_variadic: false,
            n_captures: 0, return_type: None, param_types: vec![],
            jit_code: None, hotness: 0,
        });
        calc.functions.push(FunctionDef {
            name: "<main>".into(),
            bytecode: {
                let mut w = BytecodeWriter::new();
                w.emit(Opcode::MakeClosure); w.emit_u16(0);
                w.emit(Opcode::Export); w.emit_u16(double_sid);
                w.emit(Opcode::Return); w.into_bytes()
            },
            n_locals: 4, n_params: 0, is_variadic: false,
            n_captures: 0, return_type: None, param_types: vec![],
            jit_code: None, hotness: 0,
        });
        let calc_mid = vm.load_module(calc).unwrap();
        vm.run_module_init(calc_mid).unwrap();
        vm.loader.register_loaded("calc", calc_mid as usize);

        let mut main = UtenModule::new("main");
        let calc_sid = main.intern("calc") as u16;
        let double_sid_main = main.intern("double") as u16;
        main.functions.push(FunctionDef {
            name: "<main>".into(),
            bytecode: {
                let mut w = BytecodeWriter::new();
                w.emit(Opcode::PushI32); w.emit_i32(5);
                w.emit(Opcode::PushI32); w.emit_i32(1);
                w.emit(Opcode::Import); w.emit_u16(calc_sid);
                w.emit(Opcode::ImportFunc); w.emit_u16(double_sid_main);
                w.emit(Opcode::CallValue);
                w.emit(Opcode::ReturnValue); w.into_bytes()
            },
            n_locals: 4, n_params: 0, is_variadic: false,
            n_captures: 0, return_type: None, param_types: vec![],
            jit_code: None, hotness: 0,
        });
        let main_mid = vm.load_module(main).unwrap();
        let result = vm.execute(main_mid, 0, vec![]).unwrap();
        assert_eq!(format!("{result}"), "10");
    }

    #[test]
    fn test_sync_globals_to_exports() {
        let mut vm = Vm::new();
        let mut m = UtenModule::new("pymod");
        m.globals.push(crate::bytecode::GlobalDef {
            name: "my_func".into(), init_value: None, is_exported: false,
        });
        m.functions.push(FunctionDef {
            name: "<main>".into(),
            bytecode: {
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
        assert!(vm.modules[mid as usize].export_values.contains_key("my_func"),
            "sync_globals_to_exports should export StoreGlobal-assigned functions");
    }
}
