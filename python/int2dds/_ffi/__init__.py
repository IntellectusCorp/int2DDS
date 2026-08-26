"""
Low-level FFI bindings to int2dds-ffi library.
"""

from typing import Any

from int2dds._ffi._bindings import ffi, lib

# cffi builds each `lib.X` accessor lazily under ffi._lock — the same
# non-reentrant lock ffi.new() holds while parsing a C type string. A
# GC-triggered __del__ whose close() first touches a lib symbol during such a
# parse self-deadlocks, so resolve every accessor eagerly here.
for _name in dir(lib):
    try:
        getattr(lib, _name)
    except Exception:
        pass  # a missing symbol stays lazy and raises at first real use
del _name

# cffi C-data handles are opaque to static type checkers; this alias keeps
# annotations free of Pylance's "variable not allowed in type expression".
CData = Any

__all__ = ["CData", "ffi", "lib"]
