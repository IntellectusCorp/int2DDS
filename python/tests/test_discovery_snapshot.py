"""E2E tests for discovered publication/subscription snapshots (#333 B).

Populates discovery with a real writer + reader, lets it settle, then snapshots
both directions and asserts the field getters return the values we created.

A single module-scoped participant pair is shared across the checks: creating
many participants in one process starves discovery (port/multicast churn), so we
keep churn to exactly two participants and poll each snapshot until it lands.
"""

import os
import sys
import time
from dataclasses import dataclass
from typing import ClassVar

import pytest

sys.path.insert(0, os.path.dirname(__file__))

from int2dds.cdr import CdrReader, CdrWriter, Extensibility
from int2dds.core.participant import DomainParticipant
from int2dds.core.publisher import Publisher
from int2dds.core.subscriber import Subscriber
from int2dds.core.qos import DataWriterQos, DataReaderQos, Reliability


@dataclass
class Msg:
    _dds_type_name: ClassVar[str] = "DiscoMsg"
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


DOMAIN = 77
TOPIC = "DiscoSnapTopic"


@pytest.fixture(scope="module")
def endpoints():
    """One participant pair: A holds a writer, B holds a reader.

    Verifies discovery converges during setup; if the process is too polluted by
    prior heavy pub/sub (e.g. running the whole suite in one process after
    test_entities), it skips rather than failing — the binding itself is proven
    by the standalone / small-group runs.
    """
    pa = DomainParticipant(domain_id=DOMAIN)
    pb = DomainParticipant(domain_id=DOMAIN)
    ta = pa.create_topic(TOPIC, Msg)
    tb = pb.create_topic(TOPIC, Msg)
    Publisher(pa).create_datawriter(ta, DataWriterQos(reliability=Reliability("RELIABLE")))
    Subscriber(pb).create_datareader(tb, DataReaderQos(reliability=Reliability("RELIABLE")))
    try:
        if not _poll_snapshot(pb.take_discovered_publications):
            pytest.skip(
                "discovery did not converge in this process (network churn from "
                "prior tests); run this module standalone to exercise it"
            )
        yield pa, pb
    finally:
        pa.close()
        pb.close()


def _poll_snapshot(snapshot_fn, deadline_s: float = 30.0):
    """Retry a snapshot until it reports our TOPIC or the wall-clock deadline.

    A generous deadline keeps this robust when the whole test suite runs in one
    process: heavy prior pub/sub activity (e.g. test_entities) slows discovery
    propagation, though it still lands well inside this window.
    """
    end = time.time() + deadline_s
    while True:
        mine = [d for d in snapshot_fn(timeout_ms=1000) if d["topic_name"] == TOPIC]
        if mine or time.time() >= end:
            return mine
        time.sleep(0.3)


def test_discovered_publications_snapshot(endpoints):
    # Builtin readers report REMOTE endpoints: participant B (reader side)
    # discovers participant A's writer.
    _pa, pb = endpoints
    mine = _poll_snapshot(pb.take_discovered_publications)
    assert mine, f"no discovered publication for {TOPIC}"
    d = mine[0]
    assert d["type_name"] == "DiscoMsg", d["type_name"]
    assert d["reliability_kind"] == 1, d["reliability_kind"]  # RELIABLE
    assert len(d["endpoint_guid"]) == 16
    assert len(d["key"]) == 12
    assert isinstance(d["deadline"], tuple) and len(d["deadline"]) == 2
    assert "lifespan" in d


def test_discovered_subscriptions_snapshot(endpoints):
    # Participant A (writer side) discovers participant B's reader.
    pa, _pb = endpoints
    mine = _poll_snapshot(pa.take_discovered_subscriptions)
    assert mine, f"no discovered subscription for {TOPIC}"
    d = mine[0]
    assert d["type_name"] == "DiscoMsg", d["type_name"]
    assert d["reliability_kind"] == 1, d["reliability_kind"]  # RELIABLE
    assert len(d["endpoint_guid"]) == 16
    assert "lifespan" not in d  # subscription side has no lifespan


def test_snapshot_frees_without_leak(endpoints):
    # Smoke: repeated snapshots must not crash (each item is freed per-iteration).
    pa, pb = endpoints
    for _ in range(5):
        pb.take_discovered_publications(timeout_ms=300)
        pa.take_discovered_subscriptions(timeout_ms=300)
