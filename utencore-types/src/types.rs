//! UtenCore type system and value representation.
//!
//! All runtime values are represented as `UValue`, a tagged union.
//! The type system supports:
//! - Primitive types (Nil, Bool, Int, Float, String, Complex)
//! - GC-tracked heap objects
//! - Inline value types (structs ≤ 24 bytes, stored directly in UValue)
//! - Boxed value types (structs > 24 bytes, stored on value heap)
//! - Native function callbacks
//!
//! ## Value Types (struct)
//!
//! Value types are user-defined structs that live on the VM stack or in
//! frame locals. They are copied by value (not by reference). Small structs
//! (≤ 24 bytes) are stored inline in `UValue::StructInline` — no heap
//! allocation. Larger structs are stored on the GC heap via
//! `UValue::BoxedStruct` but still follow value semantics (copied on assignment).
//!
//! ## TypeRef System
//!
//! `TypeRef` describes a type in the bytecode format. It supports:
//! - Primitive types (I32, F64, Bool, etc.)
//! - Named struct types by StructId
//! - Generic type parameters (indexed)
//! - Generic instantiations (with concrete args)

use std::fmt;
use std::hash::{Hash, Hasher};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

/// Handle to a GC-tracked object (index in GC heap).
pub type GcHandle = u32;

/// A struct type identifier (index into module's structs table).
pub type StructId = u32;

/// A function reference (index into module's function table).
pub type FuncRef = u32;

/// An index into the VM's native function registry.
pub type NativeFuncIdx = u32;

/// A module ID (index into loaded module table).
pub type ModuleId = u16;

/// A string identifier interned in the module's string pool.
pub type StringId = u32;

/// Maximum inline size for value types stored directly in UValue.
/// Value types larger than this are boxed on the GC heap.
/// 
/// 24 bytes covers Point {x:f64,y:f64}, Color {r,g,b,a:u8},
/// Vec4 {x,y,z,w:f32}, and most scripting use cases.
pub const MAX_INLINE_STRUCT_SIZE: usize = 24;

/// Operator method names used by proto-chain dispatch.
/// When an opcode encounters an Object, it walks the prototype chain
/// looking for these method names before falling through to default logic.
pub mod ops {
    pub const ADD: &str = "__add__";
    pub const SUB: &str = "__sub__";
    pub const MUL: &str = "__mul__";
    pub const DIV: &str = "__div__";
    pub const MOD: &str = "__mod__";
    pub const NEG: &str = "__neg__";
    pub const EQ: &str = "__eq__";
    pub const NE: &str = "__ne__";
    pub const LT: &str = "__lt__";
    pub const LE: &str = "__le__";
    pub const GT: &str = "__gt__";
    pub const GE: &str = "__ge__";
    pub const INDEX: &str = "__index__";
    pub const INDEX_SET: &str = "__index_set__";
    pub const CALL: &str = "__call__";
    pub const TO_STRING: &str = "__str__";
    pub const TO_INT: &str = "__int__";
    pub const TO_BOOL: &str = "__bool__";
    pub const ITER: &str = "__iter__";
    pub const NEXT: &str = "__next__";
    pub const CONTAINS: &str = "__contains__";
    pub const LEN: &str = "__len__";
    pub const HASH: &str = "__hash__";
}

/// A type tag stored in bytecode and runtime values.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum ValueTag {
    Nil = 0,
    Bool = 1,
    Int32 = 2,
    Int64 = 3,
    Float32 = 4,
    Float64 = 5,
    String = 6,
    /// Heap-allocated runtime string (not interned)
    HeapString = 7,
    /// Arbitrary-precision integer
    BigInt = 8,
    /// Iterator over a container
    Iterator = 9,
    // Heap-allocated types (tracked by GC)
    Array = 10,
    Map = 11,
    Closure = 12,
    Struct = 13,
    Function = 14,
    NativeFn = 15,
    /// User-defined opaque type (e.g., a wrapped C struct)
    Opaque = 16,
    Namespace = 20,
    Class = 21,
    Object = 22,
    Method = 23,
    Dynamic = 24,
    /// Pair/Lisp cons cell
    Pair = 30,
    Tuple = 31,
    Range = 32,
    Regex = 33,
    Continuation = 34,
    Set = 35,
    Thunk = 36,
    Bytes = 37,
    ByteArray = 38,
    Complex = 39,
    Lambda = 40,
    /// A boxed value type (struct on GC heap)
    BoxedStruct = 41,
}

/// A module-level type reference, used in bytecode StructDef.
///
/// This is the "type system" of the .uclib bytecode format. It tells
/// the VM how struct fields and function params are typed.
#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum TypeRef {
    /// Void / unit type
    Void,
    /// Primitive types
    Bool,
    I8, I16, I32, I64,
    U8, U16, U32, U64,
    F32, F64,
    /// Interned string reference
    String,
    /// Reference to a struct defined in this module
    Struct(StructId),
    /// Generic type parameter (index into enclosing definition's type_params)
    GenericParam(u16),
    /// Instantiated generic type
    GenericInst {
        def_id: StructId,
        args: Vec<TypeRef>,
    },
}

impl TypeRef {
    /// Get the byte size of this type when used as a struct field.
    /// For generic params, returns 0 (unknown until instantiation).
    pub fn byte_size(&self) -> u32 {
        match self {
            TypeRef::Void => 0,
            TypeRef::Bool | TypeRef::I8 | TypeRef::U8 => 1,
            TypeRef::I16 | TypeRef::U16 => 2,
            TypeRef::I32 | TypeRef::U32 | TypeRef::F32 => 4,
            TypeRef::I64 | TypeRef::U64 | TypeRef::F64 => 8,
            TypeRef::String => 4, // StringId
            TypeRef::Struct(_) | TypeRef::GenericInst { .. } => {
                // Size is known from StructDef, but without it return 0
                // Actual size is resolved at module load time
                0
            }
            TypeRef::GenericParam(_) => 0, // unknown until instantiation
        }
    }

    /// Get the alignment requirement of this type.
    pub fn alignment(&self) -> u32 {
        match self {
            TypeRef::Void => 1,
            TypeRef::Bool | TypeRef::I8 | TypeRef::U8 => 1,
            TypeRef::I16 | TypeRef::U16 => 2,
            TypeRef::I32 | TypeRef::U32 | TypeRef::F32 => 4,
            TypeRef::I64 | TypeRef::U64 | TypeRef::F64 => 8,
            TypeRef::String => 4,
            TypeRef::Struct(_) | TypeRef::GenericInst { .. } => 8, // conservative
            TypeRef::GenericParam(_) => 8, // conservative
        }
    }
}

/// A field within a struct definition.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct FieldDef {
    /// Field name (index into string pool)
    pub name: StringId,
    /// Field type
    pub type_ref: TypeRef,
    /// Byte offset from start of struct (computed at load time)
    pub offset: u32,
    /// Byte size (computed at load time)
    pub size: u32,
}

/// A struct type definition (value type).
///
/// Defines the memory layout of a user-defined value type. Stored in
/// the module's bytecode and resolved at module load time.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct StructDef {
    /// Struct name (index into string pool)
    pub name: StringId,
    /// Total byte size
    pub size: u32,
    /// Required alignment
    pub alignment: u32,
    /// Field definitions
    pub fields: Vec<FieldDef>,
    /// Generic type parameter names (empty for non-generic)
    pub generic_params: Vec<StringId>,
    /// Whether to pack the struct (no padding between fields)
    pub is_packed: bool,
}

/// A runtime value in UtenCore.
///
/// Tagged union: small values are stored inline; heap values
/// are stored via GC handle. Complex numbers are also inline.
/// Value types (structs) ≤ 24 bytes are stored inline via StructInline;
/// larger value types use BoxedStruct (GC-allocated but value semantics).
#[derive(Clone)]
pub enum UValue {
    Nil,
    Bool(bool),
    Int32(i32),
    Int64(i64),
    Float32(f32),
    Float64(f64),
    /// Interned string reference
    String(StringId),
    /// GC-tracked heap object
    Gc(GcHandle, ValueTag),
    /// Native (C) function pointer
    NativeFn(NativeFnHandle),
    /// VM-native function (index into VM's native function registry)
    NativeFunc(NativeFuncIdx),
    /// Complex number (inline, no heap allocation)
    Complex { real: f64, imag: f64 },
    /// Inline value type: struct_id + raw bytes (up to 24 bytes).
    /// For structs > 24 bytes, use BoxedStruct instead.
    StructInline(StructId, [u8; MAX_INLINE_STRUCT_SIZE]),
    /// Boxed value type: struct stored on GC heap but with value semantics.
    /// Used for structs too large for inline storage.
    BoxedStruct(StructId, GcHandle),
}

/// Handle to a native C function
#[derive(Clone)]
pub struct NativeFnHandle {
    pub ptr: usize,
    pub name: String,
    pub signature: FuncSignature,
}

/// Function signature for CIB cross-call resolution
#[derive(Debug, Clone, PartialEq)]
pub struct FuncSignature {
    pub param_types: Vec<ValueTag>,
    pub return_type: ValueTag,
    pub is_variadic: bool,
}

/// A heap object (GC-managed)
#[derive(Debug, Clone)]
pub enum HeapObject {
    Array(Vec<UValue>),
    Map(HashMap<UValue, UValue>),
    /// A user-defined struct with named fields
    Struct(Vec<(StringId, UValue)>),
    /// A closure: function index + captured environment + originating module
    Closure { func: FuncRef, captures: Vec<UValue>, module_id: ModuleId },
    /// An opaque C object
    Opaque { type_name: StringId, data: Vec<u8> },
    /// A namespace: named container for members (modules, classes, etc.)
    Namespace { name: StringId, members: Vec<(StringId, UValue)>, module_id: u16 },
    /// A class definition
    Class {
        name: StringId,
        /// Methods: (name_string_id, func_ref)
        methods: Vec<(StringId, FuncRef)>,
        /// Field names
        fields: Vec<StringId>,
        /// Optional parent class (GcHandle to another Class)
        parent: Option<GcHandle>,
        /// Language-agnostic constructor function.
        /// Set by the compiler (py2uc → __init__, ts2uc → constructor, etc.).
        /// The VM calls this when `ClassName(args)` is invoked, NOT a magic name.
        constructor: Option<FuncRef>,
    },
    /// An object instance with prototype chain support.
    /// The `proto` link enables prototype-based OOP (JavaScript-style):
    /// GetAttr walks `proto → proto → ...` until found.
    /// Operator dispatch (__add__, __eq__) also walks the proto chain.
    Object {
        /// Handle to the Class definition (for class-based OOP)
        class_handle: GcHandle,
        /// Field values (indexed by class's field list)
        fields: Vec<UValue>,
        /// Optional [[Prototype]] link — an Object or Class whose
        /// fields/methods are searched when GetAttr doesn't find on self.
        /// None means the prototype chain ends here.
        proto: Option<GcHandle>,
    },
    /// A bound method (class instance + method func ref)
    Method {
        /// The object instance
        object_handle: GcHandle,
        /// Function reference (index into module)
        func: FuncRef,
    },
    /// A boxed dynamic value (reference-semantics wrapper for scripting languages).
    /// The inner value is always heap-resident, enabling mutation and shared references.
    Dynamic(UValue),
    /// Lisp cons cell (pair)
    Pair { car: Box<UValue>, cdr: Box<UValue> },
    /// Fixed-size tuple of values
    Tuple(Vec<UValue>),
    /// Range: start, end, step, exclusive
    Range { start: Box<UValue>, end: Box<UValue>, step: Box<UValue>, exclusive: bool },
    /// Compiled regular expression
    Regex(String, Box<[u8]>),
    /// A continuation / coroutine state
    Continuation {
        /// Saved call frames
        saved_frames: Vec<SavedFrame>,
        /// Saved stack slice
        saved_stack: Vec<UValue>,
        /// Status: 0=running, 1=suspended, 2=dead
        status: u8,
    },
    /// A set of unique values
    Set(HashSet<UValue>),
    /// A thunk for lazy evaluation
    Thunk {
        /// Whether evaluated yet
        evaluated: bool,
        /// Cached value (if evaluated)
        value: Box<UValue>,
        /// Function to evaluate (if not yet)
        func: Option<(ModuleId, FuncRef)>,
        /// Captured environment for evaluation
        captures: Vec<UValue>,
    },
    /// Runtime heap-allocated string (not interned in symbol table)
    HeapString(String),
    /// Arbitrary-precision integer
    BigInt(num_bigint::BigInt),
    /// Iterator over a container
    Iterator {
        /// Handle to the container being iterated
        container_handle: GcHandle,
        /// Current index (for array/string/bytes iteration)
        index: usize,
        /// The tag of the container (ValueTag::Array, ValueTag::Map, etc.)
        container_tag: ValueTag,
    },
    /// Immutable bytes
    Bytes(Vec<u8>),
    /// Mutable byte array
    ByteArray(Vec<u8>),
    /// Lambda: anonymous closure with captured environment
    Lambda { func: FuncRef, captures: Vec<UValue>, module_id: ModuleId },
    /// Boxed value type: raw bytes of struct on GC heap
    BoxedStructBytes(Vec<u8>),
}

/// A saved call frame for continuations / coroutines
#[derive(Debug, Clone)]
pub struct SavedFrame {
    pub func_ref: FuncRef,
    pub module_id: ModuleId,
    pub return_pc: usize,
    pub stack_base: usize,
    pub locals: Vec<UValue>,
    pub captures: Vec<UValue>,
}

// ═══════════════════════════════════════════════════════════
// UValue helpers
// ═══════════════════════════════════════════════════════════

/// The raw byte buffer type used for inline struct storage.
pub type InlineStructBuf = [u8; MAX_INLINE_STRUCT_SIZE];

impl UValue {
    /// Get the value tag identifying this variant.
    pub fn tag(&self) -> ValueTag {
        match self {
            UValue::Nil => ValueTag::Nil,
            UValue::Bool(_) => ValueTag::Bool,
            UValue::Int32(_) => ValueTag::Int32,
            UValue::Int64(_) => ValueTag::Int64,
            UValue::Float32(_) => ValueTag::Float32,
            UValue::Float64(_) => ValueTag::Float64,
            UValue::String(_) => ValueTag::String,
            UValue::Complex { .. } => ValueTag::Complex,
            UValue::NativeFn(_) => ValueTag::NativeFn,
            UValue::NativeFunc(_) => ValueTag::NativeFn, // treat as NativeFn for type checks
            UValue::Gc(_, tag) => *tag,
            UValue::StructInline(_, _) => ValueTag::Struct,
            UValue::BoxedStruct(_, _) => ValueTag::BoxedStruct,
        }
    }

    /// Get the StructId if this is a struct value (inline or boxed).
    pub fn struct_id(&self) -> Option<StructId> {
        match self {
            UValue::StructInline(sid, _) => Some(*sid),
            UValue::BoxedStruct(sid, _) => Some(*sid),
            _ => None,
        }
    }

    /// Check if this value is a value type struct (inline or boxed).
    pub fn is_struct_value(&self) -> bool {
        matches!(self, UValue::StructInline(_, _) | UValue::BoxedStruct(_, _))
    }

    /// Read a field from a struct value as an i32 (for primitives).
    /// `offset` is the byte offset within the struct.
    /// `size` is the byte width of the field.
    pub unsafe fn read_field(&self, offset: u32, size: u32) -> UValue {
        match self {
            UValue::StructInline(_, data) => {
                let start = offset as usize;
                match size {
                    1 => UValue::Int32(data[start] as i32),
                    2 => UValue::Int32(i16::from_le_bytes(
                        data[start..start+2].try_into().unwrap()) as i32),
                    4 => UValue::Int32(i32::from_le_bytes(
                        data[start..start+4].try_into().unwrap())),
                    8 => UValue::Int64(i64::from_le_bytes(
                        data[start..start+8].try_into().unwrap())),
                    _ => UValue::Nil,
                }
            }
            UValue::BoxedStruct(_, h) => {
                // Boxed struct data is in HeapObject::BoxedStructBytes
                // We can't access it here without the GC reference.
                // This should be called through the VM dispatch which has GC access.
                UValue::Nil // placeholder
            }
            _ => UValue::Nil,
        }
    }

    /// Write a primitive value into a struct field at the given offset.
    pub unsafe fn write_field(&mut self, offset: u32, size: u32, val: UValue) {
        let bytes = match size {
            1 => {
                let v = val.as_i32().unwrap_or(0) as i8 as u8;
                vec![v]
            }
            2 => {
                let v = val.as_i32().unwrap_or(0) as i16;
                v.to_le_bytes().to_vec()
            }
            4 => {
                let v = val.as_i32().unwrap_or(0);
                v.to_le_bytes().to_vec()
            }
            8 => {
                let v = val.as_i64().unwrap_or(0);
                v.to_le_bytes().to_vec()
            }
            _ => return,
        };

        match self {
            UValue::StructInline(_, data) => {
                let start = offset as usize;
                let end = start + bytes.len();
                if end <= data.len() {
                    data[start..end].copy_from_slice(&bytes);
                }
            }
            UValue::BoxedStruct(_, _) => {
                // Write-through to GC heap: handled by VM dispatch
            }
            _ => {}
        }
    }

    /// Try to extract as integer, with numeric coercion.
    pub fn as_i32(&self) -> Option<i32> {
        match self {
            UValue::Int32(v) => Some(*v),
            UValue::Int64(v) => Some(*v as i32),
            UValue::Float32(v) => Some(*v as i32),
            UValue::Float64(v) => Some(*v as i32),
            UValue::Bool(true) => Some(1),
            UValue::Bool(false) | UValue::Nil => Some(0),
            _ => None,
        }
    }

    pub fn as_i64(&self) -> Option<i64> {
        match self {
            UValue::Int32(v) => Some(*v as i64),
            UValue::Int64(v) => Some(*v),
            UValue::Float32(v) => Some(*v as i64),
            UValue::Float64(v) => Some(*v as i64),
            UValue::Bool(true) => Some(1),
            UValue::Bool(false) | UValue::Nil => Some(0),
            _ => None,
        }
    }

    pub fn as_bool(&self) -> Option<bool> {
        match self {
            UValue::Bool(b) => Some(*b),
            UValue::Nil => Some(false),
            UValue::Int32(0) => Some(false),
            UValue::Int64(0) => Some(false),
            UValue::Float32(f) => Some(*f != 0.0),
            UValue::Float64(f) => Some(*f != 0.0),
            _ => None,
        }
    }

    /// Truthy conversion (Python-style).
    pub fn truthy(&self) -> bool {
        self.as_bool().unwrap_or(true)
    }
}

// ═══════════════════════════════════════════════════════════
// Hash, Eq, Debug, Display impls
// ═══════════════════════════════════════════════════════════

impl Hash for UValue {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.tag().hash(state);
        match self {
            UValue::Nil => 0u8.hash(state),
            UValue::Bool(b) => b.hash(state),
            UValue::Int32(v) => v.hash(state),
            UValue::Int64(v) => v.hash(state),
            UValue::Float32(v) => v.to_bits().hash(state),
            UValue::Float64(v) => v.to_bits().hash(state),
            UValue::String(sid) => sid.hash(state),
            UValue::Gc(h, _) => h.hash(state),
            UValue::NativeFn(nf) => nf.ptr.hash(state),
            UValue::NativeFunc(idx) => idx.hash(state),
            UValue::Complex { real, imag } => {
                real.to_bits().hash(state);
                imag.to_bits().hash(state);
            }
            UValue::StructInline(sid, data) => {
                sid.hash(state);
                data.hash(state);
            }
            UValue::BoxedStruct(sid, h) => {
                sid.hash(state);
                h.hash(state);
            }
        }
    }
}

impl PartialEq for UValue {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (UValue::Nil, UValue::Nil) => true,
            (UValue::Bool(a), UValue::Bool(b)) => a == b,
            (UValue::Int32(a), UValue::Int32(b)) => a == b,
            (UValue::Int64(a), UValue::Int64(b)) => a == b,
            (UValue::Float32(a), UValue::Float32(b)) => a.to_bits() == b.to_bits(),
            (UValue::Float64(a), UValue::Float64(b)) => a.to_bits() == b.to_bits(),
            (UValue::String(a), UValue::String(b)) => a == b,
            (UValue::Gc(a, ta), UValue::Gc(b, tb)) => a == b && ta == tb,
            (UValue::NativeFn(a), UValue::NativeFn(b)) => a.ptr == b.ptr && a.name == b.name,
            (UValue::NativeFunc(a), UValue::NativeFunc(b)) => a == b,
            (UValue::Complex { real: ra, imag: ia }, UValue::Complex { real: rb, imag: ib }) => {
                ra.to_bits() == rb.to_bits() && ia.to_bits() == ib.to_bits()
            }
            (UValue::StructInline(sa, da), UValue::StructInline(sb, db)) => {
                sa == sb && da[..] == db[..]
            }
            (UValue::BoxedStruct(sa, ha), UValue::BoxedStruct(sb, hb)) => {
                sa == sb && ha == hb
            }
            _ => false,
        }
    }
}

impl Eq for UValue {}

impl fmt::Debug for UValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            UValue::Nil => write!(f, "nil"),
            UValue::Bool(b) => write!(f, "{b}"),
            UValue::Int32(v) => write!(f, "{v}i32"),
            UValue::Int64(v) => write!(f, "{v}i64"),
            UValue::Float32(v) => write!(f, "{v}f32"),
            UValue::Float64(v) => write!(f, "{v}f64"),
            UValue::String(sid) => write!(f, "str#{sid}"),
            UValue::Complex { real, imag } => write!(f, "{real}+{imag}i"),
            UValue::Gc(h, tag) => write!(f, "{tag:?}@{h}"),
            UValue::NativeFn(nf) => write!(f, "native_fn[{}]", nf.name),
            UValue::NativeFunc(idx) => write!(f, "native_func#{idx}"),
            UValue::StructInline(sid, data) => write!(f, "struct#{sid}({}b)", data.len()),
            UValue::BoxedStruct(sid, _) => write!(f, "boxed_struct#{sid}"),
        }
    }
}

impl fmt::Display for UValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            UValue::Nil => write!(f, "nil"),
            UValue::Bool(b) => write!(f, "{b}"),
            UValue::Int32(v) => write!(f, "{v}"),
            UValue::Int64(v) => write!(f, "{v}"),
            UValue::Float32(v) => write!(f, "{v}"),
            UValue::Float64(v) => write!(f, "{v}"),
            UValue::String(sid) => write!(f, "<str#{sid}>"),
            UValue::Complex { real, imag } => write!(f, "{real}+{imag}i"),
            UValue::Gc(_, tag) => write!(f, "<{tag:?}>"),
            UValue::NativeFn(nf) => write!(f, "<fn {}>", nf.name),
            UValue::NativeFunc(idx) => write!(f, "<native_fn#{idx}>"),
            UValue::StructInline(sid, _) => write!(f, "<struct#{sid}>"),
            UValue::BoxedStruct(sid, _) => write!(f, "<boxed_struct#{sid}>"),
        }
    }
}

// ═══════════════════════════════════════════════════════════
// From impls
// ═══════════════════════════════════════════════════════════

impl From<bool> for UValue {
    fn from(v: bool) -> Self { UValue::Bool(v) }
}

impl From<i32> for UValue {
    fn from(v: i32) -> Self { UValue::Int32(v) }
}

impl From<i64> for UValue {
    fn from(v: i64) -> Self { UValue::Int64(v) }
}

impl From<f32> for UValue {
    fn from(v: f32) -> Self { UValue::Float32(v) }
}

impl From<f64> for UValue {
    fn from(v: f64) -> Self { UValue::Float64(v) }
}
