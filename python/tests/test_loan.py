"""DataReader loan scenario through the Python binding."""

from __future__ import annotations

import pickle
import time
from dataclasses import dataclass
from typing import ClassVar

import pytest

from int2dds import DdsPreconditionNotMet, DomainParticipant
from int2dds.cdr import CdrReader, CdrWriter, Extensibility
from int2dds.core.qos import DataReaderQos, DataWriterQos, History, Reliability


@dataclass
class Item:
    _dds_type_name: ClassVar[str] = "LoanItem"
    _extensibility: ClassVar[Extensibility] = Extensibility.FINAL
    _has_key: ClassVar[bool] = True
    _dds_type_info_fields: ClassVar[list] = [("field", "index", 9, 0, 1)]  # u32, key

    index: int = 0

    def _serialize_cdr(self, xcdr2: bool = False) -> bytes:
        w = CdrWriter(extensibility=self._extensibility)
        w.write_u32(self.index)
        return w.to_bytes()

    @classmethod
    def _deserialize_cdr(cls, data: bytes) -> "Item":
        return cls(index=CdrReader(data).read_u32())


def _wait_until(what, ready):
    deadline = time.monotonic() + 5.0
    while not ready():
        assert time.monotonic() < deadline, f"timed out waiting for {what}"
        time.sleep(0.01)


def _setup(dp, topic_name, reader_count=1):
    topic = dp.create_topic(topic_name, Item)
    keep_all = History("KEEP_ALL")
    writer = dp.create_publisher().create_datawriter(
        topic, qos=DataWriterQos(reliability=Reliability("RELIABLE"), history=keep_all)
    )
    sub = dp.create_subscriber()
    readers = [
        sub.create_datareader(
            topic, qos=DataReaderQos(reliability=Reliability("RELIABLE"), history=keep_all)
        )
        for _ in range(reader_count)
    ]
    _wait_until("match", lambda: writer.matched_readers == reader_count)
    return writer, readers


def _await_samples(reader, count):
    def cached():
        with reader.read_loaned() as loan:
            return len(loan) == count

    _wait_until("delivery", cached)


def _index(sample) -> int:
    return Item._deserialize_cdr(bytes(sample.serialized_data)).index


def test_loan_scenario(domain_id):
    """Lend, refuse a foreign return, block closing, return, then close."""
    with DomainParticipant(domain_id=domain_id) as dp:
        writer, (reader, other) = _setup(dp, "LoanScenarioPy", reader_count=2)
        writer.write(Item(index=1))
        writer.write(Item(index=2))
        _await_samples(reader, 2)

        cond = reader.create_read_condition()

        # Instance variants lend one instance: by handle, or iterating from None.
        with reader.read_loaned() as everything:
            first_handle, second_handle = (s.instance_handle for s in everything)
        with reader.read_instance_loaned(second_handle) as by_handle:
            assert [_index(s) for s in by_handle] == [2]
        with reader.read_next_instance_loaned(None) as nxt:
            assert [_index(s) for s in nxt] == [1]
        with reader.read_next_instance_w_condition_loaned(first_handle, cond) as nxt:
            assert [_index(s) for s in nxt] == [2]

        loan = reader.take_w_condition_loaned(cond)
        first = loan[0]
        view = first.serialized_data
        assert first.valid_data
        assert view.readonly
        assert [_index(s) for s in loan] == [1, 2]

        with pytest.raises(DdsPreconditionNotMet):
            other.return_loan(loan)
        assert _index(first) == 1
        cond.close()
        with pytest.raises(DdsPreconditionNotMet):
            reader.close()

        # An export still held on a lent view must not stop the return.
        held = pickle.PickleBuffer(loan[1].serialized_data)
        loan.close()
        loan.close()
        del held
        with pytest.raises(ValueError):
            view[0]
        with pytest.raises(ValueError):
            first.serialized_data
        with pytest.raises(ValueError):
            len(loan)
        # An empty loan holds nothing native: the reader closes, then the loan still closes.
        with reader.read_loaned() as empty:
            assert len(empty) == 0
            reader.close()
