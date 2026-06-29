# Type System

**Document version:** 1.0 | **VM version:** >=0.0.5 | **Bytecode version:** >=3

## UValue (Runtime Value)

All runtime values are represented as a tagged union:

```rust
pub enum UValue {
    Nil,
    Bool(bool),
    Int32(i32),
    Int64(i64),
    Float32(f32),
    Float64(f64),
    String(StringId),              // Interned string pool reference
    Gc(GcHandle, ValueTag),        // GC-tracked heap object
    NativeFn(NativeFnHandle),      // C function pointer
    NativeFunc(NativeFuncIdx),     // VM-native Rust function (registry index)
    Complex { real: f64, imag: f64 },
    StructInline(StructId, [u8; 24]), // Value type ≤ 24 bytes
    BoxedStruct(StructId, GcHandle),  // Value type > 24 bytes (on GC heap)
}
```

Small values (Nil, Bool, Int, Float) are stored inline. Complex numbers are also inline (two f64s). GC-tracked objects, heap strings, containers, and closures use `GcHandle`.

## HeapObject (GC-Managed)

```rust
pub enum HeapObject {
    Array(Vec<UValue>),
    Map(HashMap<UValue, UValue>),
    Struct(Vec<(StringId, UValue)>),
    Closure { func: FuncRef, captures: Vec<UValue>, module_id: ModuleId },
    Opaque { type_name: StringId, data: Vec<u8> },
    Namespace { name: StringId, members: Vec<(StringId, UValue)>, module_id: u16 },
    Class { name, methods, fields, parent, constructor },
    Object { class_handle, fields, proto },
    Method { object_handle, func },
    Dynamic(UValue),
    Pair { car, cdr },
    Tuple(Vec<UValue>),
    Range { start, end, step, exclusive },
    Regex(String, Box<[u8]>),
    Continuation { saved_frames, saved_stack, status },
    Set(HashSet<UValue>),
    Thunk { evaluated, value, func, captures },
    HeapString(String),
    BigInt(num_bigint::BigInt),
    Bytes(Vec<u8>),
    ByteArray(Vec<u8>),
    Lambda { func, captures, module_id },
    Iterator { container_handle, index, container_tag },
}
```

## ValueTag

Each `UValue::Gc` carries a `ValueTag` for runtime type identification:

```rust
pub enum ValueTag {
    Nil, Bool, Int32, Int64, Float32, Float64, String, HeapString,
    BigInt, Iterator, Array, Map, Closure, Struct, Function, NativeFn,
    Opaque, Namespace, Class, Object, Method, Dynamic,
    Pair, Tuple, Range, Regex, Continuation, Set, Thunk,
    Bytes, ByteArray, Complex, Lambda, BoxedStruct,
}
```

## TypeRef (Bytecode Type Descriptor)

```rust
pub enum TypeRef {
    Void, Bool,
    I8, I16, I32, I64,
    U8, U16, U32, U64,
    F32, F64,
    String,
    Struct(StructId),
    GenericParam(u16),
    GenericInst { def_id: StructId, args: Vec<TypeRef> },
}
```

## Value Type System

Value types (user-defined structs) support inline and boxed layouts:
- **Inline** (≤ 24 bytes): stored directly in `UValue::StructInline`
- **Boxed** (> 24 bytes): stored on GC heap via `UValue::BoxedStruct`
- Both follow value semantics (copied on assignment)

## Operator Overloading

Objects can implement operators through the prototype chain. The VM looks for these method names:

- `__add__`, `__sub__`, `__mul__`, `__div__`, `__mod__`, `__neg__`
- `__eq__`, `__ne__`, `__lt__`, `__le__`, `__gt__`, `__ge__`
- `__index__`, `__index_set__`, `__call__`
- `__str__`, `__int__`, `__bool__`
- `__iter__`, `__next__`, `__contains__`, `__len__`, `__hash__`

## Numeric Promotion

Operations between different numeric types promote to the wider type:
- i32 + i64 → i64
- i32 + f64 → f64
- BigInt + i64 → BigInt
