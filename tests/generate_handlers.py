#!/usr/bin/env python3
"""
Generate handlers.rs from dispatch.rs match arms.
Fixed version: proper self.pc → *pc, dedent, handle empty bodies.
"""
import re
import sys

def generate():
    with open("/home/awarrz/Documents/utencore/utencore-core/src/vm/dispatch.rs") as f:
        content = f.read()
    
    # Find the match body
    match_start = content.find("match op {")
    if match_start == -1:
        sys.exit(1)
    
    brace_depth = 0
    i = match_start
    while i < len(content):
        if content[i] == '{':
            brace_depth += 1
        elif content[i] == '}':
            brace_depth -= 1
            if brace_depth == 0:
                match_end = i
                break
        i += 1
    
    body = content[match_start:match_end+1]
    lines = body.split('\n')
    
    # The match body lines start from index 1 (after "match op {") to -1 (before final "}")
    body_lines = lines[1:-1]
    
    # Determine base indentation from the first match arm
    base_indent = ""
    for line in body_lines:
        stripped = line.strip()
        if stripped and not stripped.startswith('//') and stripped != 'use Opcode::*;':
            base_indent = line[:len(line) - len(line.lstrip())]
            break
    
    # Parse arms
    arms = []
    current_pattern = None
    current_body_lines = []
    brace_depth = 0
    in_body_brace = False
    in_arm = False
    
    for raw_line in body_lines:
        stripped = raw_line.rstrip()
        stripped_clean = stripped.strip()
        
        if not stripped_clean or (not in_arm and stripped_clean.startswith('//')) or stripped_clean == 'use Opcode::*;':
            continue
        
        if not in_arm:
            # Try: name => {
            m = re.match(r'^\s*(\w+)\s*=>\s*\{(.*)', stripped)
            if m:
                current_pattern = m.group(1)
                rest = m.group(2).rstrip()
                cb = stripped.count('{') - stripped.count('}')
                
                if cb == 0:
                    # Closed on same line
                    if rest.endswith('}'):
                        rest = rest[:-1].strip()
                    if rest:
                        arms.append((current_pattern, [rest], True))
                    else:
                        arms.append((current_pattern, [], True))
                else:
                    in_arm = True
                    in_body_brace = True
                    brace_depth = cb
                    current_body_lines = [rest] if rest.strip() else []
                continue
            
            # Try: name => expr,
            m = re.match(r'^\s*(\w+)\s*=>\s*(.+),$', stripped)
            if m:
                arms.append((m.group(1), [m.group(2).strip()], False))
                continue
            
            # Try: name => expr (multi-line)
            m = re.match(r'^\s*(\w+)\s*=>\s*(.+)', stripped)
            if m:
                current_pattern = m.group(1)
                rest = m.group(2).strip()
                if rest.endswith(','):
                    arms.append((current_pattern, [rest[:-1]], False))
                else:
                    in_arm = True
                    in_body_brace = False
                    current_body_lines = [rest]
                continue
        
        else:
            if in_body_brace:
                for ch in stripped:
                    if ch == '{': brace_depth += 1
                    elif ch == '}': brace_depth -= 1
                
                if brace_depth == 0:
                    body_line = stripped.rstrip()
                    if body_line.rstrip().endswith(','):
                        body_line = body_line.rstrip()[:-1]
                    if body_line.strip():
                        current_body_lines.append(body_line)
                    arms.append((current_pattern, current_body_lines, True))
                    in_arm = False
                    in_body_brace = False
                    current_body_lines = []
                else:
                    current_body_lines.append(stripped)
            else:
                current_body_lines.append(stripped)
                if stripped.rstrip().endswith(','):
                    current_body_lines[-1] = current_body_lines[-1].rstrip()[:-1]
                    arms.append((current_pattern, current_body_lines, False))
                    in_arm = False
                    in_body_brace = False
                    current_body_lines = []
    
    print(f"Parsed {len(arms)} match arms")
    
    # Opcode values
    opcode_values = {
        "Nop": 0x00, "PushNil": 0x01, "PushTrue": 0x02, "PushFalse": 0x03,
        "PushI32": 0x04, "PushI64": 0x05, "PushF32": 0x06, "PushF64": 0x07,
        "PushString": 0x08, "PushConst": 0x09, "Dup": 0x0A, "DupN": 0x0B,
        "Swap": 0x0C, "Pop": 0x0D, "PopN": 0x0E, "Rot": 0x0F,
        "Add": 0x10, "Sub": 0x11, "Mul": 0x12, "Div": 0x13, "Mod": 0x14,
        "Neg": 0x15, "Inc": 0x16, "Dec": 0x17, "Abs": 0x18, "Pow": 0x19,
        "CheckedAdd": 0x1A, "CheckedSub": 0x1B, "CheckedMul": 0x1C,
        "SaturatingAdd": 0x1D, "SaturatingSub": 0x1E, "WrappingAdd": 0x1F,
        "FAdd": 0x20, "FSub": 0x21, "FMul": 0x22, "FDiv": 0x23, "FMod": 0x24,
        "FNeg": 0x25, "FPow": 0x26, "FSqrt": 0x27, "FAbs": 0x28,
        "FFloor": 0x29, "FCeil": 0x2A, "FRound": 0x2B, "FSin": 0x2C,
        "FCos": 0x2D, "FTan": 0x2E, "FAtan2": 0x2F,
        "BitAnd": 0x30, "BitOr": 0x31, "BitXor": 0x32, "BitNot": 0x33,
        "Shl": 0x34, "Shr": 0x35, "UShr": 0x36, "RotLeft": 0x37,
        "RotRight": 0x38, "PopCount": 0x39, "LeadingZeros": 0x3A,
        "TrailingZeros": 0x3B, "ByteSwap": 0x3C, "BitReverse": 0x3D,
        "UDiv": 0x3E, "UMod": 0x3F,
        "Eq": 0x40, "Ne": 0x41, "Lt": 0x42, "Le": 0x43, "Gt": 0x44,
        "Ge": 0x45, "Cmp": 0x46, "Is": 0x47, "IsNot": 0x48, "In": 0x49,
        "NotIn": 0x4A, "And": 0x4B, "Or": 0x4C, "Not": 0x4D, "Xor": 0x4E,
        "Truthy": 0x4F,
        "TypeOf": 0x50, "IsType": 0x51, "ToI32": 0x52, "ToI64": 0x53,
        "ToF32": 0x54, "ToF64": 0x55, "ToBool": 0x56, "ToString": 0x57,
        "Cast": 0x58, "BitCast": 0x59, "EnumCreate": 0x5A, "EnumMatch": 0x5B,
        "CheckIndex": 0x5C, "CheckType": 0x5D, "TypeAssert": 0x5E,
        "Unreachable": 0x5F,
        "Jump": 0x60, "JumpIfFalse": 0x61, "JumpIfTrue": 0x62,
        "JumpIfEq": 0x63, "JumpIfNe": 0x64, "JumpTable": 0x65,
        "ForPrep": 0x66, "ForStep": 0x67, "Loop": 0x68, "Switch": 0x69,
        "MatchCheck": 0x6A, "Bind": 0x6B, "GetIter": 0x6C, "Next": 0x6D,
        "Await": 0x6E, "AsyncCall": 0x6F,
        "Call": 0x70, "CallValue": 0x71, "CallMethod": 0x72,
        "TailCall": 0x73, "TailCallValue": 0x74, "Invoke": 0x75,
        "SuperCall": 0x76, "Apply": 0x77, "Return": 0x78,
        "ReturnValue": 0x79, "ReturnMultiple": 0x7A, "MakeClosure": 0x7B,
        "Capture": 0x7C, "LoadUpvalue": 0x7D, "StoreUpvalue": 0x7E,
        "Curry": 0x7F,
        "LoadLocal": 0x80, "StoreLocal": 0x81, "LoadCapture": 0x82,
        "StoreCapture": 0x83, "LoadGlobal": 0x84, "StoreGlobal": 0x85,
        "LoadDynGlobal": 0x86, "StoreDynGlobal": 0x87, "AllocFrame": 0x88,
        "LoadArg": 0x89, "LoadModuleVar": 0x8A, "StoreModuleVar": 0x8B,
        "LoadUpvalueFrom": 0x8C, "StoreUpvalueTo": 0x8D, "This": 0x8E,
        "ArgCount": 0x8F,
        "NewArray": 0x90, "ArrayLen": 0x91, "ArrayGet": 0x92,
        "ArraySet": 0x93, "ArrayPush": 0x94, "ArrayPop": 0x95,
        "ArrayUnshift": 0x96, "ArrayShift": 0x97, "ArrayInsert": 0x98,
        "ArrayRemove": 0x99, "ArraySlice": 0x9A, "ArrayConcat": 0x9B,
        "ArrayContains": 0x9C, "ArrayIndexOf": 0x9D, "ArraySort": 0x9E,
        "ArrayReverse": 0x9F,
        "NewMap": 0xA0, "MapGet": 0xA1, "MapSet": 0xA2, "MapDel": 0xA3,
        "MapContains": 0xA4, "MapKeys": 0xA5, "MapLen": 0xA6, "NewSet": 0xA7,
        "SetAdd": 0xA8, "SetRemove": 0xA9, "SetContains": 0xAA,
        "SetLen": 0xAB, "SetUnion": 0xAC, "SetIntersect": 0xAD,
        "NewRange": 0xAE, "Tuple": 0xAF,
        "StrConcat": 0xB0, "StrLen": 0xB1, "StrGet": 0xB2, "StrSub": 0xB3,
        "StrContains": 0xB4, "StrIndexOf": 0xB5, "StrReplace": 0xB6,
        "StrSplit": 0xB7, "StrJoin": 0xB8, "StrToUpper": 0xB9,
        "StrToLower": 0xBA, "StrTrim": 0xBB, "StrCmp": 0xBC,
        "StrFormat": 0xBD, "RegexCompile": 0xBE, "RegexMatch": 0xBF,
        "NewNamespace": 0xC0, "NewClass": 0xC1, "NewObject": 0xC2,
        "ClassAddMethod": 0xC3, "ClassAddField": 0xC4, "ClassSetParent": 0xC5,
        "GetAttr": 0xC6, "SetAttr": 0xC7, "HasAttr": 0xC8,
        "InstanceOf": 0xC9, "GetField": 0xCA, "SetField": 0xCB,
        "GetFieldIdx": 0xCC, "SetFieldIdx": 0xCD, "HasField": 0xCE,
        "InitStruct": 0xCF,
        "Cons": 0xD0, "Car": 0xD1, "Cdr": 0xD2, "List": 0xD3,
        "IsList": 0xD4, "MapFn": 0xD5, "FilterFn": 0xD6, "ReduceFn": 0xD7,
        "Compose": 0xD8, "Delay": 0xD9, "Force": 0xDA,
        "MakeCoroutine": 0xDB, "CoroutineStatus": 0xDC,
        "CoroutineYield": 0xDD, "ResumeWith": 0xDE, "Continuation": 0xDF,
        "CibLoad": 0xE0, "CibSym": 0xE1, "CibCall": 0xE2,
        "CibWrap": 0xE3, "CibUnwrap": 0xE4, "CibFree": 0xE5,
        "CibStrToC": 0xE6, "CibStrFromC": 0xE7, "CibSizeOf": 0xE8,
        "CibCallTyped": 0xE9, "CibLoadInterface": 0xEA, "CibStructPack": 0xEB,
        "Import": 0xEC, "ImportFunc": 0xED, "ImportValue": 0xEE,
        "Export": 0xEF,
        "Alloc": 0xF0, "GcCollect": 0xF1, "GcPin": 0xF2, "GcUnpin": 0xF3,
        "GcStats": 0xF4, "WriteBarrier": 0xF5, "GcSetThreshold": 0xF6,
        "JitCompile": 0xF7, "JitInvalidate": 0xF8, "JitStat": 0xF9,
        "Print": 0xFA, "Trace": 0xFB, "Breakpoint": 0xFC, "Line": 0xFD,
        "Halt": 0xFE, "Raise": 0xFF,
    }
    
    def dedent_line(line):
        """Remove the base indentation from a line."""
        if line.startswith(base_indent):
            return line[len(base_indent):]
        return line
    
    def transform_body(text):
        """Apply self → vm, self.pc → *pc, Self:: → Vm:: transformations."""
        # Order matters: self.pc before self.
        text = text.replace("self.pc", "*pc")
        text = text.replace("self.", "vm.")
        text = text.replace("Self::", "Vm::")
        text = text.replace("self)", "vm)")
        text = text.replace("self,", "vm,")
        text = re.sub(r'\bself\b', 'vm', text)
        # Handle current_bytecode_ptr/len references (already replaced to vm.*)
        text = text.replace("vm.current_bytecode_ptr", "bytecode.as_ptr()")
        text = text.replace("vm.current_bytecode_len", "bytecode.len()")
        return text
    
    def normalize_indent(lines):
        """Remove common leading whitespace, giving proper function-body indentation."""
        # Find minimum indentation of non-empty lines
        min_indent = None
        for line in lines:
            stripped = line.lstrip()
            if stripped:
                ws = line[:len(line) - len(stripped)]
                if min_indent is None or len(ws) < min_indent:
                    min_indent = len(ws)
        
        if min_indent is None:
            return ["" for _ in lines]
        
        # Remove common indent, keep relative indentation
        result = []
        for line in lines:
            stripped = line.strip()
            if stripped:
                result.append(line[min_indent:])
            else:
                result.append("")
        return result
    
    # Generate handler functions
    handler_funcs = []
    
    for pattern, body_lines, is_braced in arms:
        fn_name = f"handle_{pattern.lower()}"
        
        if not body_lines or all(not l.strip() for l in body_lines):
            # Empty body
            func = f"fn {fn_name}(vm: &mut Vm, _operand: u32, _pc: &mut usize, _bytecode: &[u8]) -> UtenResult<()> {{\n    Ok(())\n}}"
        else:
            if is_braced:
                # Braced body: remove common leading whitespace, keep relative indentation
                min_indent = None
                for bl in body_lines:
                    s = bl.lstrip()
                    if s:
                        ws = len(bl) - len(s)
                        if min_indent is None or ws < min_indent:
                            min_indent = ws
                dedented_lines = []
                for bl in body_lines:
                    if bl.strip():
                        dedented_lines.append(bl[min_indent:] if min_indent else bl)
                    else:
                        dedented_lines.append("")
            else:
                # Non-braced body: strip all, then add 4-space indent
                dedented_lines = [f"    {bl.strip()}" for bl in body_lines if bl.strip()]
            
            body_text = "\n".join(dedented_lines)
            
            # Remove trailing } that was part of the original brace
            if is_braced:
                if body_text.rstrip().endswith('}'):
                    body_text = body_text.rstrip()
                    if body_text.endswith('}'):
                        body_text = body_text[:-1].rstrip()
            
            # Fix indentation: if body lines start at column 0, add 4 spaces
            fixed_lines = []
            for bl in body_text.split('\n'):
                if bl.strip() and not bl.startswith(' '):
                    fixed_lines.append(f"    {bl}")
                else:
                    fixed_lines.append(bl)
            body_text = "\n".join(fixed_lines)
            
            body_text = transform_body(body_text)
            
            # Ensure the function returns Ok(()) if the body doesn't explicitly return
            # Check if the last line contains a return statement
            last_line = None
            for bl in body_text.split('\n'):
                stripped = bl.strip()
                if stripped:
                    last_line = stripped
            
            needs_ok = True
            if last_line and 'return ' in last_line:
                needs_ok = False
            
            if needs_ok:
                body_text = body_text.rstrip() + "\n    Ok(())"
            
            func = f"fn {fn_name}(vm: &mut Vm, operand: u32, pc: &mut usize, bytecode: &[u8]) -> UtenResult<()> {{\n{body_text}\n}}"
        
        handler_funcs.append((pattern, fn_name, func))
    
    # Generate dispatch table
    table_entries = [None] * 256
    
    for pattern, fn_name, _ in handler_funcs:
        if pattern in opcode_values:
            idx = opcode_values[pattern]
            table_entries[idx] = f"Some(handle_{pattern.lower()})"
    
    table_lines = []
    for i in range(256):
        entry = table_entries[i]
        if entry is None:
            table_lines.append("    None, // 0x{:02X}\n".format(i))
        else:
            table_lines.append("    {}, // 0x{:02X}\n".format(entry, i))
    
    # Write handlers.rs
    header = """// ── Opcode Handlers ──
// Auto-generated from dispatch match arms.
// Each opcode has its own handler function.

use std::collections::{HashMap, HashSet};
use crate::bytecode::ExportEntry;
use crate::error::{UtenError, UtenResult};
use crate::opcodes::{Opcode, OpFlags, opcode_info};
use crate::types::*;
use super::*;

/// Type signature for an opcode handler function.
pub type OpcodeHandler = fn(&mut Vm, operand: u32, pc: &mut usize, bytecode: &[u8]) -> UtenResult<()>;

"""
    
    output = header
    for _, _, func in handler_funcs:
        output += func + "\n\n"
    
    output += "/// Dispatch table: maps opcode byte (0-255) to its handler function.\n"
    output += "/// Unused opcode slots are `None`.\n"
    output += "pub static OPCODE_HANDLERS: [Option<OpcodeHandler>; 256] = [\n"
    output += "".join(table_lines)
    output += "];\n"
    
    with open("/home/awarrz/Documents/utencore/utencore-core/src/vm/handlers.rs", "w") as f:
        f.write(output)
    
    print(f"Generated handlers.rs with {len(handler_funcs)} handler functions")
    missing = sum(1 for e in table_entries if e is None)
    print(f"Table entries: {256 - missing} filled, {missing} None")
    
    # Show some sample handlers
    print("\n=== Sample handlers ===")
    for pattern in ["Nop", "PushF64", "Return", "Jump", "Line", "Raise"]:
        for p, fn_name, func in handler_funcs:
            if p == pattern:
                print(f"\n--- {fn_name} ---")
                print(func)
                break

if __name__ == "__main__":
    generate()
