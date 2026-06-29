# CCIS — Compiler Plugin System

**Document version:** 1.0 | **VM version:** >=0.0.5 | **Bytecode version:** >=3

## Overview

CCIS (Common Compiler Interface Specification) allows multiple source languages to target the same UtenCore runtime. Each compiler is a plugin that registers with the VM's `PluginManager`.

## PluginManifest

```rust
pub struct PluginManifest {
    pub manifest_version: u32,
    pub name: String,                    // "py2uc", "ts2uc"
    pub version: String,
    pub plugin_type: PluginType,         // Compiler or Runtime
    pub description: String,
    pub bytecode_version: BytecodeVersionRange,
    pub ccis: Option<CcisPluginInfo>,
}
```

### CcisPluginInfo

```rust
pub struct CcisPluginInfo {
    pub language: String,                // "Python 3", "TypeScript"
    pub extensions: Vec<String>,         // ["py"], ["ts"]
    pub default_gc: String,              // "generational"
    pub cib_interfaces: Vec<String>,     // Required C interfaces
}
```

## Compilation Flow

```
source.py → PluginManager::compile("source.py")
  → Find compiler by extension (.py → py2uc)
  → Create CompileContext(source, filename, &mut module, options)
  → Call compile_fn(&mut ctx)
  → Compiler fills module via ModuleBuilder
  → Return UtenModule
```

## CompileContext

```rust
pub struct CompileContext<'a, 'b> {
    pub source: &'a str,          // Source code
    pub filename: &'a str,        // Source filename
    pub builder: &'b mut ModuleBuilder<'a>,  // Module builder
    pub options: &'a CompilerOptions,
}
```

## CompilerOptions

```rust
pub struct CompilerOptions {
    pub optimize: u8,       // 0=none, 1=basic, 2=aggressive
    pub emit_debug: bool,   // Emit debug info (line numbers)
    pub emit_line_map: bool,// Emit line mapping in module header
}
```

## Writing a Compiler Plugin

```rust
// 1. Define a compile function
pub fn compile(ctx: &mut CompileContext) -> Result<(), Vec<CompileError>> {
    // Parse source, generate bytecode via ModuleBuilder
    Ok(())
}

// 2. Create a manifest
pub fn plugin_manifest() -> PluginManifest {
    PluginManifest {
        name: "ts2uc".into(),
        plugin_type: PluginType::Compiler,
        bytecode_version: BytecodeVersionRange { min: 1, max: BYTECODE_VERSION },
        ccis: Some(CcisPluginInfo {
            language: "TypeScript".into(),
            extensions: vec!["ts".into()],
            default_gc: "generational".into(),
            ..Default::default()
        }),
        ..Default::default()
    }
}

// 3. Register with PluginManager
pub fn register(pm: &mut PluginManager) {
    pm.register_compiler_from_manifest(manifest, Arc::new(compile))
        .expect("register ts2uc");
}
```

## Dynamic Plugins

CCIS supports dynamic loading of `.so`/`.dll` plugins via C FFI:

```c
// Required exports for a dynamic plugin
const char* ccis_init();             // Returns JSON manifest
CcisCompileResult ccis_compile(...); // Compile function
void ccis_free_result(result);       // Free result
```

## CompileError

```rust
pub struct CompileError {
    pub message: String,
    pub file: String,
    pub line: usize,
    pub col: usize,
    pub level: ErrorLevel,  // Error | Warning | Note
}
```
