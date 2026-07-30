#!/usr/bin/env python3
"""
HelloWorld Subscriber Example

Subscribes to HelloWorld samples to demonstrate int2dds Python bindings.

Usage:
    python hello_world_sub.py [-d DOMAIN] [--reliable]
"""
import argparse
import sys
import os
sys.path.insert(0, os.path.join(os.path.dirname(__file__), ".."))

from hello_world_type import HelloWorld

from int2dds import DomainParticipant, DdsTimeout, WaitSet, DataReaderQos, Reliability
from int2dds.core.conditions import STATUS_DATA_AVAILABLE, STATUS_SUBSCRIPTION_MATCHED

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
    parser = argparse.ArgumentParser(description="HelloWorld DDS Subscriber (Python)")
    parser.add_argument("-d", "--domain", type=int, default=0, help="Domain ID (default 0)")
    parser.add_argument("--reliable", action="store_true",
                        help="Use RELIABLE reliability (default BEST_EFFORT)")
    args = parser.parse_args()

    # Create domain participant
    with DomainParticipant(domain_id=args.domain, name="PythonSubscriber") as dp:
        # Create topic
        topic = dp.create_topic(TOPIC_NAME, HelloWorld)

        # Create subscriber and data reader (BEST_EFFORT by default, --reliable for RELIABLE)
        sub = dp.create_subscriber()
        reliability = (
            Reliability("RELIABLE", max_blocking_time=0.1)
            if args.reliable
            else Reliability("BEST_EFFORT")
        )
        reader = sub.create_datareader(topic, DataReaderQos(reliability=reliability))

        rqos = reader.get_qos()
        print(f"[subscriber INFO] domain_id: {args.domain}, topic: {TOPIC_NAME}")
        print(
            f"[subscriber qos] reliability: {_kind_name(rqos.reliability.kind)}, "
            f"durability: {_kind_name(rqos.durability.kind)}, "
            f"history: {_history_name(rqos.history)}"
        )

        # Get StatusCondition and configure for discovery phase
        status_cond = reader.get_statuscondition()
        status_cond.set_enabled_statuses(STATUS_SUBSCRIPTION_MATCHED)

        # Wait for publisher to connect
        waitset = WaitSet()
        waitset.attach(status_cond)

        while reader.matched_writers == 0:
            try:
                waitset.wait(timeout=1.0)
            except DdsTimeout:
                pass  # Timeout, check again

        print("Publisher matched!")

        # Switch to DATA_AVAILABLE only for data reception phase
        status_cond.set_enabled_statuses(STATUS_DATA_AVAILABLE)

        # Receive samples until Ctrl-C, then the participant context manager
        # cleans up gracefully (matches the Rust example).
        samples_received = 0

        try:
            while True:
                for sample in reader.take():
                    if sample.valid_data:
                        data = sample.data
                        print(f'Read sample: HelloWorld {{ index: {data.index}, message: "{data.message}" }}')
                        samples_received += 1
                    else:
                        print("Received dispose/unregister notification")

                try:
                    waitset.wait(timeout=2.0)
                except DdsTimeout:
                    pass  # No data yet, keep waiting
        except KeyboardInterrupt:
            print("\nShutting down...")

        print(f"Done. Received {samples_received} samples.")


if __name__ == "__main__":
    main()
