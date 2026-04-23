#!/usr/bin/env python3
"""
Cross-process communication test: Python Publisher

Publishes HelloWorld samples for cross-process testing with Rust subscriber.
CLI options match Rust hello_world binary for easy pairing.

Usage:
    # Pair with Rust subscriber:
    #   Terminal 1: python cross_test_pub.py -T hello_world_topic
    #   Terminal 2: hello_world.exe -S -T hello_world_topic
    #
    # Reliable QoS:
    #   Terminal 1: python cross_test_pub.py -r -T hello_world_topic
    #   Terminal 2: hello_world.exe -S -r -T hello_world_topic

    python cross_test_pub.py [OPTIONS]
"""

import argparse
import platform
import sys
import os
import time

sys.path.insert(0, os.path.join(os.path.dirname(__file__), "..", ".."))

from hello_world_type import HelloWorld
from int2dds import (
    DataWriterQos,
    Deadline,
    DdsTimeout,
    DomainParticipant,
    Durability,
    History,
    Ownership,
    OwnershipStrength,
    Partition,
    Publisher,
    PublisherQos,
    Reliability,
    WaitSet,
)


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="int2DDS Python Publisher (cross-process test)",
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
        "-i", "--interval", type=int, default=1000,
        help="Publish interval in ms (default: 1000)",
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
        "-n", "--count", type=int, default=0,
        help="Number of samples to publish (0 = infinite, default: 0)",
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
        "-o", "--ownership", type=int, nargs="?", const=1, default=None,
        help="Use exclusive ownership with strength (default: shared)",
    )
    parser.add_argument(
        "-p", "--partition", type=str, default=None,
        help="Partition name",
    )
    return parser.parse_args()


def main() -> None:
    args = parse_args()

    reliability_str = "RELIABLE" if args.reliable else "BEST_EFFORT"
    durability_str = "TRANSIENT_LOCAL" if args.transient_local else "VOLATILE"
    history_str = "KEEP_ALL" if args.keep == 0 else f"KEEP_LAST({args.keep})"
    deadline_str = f"{args.deadline}ms" if args.deadline is not None else "INFINITE"
    ownership_str = f"EXCLUSIVE(strength={args.ownership})" if args.ownership is not None else "SHARED"
    partition_str = args.partition or "(none)"
    hostname = platform.node()

    print("=" * 60)
    print("  int2DDS Python Publisher (cross-process test)")
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
    print(f"  interval:    {args.interval}ms")
    print(f"  count:       {'infinite' if args.count == 0 else args.count}")
    print("=" * 60)

    writer_qos = DataWriterQos(
        reliability=Reliability(reliability_str),
        durability=Durability(durability_str),
        history=History("KEEP_ALL" if args.keep == 0 else "KEEP_LAST", depth=args.keep),
    )
    if args.deadline is not None:
        writer_qos.deadline = Deadline(period=args.deadline / 1000.0)
    if args.ownership is not None:
        writer_qos.ownership = Ownership("EXCLUSIVE")
        writer_qos.ownership_strength = OwnershipStrength(args.ownership)

    publisher_qos = None
    if args.partition:
        publisher_qos = PublisherQos(partition=Partition(names=[args.partition]))

    with DomainParticipant(domain_id=args.domain, name="PythonPublisher") as dp:
        print(f"[INFO] Created DomainParticipant on domain {args.domain}")

        topic = dp.create_topic(args.topic, HelloWorld)
        print(f"[INFO] Created topic: {topic.name} (type: {topic.type_name})")

        pub = Publisher(dp, qos=publisher_qos) if publisher_qos else dp.create_publisher()
        writer = pub.create_datawriter(topic, qos=writer_qos)
        print(f"[INFO] Created DataWriter (QoS: {reliability_str}, {durability_str})")

        # Wait for subscriber discovery
        print("[INFO] Waiting for subscriber...")
        waitset = WaitSet()
        waitset.attach(writer)

        while writer.matched_readers == 0:
            try:
                waitset.wait(timeout=1.0)
            except (DdsTimeout, Exception):
                pass

        print(f"[MATCHED] Discovered {writer.matched_readers} reader(s)")

        # Publish samples
        i = 1
        interval_sec = args.interval / 1000.0
        try:
            while args.count == 0 or i <= args.count:
                msg = f"[{hostname}]HelloWorld_{reliability_str.lower()}_d{args.domain}"
                sample = HelloWorld(index=i, message=msg)
                writer.write(sample)
                print(f"[PUB #{i:04d}] index={sample.index}, message='{sample.message}'")
                time.sleep(interval_sec)
                i += 1
        except KeyboardInterrupt:
            print(f"\n[INFO] Interrupted. Published {i - 1} samples.")
            return

        print(f"[DONE] Published {i - 1} samples.")


if __name__ == "__main__":
    main()
