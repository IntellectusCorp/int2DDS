#!/usr/bin/env python3
"""
Instance Management Publisher

Demonstrates keyed topic pub/sub with instance lifecycle:
  register -> write -> dispose -> unregister

Usage:
    1. Start instance_sub.py first
    2. Then run: python instance_pub.py
"""

import time
import sys
import os

sys.path.insert(0, os.path.join(os.path.dirname(__file__), ".."))

from dataclasses import dataclass
from typing import ClassVar

from int2dds import DomainParticipant, WaitSet
from int2dds.cdr import CdrReader, CdrWriter, Extensibility
from int2dds.cdr.writer import CdrKeyWriter


# ---------------------------------------------------------------------------
# Keyed data type
# ---------------------------------------------------------------------------

@dataclass
class SensorData:
    """Keyed type: sensor_id is the key, temperature is the data."""

    _dds_type_name: ClassVar[str] = "SensorData"
    _extensibility: ClassVar[Extensibility] = Extensibility.FINAL
    _has_key: ClassVar[bool] = True

    sensor_id: int = 0
    temperature: float = 0.0

    def _serialize_cdr(self) -> bytes:
        w = CdrWriter(extensibility=self._extensibility)
        w.write_u32(self.sensor_id)
        w.write_f32(self.temperature)
        return w.to_bytes()

    @classmethod
    def _deserialize_cdr(cls, data: bytes) -> "SensorData":
        r = CdrReader(data)
        return cls(sensor_id=r.read_u32(), temperature=r.read_f32())

    def _serialize_key(self) -> bytes:
        kw = CdrKeyWriter()
        kw.write_u32(self.sensor_id)
        return kw.to_bytes()


HANDLE_NIL = b"\x00" * 16


def fmt_handle(h: bytes) -> str:
    if h == HANDLE_NIL:
        return "NIL"
    return h.hex()[:16] + "..."


# ---------------------------------------------------------------------------
# Main
# ---------------------------------------------------------------------------

def main():
    print("=" * 60)
    print("  Instance Management Publisher")
    print("=" * 60)

    with DomainParticipant(domain_id=1, name="InstancePub") as dp:
        print(f"[Setup] DomainParticipant (domain=0)")

        topic = dp.create_topic("instance_demo", SensorData)
        pub = dp.create_publisher()
        writer = pub.create_datawriter(topic)
        print(f"[Setup] Topic='{topic.name}', Type=SensorData (keyed)")

        # Wait for subscriber
        print("\n[Discovery] Waiting for subscriber...")
        waitset = WaitSet()
        waitset.attach(writer)

        while writer.matched_readers == 0:
            try:
                waitset.wait(timeout=1.0)
            except Exception:
                pass

        print(f"[Discovery] Matched {writer.matched_readers} reader(s)\n")
        time.sleep(0.5)  # let discovery settle

        # -------------------------------------------------------------------
        # Phase 1: Register instances
        # -------------------------------------------------------------------
        print("--- Phase 1: Register Instances ---")

        sensor1 = SensorData(sensor_id=1)
        sensor2 = SensorData(sensor_id=2)
        sensor3 = SensorData(sensor_id=3)

        handle1 = writer.register_instance(sensor1)
        print(f"  register(sensor_id=1) -> handle={fmt_handle(handle1)}")

        handle2 = writer.register_instance(sensor2)
        print(f"  register(sensor_id=2) -> handle={fmt_handle(handle2)}")

        handle3 = writer.register_instance(sensor3)
        print(f"  register(sensor_id=3) -> handle={fmt_handle(handle3)}")

        time.sleep(0.5)

        # -------------------------------------------------------------------
        # Phase 2: Write data for each instance
        # -------------------------------------------------------------------
        print("\n--- Phase 2: Write Data ---")

        samples = [
            SensorData(sensor_id=1, temperature=25.5),
            SensorData(sensor_id=2, temperature=30.0),
            SensorData(sensor_id=3, temperature=18.3),
            SensorData(sensor_id=1, temperature=26.1),
            SensorData(sensor_id=2, temperature=31.2),
        ]

        for s in samples:
            writer.write(s)
            print(f"  write(sensor_id={s.sensor_id}, temp={s.temperature})")
            time.sleep(0.3)

        time.sleep(0.5)

        # -------------------------------------------------------------------
        # Phase 3: Dispose instance 3 (no longer valid)
        # -------------------------------------------------------------------
        print("\n--- Phase 3: Dispose Instance ---")
        print(f"  dispose(sensor_id=3, handle={fmt_handle(handle3)})")
        writer.dispose(sensor3, handle3)
        time.sleep(0.5)

        # -------------------------------------------------------------------
        # Phase 4: Write more data (only sensor 1 & 2)
        # -------------------------------------------------------------------
        print("\n--- Phase 4: More Data (sensor 1 & 2 only) ---")

        more_samples = [
            SensorData(sensor_id=1, temperature=27.0),
            SensorData(sensor_id=2, temperature=29.8),
        ]

        for s in more_samples:
            writer.write(s)
            print(f"  write(sensor_id={s.sensor_id}, temp={s.temperature})")
            time.sleep(0.3)

        time.sleep(0.5)

        # -------------------------------------------------------------------
        # Phase 5: Unregister instance 2 (writer no longer owns it)
        # -------------------------------------------------------------------
        print("\n--- Phase 5: Unregister Instance ---")
        print(f"  unregister(sensor_id=2, handle={fmt_handle(handle2)})")
        writer.unregister_instance(sensor2, handle2)
        time.sleep(0.5)

        # -------------------------------------------------------------------
        # Phase 6: Final write (only sensor 1 alive)
        # -------------------------------------------------------------------
        print("\n--- Phase 6: Final Write (sensor 1 only) ---")
        final = SensorData(sensor_id=1, temperature=28.5)
        writer.write(final)
        print(f"  write(sensor_id={final.sensor_id}, temp={final.temperature})")

        # Give subscriber time to receive everything
        time.sleep(2.0)

        print("\n" + "=" * 60)
        print("  Publisher finished")
        print("=" * 60)


if __name__ == "__main__":
    main()
