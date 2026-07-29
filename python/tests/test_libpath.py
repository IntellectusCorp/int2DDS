"""Unit tests for path resolution in int2dds._ffi._libpath."""

import sys
from pathlib import Path

import pytest

from int2dds._ffi import _libpath


def test_library_filename_matches_platform():
    name = _libpath.library_filename()
    if sys.platform == "win32":
        assert name == "int2dds_ffi.dll"
    elif sys.platform == "darwin":
        assert name == "libint2dds_ffi.dylib"
    else:
        assert name == "libint2dds_ffi.so"


def test_bundled_dir_is_inside_package():
    bundled = _libpath.bundled_dir()
    assert bundled.name == "_native"
    assert bundled.parent.name == "int2dds"


def test_bundled_path_is_a_candidate():
    name = _libpath.library_filename()
    paths = _libpath.candidate_paths(name)
    assert _libpath.bundled_dir() / name in paths


def test_env_var_outranks_bundled(monkeypatch):
    monkeypatch.setenv("INT2DDS_FFI_PATH", "/custom/dir")
    name = _libpath.library_filename()
    paths = _libpath.candidate_paths(name)
    env_base = Path("/custom/dir")

    # Both forms must be candidates: pointing directly at the file (documented in
    # the README) and pointing at a directory (used by the C# binding and CI).
    assert env_base in paths
    assert env_base / name in paths

    bundled_index = paths.index(_libpath.bundled_dir() / name)
    assert paths.index(env_base) < bundled_index
    assert paths.index(env_base / name) < bundled_index


def test_env_var_file_form_outranks_dir_form(monkeypatch):
    """The file form is the more specific interpretation, so it comes before the
    directory form."""
    monkeypatch.setenv("INT2DDS_FFI_PATH", "/custom/dir")
    name = _libpath.library_filename()
    paths = _libpath.candidate_paths(name)
    env_base = Path("/custom/dir")

    file_index = paths.index(env_base)
    dir_index = paths.index(env_base / name)
    assert file_index < dir_index


def test_bundled_outranks_system_paths(monkeypatch):
    monkeypatch.delenv("INT2DDS_FFI_PATH", raising=False)
    name = _libpath.library_filename()
    paths = _libpath.candidate_paths(name)
    bundled_index = paths.index(_libpath.bundled_dir() / name)
    # The system paths belong in the back half of the candidate list.
    assert bundled_index < len(paths) - 1


def test_find_library_falls_back_to_bare_name(monkeypatch, tmp_path):
    monkeypatch.setenv("INT2DDS_FFI_PATH", str(tmp_path / "nonexistent"))
    monkeypatch.setattr(_libpath, "candidate_paths", lambda name: [])
    assert _libpath.find_library() == _libpath.library_filename()
