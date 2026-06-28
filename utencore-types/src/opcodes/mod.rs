//! UtenCore opcode definitions.
//!
//! Complete 256-opcode table (0x00–0xFF), organized into 16 × 16-entry
//! categories. Each opcode carries operand width, flags, and stack effect.
//!
//! Coverage: stack, arithmetic (int/float/checked/vector), bitwise,
//! comparison, logicals, type conversions, control flow, call/return,
//! variables/closures, containers (array/list/map/set/range),
//! strings/regex, OOP/type-system, functional/coroutine, CIB/module/plugin,
//! GC/JIT/debug.
//!
//! This design targets TS, Lua, Ruby, Python, Lisp, and future languages.

use bitflags::bitflags;
use std::fmt;

/// Sub-module with opcode_info() metadata table
pub mod info;
pub use info::opcode_info;

bitflags! {
    /// Flags describing an opcode's properties
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct OpFlags: u16 {
        /// Operand is a local/frame index
        const HAS_LOCAL     = 0x0001;
        /// Operand is a jump target (relative i16 offset)
        const IS_JUMP       = 0x0002;
        /// Operand is a string pool index
        const HAS_STRING    = 0x0004;
        /// Operand is a function index
        const HAS_FUNC      = 0x0008;
        /// Operand is a module index
        const HAS_MODULE    = 0x0010;
        /// Operand is a type tag
        const HAS_TYPE      = 0x0020;
        /// Can trigger GC (needs safe-point check)
        const MAY_GC        = 0x0040;
        /// Has immediate 32-bit payload
        const IMM32         = 0x0080;
        /// Has immediate 64-bit payload
        const IMM64         = 0x0100;
        /// Terminates basic block (no fallthrough)
        const TERMINATOR    = 0x0200;
        /// Operand is a constant pool index
        const HAS_CONST     = 0x0400;
        /// Operand is a global index
        const HAS_GLOBAL    = 0x0800;
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// Opcode Enum (256 entries, 16×16 categories)
// ═══════════════════════════════════════════════════════════════════════════

/// All UtenCore opcodes.
///
/// Every language compiler emits these; the VM dispatches them.
/// Categories are contiguous blocks of 16 for easy dispatch clustering.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum Opcode {
    // ── 0x00–0x0F: Stack Manipulation ──
    Nop         = 0x00,  // no-op
    PushNil     = 0x01,  // push nil
    PushTrue    = 0x02,  // push true
    PushFalse   = 0x03,  // push false
    PushI32     = 0x04,  // push i32 immediate   (5B: op + i32)
    PushI64     = 0x05,  // push i64 immediate   (9B: op + i64)
    PushF32     = 0x06,  // push f32 immediate   (5B: op + f32)
    PushF64     = 0x07,  // push f64 immediate   (9B: op + f64)
    PushString  = 0x08,  // push string pool idx (3B: op + u16)
    PushConst   = 0x09,  // push constant pool   (3B: op + u16)
    Dup         = 0x0A,  // duplicate top
    DupN        = 0x0B,  // duplicate top N items(3B: op + u8 count)
    Swap        = 0x0C,  // swap top two
    Pop         = 0x0D,  // pop & discard
    PopN        = 0x0E,  // pop N values        (3B: op + u16 count)
    Rot         = 0x0F,  // rotate top 3: (a b c) → (b c a)

    // ── 0x10–0x1F: Integer Arithmetic ──
    Add         = 0x10,  // a + b
    Sub         = 0x11,  // a - b
    Mul         = 0x12,  // a * b
    Div         = 0x13,  // a / b (signed floor)
    Mod         = 0x14,  // a % b
    Neg         = 0x15,  // -a
    Inc         = 0x16,  // a + 1
    Dec         = 0x17,  // a - 1
    Abs         = 0x18,  // |a|
    Pow         = 0x19,  // a ** b
    CheckedAdd  = 0x1A,  // a + b, trap on overflow
    CheckedSub  = 0x1B,  // a - b, trap on overflow
    CheckedMul  = 0x1C,  // a * b, trap on overflow
    SaturatingAdd=0x1D,  // a + b, saturate
    SaturatingSub=0x1E,  // a - b, saturate
    WrappingAdd = 0x1F,  // a + b, wrap

    // ── 0x20–0x2F: Float Arithmetic ──
    FAdd        = 0x20,  // f + g
    FSub        = 0x21,  // f - g
    FMul        = 0x22,  // f * g
    FDiv        = 0x23,  // f / g
    FMod        = 0x24,  // f % g
    FNeg        = 0x25,  // -f
    FPow        = 0x26,  // f ** g
    FSqrt       = 0x27,  // sqrt(f)
    FAbs        = 0x28,  // |f|
    FFloor      = 0x29,  // floor(f)
    FCeil       = 0x2A,  // ceil(f)
    FRound      = 0x2B,  // round(f)
    FSin        = 0x2C,  // sin(f)
    FCos        = 0x2D,  // cos(f)
    FTan        = 0x2E,  // tan(f)
    FAtan2      = 0x2F,  // atan2(f, g)

    // ── 0x30–0x3F: Bitwise & Math Extension ──
    BitAnd      = 0x30,  // a & b
    BitOr       = 0x31,  // a | b
    BitXor      = 0x32,  // a ^ b
    BitNot      = 0x33,  // ~a
    Shl         = 0x34,  // a << b
    Shr         = 0x35,  // a >> b (arithmetic)
    UShr        = 0x36,  // a >>> b (logical)
    RotLeft     = 0x37,  // rotate left
    RotRight    = 0x38,  // rotate right
    PopCount    = 0x39,  // popcnt(a)
    LeadingZeros= 0x3A,  // clz(a)
    TrailingZeros=0x3B,  // ctz(a)
    ByteSwap    = 0x3C,  // bswap(a)
    BitReverse  = 0x3D,  // bitreverse(a)
    UDiv        = 0x3E,  // unsigned a / b
    UMod        = 0x3F,  // unsigned a % b

    // ── 0x40–0x4F: Comparison & Logical ──
    Eq          = 0x40,  // a == b
    Ne          = 0x41,  // a != b
    Lt          = 0x42,  // a < b
    Le          = 0x43,  // a <= b
    Gt          = 0x44,  // a > b
    Ge          = 0x45,  // a >= b
    Cmp         = 0x46,  // compare → -1/0/1
    Is          = 0x47,  // identity (same ref)
    IsNot       = 0x48,  // not identity
    In          = 0x49,  // container contains
    NotIn       = 0x4A,  // container not contains
    And         = 0x4B,  // a && b (short-circuit handled by compiler)
    Or          = 0x4C,  // a || b (short-circuit handled by compiler)
    Not         = 0x4D,  // !a
    Xor         = 0x4E,  // logical xor
    Truthy      = 0x4F,  // convert to bool (via truthy rules)

    // ── 0x50–0x5F: Type & Conversion ──
    TypeOf      = 0x50,  // push type tag (i32)
    IsType      = 0x51,  // check type tag, push bool
    ToI32       = 0x52,  // → i32
    ToI64       = 0x53,  // → i64
    ToF32       = 0x54,  // → f32
    ToF64       = 0x55,  // → f64
    ToBool      = 0x56,  // → bool
    ToString    = 0x57,  // → string (via pool)
    Cast        = 0x58,  // safe cast with runtime check
    BitCast     = 0x59,  // reinterpret bits
    EnumCreate  = 0x5A,  // (tag, payload) → enum
    EnumMatch   = 0x5B,  // enum → match tag, push payload
    CheckIndex  = 0x5C,  // bounds check (index, len) → trap or ok
    CheckType   = 0x5D,  // assert type, trap on mismatch
    TypeAssert  = 0x5E,  // assert + panic on fail
    Unreachable = 0x5F,  // trap (should never execute)

    // ── 0x60–0x6F: Control Flow ──
    Jump        = 0x60,  // unconditional       (3B: op + i16)
    JumpIfFalse = 0x61,  // conditional          (3B: op + i16)
    JumpIfTrue  = 0x62,  // conditional          (3B: op + i16)
    JumpIfEq    = 0x63,  // if equal             (3B: op + i16)
    JumpIfNe    = 0x64,  // if not equal         (3B: op + i16)
    JumpTable   = 0x65,  // switch               (varies)
    ForPrep     = 0x66,  // for loop preamble    (3B: op + i16)
    ForStep     = 0x67,  // for loop step+check  (3B: op + i16)
    Loop        = 0x68,  // continue-loop jump   (3B: op + i16)
    Switch      = 0x69,  // value-based switch   (varies)
    MatchCheck  = 0x6A,  // pattern-match head
    Bind        = 0x6B,  // pattern bind
    GetIter     = 0x6C,  // get iterator from container
    Next        = 0x6D,  // advance iterator, push value or JumpIfFalse
    Await       = 0x6E,  // await async call
    AsyncCall   = 0x6F,  // async function call

    // ── 0x70–0x7F: Call & Return ──
    Call        = 0x70,  // call func_index      (3B: op + u16)
    CallValue   = 0x71,  // call value on stack
    CallMethod  = 0x72,  // call by name         (3B: op + u16 string_id)
    TailCall    = 0x73,  // tail-call func_index
    TailCallValue=0x74,  // tail-call value
    Invoke      = 0x75,  // virtual method call  (3B: op + u16 arg_count)
    SuperCall   = 0x76,  // super call
    Apply       = 0x77,  // (fn, args_array) → call
    Return      = 0x78,  // return nil
    ReturnValue = 0x79,  // return value
    ReturnMultiple=0x7A, // multi-return (Lua)
    MakeClosure = 0x7B,  // (func_idx, n_captures) → closure
    Capture     = 0x7C,  // push capture value
    LoadUpvalue = 0x7D,  // load upvalue
    StoreUpvalue= 0x7E,  // store upvalue
    Curry       = 0x7F,  // partial application

    // ── 0x80–0x8F: Variables & Environment ──
    LoadLocal   = 0x80,  // load local          (3B: op + u16)
    StoreLocal  = 0x81,  // store local         (3B: op + u16)
    LoadCapture = 0x82,  // load from closure
    StoreCapture= 0x83,  // store to closure
    LoadGlobal  = 0x84,  // load global var     (3B: op + u16)
    StoreGlobal = 0x85,  // store global var    (3B: op + u16)
    LoadDynGlobal=0x86,  // load global by name (string_id)
    StoreDynGlobal=0x87, // store global by name
    AllocFrame  = 0x88,  // resize frame locals (u16 n)
    LoadArg     = 0x89,  // load param by index
    LoadModuleVar=0x8A,  // module-level variable
    StoreModuleVar=0x8B, // module-level variable
    LoadUpvalueFrom=0x8C,// load from specific closure
    StoreUpvalueTo=0x8D, // store to specific closure
    This        = 0x8E,  // push self/this
    ArgCount    = 0x8F,  // push number of args

    // ── 0x90–0x9F: Container: Array & List ──
    NewArray    = 0x90,  // array from N items  (3B: op + u16)
    ArrayLen    = 0x91,  // array length
    ArrayGet    = 0x92,  // array[index]
    ArraySet    = 0x93,  // array[index] = val
    ArrayPush   = 0x94,  // push back
    ArrayPop    = 0x95,  // pop back
    ArrayUnshift= 0x96,  // insert at front
    ArrayShift  = 0x97,  // remove from front
    ArrayInsert = 0x98,  // insert at index
    ArrayRemove = 0x99,  // remove at index
    ArraySlice  = 0x9A,  // slice (start, end)
    ArrayConcat = 0x9B,  // merge arrays
    ArrayContains=0x9C,  // contains value
    ArrayIndexOf= 0x9D,  // find index
    ArraySort   = 0x9E,  // sort in-place
    ArrayReverse= 0x9F,  // reverse in-place

    // ── 0xA0–0xAF: Map, Set, Range, Tuple ──
    NewMap      = 0xA0,  // create empty map
    MapGet      = 0xA1,  // map[key]
    MapSet      = 0xA2,  // map[key] = val
    MapDel      = 0xA3,  // delete key
    MapContains = 0xA4,  // has key?
    MapKeys     = 0xA5,  // keys as array
    MapLen      = 0xA6,  // entry count
    NewSet      = 0xA7,  // create empty set
    SetAdd      = 0xA8,  // add element
    SetRemove   = 0xA9,  // remove element
    SetContains = 0xAA,  // has element?
    SetLen      = 0xAB,  // element count
    SetUnion    = 0xAC,  // ∪
    SetIntersect= 0xAD,  // ∩
    NewRange    = 0xAE,  // (start, end, step) → range
    Tuple       = 0xAF,  // (N items) → tuple

    // ── 0xB0–0xBF: String & Regex ──
    StrConcat   = 0xB0,  // concatenation
    StrLen      = 0xB1,  // length
    StrGet      = 0xB2,  // char at index
    StrSub      = 0xB3,  // substring (start, end)
    StrContains = 0xB4,  // contains substring
    StrIndexOf  = 0xB5,  // find index
    StrReplace  = 0xB6,  // replace all
    StrSplit    = 0xB7,  // split by delimiter
    StrJoin     = 0xB8,  // join array
    StrToUpper  = 0xB9,  // to uppercase
    StrToLower  = 0xBA,  // to lowercase
    StrTrim     = 0xBB,  // trim whitespace
    StrCmp      = 0xBC,  // lexicographic compare → -1/0/1
    StrFormat   = 0xBD,  // format string
    RegexCompile= 0xBE,  // compile regex
    RegexMatch  = 0xBF,  // regex match

    // ── 0xC0–0xCF: OOP & Type System ──
    NewNamespace= 0xC0,  // create namespace    (3B: op + u16 string_id)
    NewClass    = 0xC1,  // create class        (stack: namespace_handle)
    NewObject   = 0xC2,  // instantiate         (stack: class_handle)
    ClassAddMethod=0xC3, // add method          (3B: op + u16 string_id)
    ClassAddField=0xC4,  // add field           (3B: op + u16 string_id)
    ClassSetParent=0xC5, // set parent class
    GetAttr     = 0xC6,  // get attribute       (3B: op + u16 string_id)
    SetAttr     = 0xC7,  // set attribute       (3B: op + u16 string_id)
    HasAttr     = 0xC8,  // has attribute?      (3B: op + u16 string_id)
    InstanceOf  = 0xC9,  // instanceof check
    GetField    = 0xCA,  // struct field by index
    SetField    = 0xCB,  // struct field by index
    GetFieldIdx = 0xCC,  // struct field by idx (u16 raw offset)
    SetFieldIdx = 0xCD,  // struct field by idx
    HasField    = 0xCE,  // struct has field?
    /// Push a zero-initialized value type struct onto the stack.
    /// Operand: u16 struct_id (index into module's StructDef table).
    /// If struct size ≤ 24 bytes → UValue::StructInline
    /// If struct size > 24 bytes → UValue::BoxedStruct (GC-allocated)
    InitStruct  = 0xCF,

    // ── 0xD0–0xDF: Functional & Coroutine ──
    Cons        = 0xD0,  // (a, b) → pair
    Car         = 0xD1,  // first of pair
    Cdr         = 0xD2,  // rest of pair
    List        = 0xD3,  // N items → list
    IsList      = 0xD4,  // is list? push bool
    MapFn       = 0xD5,  // map over container
    FilterFn    = 0xD6,  // filter container
    ReduceFn    = 0xD7,  // fold/reduce
    Compose     = 0xD8,  // function composition
    Delay       = 0xD9,  // create thunk
    Force       = 0xDA,  // evaluate thunk
    MakeCoroutine=0xDB,  // create coroutine
    CoroutineStatus=0xDC,// status of coroutine
    CoroutineYield=0xDD, // yield value
    ResumeWith  = 0xDE,  // resume with args
    Continuation= 0xDF,  // delimited continuation

    // ── 0xE0–0xEF: CIB, Module, Plugin ──
    CibLoad     = 0xE0,  // dlopen
    CibSym      = 0xE1,  // dlsym
    CibCall     = 0xE2,  // call C function
    CibWrap     = 0xE3,  // wrap C ptr → opaque
    CibUnwrap   = 0xE4,  // unwrap opaque → ptr
    CibFree     = 0xE5,  // free C resource
    CibStrToC   = 0xE6,  // UC string → C string
    CibStrFromC = 0xE7,  // C string → UC string
    CibSizeOf   = 0xE8,  // sizeof C type
    CibCallTyped= 0xE9,  // typed call via UCIF (u16 func index)
    CibLoadInterface=0xEA,// load UCIF interface (u16 string_id)
    CibStructPack=0xEB,  // pack struct for C ABI
    Import      = 0xEC,  // import module (u16 string_id)
    ImportFunc  = 0xED,  // get exported function
    ImportValue = 0xEE,  // get exported value
    Export      = 0xEF,  // export a value

    // ── 0xF0–0xFF: GC, JIT, Debug, Reserved ──
    Alloc       = 0xF0,  // gc-allocate          (tag, nslots)
    GcCollect   = 0xF1,  // trigger GC
    GcPin       = 0xF2,  // pin object
    GcUnpin     = 0xF3,  // unpin object
    GcStats     = 0xF4,  // push GC stats map
    WriteBarrier= 0xF5,  // notify GC of ptr store
    GcSetThreshold=0xF6, // set GC frequency
    JitCompile  = 0xF7,  // JIT compile function
    JitInvalidate=0xF8,  // invalidate JIT cache
    JitStat     = 0xF9,  // JIT statistics
    Print       = 0xFA,  // debug print
    Trace       = 0xFB,  // stack trace
    Breakpoint  = 0xFC,  // debugger trap
    Line        = 0xFD,  // source line info (3B: op + u16 line)
    Halt        = 0xFE,  // stop the VM
    /// Raise an exception
    Raise       = 0xFF,
}

// ═══════════════════════════════════════════════════════════════════════════
// Opcode Metadata
// ═══════════════════════════════════════════════════════════════════════════

/// Metadata for each opcode
#[derive(Clone)]
pub struct OpcodeInfo {
    pub mnemonic: &'static str,
    pub operand_size: u8,   // 0, 1, 2, 4, 8
    pub flags: OpFlags,
    pub stack_effect: i8,   // net change to stack depth
}

fn info(
    mnemonic: &'static str,
    operand_size: u8,
    flags: OpFlags,
    stack_effect: i8,
) -> OpcodeInfo {
    OpcodeInfo { mnemonic, operand_size, flags, stack_effect }
}


// ── Opcode impl (from_byte + Display) ──

impl Opcode {
    /// Convert u8 byte to Opcode
    pub fn from_byte(byte: u8) -> Option<Opcode> {
        use Opcode::*;
        match byte {
            // Stack
            0x00 => Some(Nop), 0x01 => Some(PushNil), 0x02 => Some(PushTrue),
            0x03 => Some(PushFalse), 0x04 => Some(PushI32), 0x05 => Some(PushI64),
            0x06 => Some(PushF32), 0x07 => Some(PushF64), 0x08 => Some(PushString),
            0x09 => Some(PushConst), 0x0A => Some(Dup), 0x0B => Some(DupN),
            0x0C => Some(Swap), 0x0D => Some(Pop), 0x0E => Some(PopN),
            0x0F => Some(Rot),
            // Int arith
            0x10 => Some(Add), 0x11 => Some(Sub), 0x12 => Some(Mul),
            0x13 => Some(Div), 0x14 => Some(Mod), 0x15 => Some(Neg),
            0x16 => Some(Inc), 0x17 => Some(Dec), 0x18 => Some(Abs),
            0x19 => Some(Pow), 0x1A => Some(CheckedAdd), 0x1B => Some(CheckedSub),
            0x1C => Some(CheckedMul), 0x1D => Some(SaturatingAdd), 0x1E => Some(SaturatingSub),
            0x1F => Some(WrappingAdd),
            // Float arith
            0x20 => Some(FAdd), 0x21 => Some(FSub), 0x22 => Some(FMul),
            0x23 => Some(FDiv), 0x24 => Some(FMod), 0x25 => Some(FNeg),
            0x26 => Some(FPow), 0x27 => Some(FSqrt), 0x28 => Some(FAbs),
            0x29 => Some(FFloor), 0x2A => Some(FCeil), 0x2B => Some(FRound),
            0x2C => Some(FSin), 0x2D => Some(FCos), 0x2E => Some(FTan),
            0x2F => Some(FAtan2),
            // Bitwise
            0x30 => Some(BitAnd), 0x31 => Some(BitOr), 0x32 => Some(BitXor),
            0x33 => Some(BitNot), 0x34 => Some(Shl), 0x35 => Some(Shr),
            0x36 => Some(UShr), 0x37 => Some(RotLeft), 0x38 => Some(RotRight),
            0x39 => Some(PopCount), 0x3A => Some(LeadingZeros), 0x3B => Some(TrailingZeros),
            0x3C => Some(ByteSwap), 0x3D => Some(BitReverse), 0x3E => Some(UDiv),
            0x3F => Some(UMod),
            // Compare & Logical
            0x40 => Some(Eq), 0x41 => Some(Ne), 0x42 => Some(Lt), 0x43 => Some(Le),
            0x44 => Some(Gt), 0x45 => Some(Ge), 0x46 => Some(Cmp), 0x47 => Some(Is),
            0x48 => Some(IsNot), 0x49 => Some(In), 0x4A => Some(NotIn), 0x4B => Some(And),
            0x4C => Some(Or), 0x4D => Some(Not), 0x4E => Some(Xor), 0x4F => Some(Truthy),
            // Type
            0x50 => Some(TypeOf), 0x51 => Some(IsType), 0x52 => Some(ToI32),
            0x53 => Some(ToI64), 0x54 => Some(ToF32), 0x55 => Some(ToF64),
            0x56 => Some(ToBool), 0x57 => Some(ToString), 0x58 => Some(Cast),
            0x59 => Some(BitCast), 0x5A => Some(EnumCreate), 0x5B => Some(EnumMatch),
            0x5C => Some(CheckIndex), 0x5D => Some(CheckType), 0x5E => Some(TypeAssert),
            0x5F => Some(Unreachable),
            // Control Flow
            0x60 => Some(Jump), 0x61 => Some(JumpIfFalse), 0x62 => Some(JumpIfTrue),
            0x63 => Some(JumpIfEq), 0x64 => Some(JumpIfNe), 0x65 => Some(JumpTable),
            0x66 => Some(ForPrep), 0x67 => Some(ForStep), 0x68 => Some(Loop),
            0x69 => Some(Switch), 0x6A => Some(MatchCheck), 0x6B => Some(Bind),
            0x6C => Some(GetIter), 0x6D => Some(Next), 0x6E => Some(Await),
            0x6F => Some(AsyncCall),
            // Call
            0x70 => Some(Call), 0x71 => Some(CallValue), 0x72 => Some(CallMethod),
            0x73 => Some(TailCall), 0x74 => Some(TailCallValue), 0x75 => Some(Invoke),
            0x76 => Some(SuperCall), 0x77 => Some(Apply), 0x78 => Some(Return),
            0x79 => Some(ReturnValue), 0x7A => Some(ReturnMultiple), 0x7B => Some(MakeClosure),
            0x7C => Some(Capture), 0x7D => Some(LoadUpvalue), 0x7E => Some(StoreUpvalue),
            0x7F => Some(Curry),
            // Variables
            0x80 => Some(LoadLocal), 0x81 => Some(StoreLocal), 0x82 => Some(LoadCapture),
            0x83 => Some(StoreCapture), 0x84 => Some(LoadGlobal), 0x85 => Some(StoreGlobal),
            0x86 => Some(LoadDynGlobal), 0x87 => Some(StoreDynGlobal), 0x88 => Some(AllocFrame),
            0x89 => Some(LoadArg), 0x8A => Some(LoadModuleVar), 0x8B => Some(StoreModuleVar),
            0x8C => Some(LoadUpvalueFrom), 0x8D => Some(StoreUpvalueTo), 0x8E => Some(This),
            0x8F => Some(ArgCount),
            // Array
            0x90 => Some(NewArray), 0x91 => Some(ArrayLen), 0x92 => Some(ArrayGet),
            0x93 => Some(ArraySet), 0x94 => Some(ArrayPush), 0x95 => Some(ArrayPop),
            0x96 => Some(ArrayUnshift), 0x97 => Some(ArrayShift), 0x98 => Some(ArrayInsert),
            0x99 => Some(ArrayRemove), 0x9A => Some(ArraySlice), 0x9B => Some(ArrayConcat),
            0x9C => Some(ArrayContains), 0x9D => Some(ArrayIndexOf), 0x9E => Some(ArraySort),
            0x9F => Some(ArrayReverse),
            // Map, Set, Range, Tuple
            0xA0 => Some(NewMap), 0xA1 => Some(MapGet), 0xA2 => Some(MapSet),
            0xA3 => Some(MapDel), 0xA4 => Some(MapContains), 0xA5 => Some(MapKeys),
            0xA6 => Some(MapLen), 0xA7 => Some(NewSet), 0xA8 => Some(SetAdd),
            0xA9 => Some(SetRemove), 0xAA => Some(SetContains), 0xAB => Some(SetLen),
            0xAC => Some(SetUnion), 0xAD => Some(SetIntersect), 0xAE => Some(NewRange),
            0xAF => Some(Tuple),
            // String
            0xB0 => Some(StrConcat), 0xB1 => Some(StrLen), 0xB2 => Some(StrGet),
            0xB3 => Some(StrSub), 0xB4 => Some(StrContains), 0xB5 => Some(StrIndexOf),
            0xB6 => Some(StrReplace), 0xB7 => Some(StrSplit), 0xB8 => Some(StrJoin),
            0xB9 => Some(StrToUpper), 0xBA => Some(StrToLower), 0xBB => Some(StrTrim),
            0xBC => Some(StrCmp), 0xBD => Some(StrFormat), 0xBE => Some(RegexCompile),
            0xBF => Some(RegexMatch),
            // OOP
            0xC0 => Some(NewNamespace), 0xC1 => Some(NewClass), 0xC2 => Some(NewObject),
            0xC3 => Some(ClassAddMethod), 0xC4 => Some(ClassAddField), 0xC5 => Some(ClassSetParent),
            0xC6 => Some(GetAttr), 0xC7 => Some(SetAttr), 0xC8 => Some(HasAttr),
            0xC9 => Some(InstanceOf), 0xCA => Some(GetField), 0xCB => Some(SetField),
            0xCC => Some(GetFieldIdx), 0xCD => Some(SetFieldIdx), 0xCE => Some(HasField),
            0xCF => Some(InitStruct),
            // Functional
            0xD0 => Some(Cons), 0xD1 => Some(Car), 0xD2 => Some(Cdr), 0xD3 => Some(List),
            0xD4 => Some(IsList), 0xD5 => Some(MapFn), 0xD6 => Some(FilterFn),
            0xD7 => Some(ReduceFn), 0xD8 => Some(Compose), 0xD9 => Some(Delay),
            0xDA => Some(Force), 0xDB => Some(MakeCoroutine), 0xDC => Some(CoroutineStatus),
            0xDD => Some(CoroutineYield), 0xDE => Some(ResumeWith), 0xDF => Some(Continuation),
            // CIB
            0xE0 => Some(CibLoad), 0xE1 => Some(CibSym), 0xE2 => Some(CibCall),
            0xE3 => Some(CibWrap), 0xE4 => Some(CibUnwrap), 0xE5 => Some(CibFree),
            0xE6 => Some(CibStrToC), 0xE7 => Some(CibStrFromC), 0xE8 => Some(CibSizeOf),
            0xE9 => Some(CibCallTyped), 0xEA => Some(CibLoadInterface), 0xEB => Some(CibStructPack),
            0xEC => Some(Import), 0xED => Some(ImportFunc), 0xEE => Some(ImportValue),
            0xEF => Some(Export),
            // GC, JIT, Debug
            0xF0 => Some(Alloc), 0xF1 => Some(GcCollect), 0xF2 => Some(GcPin),
            0xF3 => Some(GcUnpin), 0xF4 => Some(GcStats), 0xF5 => Some(WriteBarrier),
            0xF6 => Some(GcSetThreshold), 0xF7 => Some(JitCompile), 0xF8 => Some(JitInvalidate),
            0xF9 => Some(JitStat), 0xFA => Some(Print), 0xFB => Some(Trace),
            0xFC => Some(Breakpoint), 0xFD => Some(Line), 0xFE => Some(Halt),
            0xFF => Some(Raise),
        }
    }
}

impl fmt::Display for Opcode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", crate::opcodes::opcode_info(*self).mnemonic)
    }
}
