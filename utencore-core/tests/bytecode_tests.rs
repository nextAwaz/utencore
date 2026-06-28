// Bytecode serialization/deserialization tests.

use utencore::bytecode::{BytecodeReader, BytecodeWriter, UtenModule};
use utencore::opcodes::Opcode;

#[test]
fn test_write_read_opcodes() {
    let mut w = BytecodeWriter::new();
    w.emit(Opcode::PushNil);
    w.emit(Opcode::PushTrue);
    w.emit(Opcode::PushFalse);
    w.emit(Opcode::PushI32); w.emit_i32(42);
    w.emit(Opcode::Dup);
    w.emit(Opcode::Swap);
    w.emit(Opcode::Pop);
    w.emit(Opcode::Add);
    w.emit(Opcode::Return);
    let bytes = w.into_bytes();
    assert!(!bytes.is_empty());

    let mut r = BytecodeReader::new(&bytes);
    assert_eq!(r.next_opcode(), Some(Opcode::PushNil));
    assert_eq!(r.next_opcode(), Some(Opcode::PushTrue));
    assert_eq!(r.next_opcode(), Some(Opcode::PushFalse));
    assert_eq!(r.next_opcode(), Some(Opcode::PushI32));
    assert_eq!(r.read_i32(), 42);
    assert_eq!(r.next_opcode(), Some(Opcode::Dup));
    assert_eq!(r.next_opcode(), Some(Opcode::Swap));
    assert_eq!(r.next_opcode(), Some(Opcode::Pop));
    assert_eq!(r.next_opcode(), Some(Opcode::Add));
    assert_eq!(r.next_opcode(), Some(Opcode::Return));
    assert_eq!(r.next_opcode(), None);
}

#[test]
fn test_module_serialization() {
    let mut m = UtenModule::new("test_mod");
    m.intern("hello");
    m.intern("world");

    let mut w = BytecodeWriter::new();
    w.emit(Opcode::PushI32); w.emit_i32(1);
    w.emit(Opcode::PushI32); w.emit_i32(2);
    w.emit(Opcode::Add);
    w.emit(Opcode::ReturnValue);
    m.functions.push(utencore::bytecode::FunctionDef {
        name: "main".into(), bytecode: w.into_bytes(),
        n_locals: 4, n_params: 0, is_variadic: false,
        n_captures: 0, return_type: None, param_types: vec![],
        jit_code: None, hotness: 0,
    });

    let bytes = m.to_bytes().unwrap();
    let m2 = UtenModule::from_bytes(&bytes).unwrap();
    assert_eq!(m2.header.name, "test_mod");
    assert_eq!(m2.strings.len(), 2);
    assert_eq!(m2.functions.len(), 1);
    assert_eq!(m2.functions[0].name, "main");
}

#[test]
fn test_empty_module() {
    let m = UtenModule::new("empty");
    let bytes = m.to_bytes().unwrap();
    let m2 = UtenModule::from_bytes(&bytes).unwrap();
    assert_eq!(m2.header.name, "empty");
    assert!(m2.functions.is_empty());
}

#[test]
fn test_invalid_magic() {
    let r = UtenModule::from_bytes(b"XXXXsomegarbage");
    assert!(r.is_err());
}

#[test]
fn test_cache_bytes() {
    let m = UtenModule::new("cache_test");
    let cache = m.to_cache_bytes().unwrap();
    assert_eq!(&cache[..4], b"UCCH");
    let m2 = UtenModule::from_bytes(&cache).unwrap();
    assert_eq!(m2.header.name, "cache_test");
}

#[test]
fn test_bytecode_writer_len() {
    let mut w = BytecodeWriter::new();
    assert_eq!(w.len(), 0);
    w.emit(Opcode::Nop);
    assert_eq!(w.len(), 1);
    w.emit(Opcode::PushI32); w.emit_i32(0);
    assert_eq!(w.len(), 6);
}

#[test]
fn test_bytes_mut() {
    let mut w = BytecodeWriter::new();
    w.emit(Opcode::PushNil);
    assert_eq!(w.len(), 1);
    {
        let b = w.bytes_mut();
        b.push(0x01); // PushTrue
    }
    assert_eq!(w.len(), 2);
}

#[test]
fn test_each_opcode_has_info() {
    for byte in 0..=0xFEu8 {
        if byte > 0xB1 && byte < 0xC0 { continue; }
        if byte >= 0xC0 { break; }
        if let Some(op) = Opcode::from_byte(byte) {
            let info = utencore::opcodes::opcode_info(op);
            assert!(!info.mnemonic.is_empty(), "opcode 0x{byte:02x} has no mnemonic");
        }
    }
}
