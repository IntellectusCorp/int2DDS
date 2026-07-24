"""Environment-variable–driven configuration for int2DDS.

These helpers wrap the underlying ``INT2DDS_*`` environment variables that the
Rust core consults during participant creation. They mutate the *current
process* environment, so they must be called before the first
:class:`int2dds.DomainParticipant` is created in order to take effect.

Example:
    >>> from int2dds import env
    >>> env.set_multicast_ttl(32)            # INT2DDS_MULTICAST_TTL=32
    >>> participant = int2dds.DomainParticipant(...)
"""

from __future__ import annotations

from int2dds._ffi import ffi, lib
from int2dds.exceptions import DdsError, DdsInvalidArgument


_RET_OK = 0


def _check(ret: int, op: str) -> None:
    if ret == _RET_OK:
        return
    if ret == 11 or ret == 100:  # INVALID_ARGUMENT, NULL_POINTER
        raise DdsInvalidArgument(f"{op} failed: ret={ret}")
    raise DdsError(f"{op} failed: ret={ret}")


def set_multicast_ttl(ttl: int) -> None:
    """Set the IPv4 multicast TTL fallback via ``INT2DDS_MULTICAST_TTL``.

    The Rust core uses this value only when a participant's
    ``PropertyQosPolicy`` does not carry an explicit
    ``int2dds.transport.UDPv4.multicast_ttl`` entry, so explicit QoS settings
    always win.

    Args:
        ttl: 0-255. Values outside this range raise :class:`ValueError`.
    """
    if not isinstance(ttl, int) or not 0 <= ttl <= 255:
        raise ValueError(f"multicast TTL must be in [0, 255], got {ttl!r}")
    _check(lib.int2dds_env_set_multicast_ttl(ttl), "int2dds_env_set_multicast_ttl")


def get_multicast_ttl() -> int | None:
    """Read the current ``INT2DDS_MULTICAST_TTL`` override.

    Returns:
        The TTL as an ``int`` when the variable is set to a valid ``u8``
        (0-255); ``None`` when unset, empty, or invalid.
    """
    ttl_p = ffi.new("uint8_t *")
    has_p = ffi.new("bool *")
    _check(lib.int2dds_env_get_multicast_ttl(ttl_p, has_p), "int2dds_env_get_multicast_ttl")
    return int(ttl_p[0]) if has_p[0] else None


def set_qos_profile(path: str) -> None:
    """Set the QoS profile file path(s) to auto-load via ``DDS_QOS_PROFILE``.

    The ``DomainParticipantFactory`` auto-loads these when the first participant
    is created, and the default-QoS resolution then draws from the selected
    default profile. Call *before* creating the first participant. Multiple paths
    may be joined with ``,`` (also ``;`` on Windows / ``:`` on Unix).
    """
    _check(
        lib.int2dds_env_set_qos_profile(path.encode("utf-8")),
        "int2dds_env_set_qos_profile",
    )


def set_default_qos_profile(profile: str) -> None:
    """Select the default QoS profile (``"Library::Profile"``) via
    ``DDS_DEFAULT_QOS_PROFILE``.

    The ``*_QOS_DEFAULT`` resolution reads this at entity-creation time, so
    default-QoS participants/publishers/writers draw from this profile (e.g.
    ``"HelloWorldDataFrag::Reliable"``).
    """
    _check(
        lib.int2dds_env_set_default_qos_profile(profile.encode("utf-8")),
        "int2dds_env_set_default_qos_profile",
    )


__all__ = [
    "set_multicast_ttl",
    "get_multicast_ttl",
    "set_qos_profile",
    "set_default_qos_profile",
]
