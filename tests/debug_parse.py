#!/usr/bin/env python3
"""
Debug the match arm parsing.
"""
import re

with open("/home/awarrz/Documents/utencore/utencore-core/src/vm/dispatch.rs") as f:
    content = f.read()

# Find the match start
match_start = content.find("match op {")
print(f"match_start at position {match_start}")

# Find the matching closing brace
brace_depth = 0
i = match_start
while i < len(content):
    if content[i] == '{':
        brace_depth += 1
    elif content[i] == '}':
        brace_depth -= 1
        if brace_depth == 0:
            match_end = i
            print(f"match_end at position {match_end}")
            break
    i += 1

body = content[match_start:match_end+1]
print(f"Body length: {len(body)} chars, {len(body.split(chr(10)))} lines")

# Let me look at some sample lines
lines = body.split('\n')
print(f"\nFirst 30 lines of match body:")
for i, line in enumerate(lines[:30]):
    print(f"  [{i:3d}] repr={repr(line)}")

# Look for patterns that match => 
print(f"\nLines with '=>' in first 100 lines:")
for i, line in enumerate(lines[1:101], 1):  # skip first line "match op {"
    if '=>' in line:
        print(f"  [{i:3d}] {repr(line[:120])}")
