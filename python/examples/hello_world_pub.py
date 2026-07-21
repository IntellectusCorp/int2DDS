#!/usr/bin/env python3
"""
HelloWorld Publisher Example

Publishes HelloWorld samples to demonstrate int2dds Python bindings.

Usage:
    python hello_world_pub.py [-d DOMAIN] [--reliable]
"""

import argparse
import time
import sys
import os
sys.path.insert(0, os.path.join(os.path.dirname(__file__), ".."))

from hello_world_type import HelloWorld

from int2dds import DomainParticipant, WaitSet, DataWriterQos, Reliability


def main() -> None:
    # Reliability and domain are selectable on the CLI, matching the Rust/C#/C examples.
    parser = argparse.ArgumentParser(description="HelloWorld DDS Publisher (Python)")
    parser.add_argument("-d", "--domain", type=int, default=0, help="Domain ID (default 0)")
    parser.add_argument("--reliable", action="store_true",
                        help="Use RELIABLE reliability (default BEST_EFFORT)")
    args = parser.parse_args()

    print("=== HelloWorld Publisher (Python) ===")
    print(f"QoS: {'RELIABLE' if args.reliable else 'BEST_EFFORT'}")

    # Create domain participant
    with DomainParticipant(domain_id=args.domain, name="PythonPublisher") as dp:
        print(f"Created participant on domain {dp.domain_id}")

        # Create topic
        topic = dp.create_topic("hello_world_topic", HelloWorld)
        print(f"Created topic: {topic.name} ({topic.type_name})")

        # Create publisher and data writer (BEST_EFFORT by default, --reliable for RELIABLE)
        pub = dp.create_publisher()
        reliability = (
            Reliability("RELIABLE", max_blocking_time=0.1)
            if args.reliable
            else Reliability("BEST_EFFORT")
        )
        writer = pub.create_datawriter(topic, DataWriterQos(reliability=reliability))
        print("Created publisher and data writer")

        # Wait for subscriber to connect
        print("Waiting for subscriber...")
        waitset = WaitSet()
        waitset.attach(writer)

        # Run until Ctrl-C, then the participant context manager cleans up
        # gracefully (matches the Rust example).
        try:
            while writer.matched_readers == 0:
                try:
                    waitset.wait(timeout=1.0)
                except Exception:
                    pass  # Timeout, check again

            print(f"Matched {writer.matched_readers} reader(s)")

            # Publish samples until Ctrl-C, like the Rust/C#/C examples
            i = 0
            while True:
                sample = HelloWorld(index=i, message=f"Hello from Python! ({i})")
                writer.write(sample)
                print(f"Published: index={sample.index}, message='{sample.message}'")
                time.sleep(1.0)
                i += 1
        except KeyboardInterrupt:
            print("\nShutting down...")


if __name__ == "__main__":
    main()
