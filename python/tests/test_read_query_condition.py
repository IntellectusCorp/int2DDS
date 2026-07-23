"""End-to-end tests for ReadCondition / QueryCondition (issue #333 A(4))."""

from __future__ import annotations

import time
from dataclasses import dataclass
from typing import ClassVar

from int2dds import (
    ANY_INSTANCE_STATE,
    ANY_VIEW_STATE,
    NOT_READ_SAMPLE_STATE,
    DdsTimeout,
    DomainParticipant,
    QueryCondition,
    ReadCondition,
    WaitSet,
)
from int2dds.cdr import CdrReader, CdrWriter, Extensibility
from int2dds.core.conditions import STATUS_DATA_AVAILABLE, STATUS_SUBSCRIPTION_MATCHED
from int2dds.core.qos import DataReaderQos, DataWriterQos, History, Reliability


@dataclass
class Filterable:
    """Type with field descriptors so QueryCondition SQL filtering applies."""

    _dds_type_name: ClassVar[str] = "Filterable"
    _extensibility: ClassVar[Extensibility] = Extensibility.FINAL
    _has_key: ClassVar[bool] = False
    # (name, type-string) drives create_topic_with_field_descriptors.
    _all_fields: ClassVar[list] = [("index", "u32"), ("message", "string")]

    index: int = 0
    message: str = ""

    def _serialize_cdr(self, xcdr2: bool = False) -> bytes:
        w = CdrWriter(extensibility=self._extensibility)
        w.write_u32(self.index)
        w.write_string(self.message)
        return w.to_bytes()

    @classmethod
    def _deserialize_cdr(cls, data: bytes) -> "Filterable":
        r = CdrReader(data)
        return cls(index=r.read_u32(), message=r.read_string())

    def _serialize_key(self) -> bytes:
        return b""


def _reliable_pair(dp, topic_name):
    topic = dp.create_topic(topic_name, Filterable)
    pub = dp.create_publisher()
    sub = dp.create_subscriber()
    keep_all = History("KEEP_ALL")
    writer = pub.create_datawriter(
        topic, qos=DataWriterQos(reliability=Reliability("RELIABLE"), history=keep_all)
    )
    reader = sub.create_datareader(
        topic, qos=DataReaderQos(reliability=Reliability("RELIABLE"), history=keep_all)
    )

    status_cond = reader.get_statuscondition()
    status_cond.set_enabled_statuses(STATUS_SUBSCRIPTION_MATCHED)
    waitset = WaitSet()
    waitset.attach(status_cond)
    deadline = 5.0
    while writer.matched_readers == 0 and deadline > 0:
        try:
            waitset.wait(timeout=1.0)
        except DdsTimeout:
            pass
        deadline -= 1.0
    assert writer.matched_readers > 0, "discovery failed"
    return topic, writer, reader, waitset, status_cond


def _write_and_settle(writer, reader, waitset, status_cond, values):
    # Wake on data arrival, but do NOT read here: a read would flip samples to
    # READ state and defeat NOT_READ filtering in the tests below.
    status_cond.set_enabled_statuses(STATUS_DATA_AVAILABLE)
    for v in values:
        writer.write(Filterable(index=v, message=f"m{v}"))
    try:
        waitset.wait(timeout=5.0)
    except DdsTimeout:
        pass
    time.sleep(0.5)


def test_read_condition_take(domain_id, topic_name):
    with DomainParticipant(domain_id=domain_id) as dp:
        _, writer, reader, waitset, status_cond = _reliable_pair(dp, topic_name)
        _write_and_settle(writer, reader, waitset, status_cond, [10, 200, 30])

        cond = reader.create_read_condition(sample_states=NOT_READ_SAMPLE_STATE)
        assert isinstance(cond, ReadCondition)
        waitset.attach(cond)

        samples = reader.take_w_condition(cond)
        got = sorted(s.data.index for s in samples if s.valid_data)
        assert got == [10, 30, 200]

        # Taken samples are gone -> a second take is empty.
        assert reader.take_w_condition(cond) == []
        waitset.detach(cond)
        cond.close()


def test_query_condition_filters_content(domain_id, topic_name):
    with DomainParticipant(domain_id=domain_id) as dp:
        _, writer, reader, waitset, status_cond = _reliable_pair(dp, topic_name)
        _write_and_settle(writer, reader, waitset, status_cond, [10, 100, 200, 300])

        cond = reader.create_query_condition(
            "index > %0",
            ["100"],
            sample_states=NOT_READ_SAMPLE_STATE,
            view_states=ANY_VIEW_STATE,
            instance_states=ANY_INSTANCE_STATE,
        )
        assert isinstance(cond, QueryCondition)
        waitset.attach(cond)

        samples = reader.take_w_condition(cond)
        got = sorted(s.data.index for s in samples if s.valid_data)
        assert got == [200, 300], f"query 'index > 100' should match 200,300; got {got}"
        cond.close()


def test_query_condition_reparameterize(domain_id, topic_name):
    with DomainParticipant(domain_id=domain_id) as dp:
        _, writer, reader, waitset, status_cond = _reliable_pair(dp, topic_name)
        _write_and_settle(writer, reader, waitset, status_cond, [10, 100, 200])

        cond = reader.create_query_condition("index > %0", ["150"])
        got = sorted(s.data.index for s in reader.read_w_condition(cond) if s.valid_data)
        assert got == [200]

        # Loosen the bound; read (not take) so the cache is unchanged.
        cond.set_query_parameters(["50"])
        got = sorted(s.data.index for s in reader.read_w_condition(cond) if s.valid_data)
        assert got == [100, 200]
        cond.close()
