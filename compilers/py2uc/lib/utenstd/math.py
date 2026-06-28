# utenstd.math — UtenCore Standard Library Math Module
# Pure Python wrappers around utencore.Math native functions.
# Compiled to .uclib by py2uc at build time.

from utencore import Math

def sqrt(x):
    return Math.sqrt(x)

def sin(x):
    return Math.sin(x)

def cos(x):
    return Math.cos(x)

def tan(x):
    return Math.tan(x)

def floor(x):
    return Math.floor(x)

def ceil(x):
    return Math.ceil(x)

def round(x):
    return Math.round(x)

def abs(x):
    return Math.abs(x)

def pow(x, y):
    return Math.pow(x, y)

def pi():
    return Math.pi()

def e():
    return Math.e()
