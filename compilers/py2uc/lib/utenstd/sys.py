# utenstd.sys — UtenCore Standard Library System Module
# Pure Python wrappers around utencore.Sys native functions.

from utencore import Sys

def clock_ms():
    return Sys.clock_ms()

def sleep(ms):
    return Sys.sleep(ms)
