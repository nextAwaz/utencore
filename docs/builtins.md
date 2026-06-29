# Builtins — Native Functions

**Document version:** 1.0 | **VM version:** >=0.0.5 | **Bytecode version:** >=3

UtenCore provides a set of native Rust functions registered at VM startup. These are accessible through the `utencore.*` namespace and are always available.

## utencore.* (Top-level)

| Function | Args | Description |
|----------|------|-------------|
| `print(s)` | 1 | Print without newline |
| `println(s)` | 1 | Print with newline |
| `input()` | 0 | Read line from stdin (returns string) |
| `exit(code)` | 1 | Exit process with code |
| `assert(cond, msg?)` | 1-2 | Assert with optional message |

## utencore.Math.*

| Function | Args | Returns | Description |
|----------|------|---------|-------------|
| `sqrt(x)` | 1 | f64 | Square root |
| `sin(x)` | 1 | f64 | Sine (radians) |
| `cos(x)` | 1 | f64 | Cosine (radians) |
| `tan(x)` | 1 | f64 | Tangent (radians) |
| `floor(x)` | 1 | f64 | Floor |
| `ceil(x)` | 1 | f64 | Ceiling |
| `round(x)` | 1 | f64 | Round to nearest |
| `abs(x)` | 1 | f64 | Absolute value |
| `pow(a, b)` | 2 | f64 | a raised to b |
| `pi()` | 0 | f64 | Pi constant |
| `e()` | 0 | f64 | Euler's number |

## utencore.Io.*

| Function | Args | Returns | Description |
|----------|------|---------|-------------|
| `readFile(path)` | 1 | string | Read file to string |
| `writeFile(path, content)` | 2 | bool | Write string to file |
| `readLine()` | 0 | string | Read line from stdin |

## utencore.Sys.*

| Function | Args | Returns | Description |
|----------|------|---------|-------------|
| `clockMs()` | 0 | i64 | Unix timestamp in ms |
| `sleep(ms)` | 1 | nil | Sleep for ms |

## utencore.Unsafe.*

Low-level operations for standard library implementors:

| Function | Description |
|----------|-------------|
| `alloc(size)` | Raw memory allocation |
| `free(ptr)` | Raw memory deallocation |
| `readByte(ptr)` | Read byte from memory |
| `writeByte(ptr, val)` | Write byte to memory |
| `gcCollect()` | Force garbage collection |
| `gcPin(handle)` | Pin GC object |
| `dlopen(name)` | Load dynamic library |
| `dlsym(lib, name)` | Resolve symbol |
| `typeOf(val)` | Get runtime type tag |
| `aliasNs(from, to)` | Create namespace alias |

## utencore.Gc.*

| Function | Description |
|----------|-------------|
| `collect()` | Trigger GC cycle |
| `pin(handle)` | Prevent collection |
| `unpin(handle)` | Allow collection |
| `stats()` | Get GC statistics |

## utencore.Ns.*

| Function | Description |
|----------|-------------|
| `alias(from, to)` | Create namespace alias for module resolution |

## Implementation

These builtins are implemented in `vm/Builtins.rs` and registered during `Vm::init_unsafe_module()` + `Vm::init_embedded_stdlib()`. They are NOT part of the bytecode opcode set — they're called via the `CallValue` mechanism through the `utencore` module namespace.
