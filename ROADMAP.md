# UtenCore Roadmap

Target: production-ready general-purpose scripting language runtime.

## Legend

- [ ] Not started
- [/] In progress
- [x] Complete

## Phase 1: VM Core Stability

**Goal:** The VM is deterministic, memory-safe, and produces correct results for all basic operations.

### Runtime Correctness

- [x] Opcode dispatch loop — fetch/decode/execute with operand reading
- [x] All 256 opcodes defined (16×16 categories)
- [x] Stack operations (push, pop, dup, swap, rot)
- [x] Integer arithmetic (add/sub/mul/div/mod/neg/pow, checked/saturating/wrapping variants)
- [x] Float arithmetic (add/sub/mul/div/mod/pow/sqrt/trig)
- [x] Bitwise operations (and/or/xor/not/shift/rotate/popcnt/clz/ctz)
- [x] Comparison and logical operations (eq/ne/lt/le/gt/ge/cmp, short-circuit and/or/not)
- [x] Type conversion (to_i32/i64/f32/f64/bool/string, safe cast, bitcast)
- [x] Control flow (jump, conditional jump, for-loop prep/step, switch/jumptable)
- [x] Call and return (direct, value, method, tail-call, apply, closure invocation)
- [x] Variable access (local, global, capture/upvalue)
- [x] Container operations (array, map, set, range, tuple)
- [x] String operations (concat, length, indexing, substring, replace, split, join, case conversion)
- [x] Regex compilation and matching
- [x] OOP operations (namespace, class, object, method dispatch, attribute access, inheritance)
- [x] Functional operations (cons/car/cdr, map/filter/reduce, compose, delay/force)
- [x] Exception handling (raise, exception table lookup, frame unwinding)

### Memory Management

- [x] GcEngine trait with pluggable implementation
- [x] Generational GC (default) — nursery + tenured, promotion threshold
- [x] Mark-and-sweep GC
- [x] Reference counting GC
- [ ] Compactifying GC (heap compaction to eliminate fragmentation)
- [x] GC root scanning (VM stack, frame locals, closures)
- [x] Object pinning

### Type System

- [x] UValue tagged union (Nil, Bool, Int32/64, Float32/64, String, GC handle, NativeFn)
- [x] HeapObject variants (Array, Map, Struct, Closure, Opaque, Namespace, Class, Object, Method, Dynamic, Pair, Tuple, Range, Regex, Continuation, Set, Thunk)
- [x] HeapObject self-describing tags (type-safe GC access)
- [x] HeapString (runtime-allocated, not interned)
- [x] BigInt (arbitrary-precision integer via num-bigint)
- [x] Bytes / ByteArray
- [x] Complex numbers (inline f64+imag)
- [x] Lambda (anonymous closure with separate tag)
- [x] Iterator (container iteration with index tracking)

## Phase 2: FFI & Interop

**Goal:** UtenCore can call C libraries through a type-safe interface, and C can call back into UtenCore.

- [x] CIB interface definitions (UCIF format)
- [ ] C → UtenCore callbacks
- [x] libloading-based dynamic library loading (via Unsafe.dlopen/dlsym)
- [ ] Thread safety (Send/Sync for VM state)
- [ ] Full CIB integration with VM module system

## Phase 3: Compiler Infrastructure

**Goal:** The CCIS plugin system is complete, and py2uc can compile real Python programs.

### CCIS Plugin System

- [x] CCIS manifest format (name, version, extensions, GC strategy, CIB deps)
- [x] In-process compiler registration
- [x] Dynamic plugin loading (`.so`/`.dll`)
- [x] `CompileContext` with mutable module reference (no serialization round-trip)
- [x] Structured compile errors (file, line, col, level)
- [x] `CompilerOptions` (GC strategy, optimization level, debug info)
- [ ] CCIB interface auto-loading from manifest
- [ ] Cross-plugin dependency resolution

### py2uc: Python 3 Compiler

**High priority:**

- [ ] Method calls and attribute access — `obj.method(args)`, `obj.attr`
- [ ] Proper class support — `class`, `__init__`, instance variables, inheritance
- [ ] Exception handling — `try/except/finally/else`, `raise`
- [ ] Import resolution — cross-module imports via UCSL
- [ ] `*args`/`**kwargs` — variadic function parameters and call unpacking

**Medium priority:**

- [ ] `yield`/generators — coroutine frame support
- [ ] `with` statement — context manager protocol (`__enter__`/`__exit__`)
- [ ] `async`/`await` — coroutine scheduling
- [ ] `match`/`case` — structural pattern matching
- [ ] File I/O — `open`, `read`, `write`
- [ ] `lambda` expressions
- [ ] Decorators — `@decorator`, `@decorator(args)`
- [ ] Full f-string support (escape sequences, format specifiers)
- [ ] Augmented assignment (`+=`, `-=`, etc.)
- [ ] Walrus operator (`:=`)

**Low priority:**

- [ ] Type annotations (parse and pass through, no runtime enforcement)
- [ ] Comprehensive stdlib: `sys`, `os`, `math`, `json`, `re`, `collections`
- [ ] Match statement exhaustiveness checking
- [ ] Compiler optimizations (constant folding, dead code elimination)

## Phase 4: Standard Library

**Goal:** UtenCore has a standard library comparable to CPython's builtins.

- [ ] `utenstd.math` — sqrt, trig, floor/ceil/round, pow
- [ ] `utenstd.fmt` — string formatting, join, split, replace
- [ ] `utenstd.io` — print, input, file read/write
- [ ] `utenstd.collections` — list/map/set utilities
- [ ] `utenstd.re` — regular expression utilities
- [ ] `utenstd.json` — JSON parsing and serialization
- [ ] `utenstd.sys` — system interface (argv, exit, environment)

## Phase 5: JIT Compilation

**Goal:** Performance-critical code paths are JIT-compiled to native code.

- [ ] LLVM IR generation from bytecode
- [ ] Tiered compilation (interpret → quick JIT → optimized JIT)
- [ ] Inline caching for method dispatch
- [ ] Deoptimization guards

## Phase 6: Tooling & Observability

- [ ] Line-number mapping in debug output
- [ ] Source-map aware stack traces
- [ ] Profiling hooks (allocation count, hot function detection)
- [ ] REPL with history and completion
- [ ] Language Server Protocol support for py2uc

## Non-Goals

The following are explicitly out of scope for the current design:

- **Full Python compatibility** — py2uc targets the Python 3 language specification, but CPython-specific implementation details (C extension API, GIL, reference counting semantics) will not be replicated.
- **Multithreading** — The VM is single-threaded. Concurrency via coroutines (async/await) is planned, but OS-thread-level parallelism is not.
- **AOT compilation to native code** — The JIT compiles at runtime; standalone native binary generation is not a goal.
- **WebAssembly target** — The VM targets native execution; WASM is not a compilation target.
