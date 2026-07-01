# Test basic Python operations
# Each test prints "PASS" if it works

# Test 1: Basic arithmetic
result = 10 + 20
if result == 30:
    print("PASS: arithmetic")

# Test 2: Variables
x = 42
if x == 42:
    print("PASS: variables")

# Test 3: Function definition and call
def double(n):
    return n * 2

result = double(21)
if result == 42:
    print("PASS: functions")

# Test 4: Conditionals
x = 10
if x > 5:
    print("PASS: conditionals")

# Test 5: Strings
s = "hello"
if len(s) == 5:
    print("PASS: strings")

# Test 6: Lists
arr = [1, 2, 3]
if len(arr) == 3 and arr[0] == 1:
    print("PASS: lists")

# Test 7: While loop
count = 0
while count < 3:
    count += 1
if count == 3:
    print("PASS: while_loop")

# Test 8: For loop
total = 0
for i in range(3):
    total += i
if total == 3:
    print("PASS: for_loop")

print("DONE")
