"""
Low-level FFI bindings to int2dds-ffi library.
"""

from typing import Any

from int2dds._ffi._bindings import ffi, lib

# cffi C-data handles are opaque to static type checkers; this alias keeps
# annotations free of Pylance's "variable not allowed in type expression".
CData = Any

__all__ = ["CData", "ffi", "lib"]
