"""
Path resolution for the int2dds_ffi native library.

Does not import cffi. Split out of _bindings.py so the path resolution logic can
be tested on its own without loading the library.
"""

from __future__ import annotations

import os
import sys
from pathlib import Path


def library_filename() -> str:
    """Native library filename for the current platform."""
    if sys.platform == "win32":
        return "int2dds_ffi.dll"
    if sys.platform == "darwin":
        return "libint2dds_ffi.dylib"
    return "libint2dds_ffi.so"


def bundled_dir() -> Path:
    """In-package directory where platform wheels bundle the native library."""
    # __file__ = <...>/int2dds/_ffi/_libpath.py  ->  parent.parent = <...>/int2dds
    return Path(__file__).resolve().parent.parent / "_native"


def candidate_paths(lib_name: str) -> list[Path]:
    """Candidate paths in priority order."""
    paths: list[Path] = []

    # 1. Environment variable — overriding the bundled library is what makes
    #    development and debugging possible. Both forms are accepted: pointing
    #    directly at the file (documented in the README) and pointing at a
    #    directory (the form the C# binding and CI use).
    env_path = os.environ.get("INT2DDS_FFI_PATH")
    if env_path:
        env_base = Path(env_path)
        paths.append(env_base)
        paths.append(env_base / lib_name)

    # 2. Native library bundled in the package (platform wheels)
    paths.append(bundled_dir() / lib_name)

    # 3. Repository development tree
    package_dir = Path(__file__).resolve().parent.parent.parent
    paths.extend([
        package_dir / lib_name,
        package_dir.parent / "target" / "release" / lib_name,
        package_dir.parent / "target" / "debug" / lib_name,
        package_dir.parent / "ffi" / "target" / "release" / lib_name,
        package_dir.parent / "ffi" / "target" / "debug" / lib_name,
    ])

    # 4. System library paths
    if sys.platform == "win32":
        system_paths = os.environ.get("PATH", "").split(os.pathsep)
    else:
        system_paths = [
            "/usr/local/lib",
            "/usr/lib",
            os.path.expanduser("~/.local/lib"),
        ]
        ld_path = os.environ.get("LD_LIBRARY_PATH", "")
        if ld_path:
            system_paths = ld_path.split(os.pathsep) + system_paths

    paths.extend(Path(p) / lib_name for p in system_paths)
    return paths


def find_library() -> str:
    """First existing candidate path, or the bare filename so the OS loader can
    take over when none is found."""
    lib_name = library_filename()
    for path in candidate_paths(lib_name):
        if path.exists():
            return str(path)
    return lib_name
