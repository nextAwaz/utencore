# Module System

**Document version:** 1.0 | **VM version:** >=0.0.5 | **Bytecode version:** >=3

## Loading Pipeline

```
.utlib file → Vm::load_module()
  → Verify magic bytes (UCLB/UCCH)
  → Deserialize via bincode
  → Check bytecode_version ≤ VM.BYTECODE_VERSION
  → Verify module (magic, empty checks)
  → Vm::load_module()
    → Allocate globals
    → Apply GC strategy from module header
    → Register CIB interfaces from metadata
    → Return ModuleId
```

## Cross-Module Imports

The `Import` (0xEC) opcode resolves module names through:
1. Already-loaded modules (`ModuleLoader::find_loaded`)
2. UCSL registry (scanned search paths)
3. Filesystem path resolution (`ucsl/name.utlib`)
4. Creates a Namespace handle wrapping the module's exports

```python
import math        # Import → Namespace(math) → stored as global "math"
math.sqrt(4.0)     # LoadGlobal "math" → GetField "sqrt" → CallValue
```

## Embedded Standard Library

The standard library modules (`math`, `io`, `sys`) are compiled into the VM binary via `include_bytes!()` and registered at startup by `Vm::init_embedded_stdlib()`. This ensures `import math` etc. work without separate files:

```rust
const EMBEDDED_UCSL: &[(&str, &[u8])] = &[
    ("math", include_bytes!("../../src/stdlib/math.utlib")),
    ("io",   include_bytes!("../../src/stdlib/io.utlib")),
    ("sys",  include_bytes!("../../src/stdlib/sys.utlib")),
];
```

The `ModuleLoader` checks embedded modules first, then filesystem paths. External files can override embedded modules.

## UCSL (UtenCore Shared Libraries)

Shared `.utlib` libraries are discovered through standard search paths:
- `./ucsl/` (project-local)
- `~/.utencore/ucsl/` (user-global)
- `/usr/share/utencore/ucsl/` (system-wide)
- Executable-relative `../ucsl/`

Dotted names (`utenstd.math`) are resolved to filesystem paths (`ucsl/utenstd/math.utlib`).

## Module Namespace

When a module is imported, the VM creates a `HeapObject::Namespace` that wraps the module's `export_values` table. This namespace is what flows through the stack and globals:

```rust
pub fn build_module_namespace(&mut self, module_id: ModuleId, resolved_name: &str) -> GcHandle {
    let mut members = Vec::new();
    for (export_name, value) in &self.modules[module_id as usize].export_values {
        members.push((intern(export_name), value.clone()));
    }
    self.gc.alloc(HeapObject::Namespace { name, members, module_id })
}
```

## Imports Table

Compiler plugins record their imports in the module's `imports` table. This enables:
- Dependency tracking at module load time
- Version compatibility checking
- Recursive dependency resolution (future)
