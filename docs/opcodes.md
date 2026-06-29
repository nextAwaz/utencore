# Opcodes

**Document version:** 1.0 | **VM version:** >=0.0.5 | **Bytecode version:** >=3

The UtenCore opcode set (~120 opcodes) is organized in a compact byte-indexed space. Opcodes with single-line implementations were removed in v3 — use native library functions instead.

## 0x00–0x0F: Stack Manipulation

| Byte | Opcode | Operand | Stack Effect | Description |
|------|--------|---------|-------------|-------------|
| 0x00 | Nop | — | 0 | No operation |
| 0x01 | PushNil | — | +1 | Push nil value |
| 0x02 | PushTrue | — | +1 | Push true |
| 0x03 | PushFalse | — | +1 | Push false |
| 0x04 | PushI32 | i32 | +1 | Push 32-bit integer |
| 0x05 | PushI64 | i64 | +1 | Push 64-bit integer |
| 0x06 | PushF32 | f32 | +1 | Push 32-bit float |
| 0x07 | PushF64 | f64 | +1 | Push 64-bit float |
| 0x08 | PushString | u16 | +1 | Push interned string by StringId |
| 0x09 | PushConst | u16 | +1 | Push constant pool value by index |
| 0x0A | Dup | — | +1 | Duplicate top of stack |
| 0x0B | DupN | u8 | 0 | Duplicate top N items |
| 0x0C | Swap | — | 0 | Swap top two values |
| 0x0D | Pop | — | -1 | Pop and discard |
| 0x0E | PopN | u16 | -N | Pop N values |
| 0x0F | Rot | — | 0 | Rotate top 3: (a b c) → (b c a) |

## 0x10–0x15: Integer Arithmetic

| Byte | Opcode | Operand | Stack Effect | Description |
|------|--------|---------|-------------|-------------|
| 0x10 | Add | — | -1 | a + b (polymorphic: int, bigint, string concat) |
| 0x11 | Sub | — | -1 | a - b |
| 0x12 | Mul | — | -1 | a * b |
| 0x13 | Div | — | -1 | a / b (signed floor) |
| 0x14 | Mod | — | -1 | a % b |
| 0x15 | Neg | — | 0 | -a |

All integer ops handle i32, i64, and BigInt transparently. Narrower values are promoted.

## 0x20–0x25: Float Arithmetic

| Byte | Opcode | Operand | Stack Effect | Description |
|------|--------|---------|-------------|-------------|
| 0x20 | FAdd | — | -1 | f + g |
| 0x21 | FSub | — | -1 | f - g |
| 0x22 | FMul | — | -1 | f * g |
| 0x23 | FDiv | — | -1 | f / g |
| 0x24 | FMod | — | -1 | f % g |
| 0x25 | FNeg | — | 0 | -f |

## 0x30–0x35: Bitwise Operations

| Byte | Opcode | Operand | Stack Effect | Description |
|------|--------|---------|-------------|-------------|
| 0x30 | BitAnd | — | -1 | a & b |
| 0x31 | BitOr | — | -1 | a \| b |
| 0x32 | BitXor | — | -1 | a ^ b |
| 0x33 | BitNot | — | 0 | ~a |
| 0x34 | Shl | — | -1 | a << b |
| 0x35 | Shr | — | -1 | a >> b (arithmetic) |

## 0x40–0x4D: Comparison & Logical

| Byte | Opcode | Operand | Stack Effect | Description |
|------|--------|---------|-------------|-------------|
| 0x40 | Eq | — | -1 | a == b |
| 0x41 | Ne | — | -1 | a != b |
| 0x42 | Lt | — | -1 | a < b |
| 0x43 | Le | — | -1 | a <= b |
| 0x44 | Gt | — | -1 | a > b |
| 0x45 | Ge | — | -1 | a >= b |
| 0x46 | Cmp | — | -1 | Compare → -1/0/1 |
| 0x47 | Is | — | -1 | Identity (same ref) |
| 0x48 | IsNot | — | -1 | Not identity |
| 0x49 | In | — | -1 | Container contains |
| 0x4A | NotIn | — | -1 | Container not contains |
| 0x4B | And | — | -1 | a && b (truthy) |
| 0x4C | Or | — | -1 | a \|\| b (truthy) |
| 0x4D | Not | — | 0 | !a (truthy) |

## 0x50–0x5F: Type & Conversion

| Byte | Opcode | Operand | Stack Effect | Description |
|------|--------|---------|-------------|-------------|
| 0x50 | TypeOf | — | 0 | Push type tag as i32 |
| 0x52 | ToI32 | — | 0 | Convert → i32 |
| 0x53 | ToI64 | — | 0 | Convert → i64 |
| 0x54 | ToF32 | — | 0 | Convert → f32 |
| 0x55 | ToF64 | — | 0 | Convert → f64 |
| 0x56 | ToBool | — | 0 | Convert → bool (truthy) |
| 0x57 | ToString | — | 0 | Convert → string |
| 0x58 | Cast | u8 | 0 | Safe cast with runtime tag check |
| 0x59 | BitCast | — | 0 | Reinterpret bits (i32↔f32, i64↔f64) |

## 0x60–0x6D: Control Flow

| Byte | Opcode | Operand | Stack Effect | Description |
|------|--------|---------|-------------|-------------|
| 0x60 | Jump | i16 | 0 | Unconditional jump |
| 0x61 | JumpIfFalse | i16 | -1 | Conditional jump |
| 0x62 | JumpIfTrue | i16 | -1 | Conditional jump |
| 0x63 | JumpIfEq | i16 | -2 | Jump if equal |
| 0x64 | JumpIfNe | i16 | -2 | Jump if not equal |
| 0x65 | JumpTable | i16 | -1 | Jump table switch |
| 0x66 | ForPrep | i16 | 0 | For loop preamble |
| 0x67 | ForStep | i16 | 0 | For loop step + check |
| 0x68 | Loop | i16 | 0 | Continue-loop jump |
| 0x69 | Switch | — | -1 | Value-based switch |
| 0x6A | MatchCheck | — | 0 | Pattern-match head |
| 0x6B | Bind | — | -1 | Pattern bind |
| 0x6C | GetIter | — | 0 | Get iterator from container |
| 0x6D | Next | i16 | -1 | Advance iterator, push or jump |

## 0x70–0x7F: Call & Return

| Byte | Opcode | Operand | Stack Effect | Description |
|------|--------|---------|-------------|-------------|
| 0x70 | Call | u16 | 0 | Call function by FuncRef |
| 0x71 | CallValue | — | 0 | Call value on stack |
| 0x72 | CallMethod | u16 | 0 | Call by method name (StringId) |
| 0x73 | TailCall | u16 | 0 | Tail-call function |
| 0x74 | TailCallValue | — | 0 | Tail-call value |
| 0x77 | Apply | — | 0 | (fn, args_array) → call |
| 0x78 | Return | — | 0 | Return nil |
| 0x79 | ReturnValue | — | -1 | Return value |
| 0x7A | ReturnMultiple | u8 | 0 | Multi-return |
| 0x7B | MakeClosure | u16 | 0 | Create closure from FuncRef |
| 0x7C | Capture | — | -1 | Push capture for active closure |
| 0x7D | LoadUpvalue | u8 | +1 | Load upvalue |
| 0x7E | StoreUpvalue | u8 | -1 | Store upvalue |
| 0x7F | Curry | — | 0 | Partial application |

## 0x80–0x8F: Variables & Environment

| Byte | Opcode | Operand | Stack Effect | Description |
|------|--------|---------|-------------|-------------|
| 0x80 | LoadLocal | u16 | +1 | Load local variable |
| 0x81 | StoreLocal | u16 | -1 | Store local variable |
| 0x84 | LoadGlobal | u16 | +1 | Load global variable |
| 0x85 | StoreGlobal | u16 | -1 | Store global variable |
| 0x88 | AllocFrame | u16 | 0 | Resize local frame |
| 0x89 | LoadArg | u8 | +1 | Load parameter by index |
| 0x8E | This | — | +1 | Push self/this |
| 0x8F | ArgCount | — | +1 | Push number of arguments |

## 0x90–0x9F: Array Operations

| Byte | Opcode | Operand | Stack Effect | Description |
|------|--------|---------|-------------|-------------|
| 0x90 | NewArray | u16 | 0 | Create array from N stack items |
| 0x91 | ArrayLen | — | 0 | Get array length |
| 0x92 | ArrayGet | — | -1 | array[index] |
| 0x93 | ArraySet | — | -3 | array[index] = val |
| 0x94 | ArrayPush | — | -2 | Push to end |
| 0x95 | ArrayPop | — | 0 | Pop from end |

## 0xA0–0xA6: Map Operations

| Byte | Opcode | Operand | Stack Effect | Description |
|------|--------|---------|-------------|-------------|
| 0xA0 | NewMap | — | +1 | Create empty map |
| 0xA1 | MapGet | — | -1 | map[key] |
| 0xA2 | MapSet | — | -3 | map[key] = val |
| 0xA3 | MapDel | — | -2 | Delete key |
| 0xA6 | MapLen | — | 0 | Entry count |

## 0xB0–0xBF: String Operations

| Byte | Opcode | Operand | Stack Effect | Description |
|------|--------|---------|-------------|-------------|
| 0xB1 | StrLen | — | 0 | String length |
| 0xB2 | StrGet | — | -1 | Char at index |
| 0xB3 | StrSub | — | -2 | Substring |
| 0xB4 | StrContains | — | -1 | Contains substring |
| 0xB5 | StrIndexOf | — | -1 | Find index |
| 0xBC | StrCmp | — | -1 | Lexicographic compare → -1/0/1 |

## 0xC0–0xCB: OOP & Type System

| Byte | Opcode | Operand | Stack Effect | Description |
|------|--------|---------|-------------|-------------|
| 0xC0 | NewNamespace | u16 | +1 | Create namespace |
| 0xC1 | NewClass | — | +1 | Create class |
| 0xC3 | ClassAddMethod | u16 | -1 | Add method (bit 15 = constructor flag) |
| 0xC4 | ClassAddField | u16 | 0 | Add field |
| 0xC6 | GetAttr | u16 | 0 | Get attribute |
| 0xC7 | SetAttr | u16 | -2 | Set attribute |
| 0xCA | GetField | u16 | 0 | Struct field by index |
| 0xCB | SetField | u16 | -2 | Struct field by index |

## 0xE0–0xEF: CIB & Module System

| Byte | Opcode | Operand | Stack Effect | Description |
|------|--------|---------|-------------|-------------|
| 0xE0 | CibLoad | — | +1 | dlopen library |
| 0xE1 | CibSym | — | 0 | dlsym symbol |
| 0xE2 | CibCall | — | 0 | Call C function pointer |
| 0xE3 | CibWrap | — | 0 | Wrap C ptr → Opaque |
| 0xE4 | CibUnwrap | — | 0 | Unwrap Opaque → ptr |
| 0xE6 | CibStrToC | — | 0 | UC string → C string |
| 0xE7 | CibStrFromC | — | 0 | C string → UC string |
| 0xE8 | CibSizeOf | — | 0 | Size of C type |
| 0xE9 | CibCallTyped | u16 | 0 | Typed call via UCIF |
| 0xEA | CibLoadInterface | u16 | +1 | Load UCIF interface |
| 0xEC | Import | u16 | +1 | Import module by name |
| 0xED | ImportFunc | u16 | 0 | Get exported function |
| 0xEE | ImportValue | u16 | 0 | Get exported value |
| 0xEF | Export | u16 | -1 | Export a value |

## 0xF0–0xFF: VM Control

| Byte | Opcode | Operand | Stack Effect | Description |
|------|--------|---------|-------------|-------------|
| 0xF1 | GcCollect | — | 0 | Trigger garbage collection |
| 0xFE | Halt | — | 0 | Stop the VM |
| 0xFF | Raise | — | -1 | Raise exception |

## Opcode Categories at a Glance

| Range | Category | Count |
|-------|----------|-------|
| 0x00–0x0F | Stack manipulation | 16 |
| 0x10–0x15 | Integer arithmetic | 6 |
| 0x20–0x25 | Float arithmetic | 6 |
| 0x30–0x35 | Bitwise operations | 6 |
| 0x40–0x4D | Comparison & logical | 14 |
| 0x50–0x5F | Type & conversion | 11 |
| 0x60–0x6D | Control flow | 14 |
| 0x70–0x7F | Call & return | 13 |
| 0x80–0x8F | Variables | 8 |
| 0x90–0x9F | Array | 6 |
| 0xA0–0xA6 | Map | 5 |
| 0xB0–0xBF | String | 6 |
| 0xC0–0xCB | OOP | 8 |
| 0xE0–0xEF | CIB, module | 14 |
| 0xF0–0xFF | VM control | 3 |
