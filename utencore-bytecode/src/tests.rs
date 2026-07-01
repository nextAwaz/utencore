//! Bytecode format tests.

pub mod tests {
    use crate::bytecode::{BytecodeWriter, BytecodeReader, FunctionDef, UtenModule, ConstValue, ExceptionTableEntry};
    use utencore_types::{Opcode, BYTECODE_VERSION};

    #[test]
    fn test_module_construction() {
        let m = UtenModule::new("test_module");
        assert_eq!(m.header.name, "test_module");
        assert_eq!(m.magic, *b"UCLB");
        assert_eq!(m.bytecode_version, BYTECODE_VERSION);
    }

    #[test]
    fn test_string_interning() {
        let mut m = UtenModule::new("s");
        let a = m.intern("alpha");
        let b = m.intern("beta");
        let a2 = m.intern("alpha");
        assert_eq!(a, a2);
        assert_ne!(a, b);
        assert_eq!(m.strings[a as usize], "alpha");
    }

    #[test]
    fn test_module_serialization_roundtrip() {
        let mut m = UtenModule::new("ser_test");
        m.header.source_lang = "python".into();
        m.intern("x");
        m.constants.push(ConstValue::Int32(42));
        m.functions.push(FunctionDef {
            name: "main".into(),
            bytecode: vec![Opcode::PushI32 as u8, 42, 0, 0, 0, Opcode::Return as u8],
            n_locals: 2, n_params: 0, is_variadic: false,
            n_captures: 0, return_type: None, param_types: vec![],
            jit_code: None, hotness: 0,
        });
        let bytes = m.to_bytes().unwrap();
        let restored = UtenModule::from_bytes(&bytes).unwrap();
        assert_eq!(restored.header.name, "ser_test");
        assert_eq!(restored.header.source_lang, "python");
        assert_eq!(restored.functions.len(), 1);
        assert_eq!(restored.functions[0].name, "main");
        assert_eq!(restored.constants.len(), 1);
        assert!(restored.string_map.contains_key("x"));
    }

    #[test]
    fn test_cache_format() {
        let m = UtenModule::new("cache_test");
        let cache = m.to_cache_bytes().unwrap();
        assert_eq!(&cache[..4], b"UCCH");
        let restored = UtenModule::from_bytes(&cache).unwrap();
        assert_eq!(restored.header.name, "cache_test");
    }

    #[test]
    fn test_bytecode_writer_emit() {
        let mut w = BytecodeWriter::new();
        assert_eq!(w.len(), 0);
        w.emit(Opcode::PushNil);
        assert_eq!(w.len(), 1);
        w.emit(Opcode::PushI32);
        w.emit_i32(100);
        assert_eq!(w.len(), 6);
        w.emit(Opcode::ReturnValue);
        let bytes = w.into_bytes();
        assert_eq!(bytes.len(), 7);
        assert_eq!(bytes[0], Opcode::PushNil as u8);
    }

    #[test]
    fn test_bytecode_reader_instructions() {
        let mut w = BytecodeWriter::new();
        w.emit(Opcode::PushI32); w.emit_i32(42);
        w.emit(Opcode::PushTrue);
        w.emit(Opcode::Add);
        w.emit(Opcode::ReturnValue);
        let bytes = w.into_bytes();
        let mut r = BytecodeReader::new(&bytes);
        let (op, _) = r.read_instruction().unwrap();
        assert_eq!(op, Opcode::PushI32);
        let (op, _) = r.read_instruction().unwrap();
        assert_eq!(op, Opcode::PushTrue);
        let (op, _) = r.read_instruction().unwrap();
        assert_eq!(op, Opcode::Add);
        let (op, _) = r.read_instruction().unwrap();
        assert_eq!(op, Opcode::ReturnValue);
        assert!(r.read_instruction().is_none());
    }

    #[test]
    fn test_bytecode_reader_peek() {
        let mut w = BytecodeWriter::new();
        w.emit(Opcode::Nop);
        w.emit(Opcode::Halt);
        let bytes = w.into_bytes();
        let mut r = BytecodeReader::new(&bytes);
        assert_eq!(r.peek_opcode(), Some(Opcode::Nop));
        assert_eq!(r.next_opcode(), Some(Opcode::Nop));
        assert_eq!(r.peek_opcode(), Some(Opcode::Halt));
    }

    #[test]
    fn test_version_constants() {
        assert!(BYTECODE_VERSION >= 3);
    }

    #[test]
    fn test_verify_module_invalid_version() {
        let mut m = UtenModule::new("bad");
        m.bytecode_version = 999;
        assert!(crate::bytecode::verify_module(&m).is_err());
    }

    #[test]
    fn test_verify_module_valid() {
        let mut m = UtenModule::new("good");
        m.intern("main");
        m.functions.push(FunctionDef {
            name: "main".into(),
            bytecode: vec![Opcode::PushNil as u8, Opcode::Return as u8],
            n_locals: 0, n_params: 0, is_variadic: false,
            n_captures: 0, return_type: None, param_types: vec![],
            jit_code: None, hotness: 0,
        });
        assert!(crate::bytecode::verify_module(&m).is_ok());
    }

    #[test]
    fn test_module_header_metadata() {
        let mut m = UtenModule::new("meta_test");
        m.header.metadata.insert("key".into(), "val".into());
        assert_eq!(m.header.metadata.get("key").unwrap(), "val");
        let bytes = m.to_bytes().unwrap();
        let restored = UtenModule::from_bytes(&bytes).unwrap();
        assert_eq!(restored.header.metadata.get("key").unwrap(), "val");
    }

    #[test]
    fn test_exception_table_serialization() {
        let mut m = UtenModule::new("exc_test");
        m.exceptions.push(ExceptionTableEntry {
            func_index: 0,
            try_start: 0,
            try_end: 10,
            handler_pc: 20,
            catch_type: None,
            finally_pc: None,
        });
        let bytes = m.to_bytes().unwrap();
        let restored = UtenModule::from_bytes(&bytes).unwrap();
        assert_eq!(restored.exceptions.len(), 1);
        assert_eq!(restored.exceptions[0].try_start, 0);
    }

    #[test]
    fn test_constant_values() {
        let mut m = UtenModule::new("c");
        m.constants.push(ConstValue::Nil);
        m.constants.push(ConstValue::Bool(true));
        m.constants.push(ConstValue::Int32(-42));
        m.constants.push(ConstValue::Int64(i64::MAX));
        m.constants.push(ConstValue::Float32(1.5));
        m.constants.push(ConstValue::Float64(3.14));
        m.constants.push(ConstValue::String(0));
        let bytes = m.to_bytes().unwrap();
        let restored = UtenModule::from_bytes(&bytes).unwrap();
        assert_eq!(restored.constants.len(), 7);
    }
}

