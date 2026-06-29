# Architecture

**Document version:** 1.0 | **VM version:** >=0.0.5 | **Bytecode version:** >=3

## Overview

UtenCore is a stack-based virtual machine designed as a universal backend for scripting language compilers. It provides a language-agnostic bytecode format, a runtime with pluggable garbage collection, a native function interface (CIB), and a compiler plugin system (CCIS).

### Design Tenets

1. **Language-agnostic bytecode** — The opcode set and type system are designed to represent constructs from diverse languages (Python, JavaScript/Lua, Lisp, etc.) without bias toward any single source language.
2. **Compiler ↔ VM separation** — Compilers are plugins (CCIS). The VM does not contain language-specific logic. A compiler produces `.utlib` bytecode; the VM executes it.
3. **Safety through verification** — Bytecode loaded from untrusted sources passes through a verifier that checks opcode validity, operand ranges, and stack consistency before execution.
4. **Capability-based security** — Low-level operations (memory allocation, FFI, GC control) are gated behind the `utencore.Unsafe` namespace.

## Component Diagram

```
┌─────────────────────┐     ┌──────────────────┐
│  py2uc (compiler)   │     │  ts2uc (future)  │
│  Python → .utlib    │     │  TypeScript →    │
│  CCIS plugin        │     │  .utlib          │
└─────────┬───────────┘     └────────┬─────────┘
          │                          │
          ▼                          ▼
┌─────────────────────────────────────────┐
│  UTLIB Bytecode Format                  │
│  (bincode-serialized UtenModule)        │
└────────────────┬────────────────────────┘
                 │ load
                 ▼
┌─────────────────────────────────────────┐
│  UtenCore VM                            │
│  ┌─────────┐  ┌──────────┐  ┌────────┐ │
│  │ Dispatch │  │ GC (3    │  │ CIB    │ │
│  │ Loop     │  │ strats)  │  │ FFI    │ │
│  │ ~120     │  │          │  │ Bridge │ │
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

| Component | Location | Purpose |
|-----------|----------|---------|
| VM Core | `utencore-core/src/vm/` | Type system, opcodes, dispatch loop, GC, CIB FFI, module loader, plugin manager |
| Bytecode | `utencore-bytecode/` | `.utlib` module serialization, BytecodeWriter, ModuleBuilder |
| Types | `utencore-types/` | UValue, HeapObject, Opcode enum, error types |
| GC | `utencore-gc/` | Pluggable GC engines (generational, mark-sweep, refcount) |
| Binaries | `uc-binaries/` | CLI: `uc`, `ucc`, `utencore`, `ucdump` |
| py2uc | `compilers/py2uc/` | Python 3 → bytecode compiler (CCIS plugin) |
| Stdlib | `src/stdlib/` | Pre-compiled `.utlib` modules embedded in VM binary |
| UCSL | Ucsl.rs | Shared library discovery and resolution |

## Execution Flow

1. **Compilation**: py2uc (or other CCIS compiler) parses source → emits opcodes → produces `.utlib` (UtenModule serialized as bincode)
2. **Loading**: VM deserializes `.utlib` → verifies bytecode version → allocates globals → registers CIB interfaces
3. **Execution**: Dispatch loop fetches opcodes → reads operands → dispatches to handler → checks GC periodically
4. **Module resolution**: `Import` opcode triggers UCSL registry lookup → loads dependency modules → creates Namespace

## Bytecode Versioning

Each module carries a `bytecode_version`. The VM accepts modules where `module.bytecode_version ≤ VM.BYTECODE_VERSION`. This allows forward compatibility: older modules run on newer VMs.

Current bytecode version: **3**
