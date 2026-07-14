"""
XML-driven configuration — mirrors the Rust ``DomainParticipantFactory`` config
APIs over the C FFI:

- :func:`load_profiles` (``int2dds_load_profiles``) loads QoS profiles (and, for
  XML files, the ``<types>`` section) into the factory singleton.
- :func:`get_dynamic_type_support` (``int2dds_get_dynamic_type_support``) builds a
  :class:`~int2dds.types.dynamic.DynamicTypeSupport` for a type declared in a
  loaded ``<types>`` section.
- :func:`create_participant_from_config` (``int2dds_create_participant_from_config``)
  builds an entire participant tree (participant + publishers/subscribers +
  datawriters/datareaders + topics) from a ``<domain_participant_library>``
  declaration, returning a :class:`ConfiguredParticipant`.

Typical use (mirrors ``hello_world_xml_dyn_pub.rs``)::

    from int2dds import load_profiles, create_participant_from_config, get_dynamic_type_support

    load_profiles(["profiles/xml/pub_profile.xml"])
    cfg = create_participant_from_config("PL::PubApp")
    writer = cfg.datawriter("pub::writer")
    support = get_dynamic_type_support("HelloWorldType")

    data = support.create_data()
    data.set_u32("index", 1)
    data.set_string("message", "HelloWorld_dyn_1")
    writer.write(data)
"""

from __future__ import annotations

from int2dds._ffi import CData, ffi, lib
from int2dds.core.participant import _get_factory
from int2dds.exceptions import check_ret
from int2dds.types.dynamic import (
    DynamicDataReader,
    DynamicDataWriter,
    DynamicTypeSupport,
    _cstr,
)


def load_profiles(paths: list[str] | str) -> None:
    """Load QoS profiles (and, for XML files, ``<types>``) into the factory singleton.

    Accepts a single path or a list. Loaded profiles can then be used with
    :func:`create_participant_from_config`; declared types can be fetched with
    :func:`get_dynamic_type_support`.
    """
    if isinstance(paths, str):
        paths = [paths]
    encoded = [ffi.new("char[]", str(p).encode()) for p in paths]
    arr = ffi.new("char *[]", encoded)
    check_ret(lib.int2dds_load_profiles(_get_factory().handle, arr, len(encoded)))


def get_dynamic_type_support(type_name: str) -> DynamicTypeSupport:
    """Build a :class:`DynamicTypeSupport` for a type from a loaded ``<types>`` section."""
    out = ffi.new("Int2DdsDynamicTypeSupport **")
    check_ret(lib.int2dds_get_dynamic_type_support(_get_factory().handle, _cstr(type_name), out))
    return DynamicTypeSupport(out[0])


def create_participant_from_config(path: str) -> "ConfiguredParticipant":
    """Build an entire participant tree from a ``<domain_participant_library>``
    declaration at ``path`` (``ParticipantLibrary::Participant``, e.g. ``"PL::PubApp"``).
    The XML must have been loaded via :func:`load_profiles`.
    """
    out = ffi.new("Int2DdsConfiguredParticipant **")
    check_ret(lib.int2dds_create_participant_from_config(_get_factory().handle, _cstr(path), out))
    return ConfiguredParticipant(out[0])


class ConfiguredParticipant:
    """A whole participant tree built by :func:`create_participant_from_config`.

    Datawriters/readers carry :class:`~int2dds.types.dynamic.DynamicData` and are
    addressable by their XML name (``"<publisher>::<writer>"`` /
    ``"<subscriber>::<reader>"``). Closing this object tears the tree down.
    """

    def __init__(self, handle: CData) -> None:
        self._handle = handle

    def datawriter(self, name: str) -> DynamicDataWriter:
        """Return the datawriter declared as ``"<publisher>::<writer>"``."""
        out = ffi.new("Int2DdsDynamicDataWriter **")
        check_ret(
            lib.int2dds_configured_participant_get_datawriter(self._handle, _cstr(name), out)
        )
        return DynamicDataWriter(out[0])

    def datareader(self, name: str) -> DynamicDataReader:
        """Return the datareader declared as ``"<subscriber>::<reader>"``."""
        out = ffi.new("Int2DdsDynamicDataReader **")
        check_ret(
            lib.int2dds_configured_participant_get_datareader(self._handle, _cstr(name), out)
        )
        return DynamicDataReader(out[0])

    def close(self) -> None:
        """Tear down the configured tree (participant + owned entities).

        Destroy any datawriter/datareader handles obtained via :meth:`datawriter` /
        :meth:`datareader` BEFORE calling this — closing the tree deletes the
        participant's contained entities, so using those handles afterwards is
        undefined.
        """
        if getattr(self, "_handle", None) is not None:
            lib.int2dds_configured_participant_destroy(self._handle)
            self._handle = None

    def __del__(self) -> None:
        # Safety net for callers who forget close()/`with`; may run at interpreter
        # shutdown when globals are torn down, so swallow everything.
        try:
            self.close()
        except Exception:
            pass

    def __enter__(self) -> "ConfiguredParticipant":
        return self

    def __exit__(self, *exc) -> None:
        self.close()
