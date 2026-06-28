# UtenCore Architecture

## Overview

UtenCore is a stack-based virtual machine designed as a universal backend for scripting language compilers. It provides a language-agnostic bytecode format, a runtime with pluggable garbage collection, a native function interface (CIB), and a compiler plugin system (CCIS).

### Design Tenets

1. **Language-agnostic bytecode** — The opcode set and type system are designed to represent constructs from diverse languages (Python, JavaScript/Lua, Lisp, etc.) without bias toward any single source language.
2. **Compiler ↔ VM separation** — Compilers are plugins (CCIS). The VM does not contain language-specific logic. A compiler produces `.uclib` bytecode; the VM executes it.
3. **Safety through verification** — Bytecode loaded from untrusted sources passes through a verifier that checks opcode validity, operand ranges, and stack consistency before execution.
4. **Capability-based security** — Low-level operations (memory allocation, FFI, GC control) are gated behind the `utencore.Unsafe` namespace. User code must explicitly import `utencore` to access these capabilities.

---

## Bytecode Format

### File Types

| Extension | Magic  | Purpose |
|-----------|--------|---------|
| `.uclib`  | `UCLB` | Compiled library (manually produced) |
| `.ucch`   | `UCCH` | Compiled cache (auto-generated) |

### Module Structure (UtenModule)

Serialized via `bincode`:

```
magic: [u8; 4]              UCLB or UCCH
version: (u16, u16)         VM version
bytecode_version: u32       Format version (checked for VM compatibility)
header: ModuleHeader
  name: String
  source_lang: String       e.g. "python", "typescript"
  compiler: String          e.g. "py2uc", "ts2uc"
  compiler_version: String
  gc_strategy: String       "generational" | "mark-sweep" | "refcount" | "none"
  jit_recommended: bool
  metadata: HashMap<String, String>  Compiler-specific key-value data
  line_map: Vec<LineEntry>  Source line number mapping

strings: Vec<String>        Interned string pool (indexed by StringId)
constants: Vec<ConstValue>  Inline constant values (Nil, Bool, Int, Float, StringId)
functions: Vec<FunctionDef> Bytecode function definitions
  name: String
  bytecode: Vec<u8>         Raw opcode sequence
  n_locals: u16
  n_params: u16
  is_variadic: bool
  n_captures: u16

globals: Vec<GlobalDef>     Global variable descriptors
exports: HashMap<String, ExportEntry>  Named exports (Function, Global, Type)
imports: Vec<ImportEntry>   Cross-module import declarations
exceptions: Vec<ExceptionTableEntry>
  try_start: u32
  try_end: u32
  handler_pc: u32
  catch_type: Option<StringId>
  finally_pc: Option<u32>
```

### Bytecode Versioning

Each module carries a `bytecode_version`. The VM accepts modules where `module.bytecode_version ≤ VM.BYTECODE_VERSION`. This allows forward compatibility: older modules run on newer VMs.

---

## Opcode Set

256 opcodes in a 16×16 grid.

### 0x00–0x0F: Stack Manipulation

`Nop`, `PushNil`, `PushTrue`, `PushFalse`, `PushI32`, `PushI64`, `PushF32`, `PushF64`, `PushString`, `PushConst`, `Dup`, `DupN`, `Swap`, `Pop`, `PopN`, `Rot`

### 0x10–0x1F: Integer Arithmetic

`Add`, `Sub`, `Mul`, `Div`, `Mod`, `Neg`, `Inc`, `Dec`, `Abs`, `Pow`, `CheckedAdd`, `CheckedSub`, `CheckedMul`, `SaturatingAdd`, `SaturatingSub`, `WrappingAdd`

### 0x20–0x2F: Float Arithmetic

`FAdd`, `FSub`, `FMul`, `FDiv`, `FMod`, `FNeg`, `FPow`, `FSqrt`, `FAbs`, `FFloor`, `FCeil`, `FRound`, `FSin`, `FCos`, `FTan`, `FAtan2`

### 0x30–0x3F: Bitwise Operations

`BitAnd`, `BitOr`, `BitXor`, `BitNot`, `Shl`, `Shr`, `UShr`, `RotLeft`, `RotRight`, `PopCount`, `LeadingZeros`, `TrailingZeros`, `ByteSwap`, `BitReverse`, `UDiv`, `UMod`

### 0x40–0x4F: Comparison & Logical

`Eq`, `Ne`, `Lt`, `Le`, `Gt`, `Ge`, `Cmp`, `Is`, `IsNot`, `In`, `NotIn`, `And`, `Or`, `Not`, `Xor`, `Truthy`

The `Add` opcode has polymorphic behavior: String + String → concatenation, Complex + Complex → complex arithmetic, BigInt + BigInt → arbitrary-precision arithmetic, numeric + numeric → integer arithmetic, otherwise → string concatenation fallback.

### 0x50–0x5F: Type & Conversion

`TypeOf`, `IsType`, `ToI32`, `ToI64`, `ToF32`, `ToF64`, `ToBool`, `ToString`, `Cast`, `BitCast`, `EnumCreate`, `EnumMatch`, `CheckIndex`, `CheckType`, `TypeAssert`, `Unreachable`

### 0x60–0x6F: Control Flow

`Jump`, `JumpIfFalse`, `JumpIfTrue`, `JumpIfEq`, `JumpIfNe`, `JumpTable`, `ForPrep`, `ForStep`, `Loop`, `Switch`, `MatchCheck`, `Bind`, `GetIter`, `Next`, `Await`, `AsyncCall`

The `GetIter` (0x6C) opcode creates an iterator over a container (Array, Map, Set, String). The `Next` (0x6D) opcode advances the iterator, pushing the next value or jumping past the loop body when exhausted.

### 0x70–0x7F: Call & Return

`Call`, `CallValue`, `CallMethod`, `TailCall`, `TailCallValue`, `Invoke`, `SuperCall`, `Apply`, `Return`, `ReturnValue`, `ReturnMultiple`, `MakeClosure`, `Capture`, `LoadUpvalue`, `StoreUpvalue`, `Curry`

`CallValue` can dispatch to Closure, Lambda, NativeFn, and NativeFunc values. `CallMethod` performs virtual dispatch through an object's class method table.

### 0x80–0x8F: Variables & Environment

`LoadLocal`, `StoreLocal`, `LoadCapture`, `StoreCapture`, `LoadGlobal`, `StoreGlobal`, `LoadDynGlobal`, `StoreDynGlobal`, `AllocFrame`, `LoadArg`, `LoadModuleVar`, `StoreModuleVar`, `LoadUpvalueFrom`, `StoreUpvalueTo`, `This`, `ArgCount`

### 0x90–0x9F: Array & List

`NewArray`, `ArrayLen`, `ArrayGet`, `ArraySet`, `ArrayPush`, `ArrayPop`, `ArrayUnshift`, `ArrayShift`, `ArrayInsert`, `ArrayRemove`, `ArraySlice`, `ArrayConcat`, `ArrayContains`, `ArrayIndexOf`, `ArraySort`, `ArrayReverse`

### 0xA0–0xAF: Map, Set, Range, Tuple

`NewMap`, `MapGet`, `MapSet`, `MapDel`, `MapContains`, `MapKeys`, `MapLen`, `NewSet`, `SetAdd`, `SetRemove`, `SetContains`, `SetLen`, `SetUnion`, `SetIntersect`, `NewRange`, `Tuple`

Map and Set use `HashMap<UValue, UValue>` / `HashSet<UValue>` internally for O(1) average-case access. UValue implements `Hash` and `Eq` — GC object keys use handle identity (consistent with reference semantics).

### 0xB0–0xBF: String & Regex

`StrConcat`, `StrLen`, `StrGet`, `StrSub`, `StrContains`, `StrIndexOf`, `StrReplace`, `StrSplit`, `StrJoin`, `StrToUpper`, `StrToLower`, `StrTrim`, `StrCmp`, `StrFormat`, `RegexCompile`, `RegexMatch`

String ops handle both interned strings (`UValue::String(StringId)`) and heap-allocated strings (`HeapObject::HeapString`).

### 0xC0–0xCF: OOP & Type System

`NewNamespace`, `NewClass`, `NewObject`, `ClassAddMethod`, `ClassAddField`, `ClassSetParent`, `GetAttr`, `SetAttr`, `HasAttr`, `InstanceOf`, `GetField`, `SetField`, `GetFieldIdx`, `SetFieldIdx`, `HasField`, `Property`

Class objects support single inheritance via the `parent` field. Method dispatch searches the class and its ancestors linearly (no vtable or inline cache — current limitation).

### 0xD0–0xDF: Functional & Coroutine

`Cons`, `Car`, `Cdr`, `List`, `IsList`, `MapFn`, `FilterFn`, `ReduceFn`, `Compose`, `Delay`, `Force`, `MakeCoroutine`, `CoroutineStatus`, `CoroutineYield`, `ResumeWith`, `Continuation`

### 0xE0–0xEF: CIB, Module, Plugin

`CibLoad`, `CibSym`, `CibCall`, `CibWrap`, `CibUnwrap`, `CibFree`, `CibStrToC`, `CibStrFromC`, `CibSizeOf`, `CibCallTyped`, `CibLoadInterface`, `CibStructPack`, `Import`, `ImportFunc`, `ImportValue`, `Export`

The `Import` opcode applies namespace alias resolution (`resolve_ns_alias`) before loading a module, enabling flexible namespace remapping at runtime.

### 0xF0–0xFF: GC, JIT, Debug

`Alloc`, `GcCollect`, `GcPin`, `GcUnpin`, `GcStats`, `WriteBarrier`, `GcSetThreshold`, `JitCompile`, `JitInvalidate`, `JitStat`, `Print`, `Trace`, `Breakpoint`, `Line`, `Halt`, `Raise`

---

## Type System

### UValue (Runtime Value)

```
enum UValue {
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
}
```

Small values (Nil, Bool, Int, Float) are stored inline. GC-tracked objects, heap strings, BigInts, containers, and closures are heap-allocated via `GcHandle`. Complex numbers are inline (two f64s).

### HeapObject (GC-Managed)

```
enum HeapObject {
    Array(Vec<UValue>),
    Map(HashMap<UValue, UValue>),
    Struct(Vec<(StringId, UValue)>),
    Closure { func, captures, module_id },
    Opaque { type_name, data },
    Namespace { name, members },
    Class { name, methods, fields, parent },
    Object { class_handle, fields },
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

### ValueTag

Each `UValue::Gc` carries a `ValueTag` that identifies the `HeapObject` variant. The GC stores the tag alongside the object, enabling runtime type assertions and safe downcasting.

---

## Garbage Collection

### GcEngine Trait

```rust
pub trait GcEngine: Send {
    fn alloc(&mut self, obj: HeapObject) -> GcHandle;
    fn get(&self, handle: GcHandle) -> &HeapObject;
    fn get_mut(&mut self, handle: GcHandle) -> &mut HeapObject;
    fn collect(&mut self, vm: &mut Vm);
    fn pin(&mut self, handle: GcHandle);
    fn unpin(&mut self, handle: GcHandle);
    fn is_valid(&self, handle: GcHandle) -> bool;
    fn stats(&self) -> GcStats;
    fn strategy_name(&self) -> &str;
    fn shutdown(&mut self);
}
```

### Generational GC (Default)

Two-generation heap:
- **Gen0 (nursery):** ~10,000 object capacity. Collected frequently.
- **Gen1 (tenured):** Objects promoted after surviving 2 gen0 collections.
- **Major GC:** When gen0 fills 5 times without a full promotion cycle.
- **Root scanning:** VM stack + frame locals → trace reachable objects.
- **Pin:** Pinned objects are never collected or moved.

Known limitation: The generational GC does not compact the heap. Long-running workloads may experience fragmentation.

### Mark-Sweep GC

Simpler alternative: mark phase traces from roots, sweep phase frees unmarked objects.

### RefCount GC

Reference counting with no cycle detection. Inheritance: `retain()`/`release()` are currently unused (no automatic reference counting on UValue clone/drop).

---

## Module System

### Loading Pipeline

```
.uclib file → ModuleLoader::load()
  → Verify magic bytes (UCLB/UCCH)
  → Deserialize via bincode
  → Check bytecode_version ≤ VM BYTECODE_VERSION
  → Quick-verify (magic check, empty bytecode check)
  → Vm::load_module()
    → Allocate globals
    → Apply GC strategy from module header
    → Register CIB interfaces from metadata
    → Return ModuleId
```

### Cross-Module Imports

The `Import` (0xEC) opcode resolves module names through:
1. Namespace alias resolution (`Vm::resolve_ns_alias`)
2. Module loader (UCSL registry, local filesystem paths)
3. Module loading and init function execution

The `ImportFunc` (0xED) opcode retrieves an exported value from a loaded module by name.

### UCSL (UtenCore Shared Libraries)

Shared `.uclib` libraries are discovered through standard search paths:
- `./ucsl/` (project-local)
- `~/.utencore/ucsl/` (user-global)
- `/usr/share/utencore/ucsl/` (system-wide)
- Executable-relative `../ucsl/`

Dotted names (`utenstd.math`) are resolved to filesystem paths (`ucsl/utenstd/math.uclib`). Each library may have a companion `.ucsl` JSON manifest declaring exports and dependencies.

---

## Compiler Plugin System (CCIS)

### Architecture

```
CCIS Manifest:
  name: "py2uc"
  extensions: ["py"]
  default_gc: "generational"
  cib_interfaces: ["python3"]

CompileContext:
  source: &str           Source code
  filename: &str         Source filename
  module: &mut UtenModule  Output module (filled by compiler)
  options: &CompilerOptions  GC strategy, optimization level, debug flags

CompileError:
  message: String
  file: String
  line: usize
  col: usize
  level: ErrorLevel    Error | Warning | Note
```

### Compilation Flow

```
source.py → PluginManager::compile("source.py")
  → Find compiler by extension (.py → py2uc)
  → Create UtenModule
  → Create CompileContext(source, filename, &mut module, options)
  → Call compile_fn(&mut ctx)
  → Compiler fills module directly
  → Apply default GC strategy from manifest
  → Return UtenModule (no serialization round-trip)
```

### Plugin Types

- **In-process:** Linked as a Rust crate, registers via `PluginManager::register_compiler(manifest, compile_fn)`.
- **Dynamic:** `.so`/`.dll` loaded at runtime. Exports `ccis_init()` for manifest and `ccis_compile()` for compilation dispatch.

---

## Native Function Interface

### VmNativeFn

Rust closures callable from bytecode:

```rust
pub struct VmNativeFn(pub Arc<dyn Fn(&mut Vm, &[UValue]) -> UtenResult<UValue> + Send + Sync>);
```

Registered functions are indexed in `Vm::native_funcs`. Bytecode code references them via `UValue::NativeFunc(index)`. When `CallValue` encounters a `NativeFunc`, it clones the Arc, releases the borrow on `native_funcs`, and invokes the closure.

### utencore.Unsafe

The built-in `utencore` module exports an `Unsafe` namespace containing:

- **Memory:** `alloc`, `free`, `read_byte`, `write_byte`, `read_i32`, `write_i32`, `memcpy`, `memset`
- **GC:** `gc_collect`, `gc_pin`
- **CIB:** `dlopen`, `dlsym` (via libloading)
- **Type:** `type_of`, `cast`, `alloc_obj`
- **Namespace:** `alias_ns` (creates runtime namespace aliases)

---

## Error Handling

### Runtime Errors

The `Raise` (0xFF) opcode triggers exception handling:

1. Pop the exception value from the stack.
2. Search the active exception handler stack (from innermost outward).
3. For each handler, check if `handler_pc` falls within the handler's `try_start`..`try_end` range.
4. If found: unwind frames, push the exception value, jump to `handler_pc`.
5. If no handler found: propagate as `UtenError::Vm` to the calling context.

Exception handlers are registered by the compiler in the module's `exceptions` table. The `try_start`/`try_end`/`handler_pc` fields map directly to try/except blocks in the source language.

### Compiler Errors

Compilers return `Vec<CompileError>` with structured diagnostics:

```rust
CompileError {
    message: String,    // Human-readable description
    file: String,       // Source file path
    line: usize,        // 1-based line number
    col: usize,         // 0-based column number
    level: ErrorLevel,  // Error | Warning | Note
}
```

---

## Known Limitations

1. **No compacting GC** — The generational collector does not compact the heap. Fragmentation may occur under sustained allocation workloads.
2. **No cycle collection in RefCount GC** — The reference counting implementation does not detect cycles. Use the generational or mark-sweep strategies for workloads with cyclic references.
3. **No JIT compilation** — The JIT opcodes (`JitCompile`, etc.) are stubs. All code is interpreted.
4. **Single-threaded** — The VM has no threading support. Coroutine-style concurrency (async/await) is planned.
5. **Limited CIB integration** — The CIB FFI can load libraries and resolve symbols, but the typed call path (`CibCallTyped`) and C→VM callbacks are not yet implemented.
6. **py2uc is incomplete** — The Python compiler handles basic syntax but does not yet support classes, exceptions, generators, or the full import system.
