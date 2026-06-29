# UtenCore Documentation

**Document version:** 1.0  
**Compatible VM version:** >=0.0.5  
**Compatible bytecode version:** >=3  

UtenCore is a universal stack-based virtual machine designed as a backend for scripting language compilers. This documentation covers the VM architecture, bytecode format, type system, and all major subsystems.

## Versioning

| Artifact | Version | Notes |
|----------|---------|-------|
| VM | 0.0.5-alpha | Current release |
| Bytecode format | 3 | `.utlib` magic: `UCLB` |
| CCIS | 1 | Compiler plugin ABI |
| UCSL | 1 | Shared library manifest |

## Contents

| Document | Covers |
|----------|--------|
| [Architecture](architecture.md) | Overall design, components, data flow |
| [Bytecode Format](bytecode-format.md) | `.utlib` file structure, module layout |
| [Opcodes](opcodes.md) | Complete opcode reference by category |
| [Type System](type-system.md) | UValue, HeapObject, ValueTag, type conversions |
| [Garbage Collector](garbage-collector.md) | Generational, mark-sweep, refcount strategies |
| [CIB FFI](cib-ffi.md) | Native function interface and C interop |
| [Module System](module-system.md) | Module loading, UCSL, embedded stdlib |
| [Compiler Plugin](compiler-plugin.md) | CCIS, PluginManifest, writing compilers |
| [Builtins](builtins.md) | Native functions available in utencore.* |
| [Roadmap](roadmap.md) | Future development plans |

## Quick Links

- [GitHub Repository](https://github.com/nextAwaz/utencore)
- [License](../LICENSE) (MPL-2.0)
