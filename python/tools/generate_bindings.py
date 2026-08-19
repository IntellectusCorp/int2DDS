"""Generate `int2dds/_ffi/_bindings.py` from the committed C header.

The header is the ABI. Every declaration cffi is given is copied from it verbatim,
so a signature cannot drift the way a hand-written `cdef` could: an argument the
header added, a width it changed, a pointer it made an array pointer, all arrive
here unedited. What is dropped is only what cffi cannot parse -- comments and
preprocessor lines.

Dropping the `#define` lines drops the constants with them. Nothing reads them
through the library object: every attribute the binding takes off `lib` is a
function, and the return codes the package uses are declared in `int2dds/exceptions.py`.

The `#if` lines are not merely dropped, because what they guard differs per branch:
an explicitly sized enum is `enum E : int32_t` to a C23 or C++ compiler and
`typedef int32_t E` to anything older, and only the second is something cffi can
read. So the conditionals are evaluated for the translation unit cffi actually is
-- plain C, pre-C23, no compiler macros -- which is the branch whose types are the
ABI ones. Selecting a branch is why `extern "C" {` needs no special case: it sits
under `#ifdef __cplusplus`, and that is false here.

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
`ffi/include/int2dds-ffi.h` with its comments removed and its preprocessor
conditionals resolved for plain pre-C23 C; regenerate with

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


# A `#if` expression once `defined(...)` is false everywhere and every remaining
# identifier is the 0 the C preprocessor substitutes for an undefined one. What is
# left has to be arithmetic on literals, so anything outside this is a construct
# whose branch we would be guessing at -- and guessing picks types.
RESOLVED = re.compile(r"[\s0-9()]|and|or|not|[<>=!]=|[<>+*/&|^-]")


def condition_holds(expr: str) -> bool:
    """Is `expr` true where nothing is defined? That is the translation unit cffi is.

    Undefined identifiers evaluate to 0 (C17 6.10.1p4), so `__STDC_VERSION__ >=
    202311L` is false and the pre-C23 branch is the one taken -- which is the branch
    that spells an enum's width as a type cffi can parse.
    """
    reduced = re.sub(r"\bdefined\s*\(\s*\w+\s*\)|\bdefined\s+\w+", "0", expr)
    reduced = re.sub(r"\b\d+[uUlL]+\b", lambda m: m.group().rstrip("uUlL"), reduced)
    reduced = re.sub(r"\b[A-Za-z_]\w*\b", "0", reduced)
    reduced = reduced.replace("&&", " and ").replace("||", " or ")
    reduced = re.sub(r"!(?!=)", " not ", reduced)

    if RESOLVED.sub("", reduced).strip():
        raise SystemExit(
            f"{HEADER}: `#if {expr.strip()}` uses a construct this generator cannot\n"
            f"reduce, so which branch cffi should see is a guess. Teach the reduction\n"
            f"that form -- the branches of a conditional in this header hold different\n"
            f"types for the same name, and picking the wrong one is silent."
        )
    return bool(eval(reduced, {"__builtins__": {}}, {}))  # noqa: S307 - whitelisted above


def cdef_body(header: str) -> str:
    """The header reduced to declarations, each one unedited."""
    header = re.sub(r"/\*.*?\*/", "", header, flags=re.S)
    header = re.sub(r"//[^\n]*", "", header)

    kept: list[str] = []
    depth = 0
    # One entry per open conditional: whether its lines are being kept, and whether
    # any branch of it has been taken yet (which is what `#elif`/`#else` need).
    conditionals: list[tuple[bool, bool]] = []
    for line in header.splitlines():
        stripped = line.strip()
        if stripped.startswith("#"):
            parts = stripped[1:].strip().split(None, 1)
            directive = parts[0] if parts else ""
            rest = parts[1] if len(parts) > 1 else ""
            if directive in ("if", "ifdef", "ifndef"):
                expr = {
                    "if": rest,
                    "ifdef": f"defined({rest.strip()})",
                    "ifndef": f"!defined({rest.strip()})",
                }[directive]
                live = all(live for live, _ in conditionals) and condition_holds(expr)
                conditionals.append((live, live))
            elif directive in ("elif", "else"):
                if not conditionals:
                    raise SystemExit(f"{HEADER}: `#{directive}` outside any conditional")
                _, exhausted = conditionals[-1]
                live = (
                    not exhausted
                    and all(live for live, _ in conditionals[:-1])
                    and (directive == "else" or condition_holds(rest))
                )
                conditionals[-1] = (live, exhausted or live)
            elif directive == "endif":
                if not conditionals:
                    raise SystemExit(f"{HEADER}: `#endif` outside any conditional")
                conditionals.pop()
            continue
        if not all(live for live, _ in conditionals):
            continue
        depth += line.count("{") - line.count("}")
        kept.append(line.rstrip())

    if conditionals:
        raise SystemExit(f"{HEADER}: {len(conditionals)} conditional(s) left unclosed")
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
