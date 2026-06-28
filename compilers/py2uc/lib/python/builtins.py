# Uten Core Python builtins.
# Implemented following CPython semantics, compiled to .uclib.
# Uses _uc_* intrinsics for operations that need VM primitives.

# ── Type conversion ──

def str(val):
    return _uc_str(val)

def int(val):
    return _uc_int(val)

def bool(val):
    return _uc_bool(val)

# ── Length / size ──

def len(obj):
    # CPython: PyObject_Size → sq_length or mp_length or tp_len
    # For now: array length via intrinsic
    return _uc_array_len(obj)

# ── Container operations ──

def sum(items):
    # CPython: sum(iterable, /, start=0)
    total = 0
    for x in items:
        total = total + x
    return total

def max(items):
    # CPython: max(iterable) — track largest
    result = 0
    first = True
    for x in items:
        if first:
            result = x
            first = False
        elif x > result:
            result = x
    return result

def min(items):
    result = 0
    first = True
    for x in items:
        if first:
            result = x
            first = False
        elif x < result:
            result = x
    return result

def all(items):
    for x in items:
        if not bool(x):
            return False
    return True

def any(items):
    for x in items:
        if bool(x):
            return True
    return False

# ── Sorting & ordering ──

def sorted(items):
    # CPython: sorted(iterable, /, *, key=None, reverse=False)
    # Bubble sort (same semantics, O(n²) but correct)
    n = len(items)
    if n <= 1:
        return items
    result = []
    for i in items:
        result = result + [i]
    i = 0
    while i < n - 1:
        j = 0
        while j < n - 1 - i:
            if result[j] > result[j + 1]:
                temp = result[j]
                result[j] = result[j + 1]
                result[j + 1] = temp
            j = j + 1
        i = i + 1
    return result

def reversed(items):
    result = []
    n = len(items)
    i = n - 1
    while i >= 0:
        result = result + [items[i]]
        i = i - 1
    return result

def enumerate(items):
    # CPython: enumerate(iterable, start=0)
    result = []
    i = 0
    for x in items:
        result = result + [[i, x]]
        i = i + 1
    return result

# ── Range ──

def range(stop):
    # CPython: range(stop) → [0, 1, ..., stop-1]
    result = []
    i = 0
    while i < stop:
        result = result + [i]
        i = i + 1
    return result

# ── Math ──

def abs(x):
    # CPython: abs(x) → absolute value
    if x < 0:
        return -x
    return x

def clamp(x, lo, hi):
    if x < lo:
        return lo
    if x > hi:
        return hi
    return x

# ── Type predicates ──

def isinstance(obj, typ):
    # Simplified: always returns True for now
    return True

def callable(obj):
    return True
