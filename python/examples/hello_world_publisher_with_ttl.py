#!/usr/bin/env python3
"""
HelloWorld Publisher with multicast TTL = 64

Mirrors ``dds/examples/hello_world/hello_world_with_ttl.rs``: the
DomainParticipant is created with a PropertyQosPolicy carrying the
``int2dds.transport.UDPv4.multicast_ttl`` entry, set via the
:meth:`Property.set_multicast_ttl` convenience method.

Usage:
    python hello_world_publisher_with_ttl.py
"""

import os
import sys
import time

sys.path.insert(0, os.path.join(os.path.dirname(__file__), ".."))

from hello_world_type import HelloWorld

from int2dds import DomainParticipant, ParticipantQos, Property, WaitSet

MULTICAST_TTL = 64


def main() -> None:
    print(f"=== HelloWorld Publisher (multicast TTL = {MULTICAST_TTL}) ===")

    property_policy = Property()
    property_policy.set_multicast_ttl(MULTICAST_TTL)
    qos = ParticipantQos(property=property_policy)

    with DomainParticipant(domain_id=0, name="PythonPublisherWithTtl", qos=qos) as dp:
        print(f"Created participant on domain {dp.domain_id} with TTL={MULTICAST_TTL}")

        topic = dp.create_topic("hello_world_topic", HelloWorld)
        print(f"Created topic: {topic.name} ({topic.type_name})")

        pub = dp.create_publisher()
        writer = pub.create_datawriter(topic)
        print("Created publisher and data writer")

        print("Waiting for subscriber...")
        waitset = WaitSet()
        waitset.attach(writer)

        while writer.matched_readers == 0:
            try:
                waitset.wait(timeout=1.0)
            except Exception:
                pass  # Timeout, check again

        print(f"Matched {writer.matched_readers} reader(s)")

        for i in range(10):
            sample = HelloWorld(
                index=i,
                message=f"Hello from Python (ttl={MULTICAST_TTL})! ({i})",
            )
            writer.write(sample)
            print(f"Published: index={sample.index}, message='{sample.message}'")
            time.sleep(0.5)

        print("Done publishing")


if __name__ == "__main__":
    main()
