#!/usr/bin/env python3
"""
HelloWorld Subscriber with multicast TTL = 64

Mirrors ``dds/examples/hello_world/hello_world_with_ttl.rs``: the
DomainParticipant is created with a PropertyQosPolicy carrying the
``int2dds.transport.UDPv4.multicast_ttl`` entry, set via the
:meth:`Property.set_multicast_ttl` convenience method.

Usage:
    python hello_world_subscriber_with_ttl.py
"""

import os
import sys

sys.path.insert(0, os.path.join(os.path.dirname(__file__), ".."))

from hello_world_type import HelloWorld

from int2dds import (
    DdsTimeout,
    DomainParticipant,
    ParticipantQos,
    Property,
    StatusCondition,
    WaitSet,
)
from int2dds.core.conditions import STATUS_DATA_AVAILABLE, STATUS_SUBSCRIPTION_MATCHED

MULTICAST_TTL = 64


def main() -> None:
    print(f"=== HelloWorld Subscriber (multicast TTL = {MULTICAST_TTL}) ===")

    property_policy = Property()
    property_policy.set_multicast_ttl(MULTICAST_TTL)
    qos = ParticipantQos(property=property_policy)

    with DomainParticipant(domain_id=0, name="PythonSubscriberWithTtl", qos=qos) as dp:
        print(f"Created participant on domain {dp.domain_id} with TTL={MULTICAST_TTL}")

        topic = dp.create_topic("hello_world_topic", HelloWorld)
        print(f"Created topic: {topic.name} ({topic.type_name})")

        sub = dp.create_subscriber()
        reader = sub.create_datareader(topic)
        print("Created subscriber and data reader")

        status_cond = reader.get_statuscondition()
        status_cond.set_enabled_statuses(STATUS_SUBSCRIPTION_MATCHED)

        print("Waiting for publisher...")
        waitset = WaitSet()
        waitset.attach(status_cond)

        while reader.matched_writers == 0:
            try:
                waitset.wait(timeout=1.0)
            except DdsTimeout:
                pass  # Timeout, check again

        print(f"Matched {reader.matched_writers} writer(s)")

        status_cond.set_enabled_statuses(STATUS_DATA_AVAILABLE)

        print("Waiting for data...")
        samples_received = 0
        timeout_count = 0

        while timeout_count < 3:
            for sample in reader.take():
                if sample.valid_data:
                    data = sample.data
                    print(f"Received: index={data.index}, message='{data.message}'")
                    samples_received += 1
                else:
                    print("Received dispose/unregister notification")
                timeout_count = 0

            try:
                waitset.wait(timeout=2.0)

                for sample in reader.take():
                    if sample.valid_data:
                        data = sample.data
                        print(f"Received: index={data.index}, message='{data.message}'")
                        samples_received += 1
                    else:
                        print("Received dispose/unregister notification")

                timeout_count = 0
            except DdsTimeout:
                timeout_count += 1
                print(f"No data received (timeout {timeout_count}/3)")

        print(f"Done. Received {samples_received} samples.")


if __name__ == "__main__":
    main()
