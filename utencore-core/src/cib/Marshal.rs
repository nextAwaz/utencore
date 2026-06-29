//! C type system and UValue ↔ raw bytes marshalling.
//!
//! All C types that the CIB understands, plus functions to
//! convert UtenCore values into raw C values and back.

use std::mem;
use utencore_types::*;

// ── C type definitions ──

/// All C types the CIB can marshal.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum CType {
    Void,
    Bool,
    Char, UChar,
    Short, UShort,
    Int, UInt,
    Long, ULong,
    LongLong, ULongLong,
    Float, Double,
    Pointer(Box<CType>),
    ConstPointer(Box<CType>),
    FuncPtr(Box<CType>),
    Array(Box<CType>, usize),
    Struct(String),
    Enum(String, Box<CType>),
    VarArg,
}

impl CType {
    /// Size of this type in bytes.
    pub fn size(&self) -> usize {
        match self {
            CType::Void => 0,
            CType::Bool => 1,
            CType::Char | CType::UChar => 1,
            CType::Short | CType::UShort => 2,
            CType::Int | CType::UInt => 4,
            CType::Float => 4,
            CType::Long | CType::ULong => {
                if mem::size_of::<i64>() == 8 { 8 } else { 4 }
            }
            CType::LongLong | CType::ULongLong => 8,
            CType::Double => 8,
            CType::Pointer(_) | CType::ConstPointer(_) | CType::FuncPtr(_) => mem::size_of::<usize>(),
            CType::Array(inner, n) => inner.size() * n,
            CType::Struct(_) | CType::Enum(_, _) => mem::size_of::<usize>(),
            CType::VarArg => 0,
        }
    }

    /// Alignment of this type in bytes.
    pub fn alignment(&self) -> usize {
        self.size() // simplified — real alignment is more nuanced
    }
}

// ── Marshalled value ──

/// A marshalled C value ready for FFI call.
#[derive(Debug, Clone)]
pub enum MarshalledValue {
    Void,
    Int(i64),
    UInt(u64),
    Float(f64),
    Ptr(usize),
    Struct(Vec<u8>),
    Array(Vec<u8>),
    /// C string (null-terminated, owned)
    CString(Vec<u8>),
}

impl MarshalledValue {
    /// Write this value into a byte buffer.
    pub fn write_to(&self, buf: &mut [u8]) {
        match self {
            MarshalledValue::Int(v) => {
                let bytes = v.to_le_bytes();
                let len = buf.len().min(bytes.len());
                buf[..len].copy_from_slice(&bytes[..len]);
            }
            MarshalledValue::UInt(v) => {
                let bytes = v.to_le_bytes();
                let len = buf.len().min(bytes.len());
                buf[..len].copy_from_slice(&bytes[..len]);
            }
            MarshalledValue::Float(v) => {
                let bytes = v.to_le_bytes();
                let len = buf.len().min(bytes.len());
                buf[..len].copy_from_slice(&bytes[..len]);
            }
            MarshalledValue::Ptr(v) => {
                let p = *v as usize;
                let bytes = p.to_le_bytes();
                let len = buf.len().min(bytes.len());
                buf[..len].copy_from_slice(&bytes[..len]);
            }
            MarshalledValue::Struct(data) | MarshalledValue::Array(data) => {
                let len = buf.len().min(data.len());
                buf[..len].copy_from_slice(&data[..len]);
            }
            MarshalledValue::CString(data) => {
                let len = buf.len().min(data.len());
                buf[..len].copy_from_slice(&data[..len]);
            }
            MarshalledValue::Void => {}
        }
    }
}

// ── Marshal (UValue → raw C) ──

/// Marshal a UValue into a raw C representation.
pub fn marshal(val: &UValue, ctype: &CType) -> Result<MarshalledValue, String> {
    match (val, ctype) {
        // Integer promotion
        (UValue::Int32(v), _) if ctype_int(ctype) => Ok(int_marshal(*v as i64, ctype)),
        (UValue::Int64(v), _) if ctype_int(ctype) => Ok(int_marshal(*v, ctype)),
        (UValue::Bool(v), _) if ctype_int(ctype) => Ok(MarshalledValue::Int(if *v { 1 } else { 0 })),
        (UValue::Bool(v), CType::Bool) => Ok(MarshalledValue::Int(if *v { 1 } else { 0 })),

        // Float
        (UValue::Float32(v), CType::Float) => Ok(MarshalledValue::Float(*v as f64)),
        (UValue::Float32(v), CType::Double) => Ok(MarshalledValue::Float(*v as f64)),
        (UValue::Float64(v), CType::Float) => Ok(MarshalledValue::Float(*v as f64)),
        (UValue::Float64(v), CType::Double) => Ok(MarshalledValue::Float(*v)),
        (UValue::Int32(v), CType::Float) => Ok(MarshalledValue::Float(*v as f64)),
        (UValue::Int64(v), CType::Double) => Ok(MarshalledValue::Float(*v as f64)),

        // Pointer from nil
        (UValue::Nil, CType::Pointer(_) | CType::ConstPointer(_) | CType::FuncPtr(_)) =>
            Ok(MarshalledValue::Ptr(0)),

        // Pointer from GC handle
        (UValue::Gc(h, _), CType::Pointer(_) | CType::ConstPointer(_)) =>
            Ok(MarshalledValue::Ptr(*h as usize)),

        // Function pointer
        (UValue::NativeFn(nf), CType::FuncPtr(_)) =>
            Ok(MarshalledValue::Ptr(nf.ptr)),

        // Int as pointer (user data)
        (UValue::Int64(v), CType::Pointer(_)) =>
            Ok(MarshalledValue::Ptr(*v as usize)),

        // C string from String (passed as pointer to null-terminated bytes)
        (UValue::String(_), CType::Pointer(inner))
            if matches!(**inner, CType::Char | CType::Void) =>
        {
            Err("String marshalling requires module string table access — use marshal_with_str".into())
        }

        _ => Err(format!(
            "Cannot marshal {:?} as {:?}", val.tag(), ctype
        )),
    }
}

fn ctype_int(ct: &CType) -> bool {
    matches!(ct, CType::Int | CType::UInt | CType::Long | CType::ULong
        | CType::LongLong | CType::ULongLong | CType::Short | CType::UShort
        | CType::Char | CType::UChar)
}

fn int_marshal(v: i64, ct: &CType) -> MarshalledValue {
    match ct {
        CType::Char | CType::UChar => MarshalledValue::Int(v & 0xFF),
        CType::Short | CType::UShort => MarshalledValue::Int(v & 0xFFFF),
        CType::Int | CType::UInt => MarshalledValue::Int(v & 0xFFFF_FFFF),
        _ => MarshalledValue::Int(v),
    }
}

// ── Unmarshal (raw C → UValue) ──

/// Read a raw C value from bytes and convert it to a UValue.
pub fn unmarshal(bytes: &[u8], ctype: &CType) -> Result<UValue, String> {
    if bytes.len() < ctype.size() {
        return Ok(UValue::Nil);
    }

    Ok(match ctype {
        CType::Void => UValue::Nil,
        CType::Bool => UValue::Bool(bytes[0] != 0),
        CType::Char => UValue::Int32(bytes[0] as i8 as i32),
        CType::UChar => UValue::Int32(bytes[0] as i32),
        CType::Short => {
            UValue::Int32(i16::from_le_bytes([bytes[0], bytes[1]]) as i32)
        }
        CType::UShort => {
            UValue::Int32(u16::from_le_bytes([bytes[0], bytes[1]]) as i32)
        }
        CType::Int => {
            UValue::Int32(i32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
        }
        CType::UInt => {
            UValue::Int64(u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]) as i64)
        }
        CType::Long => {
            if mem::size_of::<i64>() == 8 {
                UValue::Int64(i64::from_le_bytes(bytes[..8].try_into().unwrap()))
            } else {
                UValue::Int32(i32::from_le_bytes(bytes[..4].try_into().unwrap()))
            }
        }
        CType::ULong => {
            if mem::size_of::<i64>() == 8 {
                UValue::Int64(u64::from_le_bytes(bytes[..8].try_into().unwrap()) as i64)
            } else {
                UValue::Int64(u32::from_le_bytes(bytes[..4].try_into().unwrap()) as i64)
            }
        }
        CType::LongLong => {
            UValue::Int64(i64::from_le_bytes(bytes[..8].try_into().unwrap()))
        }
        CType::ULongLong => {
            UValue::Int64(u64::from_le_bytes(bytes[..8].try_into().unwrap()) as i64)
        }
        CType::Float => {
            UValue::Float32(f32::from_le_bytes(bytes[..4].try_into().unwrap()))
        }
        CType::Double => {
            UValue::Float64(f64::from_le_bytes(bytes[..8].try_into().unwrap()))
        }
        CType::Pointer(_) | CType::ConstPointer(_) | CType::FuncPtr(_) => {
            let ptr = usize::from_le_bytes(bytes[..mem::size_of::<usize>()].try_into().unwrap());
            if ptr == 0 { UValue::Nil } else { UValue::Int64(ptr as i64) }
        }
        _ => UValue::Nil,
    })
}
