# UtenCore

A universal stack-based virtual machine for scripting language compilers. Written in Rust.

UtenCore provides a language-agnostic bytecode format (`.utlib`), a register-less stack VM with pluggable garbage collection, a native function interface (CIB) for C/Rust interop, and a plugin-based compiler system (CCIS) that allows multiple source languages to target the same runtime.

**Version:** 0.0.5-alpha  
**License:** MPL-2.0  
**Full documentation:** [`docs/`](docs/index.md)

## Quick Start

```bash
# Build
cargo build --release

# Run a Python script (compile + execute)
./target/release/utencore hello.py

# Compile to bytecode only
./target/release/ucc hello.py -o hello.utlib

# Execute pre-compiled bytecode
./target/release/uc hello.utlib

# Disassemble
./target/release/ucdump hello.utlib
```

## Project Status

| Component | Status |
|-----------|--------|
| VM core (dispatch, GC, module loader) | ✅ Functional |
| Bytecode format (`.utlib`) | ✅ v3 |
| CIB FFI bridge | ✅ Functional |
| CCIS compiler plugin system | ✅ Operational |
| py2uc (Python → bytecode) | 🚧 Partial coverage |
| JIT compilation | ❌ Not yet |

## Documentation

See [`docs/index.md`](docs/index.md) for the full documentation index,
including architecture, opcode reference, type system, garbage collection,
and compiler plugin development guides.

## Building

```bash
# Build all targets
cargo build

# Build release
cargo build --release

# Run tests
cargo test --lib
```

## License

MPL-2.0. See [LICENSE](LICENSE) for details.
