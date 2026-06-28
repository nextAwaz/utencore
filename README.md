# UtenCore

A universal stack-based virtual machine for scripting language compilers. Written in Rust.

UtenCore provides a language-agnostic bytecode format (`.uclib`), a register-less stack VM with pluggable garbage collection, a native function interface (CIB) for C/Rust interop, and a plugin-based compiler system (CCIS) that allows multiple source languages to target the same runtime.

## Project Status

**Version:** 0.0.5-alpha  
**License:** MPL-2.0  
**Target:** General-purpose scripting language runtime

The VM core (opcode dispatch, GC, module loading, bytecode format) is functional. The compiler plugin system (CCIS) is operational. The Python-to-`uclib` compiler (py2uc) is under active development — language feature coverage is partial.

## Quick Start

```bash
# Build
cargo build --release

# Run a Python script (compile + execute)
./target/release/utencore hello.py

# Compile to bytecode only
./target/release/ucc hello.py -o hello.uclib

# Execute pre-compiled bytecode
./target/release/uc hello.uclib

# Disassemble
./target/release/ucdump hello.uclib
```

## Architecture

```
┌─────────────────────┐     ┌──────────────────┐
│  py2uc (compiler)   │     │  ts2uc (future)  │
│  Python → .uclib    │     │  TypeScript →    │
│  CCIS plugin        │     │  .uclib          │
└─────────┬───────────┘     └────────┬─────────┘
          │                          │
          ▼                          ▼
┌─────────────────────────────────────────┐
│  UCLIB Bytecode Format                  │
│  (bincode-serialized UtenModule)        │
└────────────────┬────────────────────────┘
                 │ load
                 ▼
┌─────────────────────────────────────────┐
│  UtenCore VM                            │
│  ┌─────────┐  ┌──────────┐  ┌────────┐ │
│  │ Dispatch │  │ GC (3    │  │ CIB    │ │
│  │ Loop     │  │ strats)  │  │ FFI    │ │
│  │ 256      │  │          │  │ Bridge │ │
│  │ opcodes  │  │          │  │        │ │
│  └─────────┘  └──────────┘  └────────┘ │
│  ┌─────────┐  ┌──────────┐  ┌────────┐ │
│  │ Module  │  │ UCSL     │  │ Unsafe │ │
│  │ Loader  │  │ Std Lib  │  │ Native │ │
│  │         │  │ Resolver │  │ API    │ │
│  └─────────┘  └──────────┘  └────────┘ │
└─────────────────────────────────────────┘
```

### Components

| Directory | Purpose |
|-----------|---------|
| `utencore-core/` | VM core: type system, opcodes, bytecode format, dispatch loop, GC, CIB FFI, module loader, UCSL shared library system, plugin manager |
| `uc-binaries/` | CLI entry points: `uc` (VM launcher), `ucc` (compiler), `utencore` (all-in-one), `ucdump` (disassembler) |
| `compilers/py2uc/` | Python 3 → .uclib compiler (CCIS plugin), including partial stdlib implementation |
| `ucsl/` | Pre-compiled shared library files (.uclib) discoverable by UCSL resolver |
| `ucif/` | CIB interface definition files (.ucif) for C library interop |

### Bytecode Format

`.uclib` files use a bincode-serialized `UtenModule` structure:

```
magic: [u8;4]         UCLB (library) or UCCH (cache)
version: (u16,u16)    VM version that produced this module
bytecode_version: u32 Format version (checked against VM)
header: ModuleHeader  name, source_lang, gc_strategy, metadata
strings: Vec<String>  Interned string pool
constants: Vec<ConstValue>  Embedded constant pool
functions: Vec<FunctionDef>  Bytecode function definitions
globals: Vec<GlobalDef>  Global variable descriptors
exports: HashMap<String, ExportEntry>  Named exports
imports: Vec<ImportEntry>  Cross-module import declarations
exceptions: Vec<ExceptionTableEntry>  try/catch handler table
```

### Opcode Map

256 opcodes organized in 16×16 categories:

| Range | Category | Count |
|-------|----------|-------|
| 0x00–0x0F | Stack manipulation | 16 |
| 0x10–0x1F | Integer arithmetic | 16 |
| 0x20–0x2F | Float arithmetic | 16 |
| 0x30–0x3F | Bitwise operations | 16 |
| 0x40–0x4F | Comparison & logical | 16 |
| 0x50–0x5F | Type conversion | 16 |
| 0x60–0x6F | Control flow | 16 |
| 0x70–0x7F | Call & return | 16 |
| 0x80–0x8F | Variables & environment | 16 |
| 0x90–0x9F | Array & list operations | 16 |
| 0xA0–0xAF | Map, set, range, tuple | 16 |
| 0xB0–0xBF | String & regex | 16 |
| 0xC0–0xCF | OOP & type system | 16 |
| 0xD0–0xDF | Functional & coroutine | 16 |
| 0xE0–0xEF | CIB, module, plugin | 16 |
| 0xF0–0xFF | GC, JIT, debug | 16 |

Full opcode reference: see `utencore-core/src/opcodes/mod.rs`.

### Garbage Collection

Three pluggable strategies, selectable per-module via `header.gc_strategy`:

- **Generational** (default) — two-generation: nursery (gen0) and tenured (gen1). Objects promoted after surviving 2 gen0 collections. Major GC collects both generations.
- **Mark-Sweep** — simple mark-and-sweep with root tracing from VM stack and frames.
- **RefCount** — reference counting (no cycle collection).

The GC strategy is declared by the compiler in the `.uclib` header and applied at module load time.

### CIB (Central Interface Bridge)

FFI bridge for calling C functions from bytecode. Uses `libloading` for dynamic library loading and `.ucif` interface definition files to describe C function signatures, struct layouts, and constants. Compilers embed UCIF interface requirements in their CCIS manifest, and the VM auto-loads them at module initialization.

### UCSL (UtenCore Shared Library)

Shared library discovery and resolution system. `UtenCore` modules can import functionality from pre-compiled `.uclib` files located in standard search paths (`./ucsl/`, `~/.utencore/ucsl/`, system paths). UCSL manifests (`.ucsl` JSON files) declare exports, dependencies, and version requirements.

### CCIS (Common Compiler Interface Specification)

Plugin architecture for language compilers. Each compiler registers a CCIS manifest declaring supported file extensions, default GC strategy, and CIB interface requirements. Compilers receive a `CompileContext` with source text, filename, a mutable `UtenModule` to fill, and compiler options — no serialization round-trip between compilation and loading.

### Unsafe API (`utencore.Unsafe`)

Built-in module providing low-level operations for standard library implementors. Available only to code that imports `utencore` and accesses the `Unsafe` namespace. Capabilities include raw memory allocation, GC control, CIB FFI, type introspection, and namespace aliasing.

## Building

```bash
# Build all targets
cargo build

# Build release
cargo build --release

# Run tests
cargo test --lib
```

### Dependencies

- Rust 2021 edition
- `libffi-sys` (Unix: system package, Windows: bundled)
- `libloading` (for CIB dynamic library loading)
- `llvm-sys` (optional, for JIT feature)

## License

MPL-2.0. See LICENSE file for details.
