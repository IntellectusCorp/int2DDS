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

TOPIC_NAME = "hello_world_topic"


def _kind_name(kind: str) -> str:
    """Render a QoS kind the way the Rust example's Debug output does."""
    return "".join(part.capitalize() for part in kind.split("_"))


def _history_name(history) -> str:
    if history.kind == "KEEP_ALL":
        return "KeepAll"
    return f"KeepLast({history.depth})"


def main() -> None:
    # Reliability and domain are selectable on the CLI, matching the Rust/C#/C examples.
    parser = argparse.ArgumentParser(description="HelloWorld DDS Publisher (Python)")
    parser.add_argument("-d", "--domain", type=int, default=0, help="Domain ID (default 0)")
    parser.add_argument("--reliable", action="store_true",
                        help="Use RELIABLE reliability (default BEST_EFFORT)")
    args = parser.parse_args()

    # Create domain participant
    with DomainParticipant(domain_id=args.domain, name="PythonPublisher") as dp:
        # Create topic
        topic = dp.create_topic(TOPIC_NAME, HelloWorld)

        # Create publisher and data writer (BEST_EFFORT by default, --reliable for RELIABLE)
        pub = dp.create_publisher()
        reliability = (
            Reliability("RELIABLE", max_blocking_time=0.1)
            if args.reliable
            else Reliability("BEST_EFFORT")
        )
        writer = pub.create_datawriter(topic, DataWriterQos(reliability=reliability))

        wqos = writer.get_qos()
        print(f"[publisher INFO] domain_id: {args.domain}, topic: {TOPIC_NAME}")
        print(
            f"[publisher qos] reliability: {_kind_name(wqos.reliability.kind)}, "
            f"durability: {_kind_name(wqos.durability.kind)}, "
            f"history: {_history_name(wqos.history)}"
        )

        # Wait for subscriber to connect
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

            print("Subscriber matched!")

            # Publish samples until Ctrl-C, like the Rust/C#/C examples
            i = 1
            while True:
                sample = HelloWorld(index=i, message=f"[Python]HelloWorld_d{args.domain}")
                writer.write(sample)
                print(f'Published HelloWorld {{ index: {sample.index}, message: "{sample.message}" }}')
                time.sleep(1.0)
                i += 1
        except KeyboardInterrupt:
            print("\nShutting down...")


if __name__ == "__main__":
    main()
