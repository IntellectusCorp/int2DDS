"""Discovered publication/subscription snapshots.

``take_discovered_publications`` / ``take_discovered_subscriptions`` collect a
point-in-time snapshot of the endpoints a participant has discovered, reading
each item's builtin-topic-data fields into a plain ``dict``. The native item and
sequence handles are freed before returning, so the caller owns only the dicts.
"""

from __future__ import annotations

from typing import TYPE_CHECKING

from int2dds._ffi import ffi, lib
from int2dds.exceptions import check_ret

if TYPE_CHECKING:
    from int2dds.core.participant import DomainParticipant


def _read_string(getter, handle) -> str:
    """Query-size-then-fill for a null-terminated UTF-8 string getter."""
    size_out = ffi.new("uintptr_t *")
    check_ret(getter(handle, ffi.NULL, 0, size_out))
    size = size_out[0]
    if size == 0:
        return ""
    buf = ffi.new("uint8_t[]", size)
    check_ret(getter(handle, buf, size, size_out))
    # size includes the NUL terminator; strip it.
    return bytes(ffi.buffer(buf, size_out[0])).split(b"\x00", 1)[0].decode("utf-8")


def _read_bytes(getter, handle) -> bytes:
    """Query-size-then-fill for a raw byte-buffer getter (e.g. user_data)."""
    size_out = ffi.new("uintptr_t *")
    check_ret(getter(handle, ffi.NULL, 0, size_out))
    size = size_out[0]
    if size == 0:
        return b""
    buf = ffi.new("uint8_t[]", size)
    check_ret(getter(handle, buf, size, size_out))
    return bytes(ffi.buffer(buf, size_out[0]))


def _read_fixed(getter, handle, length: int) -> bytes:
    """Read a fixed-length array getter (key / guid) of ``length`` bytes."""
    buf = ffi.new("uint8_t[%d]" % length)
    check_ret(getter(handle, ffi.cast("uint8_t(*)[%d]" % length, buf)))
    return bytes(ffi.buffer(buf, length))


def _read_kind(getter, handle) -> int:
    kind_out = ffi.new("int32_t *")
    check_ret(getter(handle, kind_out))
    return kind_out[0]


def _read_duration(getter, handle) -> tuple[int, int]:
    sec_out = ffi.new("int32_t *")
    nsec_out = ffi.new("uint32_t *")
    check_ret(getter(handle, sec_out, nsec_out))
    return sec_out[0], nsec_out[0]


def _extract_publication(item) -> dict:
    return {
        "key": _read_fixed(lib.int2dds_publication_builtin_topic_data_get_key, item, 12),
        "endpoint_guid": _read_fixed(
            lib.int2dds_publication_builtin_topic_data_get_endpoint_guid, item, 16),
        "participant_key": _read_fixed(
            lib.int2dds_publication_builtin_topic_data_get_participant_key, item, 12),
        "topic_name": _read_string(
            lib.int2dds_publication_builtin_topic_data_get_topic_name, item),
        "type_name": _read_string(
            lib.int2dds_publication_builtin_topic_data_get_type_name, item),
        "reliability_kind": _read_kind(
            lib.int2dds_publication_builtin_topic_data_get_reliability_kind, item),
        "durability_kind": _read_kind(
            lib.int2dds_publication_builtin_topic_data_get_durability_kind, item),
        "liveliness_kind": _read_kind(
            lib.int2dds_publication_builtin_topic_data_get_liveliness_kind, item),
        "liveliness_lease_duration": _read_duration(
            lib.int2dds_publication_builtin_topic_data_get_liveliness_lease_duration, item),
        "deadline": _read_duration(
            lib.int2dds_publication_builtin_topic_data_get_deadline, item),
        "lifespan": _read_duration(
            lib.int2dds_publication_builtin_topic_data_get_lifespan, item),
        "user_data": _read_bytes(
            lib.int2dds_publication_builtin_topic_data_get_user_data, item),
    }


def _extract_subscription(item) -> dict:
    return {
        "key": _read_fixed(lib.int2dds_subscription_builtin_topic_data_get_key, item, 12),
        "endpoint_guid": _read_fixed(
            lib.int2dds_subscription_builtin_topic_data_get_endpoint_guid, item, 16),
        "participant_key": _read_fixed(
            lib.int2dds_subscription_builtin_topic_data_get_participant_key, item, 12),
        "topic_name": _read_string(
            lib.int2dds_subscription_builtin_topic_data_get_topic_name, item),
        "type_name": _read_string(
            lib.int2dds_subscription_builtin_topic_data_get_type_name, item),
        "reliability_kind": _read_kind(
            lib.int2dds_subscription_builtin_topic_data_get_reliability_kind, item),
        "durability_kind": _read_kind(
            lib.int2dds_subscription_builtin_topic_data_get_durability_kind, item),
        "liveliness_kind": _read_kind(
            lib.int2dds_subscription_builtin_topic_data_get_liveliness_kind, item),
        "liveliness_lease_duration": _read_duration(
            lib.int2dds_subscription_builtin_topic_data_get_liveliness_lease_duration, item),
        "deadline": _read_duration(
            lib.int2dds_subscription_builtin_topic_data_get_deadline, item),
        "user_data": _read_bytes(
            lib.int2dds_subscription_builtin_topic_data_get_user_data, item),
    }


def take_discovered_publications(
    participant: "DomainParticipant", timeout_ms: int = 0
) -> list[dict]:
    """Snapshot the publications this participant has discovered.

    Returns a list of dicts (one per discovered writer). ``timeout_ms`` is how
    long to wait for the builtin reader to settle (0 = no wait).
    """
    seq_out = ffi.new("Int2DdsPublicationBuiltinTopicDataSeq **")
    check_ret(lib.int2dds_take_discovered_publications_snapshot(
        participant._handle, timeout_ms, seq_out))
    seq = seq_out[0]
    try:
        count_out = ffi.new("uintptr_t *")
        check_ret(lib.int2dds_publication_builtin_topic_data_seq_len(seq, count_out))
        result = []
        for i in range(count_out[0]):
            item_out = ffi.new("Int2DdsPublicationBuiltinTopicData **")
            check_ret(lib.int2dds_publication_builtin_topic_data_seq_get(seq, i, item_out))
            item = item_out[0]
            try:
                result.append(_extract_publication(item))
            finally:
                lib.int2dds_publication_builtin_topic_data_destroy(item)
        return result
    finally:
        lib.int2dds_publication_builtin_topic_data_seq_destroy(seq)


def take_discovered_subscriptions(
    participant: "DomainParticipant", timeout_ms: int = 0
) -> list[dict]:
    """Snapshot the subscriptions this participant has discovered.

    Returns a list of dicts (one per discovered reader). ``timeout_ms`` is how
    long to wait for the builtin reader to settle (0 = no wait).
    """
    seq_out = ffi.new("Int2DdsSubscriptionBuiltinTopicDataSeq **")
    check_ret(lib.int2dds_take_discovered_subscriptions_snapshot(
        participant._handle, timeout_ms, seq_out))
    seq = seq_out[0]
    try:
        count_out = ffi.new("uintptr_t *")
        check_ret(lib.int2dds_subscription_builtin_topic_data_seq_len(seq, count_out))
        result = []
        for i in range(count_out[0]):
            item_out = ffi.new("Int2DdsSubscriptionBuiltinTopicData **")
            check_ret(lib.int2dds_subscription_builtin_topic_data_seq_get(seq, i, item_out))
            item = item_out[0]
            try:
                result.append(_extract_subscription(item))
            finally:
                lib.int2dds_subscription_builtin_topic_data_destroy(item)
        return result
    finally:
        lib.int2dds_subscription_builtin_topic_data_seq_destroy(seq)
