# utenstd.io — UtenCore Standard Library I/O Module
# Pure Python wrappers around utencore.Io native functions.

from utencore import Io

def read_file(path):
    return Io.read_file(path)

def write_file(path, content):
    return Io.write_file(path, content)

def read_line():
    return Io.read_line()
