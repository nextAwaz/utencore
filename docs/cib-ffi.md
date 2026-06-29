# CIB — Native Function Interface

**Document version:** 1.0 | **VM version:** >=0.0.5 | **Bytecode version:** >=3

CIB (Central Interface Bridge) provides the FFI mechanism for calling C functions from bytecode.

## Architecture

```
Bytecode → CibCall/CibCallTyped → libffi → native C function
                                        → return value marshalled back
```

Components:
- **CibEngine** — Central registry for libraries, interfaces, struct layouts
- **Ffi** (Cib/Ffi.rs) — Raw libffi bindings (prepare_cif, call)
- **Marshal** (Cib/Marshal.rs) — UValue ↔ C type conversion
- **Structs** (Cib/Structs.rs) — C struct layout computation
- **Ucif** (Cib/Ucif.rs) — UCIF interface definition parsing

## Typical Usage

```rust
// 1. Load a library
CibLoad "libm"

// 2. Resolve a symbol
//    (pushes Opaque handle containing function pointer)
CibSym "sqrt"

// 3. Call it
//    push args, then CibCall
```

For typed calls with full signature validation, use `CibLoadInterface` + `CibCallTyped`:

```rust
// 1. Load a UCIF interface definition
CibLoadInterface "libc"

// 2. Call a typed function
//    (automatically marshals args/unmarshals return value)
CibCallTyped func_idx
```

## UCIF Interface Files

UCIF (`.ucif`) files define C library interfaces in JSON:

```json
{
    "name": "libc",
    "version": "1.0",
    "libraries": ["libc.so.6"],
    "functions": [
        {
            "name": "printf",
            "ret": "Int",
            "params": [
                {"name": "fmt", "type": "Pointer(Char)"}
            ],
            "variadic": true
        }
    ],
    "structs": [...],
    "constants": [...]
}
```

## Type Mapping

| UtenCore | C Type |
|----------|--------|
| Nil | void |
| Bool | bool (u8) |
| Int32 | int (32-bit) |
| Int64 | long long |
| Float32 | float |
| Float64 | double |
| String | Pointer(Char) |
| Opaque | Pointer(Void) |

## CibCall vs CibCallTyped

| Aspect | CibCall | CibCallTyped |
|--------|---------|-------------|
| Signature | From NativeFnHandle | From UCIF interface |
| Validation | None | Full type checking |
| Marshalling | Automatic via ValueTag | Automatic via UCIF |
| Performance | CIF prepared per-call | CIF cached at load time |
| Best for | Ad-hoc calls, prototyping | Production use |

## Platform Support

CIB handles platform differences in:
- Library naming (`.so`, `.dylib`, `.dll`)
- `errno` location (Linux `__errno_location`, macOS `__error`, Windows `_errno`)
- Calling conventions (via libffi)
