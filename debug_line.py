#!/usr/bin/env python3
"""Debug the Line arm parsing."""
import re

with open("/home/awarrz/Documents/utencore/utencore-core/src/vm/dispatch.rs") as f:
    content = f.read()

# Find the Line arm
idx = content.find("Line => {")
if idx >= 0:
    # Show context
    start = max(0, idx - 5)
    end = min(len(content), idx + 200)
    print("Context around Line => {:")
    print(content[start:end])
    
# Also check the parser output more carefully
match_start = content.find("match op {")
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

# Find the Line arm
for line_idx, line in enumerate(lines):
    if 'Line' in line and '=>' in line:
        print(f"\nLine arm at index {line_idx}: {repr(line[:100])}")
        # Print next few lines
        for j in range(line_idx, min(line_idx + 5, len(lines))):
            print(f"  [{j}] {repr(lines[j][:100])}")
