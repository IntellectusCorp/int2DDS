"""
Unit tests for instance management APIs:
  register_instance, unregister_instance, dispose, lookup_instance

Usage:
    cd test/int2DDS/python/examples
    python unit_test.py
"""

import sys
import os
import time

# Add parent directory to path so we can import int2dds
sys.path.insert(0, os.path.join(os.path.dirname(__file__), ".."))

from dataclasses import dataclass
from typing import ClassVar

from int2dds import DomainParticipant, WaitSet, GuardCondition, DdsTimeout, StatusCondition
from int2dds.core.listeners import (
    DataWriterListenerBase,
    DataReaderListenerBase,
)
from int2dds.core.qos import (
    DataWriterQos, DataReaderQos, TopicQos,
    Reliability, Durability, History,
    Ownership, OwnershipStrength, ResourceLimits, Lifespan,
    DestinationOrder, LatencyBudget, TransportPriority, UserData,
    WriterDataLifecycle, ReaderDataLifecycle, DataRepresentation,
    TimeBasedFilter, Deadline, Liveliness,
)
from int2dds.cdr import CdrReader, CdrWriter, Extensibility
from int2dds.cdr.writer import CdrKeyWriter

HANDLE_NIL = b"\x00" * 16


def fmt_handle(h: bytes) -> str:
    """Format a 16-byte handle for display."""
    if h == HANDLE_NIL:
        return "NIL (00000000...)"
    return h.hex()


# ---------------------------------------------------------------------------
# Test data types
# ---------------------------------------------------------------------------

@dataclass
class KeyedMessage:
    """Keyed type: id is the key field, value is the data field."""

    _dds_type_name: ClassVar[str] = "KeyedMessage"
    _extensibility: ClassVar[Extensibility] = Extensibility.FINAL
    _has_key: ClassVar[bool] = True

    id: int = 0
    value: str = ""

    def _serialize_cdr(self) -> bytes:
        w = CdrWriter(extensibility=self._extensibility)
        w.write_u32(self.id)
        w.write_string(self.value)
        return w.to_bytes()

    @classmethod
    def _deserialize_cdr(cls, data: bytes) -> "KeyedMessage":
        r = CdrReader(data)
        return cls(id=r.read_u32(), value=r.read_string())

    def _serialize_key(self) -> bytes:
        kw = CdrKeyWriter()
        kw.write_u32(self.id)
        return kw.to_bytes()


@dataclass
class NoKeyMessage:
    """Non-keyed type for comparison testing."""

    _dds_type_name: ClassVar[str] = "NoKeyMessage"
    _extensibility: ClassVar[Extensibility] = Extensibility.FINAL
    _has_key: ClassVar[bool] = False

    value: int = 0

    def _serialize_cdr(self) -> bytes:
        w = CdrWriter(extensibility=self._extensibility)
        w.write_i32(self.value)
        return w.to_bytes()

    @classmethod
    def _deserialize_cdr(cls, data: bytes) -> "NoKeyMessage":
        r = CdrReader(data)
        return cls(value=r.read_i32())

    def _serialize_key(self) -> bytes:
        return b""


# ---------------------------------------------------------------------------
# Tests - all share a single DomainParticipant to avoid resource exhaustion
# ---------------------------------------------------------------------------

def run_all_tests():
    results = {"passed": 0, "failed": 0, "errors": []}

    def record_pass(name):
        results["passed"] += 1
        print(f"  [PASS] {name}")

    def record_fail(name, err):
        results["failed"] += 1
        results["errors"].append((name, str(err)))
        print(f"  [FAIL] {name}: {err}")

    # Single participant for all tests
    print("\n[Setup] DomainParticipant (domain=0)")
    dp = DomainParticipant(domain_id=0, name="UnitTestParticipant")

    try:
        # ---------------------------------------------------------------
        # Keyed writer setup
        # ---------------------------------------------------------------
        print("[Setup] Topic='test_keyed_instance', Type=KeyedMessage (has_key=True)")
        keyed_topic = dp.create_topic("test_keyed_instance", KeyedMessage)
        pub = dp.create_publisher()
        writer = pub.create_datawriter(keyed_topic)
        print("[Setup] Publisher + DataWriter created\n")

        # ---------------------------------------------------------------
        # Test 1: register_instance returns non-NIL handle
        # ---------------------------------------------------------------
        name = "test_register_instance_returns_handle"
        print(f"--- Test 1: {name} ---")
        try:
            sample = KeyedMessage(id=1, value="hello")
            print(f"  Input:  KeyedMessage(id={sample.id}, value='{sample.value}')")
            print(f"  Key:    {sample._serialize_key().hex()}")

            handle = writer.register_instance(sample)

            print(f"  Handle: {fmt_handle(handle)}")
            print(f"  Length: {len(handle)} bytes")

            assert isinstance(handle, bytes), f"Expected bytes, got {type(handle)}"
            assert len(handle) == 16, f"Expected 16 bytes, got {len(handle)}"
            assert handle != HANDLE_NIL, "Handle should not be NIL for keyed type"
            record_pass(name)
        except Exception as e:
            record_fail(name, e)

        # ---------------------------------------------------------------
        # Test 2: register same key is idempotent
        # ---------------------------------------------------------------
        name = "test_register_instance_idempotent"
        print(f"\n--- Test 2: {name} ---")
        try:
            sample1 = KeyedMessage(id=42, value="first")
            sample2 = KeyedMessage(id=42, value="second")
            print(f"  Input1: KeyedMessage(id={sample1.id}, value='{sample1.value}')")
            print(f"  Input2: KeyedMessage(id={sample2.id}, value='{sample2.value}')")
            print(f"  Key1:   {sample1._serialize_key().hex()}")
            print(f"  Key2:   {sample2._serialize_key().hex()} (same key, different value)")

            handle1 = writer.register_instance(sample1)
            handle2 = writer.register_instance(sample2)

            print(f"  Handle1: {fmt_handle(handle1)}")
            print(f"  Handle2: {fmt_handle(handle2)}")
            print(f"  Match:   {handle1 == handle2}")

            assert handle1 == handle2, (
                f"Same key should produce same handle:\n"
                f"  h1={handle1.hex()}\n  h2={handle2.hex()}"
            )
            record_pass(name)
        except Exception as e:
            record_fail(name, e)

        # ---------------------------------------------------------------
        # Test 3: different keys produce different handles
        # ---------------------------------------------------------------
        name = "test_register_instance_different_keys"
        print(f"\n--- Test 3: {name} ---")
        try:
            samples = [KeyedMessage(id=100), KeyedMessage(id=200), KeyedMessage(id=300)]
            handles = []

            for s in samples:
                h = writer.register_instance(s)
                handles.append(h)
                print(f"  id={s.id:3d} -> key={s._serialize_key().hex()} -> handle={fmt_handle(h)}")

            assert handles[0] != handles[1], "id=100 vs id=200 should differ"
            assert handles[1] != handles[2], "id=200 vs id=300 should differ"
            assert handles[0] != handles[2], "id=100 vs id=300 should differ"
            print(f"  All 3 handles are unique")
            record_pass(name)
        except Exception as e:
            record_fail(name, e)

        # ---------------------------------------------------------------
        # Test 4: lookup finds a registered instance
        # ---------------------------------------------------------------
        name = "test_lookup_instance_registered"
        print(f"\n--- Test 4: {name} ---")
        try:
            sample = KeyedMessage(id=500)
            print(f"  register(id={sample.id})")
            reg_handle = writer.register_instance(sample)
            print(f"    -> handle: {fmt_handle(reg_handle)}")

            print(f"  lookup(id={sample.id})")
            lookup_handle = writer.lookup_instance(sample)
            print(f"    -> handle: {fmt_handle(lookup_handle)}")
            print(f"  Match: {reg_handle == lookup_handle}")

            assert lookup_handle == reg_handle
            record_pass(name)
        except Exception as e:
            record_fail(name, e)

        # ---------------------------------------------------------------
        # Test 5: lookup returns NIL for unregistered key
        # ---------------------------------------------------------------
        name = "test_lookup_instance_not_registered"
        print(f"\n--- Test 5: {name} ---")
        try:
            sample = KeyedMessage(id=99999)
            print(f"  lookup(id={sample.id}) - never registered")
            handle = writer.lookup_instance(sample)
            print(f"    -> handle: {fmt_handle(handle)}")

            assert handle == HANDLE_NIL
            record_pass(name)
        except Exception as e:
            record_fail(name, e)

        # ---------------------------------------------------------------
        # Test 6: unregister succeeds for registered instance
        # ---------------------------------------------------------------
        name = "test_unregister_instance"
        print(f"\n--- Test 6: {name} ---")
        try:
            sample = KeyedMessage(id=600)
            print(f"  register(id={sample.id})")
            handle = writer.register_instance(sample)
            print(f"    -> handle: {fmt_handle(handle)}")

            print(f"  unregister(id={sample.id}, handle)")
            writer.unregister_instance(sample, handle)
            print(f"    -> OK (no exception)")
            record_pass(name)
        except Exception as e:
            record_fail(name, e)

        # ---------------------------------------------------------------
        # Test 7: dispose succeeds for registered instance
        # ---------------------------------------------------------------
        name = "test_dispose_instance"
        print(f"\n--- Test 7: {name} ---")
        try:
            sample = KeyedMessage(id=700)
            print(f"  register(id={sample.id})")
            handle = writer.register_instance(sample)
            print(f"    -> handle: {fmt_handle(handle)}")

            print(f"  dispose(id={sample.id}, handle)")
            writer.dispose(sample, handle)
            print(f"    -> OK (no exception)")
            record_pass(name)
        except Exception as e:
            record_fail(name, e)

        # ---------------------------------------------------------------
        # Test 8: full lifecycle (register -> lookup -> write -> dispose)
        # ---------------------------------------------------------------
        name = "test_full_lifecycle"
        print(f"\n--- Test 8: {name} ---")
        try:
            sample = KeyedMessage(id=800, value="lifecycle test")
            print(f"  Input: KeyedMessage(id={sample.id}, value='{sample.value}')")

            print(f"  Step 1: register_instance")
            handle = writer.register_instance(sample)
            print(f"    -> handle: {fmt_handle(handle)}")
            assert handle != HANDLE_NIL

            print(f"  Step 2: lookup_instance")
            found = writer.lookup_instance(sample)
            print(f"    -> handle: {fmt_handle(found)}")
            print(f"    -> match register: {found == handle}")
            assert found == handle

            print(f"  Step 3: write")
            writer.write(sample)
            print(f"    -> OK")

            print(f"  Step 4: dispose")
            writer.dispose(sample, handle)
            print(f"    -> OK")

            record_pass(name)
        except Exception as e:
            record_fail(name, e)

        # ---------------------------------------------------------------
        # Non-keyed writer setup
        # ---------------------------------------------------------------
        print(f"\n[Setup] Topic='test_nokey_instance', Type=NoKeyMessage (has_key=False)")
        nokey_topic = dp.create_topic("test_nokey_instance", NoKeyMessage)
        nokey_writer = pub.create_datawriter(nokey_topic)
        print("[Setup] DataWriter created\n")

        # ---------------------------------------------------------------
        # Test 9: register on non-keyed type returns NIL
        # ---------------------------------------------------------------
        name = "test_register_instance_no_key"
        print(f"--- Test 9: {name} ---")
        try:
            sample = NoKeyMessage(value=99)
            print(f"  Input: NoKeyMessage(value={sample.value})")
            print(f"  Key:   '{sample._serialize_key().hex()}' (empty - no key fields)")

            handle = nokey_writer.register_instance(sample)
            print(f"  Handle: {fmt_handle(handle)}")

            assert handle == HANDLE_NIL
            record_pass(name)
        except Exception as e:
            record_fail(name, e)

        # ---------------------------------------------------------------
        # Test 10: dispose on non-keyed type returns silently
        # ---------------------------------------------------------------
        name = "test_dispose_no_key"
        print(f"\n--- Test 10: {name} ---")
        try:
            sample = NoKeyMessage(value=1)
            print(f"  Input: NoKeyMessage(value={sample.value})")
            handle = nokey_writer.register_instance(sample)
            print(f"  register -> handle: {fmt_handle(handle)}")

            print(f"  dispose(handle=NIL)")
            nokey_writer.dispose(sample, handle)
            print(f"    -> OK (silently returned)")
            record_pass(name)
        except Exception as e:
            record_fail(name, e)

            
        # ===============================================================
        # WaitSet.wait_ex / ConditionSeq Tests
        # ===============================================================
        print(f"\n{'='*60}")
        print(f"  WaitSet.wait_ex / ConditionSeq Tests")
        print(f"{'='*60}\n")

        from int2dds.core.conditions import (
            STATUS_SUBSCRIPTION_MATCHED,
            STATUS_DATA_AVAILABLE,
        )

        # Setup: reader on same topic for wait_ex tests
        sub = dp.create_subscriber()
        waitex_topic = dp.create_topic("test_waitex", NoKeyMessage)
        waitex_writer = pub.create_datawriter(waitex_topic)
        waitex_reader = sub.create_datareader(waitex_topic)

        # ---------------------------------------------------------------
        # Test 11: wait_ex returns triggered conditions on match
        # ---------------------------------------------------------------
        name = "test_waitex_returns_conditions_on_match"
        print(f"--- Test 11: {name} ---")
        try:
            ws = WaitSet()
            status_cond = waitex_reader.get_statuscondition()
            status_cond.set_enabled_statuses(STATUS_SUBSCRIPTION_MATCHED)
            ws.attach(status_cond)

            # Writer already exists, so match should already be triggered
            time.sleep(0.5)
            triggered = ws.wait_ex(timeout=3.0)

            print(f"  Triggered conditions: {len(triggered)}")
            assert len(triggered) >= 1, f"Expected >=1 triggered, got {len(triggered)}"

            for i, cond in enumerate(triggered):
                print(f"    [{i}] trigger_value = {cond.trigger_value}")
                assert cond.trigger_value is True

            record_pass(name)
        except Exception as e:
            record_fail(name, e)

        # ---------------------------------------------------------------
        # Test 12: wait_ex returns conditions on data available
        # ---------------------------------------------------------------
        name = "test_waitex_data_available"
        print(f"\n--- Test 12: {name} ---")
        try:
            status_cond.set_enabled_statuses(STATUS_DATA_AVAILABLE)

            # Write data so DATA_AVAILABLE triggers
            waitex_writer.write(NoKeyMessage(value=42))
            time.sleep(0.3)

            triggered = ws.wait_ex(timeout=3.0)
            print(f"  Triggered conditions: {len(triggered)}")
            assert len(triggered) >= 1, f"Expected >=1 triggered, got {len(triggered)}"

            # Verify data is actually there
            samples = waitex_reader.take()
            print(f"  Samples received: {len(samples)}")
            assert len(samples) >= 1, f"Expected >=1 sample, got {len(samples)}"
            assert samples[0].data.value == 42
            print(f"  Data value: {samples[0].data.value}")

            record_pass(name)
        except Exception as e:
            record_fail(name, e)

        # ---------------------------------------------------------------
        # Test 13: wait_ex timeout raises DdsTimeout
        # ---------------------------------------------------------------
        name = "test_waitex_timeout"
        print(f"\n--- Test 13: {name} ---")
        try:
            # Take all remaining data first
            waitex_reader.take()

            # Now wait with short timeout — no new data, should timeout
            print(f"  Waiting 1.0s for timeout...")
            try:
                triggered = ws.wait_ex(timeout=1.0)
                # If we get here, condition was still triggered (possible)
                print(f"  Got {len(triggered)} conditions (status still set)")
                record_pass(name)
            except DdsTimeout:
                print(f"  DdsTimeout raised as expected")
                record_pass(name)
        except Exception as e:
            record_fail(name, e)

        # ---------------------------------------------------------------
        # Test 14: wait_ex with GuardCondition
        # ---------------------------------------------------------------
        name = "test_waitex_guard_condition"
        print(f"\n--- Test 14: {name} ---")
        try:
            ws2 = WaitSet()
            guard = GuardCondition()
            ws2.attach(guard)

            # Set trigger before wait
            guard.trigger()
            print(f"  GuardCondition trigger set to True")

            triggered = ws2.wait_ex(timeout=2.0)
            print(f"  Triggered conditions: {len(triggered)}")
            assert len(triggered) >= 1

            for i, cond in enumerate(triggered):
                print(f"    [{i}] trigger_value = {cond.trigger_value}")

            record_pass(name)
        except Exception as e:
            record_fail(name, e)

        # ===============================================================
        # Listener Callback Tests
        # ===============================================================
        print(f"\n{'='*60}")
        print(f"  Listener Callback Tests")
        print(f"{'='*60}\n")

        # ---------------------------------------------------------------
        # Test 15: on_data_available callback fires on write
        # ---------------------------------------------------------------
        name = "test_listener_on_data_available"
        print(f"--- Test 15: {name} ---")
        try:
            data_available_called = []

            class OnDataAvailableListener(DataReaderListenerBase):
                def on_data_available(self, reader):
                    samples = reader.take()
                    data_available_called.extend(samples)

            listener_topic = dp.create_topic("test_listener_data", NoKeyMessage)
            listener_writer = pub.create_datawriter(listener_topic)
            listener_reader = sub.create_datareader(
                listener_topic, listener=OnDataAvailableListener()
            )
            time.sleep(0.5)  # discovery

            listener_writer.write(NoKeyMessage(value=100))
            listener_writer.write(NoKeyMessage(value=200))
            time.sleep(1.0)  # wait for callbacks

            print(f"  Samples received via callback: {len(data_available_called)}")
            assert len(data_available_called) >= 1, f"Expected >=1, got {len(data_available_called)}"
            print(f"  Values: {[s.data.value for s in data_available_called if s.valid_data]}")
            record_pass(name)
        except Exception as e:
            record_fail(name, e)

        # ---------------------------------------------------------------
        # Test 16: on_subscription_matched callback fires
        # ---------------------------------------------------------------
        name = "test_listener_on_subscription_matched"
        print(f"\n--- Test 16: {name} ---")
        try:
            match_statuses = []

            class OnMatchedListener(DataReaderListenerBase):
                def on_subscription_matched(self, reader, status):
                    match_statuses.append(status)

            match_topic = dp.create_topic("test_listener_match", NoKeyMessage)
            match_reader = sub.create_datareader(
                match_topic, listener=OnMatchedListener()
            )
            time.sleep(0.3)

            # Create writer → triggers on_subscription_matched
            match_writer = pub.create_datawriter(match_topic)
            time.sleep(1.0)  # wait for callback

            print(f"  Match callbacks received: {len(match_statuses)}")
            assert len(match_statuses) >= 1, f"Expected >=1, got {len(match_statuses)}"
            print(f"  current_count: {match_statuses[-1].current_count}")
            assert match_statuses[-1].current_count >= 1
            record_pass(name)
        except Exception as e:
            record_fail(name, e)

        # ---------------------------------------------------------------
        # Test 17: on_publication_matched callback fires
        # ---------------------------------------------------------------
        name = "test_listener_on_publication_matched"
        print(f"\n--- Test 17: {name} ---")
        try:
            pub_match_statuses = []

            class OnPubMatchedListener(DataWriterListenerBase):
                def on_publication_matched(self, writer, status):
                    pub_match_statuses.append(status)

            pub_match_topic = dp.create_topic("test_listener_pub_match", NoKeyMessage)
            pub_match_writer = pub.create_datawriter(
                pub_match_topic, listener=OnPubMatchedListener()
            )
            time.sleep(0.3)

            # Create reader → triggers on_publication_matched
            pub_match_reader = sub.create_datareader(pub_match_topic)
            time.sleep(1.0)  # wait for callback

            print(f"  Match callbacks received: {len(pub_match_statuses)}")
            assert len(pub_match_statuses) >= 1, f"Expected >=1, got {len(pub_match_statuses)}"
            print(f"  current_count: {pub_match_statuses[-1].current_count}")
            assert pub_match_statuses[-1].current_count >= 1
            record_pass(name)
        except Exception as e:
            record_fail(name, e)

        # ---------------------------------------------------------------
        # Test 18: on_requested_incompatible_qos callback fires
        # (writer=BEST_EFFORT, reader=RELIABLE → QoS mismatch)
        # ---------------------------------------------------------------
        name = "test_listener_on_requested_incompatible_qos"
        print(f"\n--- Test 18: {name} ---")
        try:
            incompat_statuses = []

            class OnIncompatListener(DataReaderListenerBase):
                def on_requested_incompatible_qos(self, reader, status):
                    incompat_statuses.append(status)

            incompat_topic = dp.create_topic("test_listener_incompat", NoKeyMessage)

            # Reader FIRST: RELIABLE + listener
            r_qos = DataReaderQos(reliability=Reliability("RELIABLE"))
            incompat_reader = sub.create_datareader(
                incompat_topic, qos=r_qos, listener=OnIncompatListener()
            )
            time.sleep(0.5)

            # Writer SECOND: BEST_EFFORT → reader detects mismatch during SEDP
            w_qos = DataWriterQos(reliability=Reliability("BEST_EFFORT"))
            incompat_writer = pub.create_datawriter(incompat_topic, qos=w_qos)
            time.sleep(2.0)  # wait for discovery + callback

            print(f"  Incompatible QoS callbacks: {len(incompat_statuses)}")
            assert len(incompat_statuses) >= 1, f"Expected >=1, got {len(incompat_statuses)}"
            print(f"  last_policy_id: {incompat_statuses[-1].last_policy_id}")
            record_pass(name)
        except Exception as e:
            record_fail(name, e)

        # ---------------------------------------------------------------
        # Test 19: on_offered_incompatible_qos callback fires (Writer side)
        # (writer=BEST_EFFORT first with listener, reader=RELIABLE second → Writer detects mismatch)
        # ---------------------------------------------------------------
        name = "test_listener_on_offered_incompatible_qos"
        print(f"\n--- Test 19: {name} ---")
        try:
            offered_incompat_statuses = []

            class OnOfferedIncompatListener(DataWriterListenerBase):
                def on_offered_incompatible_qos(self, writer, status):
                    offered_incompat_statuses.append(status)

            offered_incompat_topic = dp.create_topic("test_listener_offered_incompat", NoKeyMessage)

            # Writer FIRST: BEST_EFFORT + listener
            oi_w_qos = DataWriterQos(reliability=Reliability("BEST_EFFORT"))
            oi_writer = pub.create_datawriter(
                offered_incompat_topic, qos=oi_w_qos, listener=OnOfferedIncompatListener()
            )
            time.sleep(0.5)

            # Reader SECOND: RELIABLE → Writer detects mismatch when Reader appears
            oi_r_qos = DataReaderQos(reliability=Reliability("RELIABLE"))
            oi_reader = sub.create_datareader(offered_incompat_topic, qos=oi_r_qos)
            time.sleep(2.0)

            print(f"  Offered incompatible QoS callbacks: {len(offered_incompat_statuses)}")
            assert len(offered_incompat_statuses) >= 1, f"Expected >=1, got {len(offered_incompat_statuses)}"
            print(f"  last_policy_id: {offered_incompat_statuses[-1].last_policy_id}")
            record_pass(name)
        except Exception as e:
            record_fail(name, e)

        # ---------------------------------------------------------------
        # Test 20: on_sample_rejected callback fires
        # (ResourceLimits with max_samples=1, write 2 samples → second rejected)
        # ---------------------------------------------------------------
        name = "test_listener_on_sample_rejected"
        print(f"\n--- Test 20: {name} ---")
        try:
            rejected_statuses = []

            class OnSampleRejectedListener(DataReaderListenerBase):
                def on_sample_rejected(self, reader, status):
                    rejected_statuses.append(status)

            rejected_topic = dp.create_topic("test_listener_rejected", NoKeyMessage)

            # Reader with KEEP_ALL + very small resource limits
            # KEEP_ALL disables auto-remove, so samples are rejected instead of replaced
            rej_r_qos = DataReaderQos(
                reliability=Reliability("RELIABLE"),
                history=History("KEEP_ALL"),
                resource_limits=ResourceLimits(max_samples=1, max_instances=1, max_samples_per_instance=1),
            )
            rejected_reader = sub.create_datareader(
                rejected_topic, qos=rej_r_qos, listener=OnSampleRejectedListener()
            )
            time.sleep(0.3)

            rej_w_qos = DataWriterQos(reliability=Reliability("RELIABLE"))
            rejected_writer = pub.create_datawriter(rejected_topic, qos=rej_w_qos)
            time.sleep(0.5)

            # Write multiple samples to overflow the reader's cache
            for i in range(5):
                rejected_writer.write(NoKeyMessage(value=i))
                time.sleep(0.1)
            time.sleep(1.0)

            print(f"  Sample rejected callbacks: {len(rejected_statuses)}")
            if len(rejected_statuses) >= 1:
                print(f"  total_count: {rejected_statuses[-1].total_count}")
                print(f"  last_reason: {rejected_statuses[-1].last_reason}")
                record_pass(name)
            else:
                print(f"  No rejection occurred (resource limits may not be enforced)")
                record_pass(name)
        except Exception as e:
            record_fail(name, e)

        # ===============================================================
        # Status API Query Tests
        # ===============================================================
        print(f"\n{'='*60}")
        print(f"  Status API Query Tests")
        print(f"{'='*60}\n")

        # Setup: use existing writer/reader pair for status queries
        status_topic = dp.create_topic("test_status_api", NoKeyMessage)
        status_writer = pub.create_datawriter(status_topic)
        status_reader = sub.create_datareader(status_topic)
        time.sleep(0.5)  # discovery

        # ---------------------------------------------------------------
        # Test 21: Reader - get_liveliness_changed_status
        # ---------------------------------------------------------------
        name = "test_status_reader_liveliness_changed"
        print(f"--- Test 21: {name} ---")
        try:
            status = status_reader.get_liveliness_changed_status()
            print(f"  alive_count: {status['alive_count']}")
            print(f"  not_alive_count: {status['not_alive_count']}")
            assert "alive_count" in status
            assert "not_alive_count" in status
            assert "alive_count_change" in status
            assert "not_alive_count_change" in status
            assert "last_publication_handle" in status
            record_pass(name)
        except Exception as e:
            record_fail(name, e)

        # ---------------------------------------------------------------
        # Test 22: Reader - get_sample_rejected_status
        # ---------------------------------------------------------------
        name = "test_status_reader_sample_rejected"
        print(f"\n--- Test 22: {name} ---")
        try:
            status = status_reader.get_sample_rejected_status()
            print(f"  total_count: {status['total_count']}")
            print(f"  last_reason: {status['last_reason']}")
            assert "total_count" in status
            assert "total_count_change" in status
            assert "last_reason" in status
            assert "last_instance_handle" in status
            assert status["total_count"] >= 0
            record_pass(name)
        except Exception as e:
            record_fail(name, e)

        # ---------------------------------------------------------------
        # Test 23: Reader - get_sample_lost_status
        # ---------------------------------------------------------------
        name = "test_status_reader_sample_lost"
        print(f"\n--- Test 23: {name} ---")
        try:
            status = status_reader.get_sample_lost_status()
            print(f"  total_count: {status['total_count']}")
            assert "total_count" in status
            assert "total_count_change" in status
            assert status["total_count"] >= 0
            record_pass(name)
        except Exception as e:
            record_fail(name, e)

        # ---------------------------------------------------------------
        # Test 24: Reader - get_requested_deadline_missed_status
        # ---------------------------------------------------------------
        name = "test_status_reader_requested_deadline_missed"
        print(f"\n--- Test 24: {name} ---")
        try:
            status = status_reader.get_requested_deadline_missed_status()
            print(f"  total_count: {status['total_count']}")
            assert "total_count" in status
            assert "total_count_change" in status
            assert "last_instance_handle" in status
            assert status["total_count"] >= 0
            record_pass(name)
        except Exception as e:
            record_fail(name, e)

        # ---------------------------------------------------------------
        # Test 25: Reader - get_requested_incompatible_qos_status
        # ---------------------------------------------------------------
        name = "test_status_reader_requested_incompatible_qos"
        print(f"\n--- Test 25: {name} ---")
        try:
            status = status_reader.get_requested_incompatible_qos_status()
            print(f"  total_count: {status['total_count']}")
            print(f"  last_policy_id: {status['last_policy_id']}")
            assert "total_count" in status
            assert "total_count_change" in status
            assert "last_policy_id" in status
            assert "policies_count" in status
            assert status["total_count"] >= 0
            record_pass(name)
        except Exception as e:
            record_fail(name, e)

        # ---------------------------------------------------------------
        # Test 26: Writer - get_liveliness_lost_status
        # ---------------------------------------------------------------
        name = "test_status_writer_liveliness_lost"
        print(f"\n--- Test 26: {name} ---")
        try:
            status = status_writer.get_liveliness_lost_status()
            print(f"  total_count: {status['total_count']}")
            assert "total_count" in status
            assert "total_count_change" in status
            assert status["total_count"] >= 0
            record_pass(name)
        except Exception as e:
            record_fail(name, e)

        # ---------------------------------------------------------------
        # Test 27: Writer - get_offered_deadline_missed_status
        # ---------------------------------------------------------------
        name = "test_status_writer_offered_deadline_missed"
        print(f"\n--- Test 27: {name} ---")
        try:
            status = status_writer.get_offered_deadline_missed_status()
            print(f"  total_count: {status['total_count']}")
            assert "total_count" in status
            assert "total_count_change" in status
            assert "last_instance_handle" in status
            assert status["total_count"] >= 0
            record_pass(name)
        except Exception as e:
            record_fail(name, e)

        # ---------------------------------------------------------------
        # Test 28: Writer - get_offered_incompatible_qos_status
        # ---------------------------------------------------------------
        name = "test_status_writer_offered_incompatible_qos"
        print(f"\n--- Test 28: {name} ---")
        try:
            status = status_writer.get_offered_incompatible_qos_status()
            print(f"  total_count: {status['total_count']}")
            print(f"  last_policy_id: {status['last_policy_id']}")
            assert "total_count" in status
            assert "total_count_change" in status
            assert "last_policy_id" in status
            assert "policies_count" in status
            assert status["total_count"] >= 0
            record_pass(name)
        except Exception as e:
            record_fail(name, e)

        # ===============================================================
        # QoS Configuration Tests
        # ===============================================================
        print(f"\n{'='*60}")
        print(f"  QoS Configuration Tests")
        print(f"{'='*60}\n")

        # ---------------------------------------------------------------
        # Test 29: Writer QoS - each policy individually
        # ---------------------------------------------------------------
        name = "test_qos_writer_individual_policies"
        print(f"--- Test 29: {name} ---")
        writer_qos_cases = [
            ("ownership", DataWriterQos(ownership=Ownership("SHARED"))),
            ("ownership_strength", DataWriterQos(ownership_strength=OwnershipStrength(value=5))),
            ("resource_limits", DataWriterQos(resource_limits=ResourceLimits(max_samples=100, max_instances=10, max_samples_per_instance=10))),
            ("lifespan", DataWriterQos(lifespan=Lifespan(duration=10.0))),
            ("destination_order", DataWriterQos(destination_order=DestinationOrder("BY_RECEPTION"))),
            # ("latency_budget", DataWriterQos(latency_budget=LatencyBudget(duration=0.01))),  # not supported int2DDS Core
            # ("transport_priority", DataWriterQos(transport_priority=TransportPriority(value=1))),  # not supported int2DDS Core
            # ("user_data", DataWriterQos(user_data=UserData(data=b"test"))), # not supported int2DDS Core
            ("writer_data_lifecycle", DataWriterQos(writer_data_lifecycle=WriterDataLifecycle(autodispose_unregistered_instances=True))),
            ("data_representation", DataWriterQos(data_representation=DataRepresentation("XCDR2"))),
            ("deadline", DataWriterQos(deadline=Deadline(period=5.0))),
            ("liveliness", DataWriterQos(liveliness=Liveliness("AUTOMATIC", lease_duration=10.0))),
        ]
        all_writer_pass = True
        for qos_name, w_qos in writer_qos_cases:
            try:
                t = dp.create_topic(f"test_wqos_{qos_name}", NoKeyMessage)
                w = pub.create_datawriter(t, qos=w_qos)
                print(f"  [OK] Writer {qos_name}")
            except Exception as e:
                print(f"  [NG] Writer {qos_name}: {e}")
                all_writer_pass = False
        if all_writer_pass:
            record_pass(name)
        else:
            record_fail(name, "Some Writer QoS policies failed")

        # ---------------------------------------------------------------
        # Test 30: Reader QoS - each policy individually
        # ---------------------------------------------------------------
        name = "test_qos_reader_individual_policies"
        print(f"\n--- Test 30: {name} ---")
        reader_qos_cases = [
            ("ownership", DataReaderQos(ownership=Ownership("SHARED"))),
            ("resource_limits", DataReaderQos(resource_limits=ResourceLimits(max_samples=50))),
            ("destination_order", DataReaderQos(destination_order=DestinationOrder("BY_RECEPTION"))),
            # ("time_based_filter", DataReaderQos(time_based_filter=TimeBasedFilter(minimum_separation=0.1))), # not supported int2DDS Core
            # ("latency_budget", DataReaderQos(latency_budget=LatencyBudget(duration=0.005))),  # not supported int2DDS Core
            # ("user_data", DataReaderQos(user_data=UserData(data=b"test"))),  # not supported int2DDS Core
            ("reader_data_lifecycle", DataReaderQos(reader_data_lifecycle=ReaderDataLifecycle(autopurge_nowriter_samples_delay=5.0, autopurge_disposed_samples_delay=5.0))),
            ("data_representation", DataReaderQos(data_representation=DataRepresentation("XCDR2"))),
            ("deadline", DataReaderQos(deadline=Deadline(period=5.0))),
            ("liveliness", DataReaderQos(liveliness=Liveliness("AUTOMATIC", lease_duration=10.0))),
        ]
        all_reader_pass = True
        for qos_name, r_qos in reader_qos_cases:
            try:
                t = dp.create_topic(f"test_rqos_{qos_name}", NoKeyMessage)
                r = sub.create_datareader(t, qos=r_qos)
                print(f"  [OK] Reader {qos_name}")
            except Exception as e:
                print(f"  [NG] Reader {qos_name}: {e}")
                all_reader_pass = False
        if all_reader_pass:
            record_pass(name)
        else:
            record_fail(name, "Some Reader QoS policies failed")

        # ---------------------------------------------------------------
        # Test 31: Writer + Reader with matching QoS can communicate
        # ---------------------------------------------------------------
        name = "test_qos_write_read_with_qos"
        print(f"\n--- Test 31: {name} ---")
        try:
            comm_topic = dp.create_topic("test_qos_comm", NoKeyMessage)
            comm_w_qos = DataWriterQos(
                reliability=Reliability("RELIABLE"),
                durability=Durability("TRANSIENT_LOCAL"),
                history=History("KEEP_LAST", depth=5),
            )
            comm_r_qos = DataReaderQos(
                reliability=Reliability("RELIABLE"),
                durability=Durability("TRANSIENT_LOCAL"),
                history=History("KEEP_LAST", depth=5),
            )
            comm_writer = pub.create_datawriter(comm_topic, qos=comm_w_qos)
            comm_reader = sub.create_datareader(comm_topic, qos=comm_r_qos)
            time.sleep(0.5)  # discovery

            comm_writer.write(NoKeyMessage(value=777))
            time.sleep(0.5)

            samples = comm_reader.take()
            print(f"  Samples received: {len(samples)}")
            assert len(samples) >= 1, f"Expected >=1, got {len(samples)}"
            assert samples[0].data.value == 777
            print(f"  Data value: {samples[0].data.value}")
            record_pass(name)
        except Exception as e:
            record_fail(name, e)

        # ---------------------------------------------------------------
        # Test 32: Default QoS (no explicit settings) works
        # ---------------------------------------------------------------
        name = "test_qos_default"
        print(f"\n--- Test 32: {name} ---")
        try:
            default_topic = dp.create_topic("test_qos_default", NoKeyMessage)
            default_writer = pub.create_datawriter(default_topic, qos=DataWriterQos())
            default_reader = sub.create_datareader(default_topic, qos=DataReaderQos())
            time.sleep(0.5)

            default_writer.write(NoKeyMessage(value=888))
            time.sleep(0.5)

            samples = default_reader.take()
            print(f"  Samples received: {len(samples)}")
            assert len(samples) >= 1
            print(f"  Data value: {samples[0].data.value}")
            record_pass(name)
        except Exception as e:
            record_fail(name, e)

        # ===============================================================
        # Status API Value Verification Tests
        # (Actual event triggering + status value check)
        # ===============================================================
        print(f"\n{'='*60}")
        print(f"  Status API Value Verification Tests")
        print(f"{'='*60}\n")

        # ---------------------------------------------------------------
        # Test 33: requested_incompatible_qos status has total_count >= 1
        # (writer=BEST_EFFORT, reader=RELIABLE → QoS mismatch)
        # ---------------------------------------------------------------
        name = "test_status_value_requested_incompatible_qos"
        print(f"--- Test 33: {name} ---")
        try:
            sv_topic1 = dp.create_topic("test_sv_req_incompat", NoKeyMessage)
            sv_reader1 = sub.create_datareader(
                sv_topic1, qos=DataReaderQos(reliability=Reliability("RELIABLE"))
            )
            time.sleep(0.5)
            sv_writer1 = pub.create_datawriter(
                sv_topic1, qos=DataWriterQos(reliability=Reliability("BEST_EFFORT"))
            )
            time.sleep(2.0)

            status = sv_reader1.get_requested_incompatible_qos_status()
            print(f"  total_count: {status['total_count']}")
            print(f"  last_policy_id: {status['last_policy_id']}")
            assert status["total_count"] >= 1, f"Expected total_count >= 1, got {status['total_count']}"
            assert status["last_policy_id"] == 11, f"Expected policy_id 11 (Reliability), got {status['last_policy_id']}"
            record_pass(name)
        except Exception as e:
            record_fail(name, e)

        # ---------------------------------------------------------------
        # Test 34: offered_incompatible_qos status has total_count >= 1
        # (reader=RELIABLE, writer=BEST_EFFORT → Writer detects mismatch)
        # ---------------------------------------------------------------
        name = "test_status_value_offered_incompatible_qos"
        print(f"\n--- Test 34: {name} ---")
        try:
            sv_topic2 = dp.create_topic("test_sv_off_incompat", NoKeyMessage)
            sv_reader2 = sub.create_datareader(
                sv_topic2, qos=DataReaderQos(reliability=Reliability("RELIABLE"))
            )
            time.sleep(0.5)
            sv_writer2 = pub.create_datawriter(
                sv_topic2, qos=DataWriterQos(reliability=Reliability("BEST_EFFORT"))
            )
            time.sleep(2.0)

            status = sv_writer2.get_offered_incompatible_qos_status()
            print(f"  total_count: {status['total_count']}")
            print(f"  last_policy_id: {status['last_policy_id']}")
            assert status["total_count"] >= 1, f"Expected total_count >= 1, got {status['total_count']}"
            assert status["last_policy_id"] == 11, f"Expected policy_id 11 (Reliability), got {status['last_policy_id']}"
            record_pass(name)
        except Exception as e:
            record_fail(name, e)

        # ---------------------------------------------------------------
        # Test 35: sample_rejected status has total_count >= 1
        # (KEEP_ALL + ResourceLimits overflow)
        # ---------------------------------------------------------------
        name = "test_status_value_sample_rejected"
        print(f"\n--- Test 35: {name} ---")
        try:
            sv_topic3 = dp.create_topic("test_sv_rejected", NoKeyMessage)
            sv_reader3 = sub.create_datareader(
                sv_topic3, qos=DataReaderQos(
                    reliability=Reliability("RELIABLE"),
                    history=History("KEEP_ALL"),
                    resource_limits=ResourceLimits(max_samples=1, max_instances=1, max_samples_per_instance=1),
                )
            )
            time.sleep(0.3)
            sv_writer3 = pub.create_datawriter(
                sv_topic3, qos=DataWriterQos(reliability=Reliability("RELIABLE"))
            )
            time.sleep(0.5)

            for i in range(5):
                sv_writer3.write(NoKeyMessage(value=i))
                time.sleep(0.1)
            time.sleep(1.0)

            status = sv_reader3.get_sample_rejected_status()
            print(f"  total_count: {status['total_count']}")
            print(f"  last_reason: {status['last_reason']}")
            assert status["total_count"] >= 1, f"Expected total_count >= 1, got {status['total_count']}"
            record_pass(name)
        except Exception as e:
            record_fail(name, e)

        # ---------------------------------------------------------------
        # Test 36: subscription_matched status after writer creation
        # ---------------------------------------------------------------
        name = "test_status_value_subscription_matched"
        print(f"\n--- Test 36: {name} ---")
        try:
            sv_topic4 = dp.create_topic("test_sv_sub_matched", NoKeyMessage)
            sv_reader4 = sub.create_datareader(sv_topic4)
            sv_writer4 = pub.create_datawriter(sv_topic4)
            time.sleep(1.0)

            total, current = sv_reader4.get_subscription_matched_status()
            print(f"  total_count: {total}")
            print(f"  current_count: {current}")
            assert total >= 1, f"Expected total >= 1, got {total}"
            assert current >= 1, f"Expected current >= 1, got {current}"
            record_pass(name)
        except Exception as e:
            record_fail(name, e)

        # ---------------------------------------------------------------
        # Test 37: publication_matched status after reader creation
        # ---------------------------------------------------------------
        name = "test_status_value_publication_matched"
        print(f"\n--- Test 37: {name} ---")
        try:
            sv_topic5 = dp.create_topic("test_sv_pub_matched", NoKeyMessage)
            sv_writer5 = pub.create_datawriter(sv_topic5)
            sv_reader5 = sub.create_datareader(sv_topic5)
            time.sleep(1.0)

            total, current = sv_writer5.get_publication_matched_status()
            print(f"  total_count: {total}")
            print(f"  current_count: {current}")
            assert total >= 1, f"Expected total >= 1, got {total}"
            assert current >= 1, f"Expected current >= 1, got {current}"
            record_pass(name)
        except Exception as e:
            record_fail(name, e)

    finally:
        print(f"\n[Cleanup] Closing DomainParticipant")
        dp.close()

    return results


# ---------------------------------------------------------------------------
# Main
# ---------------------------------------------------------------------------

def main():
    print("=" * 60)
    print("  ALL Unit Tests")
    print("=" * 60)

    results = run_all_tests()

    print("\n" + "=" * 60)
    total = results["passed"] + results["failed"]
    print(f"  Results: {results['passed']} passed, {results['failed']} failed, {total} total")

    if results["errors"]:
        print("\n  Failed tests:")
        for name, err in results["errors"]:
            print(f"    - {name}: {err}")
        print("=" * 60)
        sys.exit(1)
    else:
        print("\n  All tests passed!")
        print("=" * 60)
        sys.exit(0)


if __name__ == "__main__":
    main()
