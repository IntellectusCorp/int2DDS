"""Generate `int2dds/_ffi/_bindings.py` from the committed C header.

The header is the ABI. Every declaration cffi is given is copied from it verbatim,
so a signature cannot drift the way a hand-written `cdef` could: an argument the
header added, a width it changed, a pointer it made an array pointer, all arrive
here unedited. What is dropped is only what cffi cannot parse -- comments,
preprocessor lines, and the `extern "C"` wrapper.

Dropping the `#define` lines drops the constants with them. Nothing reads them
through the library object: every attribute the binding takes off `lib` is a
function, and the return codes the package uses are declared in `int2dds/exceptions.py`.

    python python/tools/generate_bindings.py            # rewrite the binding
    python python/tools/generate_bindings.py --check    # fail if it is stale

`--check` is what keeps the file generated rather than merely generated once.
`ffi/build.rs` refreshes the header whenever the ffi crate's sources change, and
that refresh does not reach here.
"""

from __future__ import annotations

import argparse
import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent.parent
HEADER = ROOT / "ffi" / "include" / "int2dds-ffi.h"
BINDING = ROOT / "python" / "int2dds" / "_ffi" / "_bindings.py"

PREAMBLE = '''"""cffi bindings for the int2dds-ffi library.

GENERATED FILE -- edits are overwritten. The declarations below are
`ffi/include/int2dds-ffi.h` with its comments, preprocessor lines and `extern "C"`
wrapper removed; regenerate with

    python python/tools/generate_bindings.py

ABI mode, so the declarations are resolved against the shared library at call
time and never compiled.
"""

from __future__ import annotations

import cffi

ffi = cffi.FFI()

ffi.cdef("""
'''

EPILOGUE = '''""")


from ._libpath import find_library as _find_library


# Load the library
_lib_path = _find_library()
try:
    lib = ffi.dlopen(_lib_path)
except OSError as e:
    raise ImportError(
        f"Could not load int2dds_ffi library from '{_lib_path}'. "
        f"Please ensure the library is built and available. "
        f"Set INT2DDS_FFI_PATH environment variable to specify the library location. "
        f"Original error: {e}"
    ) from e
'''


def cdef_body(header: str) -> str:
    """The header reduced to declarations, each one unedited."""
    header = re.sub(r"/\*.*?\*/", "", header, flags=re.S)
    header = re.sub(r"//[^\n]*", "", header)

    kept: list[str] = []
    depth = 0
    for line in header.splitlines():
        stripped = line.strip()
        if stripped.startswith("#"):
            continue
        if depth == 0 and stripped in ('extern "C" {', "}"):
            continue
        depth += line.count("{") - line.count("}")
        kept.append(line.rstrip())

    if depth != 0:
        raise SystemExit(f"{HEADER}: unbalanced braces after stripping ({depth:+})")

    body = re.sub(r"\n{3,}", "\n\n", "\n".join(kept))
    return body.strip("\n") + "\n"


def generate() -> str:
    return PREAMBLE + cdef_body(HEADER.read_text(encoding="utf-8")) + EPILOGUE


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--check",
        action="store_true",
        help="exit non-zero if the committed binding differs from what the header generates",
    )
    args = parser.parse_args()

    generated = generate()
    if args.check:
        current = BINDING.read_text(encoding="utf-8")
        if current != generated:
            print(
                f"{BINDING} is stale against {HEADER}.\n"
                f"Run: python python/tools/generate_bindings.py",
                file=sys.stderr,
            )
            return 1
        print(f"{BINDING.name} matches the header")
        return 0

    BINDING.write_text(generated, encoding="utf-8", newline="\n")
    print(f"wrote {BINDING} from {HEADER.name}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
