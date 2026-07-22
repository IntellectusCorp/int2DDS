"""E2E tests for #333 A(1)' + bonus: factory QoS, lookup_participant,
find_topic, get_current_time, contains_entity.
"""

import os
import sys
from dataclasses import dataclass
from typing import ClassVar

import pytest

sys.path.insert(0, os.path.dirname(__file__))

from int2dds.cdr import CdrReader, CdrWriter, Extensibility
from int2dds.core.participant import DomainParticipant
from int2dds.core.publisher import Publisher
from int2dds.core.subscriber import Subscriber
from int2dds.exceptions import DdsError


@dataclass
class Msg:
    _dds_type_name: ClassVar[str] = "FactoryExtrasMsg"
    _extensibility: ClassVar[Extensibility] = Extensibility.FINAL
    _has_key: ClassVar[bool] = False

    value: int = 0

    def _serialize_cdr(self, xcdr2: bool = False) -> bytes:
        w = CdrWriter(extensibility=self._extensibility)
        w.write_i32(self.value)
        return w.to_bytes()

    @classmethod
    def _deserialize_cdr(cls, data: bytes) -> "Msg":
        return cls(value=CdrReader(data).read_i32())

    def _serialize_key(self) -> bytes:
        return b""


def test_get_current_time():
    with DomainParticipant(domain_id=81) as p:
        sec, nanosec = p.get_current_time()
        # Wall-clock: sec is a real Unix timestamp (well past 2023-11).
        assert sec > 1_700_000_000, sec
        assert 0 <= nanosec < 1_000_000_000, nanosec


def test_contains_entity():
    with DomainParticipant(domain_id=81) as p:
        pub = Publisher(p)
        handle = pub.get_instance_handle()
        assert len(handle) == 16
        assert p.contains_entity(handle) is True
        assert p.contains_entity(b"\x00" * 16) is False


def test_lookup_participant():
    # lookup of an unused domain returns None (Weak upgrade fails).
    assert DomainParticipant.lookup_participant(9123) is None

    p = DomainParticipant(domain_id=82)
    # Create a publisher on the ORIGINAL so the domain has a discoverable entity.
    pub = Publisher(p)
    looked = DomainParticipant.lookup_participant(82)
    assert looked is not None
    # Usable, not just non-null: it aliases the real participant, so read ops
    # work and reflect the same domain. (Note: the looked-up clone cannot create
    # child entities — a core self_ref limitation the FFI mirrors exactly.)
    assert looked.domain_id == 82
    sec, _ = looked.get_current_time()
    assert sec > 1_700_000_000
    # The alias sees the original's publisher via contains_entity.
    assert looked.contains_entity(pub.get_instance_handle()) is True
    # Double-teardown probe: closing both handles must not crash / double-free.
    looked.close()
    p.close()


def test_find_topic():
    with DomainParticipant(domain_id=83) as p:
        created = p.create_topic("FindMe", Msg)
        # Positive case: topic exists, returns immediately, usable.
        found = p.find_topic("FindMe", Msg, timeout_ms=1000)
        assert found.name == "FindMe"
        assert found.type_name == "FactoryExtrasMsg"
        # A found topic must support endpoint creation on the raw path.
        sub = Subscriber(p)
        reader = sub.create_datareader(found)
        assert reader is not None
        # Negative case: short timeout, no hang, raises.
        with pytest.raises(DdsError):
            p.find_topic("DoesNotExist", Msg, timeout_ms=200)
        _ = created


def test_factory_qos_roundtrip():
    original = DomainParticipant.get_factory_qos()
    try:
        DomainParticipant.set_factory_qos(False)
        assert DomainParticipant.get_factory_qos() is False
        DomainParticipant.set_factory_qos(True)
        assert DomainParticipant.get_factory_qos() is True
    finally:
        DomainParticipant.set_factory_qos(original)


def test_default_participant_qos_roundtrip():
    from int2dds.core.qos import ParticipantQos, Property

    prop = Property()
    prop.add("vendor.us.int2.default_pqos", "on")
    DomainParticipant.set_default_participant_qos(ParticipantQos(property=prop))
    try:
        got = DomainParticipant.get_default_participant_qos()
        assert got.property is not None
        values = {name: value for name, value, _ in got.property.entries}
        assert values.get("vendor.us.int2.default_pqos") == "on", values
    finally:
        DomainParticipant.set_default_participant_qos(None)
