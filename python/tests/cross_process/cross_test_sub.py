#!/usr/bin/env python3
"""
Cross-process communication test: Python Subscriber

Subscribes to HelloWorld samples for cross-process testing with Rust publisher.
CLI options match Rust hello_world binary for easy pairing.

Usage:
    # Pair with Rust publisher:
    #   Terminal 1: hello_world.exe -P -T hello_world_topic
    #   Terminal 2: python cross_test_sub.py -T hello_world_topic
    #
    # Reliable QoS:
    #   Terminal 1: hello_world.exe -P -r -T hello_world_topic
    #   Terminal 2: python cross_test_sub.py -r -T hello_world_topic

    python cross_test_sub.py [OPTIONS]
"""

import argparse
import platform
import sys
import os
import time

sys.path.insert(0, os.path.join(os.path.dirname(__file__), "..", ".."))

from hello_world_type import HelloWorld
from int2dds import (
    DataReaderQos,
    Deadline,
    DdsTimeout,
    DomainParticipant,
    Durability,
    History,
    Ownership,
    Partition,
    Reliability,
    Subscriber,
    SubscriberQos,
    WaitSet,
)
from int2dds.core.conditions import STATUS_DATA_AVAILABLE, STATUS_SUBSCRIPTION_MATCHED


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="int2DDS Python Subscriber (cross-process test)",
    )
    parser.add_argument(
        "-T", "--topic", default="hello_world_topic",
        help="Topic name (default: hello_world_topic)",
    )
    parser.add_argument(
        "-d", "--domain", type=int, default=0,
        help="Domain ID (default: 0)",
    )
    parser.add_argument(
        "-r", "--reliable", action="store_true",
        help="Use RELIABLE reliability (default: BEST_EFFORT)",
    )
    parser.add_argument(
        "-k", "--keep", type=int, default=1,
        help="History depth: 0 = keep-all, N = keep-last N (default: 1)",
    )
    parser.add_argument(
        "-f", "--deadline", type=int, default=None,
        help="Deadline period in ms (default: infinite)",
    )
    parser.add_argument(
        "-t", "--transient-local", action="store_true",
        help="Use transient-local durability (default: volatile)",
    )
    parser.add_argument(
        "-o", "--ownership", action="store_true",
        help="Use exclusive ownership (default: shared)",
    )
    parser.add_argument(
        "-p", "--partition", type=str, default=None,
        help="Partition name",
    )
    parser.add_argument(
        "--timeout-count", type=int, default=5,
        help="Number of consecutive timeouts before exit (default: 5)",
    )
    return parser.parse_args()


def main() -> None:
    args = parse_args()

    reliability_str = "RELIABLE" if args.reliable else "BEST_EFFORT"
    durability_str = "TRANSIENT_LOCAL" if args.transient_local else "VOLATILE"
    history_str = "KEEP_ALL" if args.keep == 0 else f"KEEP_LAST({args.keep})"
    deadline_str = f"{args.deadline}ms" if args.deadline is not None else "INFINITE"
    ownership_str = "EXCLUSIVE" if args.ownership else "SHARED"
    partition_str = args.partition or "(none)"
    hostname = platform.node()

    print("=" * 60)
    print("  int2DDS Python Subscriber (cross-process test)")
    print("=" * 60)
    print(f"  hostname:    {hostname}")
    print(f"  domain_id:   {args.domain}")
    print(f"  topic:       {args.topic}")
    print(f"  type:        HelloWorld (index: u32, message: string)")
    print(f"  reliability: {reliability_str}")
    print(f"  durability:  {durability_str}")
    print(f"  history:     {history_str}")
    print(f"  deadline:    {deadline_str}")
    print(f"  ownership:   {ownership_str}")
    print(f"  partition:   {partition_str}")
    print(f"  timeout:     {args.timeout_count} consecutive timeouts to exit")
    print("=" * 60)

    reader_qos = DataReaderQos(
        reliability=Reliability(reliability_str),
        durability=Durability(durability_str),
        history=History("KEEP_ALL" if args.keep == 0 else "KEEP_LAST", depth=args.keep),
    )
    if args.deadline is not None:
        reader_qos.deadline = Deadline(period=args.deadline / 1000.0)
    if args.ownership:
        reader_qos.ownership = Ownership("EXCLUSIVE")

    subscriber_qos = None
    if args.partition:
        subscriber_qos = SubscriberQos(partition=Partition(names=[args.partition]))

    with DomainParticipant(domain_id=args.domain, name="PythonSubscriber") as dp:
        print(f"[INFO] Created DomainParticipant on domain {args.domain}")

        topic = dp.create_topic(args.topic, HelloWorld)
        print(f"[INFO] Created topic: {topic.name} (type: {topic.type_name})")

        sub = Subscriber(dp, qos=subscriber_qos) if subscriber_qos else dp.create_subscriber()
        reader = sub.create_datareader(topic, qos=reader_qos)
        print(f"[INFO] Created DataReader (QoS: {reliability_str}, {durability_str})")

        # Wait for publisher discovery
        status_cond = reader.get_statuscondition()
        status_cond.set_enabled_statuses(STATUS_SUBSCRIPTION_MATCHED)

        print("[INFO] Waiting for publisher...")
        waitset = WaitSet()
        waitset.attach(status_cond)

        while reader.matched_writers == 0:
            try:
                waitset.wait(timeout=1.0)
            except DdsTimeout:
                pass

        print(f"[MATCHED] Discovered {reader.matched_writers} writer(s)")

        # Switch to data reception mode
        status_cond.set_enabled_statuses(STATUS_DATA_AVAILABLE)

        # Receive samples
        print("[INFO] Waiting for data...")
        samples_received = 0
        timeout_count = 0

        try:
            while timeout_count < args.timeout_count:
                # Check for already-arrived data
                for sample in reader.take():
                    if sample.valid_data:
                        data = sample.data
                        samples_received += 1
                        print(
                            f"[SUB #{samples_received:04d}] "
                            f"index={data.index}, message='{data.message}'"
                        )
                    else:
                        print("[SUB] Received dispose/unregister notification")
                    timeout_count = 0

                try:
                    waitset.wait(timeout=2.0)

                    for sample in reader.take():
                        if sample.valid_data:
                            data = sample.data
                            samples_received += 1
                            print(
                                f"[SUB #{samples_received:04d}] "
                                f"index={data.index}, message='{data.message}'"
                            )
                        else:
                            print("[SUB] Received dispose/unregister notification")

                    timeout_count = 0

                except DdsTimeout:
                    timeout_count += 1
                    print(
                        f"[TIMEOUT] No data received "
                        f"({timeout_count}/{args.timeout_count})"
                    )

        except KeyboardInterrupt:
            print(f"\n[INFO] Interrupted.")

        print(f"[DONE] Received {samples_received} samples total.")


if __name__ == "__main__":
    main()
