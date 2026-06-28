#!/usr/bin/env python3
"""
Generate handlers.rs from the big dispatch match in dispatch.rs.
"""
import re
import sys

def generate():
    with open("/home/awarrz/Documents/utencore/utencore-core/src/vm/dispatch.rs") as f:
        content = f.read()
    
    # Find the match body
    match_start = content.find("match op {")
    if match_start == -1:
        print("ERROR: Could not find 'match op {'")
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
    else:
        sys.exit(1)
    
    body = content[match_start:match_end+1]
    lines = body.split('\n')
    
    # Parse arms
    arms = []
    current_pattern = None
    current_body_lines = []
    brace_depth = 0
    in_body_brace = False
    in_arm = False
    
    for line_idx, raw_line in enumerate(lines):
        if line_idx == 0 or line_idx == len(lines) - 1:
            continue  # skip "match op {" and final "}"
        
        stripped = raw_line.rstrip()
        stripped_clean = stripped.strip()
        
        # Skip comments, empty lines, "use Opcode::*;"
        if not stripped_clean or stripped_clean.startswith('//') or stripped_clean == 'use Opcode::*;':
            continue
        
        if not in_arm:
            # Look for a match arm pattern: whitespace, name, =>
            line_end = stripped.rstrip()
            
            # Pattern: name => { ... }
            m = re.match(r'^(\s+)(\w+)\s*=>\s*\{(.*)', stripped)
            if m:
                in_arm = True
                current_pattern = m.group(2)
                brace_depth = stripped.count('{') - stripped.count('}')
                in_body_brace = (brace_depth > 0)
                rest_after_brace = m.group(3).rstrip()
                current_body_lines = []
                if rest_after_brace:
                    # There's content after the { on the same line
                    if brace_depth == 0:
                        # Closed on same line: Nop => {}
                        current_body_lines.append(rest_after_brace.rstrip(','))
                        arms.append((current_pattern, current_body_lines, True))
                        in_arm = False
                        current_pattern = None
                        current_body_lines = []
                    else:
                        current_body_lines.append(rest_after_brace)
                continue
            
            # Pattern: name => expr,
            m = re.match(r'^(\s+)(\w+)\s*=>\s*(.+),$', stripped)
            if m:
                current_pattern = m.group(2)
                expr = m.group(3).rstrip()
                arms.append((current_pattern, [expr], False))
                continue
            
            # Pattern: name => multi-line expr (ends with comma on later line)
            m = re.match(r'^(\s+)(\w+)\s*=>\s*(.+)$', stripped)
            if m:
                current_pattern = m.group(2)
                rest = m.group(3).rstrip()
                if rest.endswith(','):
                    arms.append((current_pattern, [rest[:-1]], False))
                else:
                    in_arm = True
                    in_body_brace = False
                    current_body_lines = [rest]
                    brace_depth = 0
                continue
        
        else:
            # We're in the body of an arm
            if in_body_brace:
                # Track brace depth
                for ch in stripped:
                    if ch == '{': brace_depth += 1
                    elif ch == '}': brace_depth -= 1
                
                if brace_depth == 0:
                    if stripped.strip():
                        current_body_lines.append(stripped)
                    # Remove trailing comma from last line
                    if current_body_lines and current_body_lines[-1].rstrip().endswith(','):
                        current_body_lines[-1] = current_body_lines[-1].rstrip()[:-1]
                    arms.append((current_pattern, current_body_lines, True))
                    in_arm = False
                    current_pattern = None
                    current_body_lines = []
                    in_body_brace = False
                else:
                    current_body_lines.append(stripped)
            else:
                # Expression body - ends with comma
                current_body_lines.append(stripped)
                if stripped.rstrip().endswith(','):
                    current_body_lines[-1] = current_body_lines[-1].rstrip()[:-1]
                    arms.append((current_pattern, current_body_lines, False))
                    in_arm = False
                    current_pattern = None
                    current_body_lines = []
    
    print(f"Parsed {len(arms)} match arms")
    for i, (pattern, body_lines, is_braced) in enumerate(arms[:20]):
        print(f"  Arm {i}: pattern='{pattern}', braced={is_braced}, body_lines={len(body_lines)}")
        for bl in body_lines[:3]:
            print(f"    {bl[:80]}")

if __name__ == "__main__":
    generate()
