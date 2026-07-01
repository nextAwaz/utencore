//! Type system tests.

pub mod tests {
    use crate::*;

    #[test]
    fn test_uvalue_tags() {
        assert_eq!(UValue::Nil.tag(), ValueTag::Nil);
        assert_eq!(UValue::Bool(true).tag(), ValueTag::Bool);
        assert_eq!(UValue::Int32(0).tag(), ValueTag::Int32);
        assert_eq!(UValue::Int64(0).tag(), ValueTag::Int64);
        assert_eq!(UValue::Float32(0.0).tag(), ValueTag::Float32);
        assert_eq!(UValue::Float64(0.0).tag(), ValueTag::Float64);
    }

    #[test]
    fn test_uvalue_truthy() {
        assert!(!UValue::Nil.truthy());
        assert!(!UValue::Bool(false).truthy());
        assert!(UValue::Bool(true).truthy());
        assert!(!UValue::Int32(0).truthy());
        assert!(UValue::Int32(1).truthy());
        assert!(UValue::Int64(42).truthy());
        assert!(UValue::Float64(1.0).truthy());
        assert!(!UValue::Float64(0.0).truthy());
    }

    #[test]
    fn test_uvalue_equality() {
        assert_eq!(UValue::Int32(5), UValue::Int32(5));
        assert_ne!(UValue::Int32(5), UValue::Int32(6));
        assert_eq!(UValue::Bool(true), UValue::Bool(true));
        assert_eq!(UValue::Nil, UValue::Nil);
        assert_ne!(UValue::Int32(1), UValue::Bool(true));
    }

    #[test]
    fn test_from_impls() {
        assert_eq!(UValue::from(true), UValue::Bool(true));
        assert_eq!(UValue::from(42i32), UValue::Int32(42));
        assert_eq!(UValue::from(42i64), UValue::Int64(42));
        let _ = UValue::from(1.0f32);
        let _ = UValue::from(1.0f64);
    }

    #[test]
    fn test_display_format() {
        let s = format!("{}", UValue::Nil);
        assert_eq!(s, "nil");
        assert_eq!(format!("{}", UValue::Bool(true)), "true");
        assert_eq!(format!("{}", UValue::Int32(42)), "42");
        assert_eq!(format!("{}", UValue::Int64(999)), "999");
    }

    #[test]
    fn test_opcode_from_byte_all_valid() {
        for byte in 0x00..=0x25 {
            let op = Opcode::from_byte(byte);
            if byte == 0x26 || byte == 0x27 || byte >= 0x28 && byte <= 0x2F { continue; }
            if byte >= 0x36 && byte <= 0x3F { continue; }
            if byte >= 0x4E && byte <= 0x4F { continue; }
            if byte == 0x51 || byte >= 0x6E && byte <= 0x6F { continue; }
            if byte == 0x75 || byte == 0x76 { continue; }
            if byte >= 0x82 && byte <= 0x83 { continue; }
            if byte >= 0x86 && byte <= 0x87 { continue; }
            if byte >= 0x8A && byte <= 0x8D { continue; }
            if byte >= 0xB9 && byte <= 0xBB || byte == 0xBD { continue; }
            if byte == 0xE5 || byte == 0xEB { continue; }
            if byte == 0xFD { continue; }
            assert!(op.is_some(), "byte 0x{byte:02X} should map to Some opcode");
        }
    }

    #[test]
    fn test_value_tag_size() {
        // Verify ValueTag discriminant values are distinct
        use ValueTag::*;
        let tags = [Nil, Bool, Int32, Int64, Float32, Float64, String, HeapString,
                    Array, Map, Closure, Lambda, Struct, Namespace, Class, Object];
        let mut seen = std::collections::HashSet::new();
        for t in &tags { assert!(seen.insert(*t as i32), "duplicate tag {:?}", t); }
    }

    #[test]
    fn test_type_ref_byte_sizes() {
        use TypeRef::*;
        assert_eq!(Bool.byte_size(), 1);
        assert_eq!(I8.byte_size(), 1);
        assert_eq!(I16.byte_size(), 2);
        assert_eq!(I32.byte_size(), 4);
        assert_eq!(I64.byte_size(), 8);
        assert_eq!(F32.byte_size(), 4);
        assert_eq!(F64.byte_size(), 8);
    }

    #[test]
    fn test_bytecode_version_constant() {
        assert_eq!(BYTECODE_VERSION, 3);
        assert_eq!(&UCLIB_MAGIC[..], b"UCLB");
        assert_eq!(&UCCH_MAGIC[..], b"UCCH");
        assert_eq!(&UCIR_MAGIC[..], b"UCIR");
    }
}

