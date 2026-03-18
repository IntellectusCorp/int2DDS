#!/usr/bin/env python3
"""
HelloWorld Subscriber Example

Subscribes to HelloWorld samples to demonstrate int2dds Python bindings.

Usage:
    python hello_world_subscriber.py
"""
import sys
import os
sys.path.insert(0, os.path.join(os.path.dirname(__file__), ".."))

from hello_world_type import HelloWorld

from int2dds import DomainParticipant, DdsTimeout, StatusCondition, WaitSet
from int2dds.core.conditions import STATUS_DATA_AVAILABLE, STATUS_SUBSCRIPTION_MATCHED


def main() -> None:
    print("=== HelloWorld Subscriber ===")

    # Create domain participant
    with DomainParticipant(domain_id=0, name="PythonSubscriber") as dp:
        print(f"Created participant on domain {dp.domain_id}")

        # Create topic
        topic = dp.create_topic("hello_world_topic", HelloWorld)
        print(f"Created topic: {topic.name} ({topic.type_name})")

        # Create subscriber and data reader
        sub = dp.create_subscriber()
        reader = sub.create_datareader(topic)
        print("Created subscriber and data reader")

        # Get StatusCondition and configure for discovery phase
        status_cond = reader.get_statuscondition()
        status_cond.set_enabled_statuses(STATUS_SUBSCRIPTION_MATCHED)

        # Wait for publisher to connect
        print("Waiting for publisher...")
        waitset = WaitSet()
        waitset.attach(status_cond)

        while reader.matched_writers == 0:
            try:
                waitset.wait(timeout=1.0)
            except DdsTimeout:
                pass  # Timeout, check again

        print(f"Matched {reader.matched_writers} writer(s)")

        # Switch to DATA_AVAILABLE only for data reception phase
        status_cond.set_enabled_statuses(STATUS_DATA_AVAILABLE)

        # Receive samples
        print("Waiting for data...")
        samples_received = 0
        timeout_count = 0

        while timeout_count < 3:
            # First, check for data that may have arrived already
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

                # Take all available samples
                for sample in reader.take():
                    if sample.valid_data:
                        data = sample.data
                        print(f"Received: index={data.index}, message='{data.message}'")
                        samples_received += 1
                    else:
                        print("Received dispose/unregister notification")

                timeout_count = 0  # Reset on successful receive

            except DdsTimeout:
                timeout_count += 1
                print(f"No data received (timeout {timeout_count}/3)")

        print(f"Done. Received {samples_received} samples.")


if __name__ == "__main__":
    main()