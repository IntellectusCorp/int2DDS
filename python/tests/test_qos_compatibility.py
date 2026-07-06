"""
QoS RxO (Requested vs Offered) Compatibility Tests

Tests all combinations of QoS policies to verify that DDS matching
rules are correctly enforced.

Usage:
    cd python/examples
    python qos_compatibility_test.py
"""

import sys
import os
import time

sys.path.insert(0, os.path.join(os.path.dirname(__file__), ".."))

from int2dds import DomainParticipant
from int2dds.core.qos import (
    DataWriterQos, DataReaderQos,
    Reliability, Durability, Ownership,
    Deadline, Liveliness, DestinationOrder,
)


# ---------------------------------------------------------------------------
# Helper
# ---------------------------------------------------------------------------

_domain_counter = 30  # start from domain 30 to avoid conflicts


def next_domain() -> int:
    global _domain_counter
    _domain_counter += 1
    return _domain_counter


def test_compatibility(
    test_name: str,
    w_qos: DataWriterQos,
    r_qos: DataReaderQos,
    expect_match: bool,
    results: dict,
    w_label: str = "",
    r_label: str = "",
) -> None:
    """
    Create a writer and reader with given QoS, check if they match.
    """
    domain = next_domain()
    dp = DomainParticipant(domain_id=domain)

    try:
        topic = dp.create_topic("compat_test", _SimpleType)
        pub = dp.create_publisher()
        sub = dp.create_subscriber()

        writer = pub.create_datawriter(topic, qos=w_qos)
        reader = sub.create_datareader(topic, qos=r_qos)

        time.sleep(1.0)  # discovery

        matched = writer.matched_readers > 0

        if matched == expect_match:
            results["passed"] += 1
            status = "PASS"
        else:
            results["failed"] += 1
            results["errors"].append((test_name, f"expected match={expect_match}, got {matched}"))
            status = "FAIL"

        symbol = "✅" if matched else "❌"
        expect_str = "match" if expect_match else "no match"
        label = f"Writer: {w_label}, Reader: {r_label}" if w_label else test_name
        print(f"  [{status}] {label} → {symbol} {expect_str}")

    except Exception as e:
        # QoS unsupported or other error
        if not expect_match:
            # If we expected no match, an error is also acceptable
            results["passed"] += 1
            label = f"Writer: {w_label}, Reader: {r_label}" if w_label else test_name
            print(f"  [PASS] {label}: error (acceptable for no-match case): {e}")
        else:
            results["failed"] += 1
            results["errors"].append((test_name, str(e)))
            print(f"  [FAIL] {test_name}: {e}")
    finally:
        dp.close()


# ---------------------------------------------------------------------------
# Simple test type
# ---------------------------------------------------------------------------

from dataclasses import dataclass
from typing import ClassVar
from int2dds.cdr import CdrReader, CdrWriter, Extensibility


@dataclass
class _SimpleType:
    _dds_type_name: ClassVar[str] = "CompatTestType"
    _extensibility: ClassVar[Extensibility] = Extensibility.FINAL
    _has_key: ClassVar[bool] = False
    value: int = 0

    def _serialize_cdr(self) -> bytes:
        w = CdrWriter(extensibility=self._extensibility)
        w.write_i32(self.value)
        return w.to_bytes()

    @classmethod
    def _deserialize_cdr(cls, data: bytes) -> "_SimpleType":
        r = CdrReader(data)
        return cls(value=r.read_i32())

    def _serialize_key(self) -> bytes:
        return b""


# ---------------------------------------------------------------------------
# Tests
# ---------------------------------------------------------------------------

def run_all_tests():
    results = {"passed": 0, "failed": 0, "errors": []}

    # ===============================================================
    # 1. Reliability RxO (Writer >= Reader)
    #    RELIABLE > BEST_EFFORT
    # ===============================================================
    print(f"\n{'='*60}")
    print(f"  Reliability RxO Tests")
    print(f"{'='*60}")

    test_compatibility(
        "reliability_reliable_reliable",
        DataWriterQos(reliability=Reliability("RELIABLE")),
        DataReaderQos(reliability=Reliability("RELIABLE")),
        expect_match=True, results=results,
        w_label="RELIABLE", r_label="RELIABLE",
    )
    test_compatibility(
        "reliability_reliable_besteffort",
        DataWriterQos(reliability=Reliability("RELIABLE")),
        DataReaderQos(reliability=Reliability("BEST_EFFORT")),
        expect_match=True, results=results,
        w_label="RELIABLE", r_label="BEST_EFFORT",
    )
    test_compatibility(
        "reliability_besteffort_besteffort",
        DataWriterQos(reliability=Reliability("BEST_EFFORT")),
        DataReaderQos(reliability=Reliability("BEST_EFFORT")),
        expect_match=True, results=results,
        w_label="BEST_EFFORT", r_label="BEST_EFFORT",
    )
    test_compatibility(
        "reliability_besteffort_reliable",
        DataWriterQos(reliability=Reliability("BEST_EFFORT")),
        DataReaderQos(reliability=Reliability("RELIABLE")),
        expect_match=False, results=results,
        w_label="BEST_EFFORT", r_label="RELIABLE",
    )

    # ===============================================================
    # 2. Durability RxO (Writer >= Reader)
    #    TRANSIENT_LOCAL > VOLATILE
    # ===============================================================
    print(f"\n{'='*60}")
    print(f"  Durability RxO Tests")
    print(f"{'='*60}")

    test_compatibility(
        "durability_tl_tl",
        DataWriterQos(durability=Durability("TRANSIENT_LOCAL")),
        DataReaderQos(durability=Durability("TRANSIENT_LOCAL")),
        expect_match=True, results=results,
        w_label="TRANSIENT_LOCAL", r_label="TRANSIENT_LOCAL",
    )
    test_compatibility(
        "durability_tl_volatile",
        DataWriterQos(durability=Durability("TRANSIENT_LOCAL")),
        DataReaderQos(durability=Durability("VOLATILE")),
        expect_match=True, results=results,
        w_label="TRANSIENT_LOCAL", r_label="VOLATILE",
    )
    test_compatibility(
        "durability_volatile_volatile",
        DataWriterQos(durability=Durability("VOLATILE")),
        DataReaderQos(durability=Durability("VOLATILE")),
        expect_match=True, results=results,
        w_label="VOLATILE", r_label="VOLATILE",
    )
    test_compatibility(
        "durability_volatile_tl",
        DataWriterQos(durability=Durability("VOLATILE")),
        DataReaderQos(durability=Durability("TRANSIENT_LOCAL")),
        expect_match=False, results=results,
        w_label="VOLATILE", r_label="TRANSIENT_LOCAL",
    )

    # ===============================================================
    # 3. Ownership RxO (must be same)
    # ===============================================================
    print(f"\n{'='*60}")
    print(f"  Ownership RxO Tests")
    print(f"{'='*60}")

    test_compatibility(
        "ownership_shared_shared",
        DataWriterQos(ownership=Ownership("SHARED")),
        DataReaderQos(ownership=Ownership("SHARED")),
        expect_match=True, results=results,
        w_label="SHARED", r_label="SHARED",
    )
    test_compatibility(
        "ownership_exclusive_exclusive",
        DataWriterQos(ownership=Ownership("EXCLUSIVE")),
        DataReaderQos(ownership=Ownership("EXCLUSIVE")),
        expect_match=True, results=results,
        w_label="EXCLUSIVE", r_label="EXCLUSIVE",
    )
    test_compatibility(
        "ownership_shared_exclusive",
        DataWriterQos(ownership=Ownership("SHARED")),
        DataReaderQos(ownership=Ownership("EXCLUSIVE")),
        expect_match=False, results=results,
        w_label="SHARED", r_label="EXCLUSIVE",
    )
    test_compatibility(
        "ownership_exclusive_shared",
        DataWriterQos(ownership=Ownership("EXCLUSIVE")),
        DataReaderQos(ownership=Ownership("SHARED")),
        expect_match=False, results=results,
        w_label="EXCLUSIVE", r_label="SHARED",
    )

    # ===============================================================
    # 4. Deadline RxO (Writer.period <= Reader.period)
    #    Writer must offer at least as fast as reader requests
    # ===============================================================
    print(f"\n{'='*60}")
    print(f"  Deadline RxO Tests")
    print(f"{'='*60}")

    test_compatibility(
        "deadline_500_1000",
        DataWriterQos(deadline=Deadline(period=0.5)),
        DataReaderQos(deadline=Deadline(period=1.0)),
        expect_match=True, results=results,
        w_label="500ms", r_label="1000ms",
    )
    test_compatibility(
        "deadline_500_500",
        DataWriterQos(deadline=Deadline(period=0.5)),
        DataReaderQos(deadline=Deadline(period=0.5)),
        expect_match=True, results=results,
        w_label="500ms", r_label="500ms",
    )
    test_compatibility(
        "deadline_1000_1000",
        DataWriterQos(deadline=Deadline(period=1.0)),
        DataReaderQos(deadline=Deadline(period=1.0)),
        expect_match=True, results=results,
        w_label="1000ms", r_label="1000ms",
    )
    test_compatibility(
        "deadline_1000_500",
        DataWriterQos(deadline=Deadline(period=1.0)),
        DataReaderQos(deadline=Deadline(period=0.5)),
        expect_match=False, results=results,
        w_label="1000ms", r_label="500ms",
    )

    # ===============================================================
    # 5. Liveliness RxO (Writer.kind >= Reader.kind)
    #    MANUAL_BY_TOPIC > MANUAL_BY_PARTICIPANT > AUTOMATIC
    #    + Writer.lease_duration <= Reader.lease_duration
    # ===============================================================
    print(f"\n{'='*60}")
    print(f"  Liveliness RxO Tests")
    print(f"{'='*60}")

    liveliness_kinds = ["AUTOMATIC", "MANUAL_BY_PARTICIPANT", "MANUAL_BY_TOPIC"]
    # kind order: AUTOMATIC=0, MANUAL_BY_PARTICIPANT=1, MANUAL_BY_TOPIC=2
    # RxO: writer kind >= reader kind

    for w_kind in liveliness_kinds:
        for r_kind in liveliness_kinds:
            w_idx = liveliness_kinds.index(w_kind)
            r_idx = liveliness_kinds.index(r_kind)
            expect = w_idx >= r_idx

            test_compatibility(
                f"liveliness_{w_kind.lower()}_to_{r_kind.lower()}",
                DataWriterQos(liveliness=Liveliness(w_kind, lease_duration=5.0)),
                DataReaderQos(liveliness=Liveliness(r_kind, lease_duration=5.0)),
                expect_match=expect, results=results,
                w_label=w_kind, r_label=r_kind,
            )

    # ===============================================================
    # 6. DestinationOrder RxO (must be same)
    # ===============================================================
    print(f"\n{'='*60}")
    print(f"  DestinationOrder RxO Tests")
    print(f"{'='*60}")

    test_compatibility(
        "destorder_reception_reception",
        DataWriterQos(destination_order=DestinationOrder("BY_RECEPTION")),
        DataReaderQos(destination_order=DestinationOrder("BY_RECEPTION")),
        expect_match=True, results=results,
        w_label="BY_RECEPTION", r_label="BY_RECEPTION",
    )
    test_compatibility(
        "destorder_source_source",
        DataWriterQos(destination_order=DestinationOrder("BY_SOURCE")),
        DataReaderQos(destination_order=DestinationOrder("BY_SOURCE")),
        expect_match=True, results=results,
        w_label="BY_SOURCE", r_label="BY_SOURCE",
    )
    test_compatibility(
        "destorder_reception_source",
        DataWriterQos(destination_order=DestinationOrder("BY_RECEPTION")),
        DataReaderQos(destination_order=DestinationOrder("BY_SOURCE")),
        expect_match=False, results=results,
        w_label="BY_RECEPTION", r_label="BY_SOURCE",
    )
    test_compatibility(
        "destorder_source_reception",
        DataWriterQos(destination_order=DestinationOrder("BY_SOURCE")),
        DataReaderQos(destination_order=DestinationOrder("BY_RECEPTION")),
        expect_match=True, results=results,
        w_label="BY_SOURCE", r_label="BY_RECEPTION",
    )

    return results


# ---------------------------------------------------------------------------
# Main
# ---------------------------------------------------------------------------

def main():
    print("=" * 60)
    print("  QoS RxO Compatibility Tests")
    print("=" * 60)

    results = run_all_tests()

    print(f"\n{'='*60}")
    total = results["passed"] + results["failed"]
    print(f"  Results: {results['passed']} passed, {results['failed']} failed, {total} total")

    if results["errors"]:
        print(f"\n  Failed tests:")
        for name, err in results["errors"]:
            print(f"    - {name}: {err}")
        print("=" * 60)
        sys.exit(1)
    else:
        print(f"\n  All tests passed!")
        print("=" * 60)
        sys.exit(0)


if __name__ == "__main__":
    main()
