"""
Low-level FFI bindings to int2dds-ffi library.
"""

from typing import Any

from int2dds._ffi._bindings import ffi, lib

# Resolve lib accessors eagerly: lazy resolution takes ffi._lock, which a GC-run
# __del__ can hit while ffi.new() holds it (#482).
for _name in dir(lib):
    try:
        getattr(lib, _name)
    except Exception:
        pass
del _name

# cffi C-data handles are opaque to static type checkers; this alias keeps
# annotations free of Pylance's "variable not allowed in type expression".
CData = Any

__all__ = ["CData", "ffi", "lib"]
