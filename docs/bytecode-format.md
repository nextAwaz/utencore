# Bytecode Format

**Document version:** 1.0 | **VM version:** >=0.0.5 | **Bytecode version:** >=3

## File Types

| Extension | Magic | Purpose |
|-----------|-------|---------|
| `.utlib` | `UCLB` | Compiled library (manually produced) |
| `.ucch` | `UCCH` | Compiled cache (auto-generated) |

## Module Structure (UtenModule)

Serialized via `bincode`:

```
magic: [u8; 4]              UCLB or UCCH
version: (u16, u16)         VM version
bytecode_version: u32       Format version (checked against VM)
header: ModuleHeader
  name: String
  source_lang: String       e.g. "python", "typescript"
  compiler: String          e.g. "py2uc"
  compiler_version: String
  gc_strategy: String       "generational" | "mark-sweep" | "refcount" | "none"
  jit_recommended: bool
  metadata: HashMap<String, String>
  line_map: Vec<LineEntry>

strings: Vec<String>          Interned string pool (StringId)
constants: Vec<ConstValue>    Inline constants (Nil, Bool, Int, Float, String)
functions: Vec<FunctionDef>   Bytecode function definitions
structs: Vec<StructDef>       Value type definitions
globals: Vec<GlobalDef>       Global variable descriptors
exports: HashMap<String, ExportEntry>  Named exports
imports: Vec<ImportEntry>     Cross-module import declarations
exceptions: Vec<ExceptionTableEntry>   try/catch handler table
```

### FunctionDef

```rust
pub struct FunctionDef {
    pub name: String,          // Function name for debugging
    pub bytecode: Vec<u8>,     // Raw opcode sequence
    pub n_locals: u16,         // Number of local slots
    pub n_params: u16,         // Number of parameters
    pub is_variadic: bool,     // Accepts varargs
    pub n_captures: u16,       // Number of captured values
    pub return_type: Option<TypeRef>,
    pub param_types: Vec<TypeRef>,
}
```

### GlobalDef

```rust
pub struct GlobalDef {
    pub name: String,
    pub init_value: Option<ConstValue>,
}
```

### ExceptionTableEntry

```rust
pub struct ExceptionTableEntry {
    pub func_index: u32,       // Which function this belongs to
    pub try_start: u32,        // Bytecode offset of try start
    pub try_end: u32,          // Bytecode offset of try end
    pub handler_pc: u32,       // Handler address (0 = finally-only)
    pub catch_type: Option<StringId>,
    pub finally_pc: Option<u32>,
}
```

## Bytecode Encoding

Each instruction is:
1. **Opcode byte** (1 byte) — selects operation
2. **Operand** (0, 1, 2, 4, or 8 bytes) — depends on opcode

Operand sizes per category:

| Opcode range | Operand size | Description |
|---|---|---|
| 0x00-0x0F | 0-4 bytes | Stack ops, small immediates |
| 0x10-0x15 | 0 | Integer arithmetic |
| 0x20-0x25 | 0 | Float arithmetic |
| 0x30-0x35 | 0 | Bitwise ops |
| 0x40-0x4D | 0 | Comparison |
| 0x50-0x5F | 0-2 | Type conversion |
| 0x60-0x6D | 0-2 | Control flow (2 = jump offset) |
| 0x70-0x7F | 0-2 | Calls (2 = func ref) |
| 0x80-0x8F | 0-2 | Variables (2 = local/global index) |
| 0x90-0x9F | 0-2 | Array ops (2 = item count) |
| 0xA0-0xAF | 0-2 | Map, range ops |
| 0xB0-0xBF | 0 | String ops |
| 0xC0-0xCF | 0-2 | OOP ops (2 = string ID) |
| 0xE0-0xEF | 0-2 | CIB, module ops |
| 0xF0-0xFF | 0-2 | GC, control ops |

For full opcode details, see [Opcodes](opcodes.md).

## Opcode Encoding Rules

- Jump offsets are **relative i16** from the byte after the operand
- String IDs are **u16 indices** into the module's string pool
- Function refs are **u16 indices** into the module's function table
- Local/global indices are **u16**
- 32-bit immediates are **i32 little-endian**
- 64-bit immediates are **f64 little-endian bits** (for PushF64) or **i64 little-endian** (for PushI64)
