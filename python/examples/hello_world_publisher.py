#!/usr/bin/env python3
"""
HelloWorld Publisher Example

Publishes HelloWorld samples to demonstrate int2dds Python bindings.

Usage:
    python hello_world_publisher.py
"""

import time
import sys
import os
sys.path.insert(0, os.path.join(os.path.dirname(__file__), ".."))

from hello_world_type import HelloWorld

from int2dds import DomainParticipant, WaitSet


def main() -> None:
    print("=== HelloWorld Publisher ===")

    # Create domain participant
    with DomainParticipant(domain_id=0, name="PythonPublisher") as dp:
        print(f"Created participant on domain {dp.domain_id}")

        # Create topic
        topic = dp.create_topic("hello_world_topic", HelloWorld)
        print(f"Created topic: {topic.name} ({topic.type_name})")

        # Create publisher and data writer
        pub = dp.create_publisher()
        writer = pub.create_datawriter(topic)
        print("Created publisher and data writer")

        # Wait for subscriber to connect
        print("Waiting for subscriber...")
        waitset = WaitSet()
        waitset.attach(writer)

        while writer.matched_readers == 0:
            try:
                waitset.wait(timeout=1.0)
            except Exception:
                pass  # Timeout, check again

        print(f"Matched {writer.matched_readers} reader(s)")

        # Publish samples
        for i in range(10):
            sample = HelloWorld(index=i, message=f"Hello from Python! ({i})")
            writer.write(sample)
            print(f"Published: index={sample.index}, message='{sample.message}'")
            time.sleep(0.5)

        print("Done publishing")


if __name__ == "__main__":
    main()
