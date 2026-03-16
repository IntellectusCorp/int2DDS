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

from int2dds import DomainParticipant
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

    finally:
        print(f"\n[Cleanup] Closing DomainParticipant")
        dp.close()

    return results


# ---------------------------------------------------------------------------
# Main
# ---------------------------------------------------------------------------

def main():
    print("=" * 60)
    print("  Instance Management Unit Tests")
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
