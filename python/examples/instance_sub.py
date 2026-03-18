#!/usr/bin/env python3
"""
Instance Management Subscriber

Receives keyed data and reports instance lifecycle events:
  - ALIVE data (valid_data=True): normal data samples
  - DISPOSE notification (valid_data=False): instance disposed
  - UNREGISTER notification (valid_data=False): writer unregistered instance

Usage:
    1. Run this first: python instance_sub.py
    2. Then start: python instance_pub.py
"""

import sys
import os

sys.path.insert(0, os.path.join(os.path.dirname(__file__), ".."))

from dataclasses import dataclass
from typing import ClassVar

from int2dds import DomainParticipant, DdsTimeout, WaitSet, StatusCondition
from int2dds.core.conditions import STATUS_DATA_AVAILABLE, STATUS_SUBSCRIPTION_MATCHED
from int2dds.cdr import CdrReader, CdrWriter, Extensibility
from int2dds.cdr.writer import CdrKeyWriter


# ---------------------------------------------------------------------------
# Keyed data type (must match publisher)
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


# ---------------------------------------------------------------------------
# Main
# ---------------------------------------------------------------------------

def main():
    print("=" * 60)
    print("  Instance Management Subscriber")
    print("=" * 60)

    with DomainParticipant(domain_id=1, name="InstanceSub") as dp:
        print(f"[Setup] DomainParticipant {dp.domain_id}")

        topic = dp.create_topic("instance_demo", SensorData)
        sub = dp.create_subscriber()
        reader = sub.create_datareader(topic)
        print(f"[Setup] Topic='{topic.name}', Type=SensorData (keyed)")

        # Wait for publisher to connect
        print("\n[Discovery] Waiting for publisher...")
        status_cond = reader.get_statuscondition()
        status_cond.set_enabled_statuses(STATUS_SUBSCRIPTION_MATCHED)

        waitset = WaitSet()
        waitset.attach(status_cond)

        while reader.matched_writers == 0:
            try:
                waitset.wait(timeout=1.0)
            except DdsTimeout:
                pass

        print(f"[Discovery] Matched {reader.matched_writers} writer(s)\n")

        # Switch to DATA_AVAILABLE for data reception
        status_cond.set_enabled_statuses(STATUS_DATA_AVAILABLE)

        # Receive loop
        print("--- Waiting for data ---")
        data_count = 0
        dispose_count = 0
        unregister_count = 0
        timeout_count = 0

        while timeout_count < 5:
            try:
                waitset.wait(timeout=2.0)

                for sample in reader.take():
                    if sample.valid_data:
                        d = sample.data
                        print(f"  [DATA]        sensor_id={d.sensor_id}, "
                              f"temperature={d.temperature}")
                        data_count += 1
                    else:
                        # Dispose or unregister notification
                        # (DDS doesn't distinguish in valid_data=False,
                        #  but we count them for the summary)
                        print(f"  [LIFECYCLE]   instance event "
                              f"(valid_data=False - dispose or unregister)")
                        dispose_count += 1

                timeout_count = 0  # reset on successful receive

            except DdsTimeout:
                timeout_count += 1
                print(f"  ... no data (timeout {timeout_count}/5)")

        # Summary
        print("\n" + "=" * 60)
        print(f"  Summary:")
        print(f"    Data samples received : {data_count}")
        print(f"    Lifecycle events      : {dispose_count}")
        print(f"    Total                 : {data_count + dispose_count}")
        print("=" * 60)


if __name__ == "__main__":
    main()
