#!/usr/bin/env python3
"""
int2DDS Python Shapes Demo for OMG DDS interoperability testing.

Output format matches the OMG dds-rtps interoperability_report.py expectations.
CLI options match the Rust shapes-demo-interoperability binary.

Usage:
    # Publisher
    python shapes_demo.py -P -t Square -c BLUE

    # Subscriber
    python shapes_demo.py -S -t Square

    # With interoperability_report.py
    python3 interoperability_report.py -P "python3 shapes_demo.py" -S shapes-demo-interoperability
"""

import argparse
import random
import signal
import sys
import os
import time

# Suppress int2DDS internal logs BEFORE importing int2dds
# (DLL reads env vars at load time, so this must be set before import)
os.environ["INT2DDS_LOG_TYPE"] = "none"

sys.path.insert(0, os.path.join(os.path.dirname(__file__), "..", ".."))

from shape_type import ShapeType, set_encoding
from int2dds import (
    DataReaderQos,
    Deadline,
    DataRepresentation,
    DataWriterQos,
    DomainParticipant,
    Durability,
    History,
    Lifespan,
    Ownership,
    OwnershipStrength,
    Partition,
    Reliability,
    Subscriber,
    SubscriberQos,
    Publisher,
    PublisherQos,
)
from int2dds.core.listeners import DataWriterListenerBase, DataReaderListenerBase

# Display area dimensions matching Rust shapes-demo
DA_WIDTH = 240
DA_HEIGHT = 270

# DDS QoS Policy ID to name mapping (OMG DDS spec Table 7.82)
_QOS_POLICY_NAMES = {
    1: "Durability",
    2: "Presentation",
    3: "Deadline",
    4: "LatencyBudget",
    5: "Ownership",
    6: "OwnershipStrength",
    7: "Liveliness",
    8: "TimeBasedFilter",
    9: "Partition",
    10: "Reliability",
    11: "DestinationOrder",
    12: "History",
    13: "ResourceLimits",
    14: "EntityFactory",
    15: "WriterDataLifecycle",
    16: "ReaderDataLifecycle",
    17: "TopicData",
    18: "GroupData",
    19: "TransportPriority",
    20: "Lifespan",
    21: "DurabilityService",
    23: "DataRepresentation",
}


def _qos_policy_name(policy_id: int) -> str:
    return _QOS_POLICY_NAMES.get(policy_id, "Unknown")


_DURABILITY_MAP = {
    "v": "VOLATILE",
    "l": "TRANSIENT_LOCAL",
    "t": "TRANSIENT",
    "p": "PERSISTENT",
}


class ShapesWriterListener(DataWriterListenerBase):
    """Writer listener matching Rust PubListener output format."""

    def __init__(self, topic_name: str):
        super().__init__()
        self._topic_name = topic_name

    def on_publication_matched(self, writer, status) -> None:
        print(
            f"on_publication_matched() topic: '{self._topic_name}'  "
            f"type: 'ShapeType' : matched readers {status.current_count} "
            f"(change = {status.current_count_change})",
            flush=True,
        )

    def on_offered_incompatible_qos(self, writer, status) -> None:
        print(
            f"on_offered_incompatible_qos() topic: '{self._topic_name}'  "
            f"type: 'ShapeType' : {status.last_policy_id} ({_qos_policy_name(status.last_policy_id)})",
            flush=True,
        )

    def on_offered_deadline_missed(self, writer, status) -> None:
        print(
            f"on_offered_deadline_missed() topic: '{self._topic_name}'  "
            f"type: 'ShapeType' : (total = {status.total_count}, "
            f"change = {status.total_count_change})",
            flush=True,
        )

    def on_liveliness_lost(self, writer, status) -> None:
        print(
            f"on_liveliness_lost() topic: '{self._topic_name}'  "
            f"type: 'ShapeType' : (total = {status.total_count}, "
            f"change = {status.total_count_change})",
            flush=True,
        )


class ShapesReaderListener(DataReaderListenerBase):
    """Reader listener matching Rust SubListener output format."""

    def __init__(self, topic_name: str):
        super().__init__()
        self._topic_name = topic_name

    def on_subscription_matched(self, reader, status) -> None:
        print(
            f"on_subscription_matched() topic: '{self._topic_name}'  "
            f"type: 'ShapeType' : matched writers {status.current_count} "
            f"(change = {status.current_count_change})",
            flush=True,
        )

    def on_requested_incompatible_qos(self, reader, status) -> None:
        print(
            f"on_requested_incompatible_qos() topic: '{self._topic_name}'  "
            f"type: 'ShapeType' : {status.last_policy_id} ({_qos_policy_name(status.last_policy_id)})",
            flush=True,
        )

    def on_requested_deadline_missed(self, reader, status) -> None:
        print(
            f"on_requested_deadline_missed() topic: '{self._topic_name}'  "
            f"type: 'ShapeType' : (total = {status.total_count}, "
            f"change = {status.total_count_change})",
            flush=True,
        )

    def on_liveliness_changed(self, reader, status) -> None:
        print(
            f"on_liveliness_changed() topic: '{self._topic_name}'  "
            f"type: 'ShapeType' : (alive = {status.alive_count}, "
            f"not_alive = {status.not_alive_count})",
            flush=True,
        )


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description="int2DDS Python Shapes Demo")
    parser.add_argument("-P", "--publisher", action="store_true", help="Publish samples")
    parser.add_argument("-S", "--subscriber", action="store_true", help="Subscribe samples")
    parser.add_argument("-d", type=int, default=0, help="Domain ID (default: 0)")
    parser.add_argument("-b", action="store_true", help="BEST_EFFORT reliability")
    parser.add_argument("-r", action="store_true", help="RELIABLE reliability (default)")
    parser.add_argument(
        "-k", type=int, default=-1,
        help="History depth: 0 = KEEP_ALL, -1 = default, N = KEEP_LAST(N)",
    )
    parser.add_argument("-f", type=int, default=0, help="Deadline interval in ms (0: OFF)")
    parser.add_argument(
        "-s", type=int, default=-1,
        help="Ownership strength (-1: SHARED, >=0: EXCLUSIVE)",
    )
    parser.add_argument("-t", type=str, default=None, help="Topic name")
    parser.add_argument("-c", type=str, default=None, help="Color (publisher: required)")
    parser.add_argument("-p", type=str, default=None, help="Partition name")
    parser.add_argument(
        "-D", type=str, default="v",
        help="Durability (v=volatile, l=transient-local, t=transient, p=persistent)",
    )
    parser.add_argument("-w", action="store_true", help="Print Publisher's samples")
    parser.add_argument("-z", type=int, default=20, help="Shapesize (0: increase, default: 20)")
    parser.add_argument(
        "-x", type=int, default=1,
        help="Data representation (1: XCDR, 2: XCDR2) (default: 1)",
    )
    parser.add_argument("-R", action="store_true", help="Use read() instead of take()")
    parser.add_argument(
        "--write-period", type=int, default=33,
        help="Write period in ms (default: 33)",
    )
    parser.add_argument(
        "--read-period", type=int, default=100,
        help="Read period in ms (default: 100)",
    )
    parser.add_argument(
        "--num-iterations", type=int, default=0,
        help="Number of iterations (0: infinite)",
    )
    parser.add_argument(
        "--num-instances", type=int, default=1,
        help="Number of instances a DataWriter writes (default: 1)",
    )
    parser.add_argument(
        "--additional-payload-size", type=int, default=0,
        help="Amount of bytes added to samples (default: 0)",
    )
    parser.add_argument(
        "--lifespan", type=int, default=0,
        help="Lifespan of a sample in ms (0: infinite, default: 0)",
    )
    parser.add_argument(
        "--size-modulo", type=int, default=0,
        help="If set, modulo applied to shapesize (default: 0, off)",
    )
    parser.add_argument(
        "--cft", type=str, default=None,
        help="ContentFilteredTopic filter expression (e.g., \"color = 'RED'\")",
    )
    return parser.parse_args()


def _get_durability(args: argparse.Namespace) -> Durability:
    """Map -D flag to Durability QoS."""
    kind = _DURABILITY_MAP.get(args.D, "VOLATILE")
    return Durability(kind)


def build_writer_qos(args: argparse.Namespace) -> DataWriterQos:
    """Build DataWriter QoS from CLI args."""
    if args.b:
        rel = Reliability("BEST_EFFORT")
    else:
        rel = Reliability("RELIABLE")

    dur = _get_durability(args)

    if args.k == 0:
        hist = History("KEEP_ALL")
    elif args.k > 0:
        hist = History("KEEP_LAST", depth=args.k)
    else:
        hist = History("KEEP_LAST", depth=1)

    data_rep = DataRepresentation("XCDR2" if args.x == 2 else "XCDR1")
    qos = DataWriterQos(reliability=rel, durability=dur, history=hist, data_representation=data_rep)

    if args.s >= 0:
        qos.ownership = Ownership("EXCLUSIVE")
        qos.ownership_strength = OwnershipStrength(args.s)

    if args.f > 0:
        qos.deadline = Deadline(period=args.f / 1000.0)

    if args.lifespan > 0:
        qos.lifespan = Lifespan(duration=args.lifespan / 1000.0)

    return qos


def build_reader_qos(args: argparse.Namespace) -> DataReaderQos:
    """Build DataReader QoS from CLI args."""
    if args.b:
        rel = Reliability("BEST_EFFORT")
    else:
        rel = Reliability("RELIABLE")

    dur = _get_durability(args)

    if args.k == 0:
        hist = History("KEEP_ALL")
    elif args.k > 0:
        hist = History("KEEP_LAST", depth=args.k)
    else:
        hist = History("KEEP_LAST", depth=1)

    data_rep = DataRepresentation("XCDR2" if args.x == 2 else "XCDR1")
    qos = DataReaderQos(reliability=rel, durability=dur, history=hist, data_representation=data_rep)

    if args.s >= 0:
        qos.ownership = Ownership("EXCLUSIVE")

    if args.f > 0:
        qos.deadline = Deadline(period=args.f / 1000.0)

    return qos


def run_publisher(args: argparse.Namespace) -> None:
    """Run shapes demo publisher — matches Rust run_publisher() flow."""
    color = args.c
    if color is None:
        print("warning: color was not specified, defaulting to \"BLUE\"", flush=True)
        color = "BLUE"
    topic_name = args.t or "Square"

    with DomainParticipant(domain_id=args.d) as dp:
        topic = dp.create_topic(topic_name, ShapeType)
        print(f"Create topic: {topic_name}", flush=True)

        publisher_qos = None
        if args.p:
            publisher_qos = PublisherQos(partition=Partition(names=[args.p]))

        pub = Publisher(dp, qos=publisher_qos) if publisher_qos else dp.create_publisher()

        writer_qos = build_writer_qos(args)
        writer_listener = ShapesWriterListener(topic_name)
        print(f"Create writer for topic: {topic_name} color: {color}", flush=True)
        writer = pub.create_datawriter(topic, qos=writer_qos, listener=writer_listener)

        # No explicit wait for match — start writing immediately like Rust shapes-demo.
        # on_publication_matched() is printed by the listener callback.

        # Initialize shape with random position and velocity (matching Rust)
        x = random.randint(0, DA_WIDTH - 1)
        y = random.randint(0, DA_HEIGHT - 1)
        xvel = random.randint(1, 5) * random.choice([-1, 1])
        yvel = random.randint(1, 5) * random.choice([-1, 1])
        shapesize = 0 if args.z == 0 else args.z
        write_period = args.write_period / 1000.0

        # Build additional payload
        additional_payload = b""
        if args.additional_payload_size > 0:
            payload = bytearray(args.additional_payload_size)
            payload[-1] = 255
            additional_payload = bytes(payload)

        iteration = 0

        try:
            while args.num_iterations == 0 or iteration < args.num_iterations:
                # Move shape (matching Rust bounce logic)
                x += xvel
                y += yvel

                if x < 0:
                    x = 0
                    xvel = -xvel
                if x > DA_WIDTH:
                    x = DA_WIDTH
                    xvel = -xvel
                if y < 0:
                    y = 0
                    yvel = -yvel
                if y > DA_HEIGHT:
                    y = DA_HEIGHT
                    yvel = -yvel

                if args.z == 0:
                    shapesize += 1
                    if args.size_modulo > 0:
                        shapesize = (shapesize % args.size_modulo) + 1

                # Write data for each instance
                for j in range(args.num_instances):
                    instance_color = f"{color}{j}" if args.num_instances > 1 and j > 0 else color
                    sample = ShapeType(
                        color=instance_color, x=x, y=y, shapesize=shapesize,
                        additional_payload_size=additional_payload,
                    )
                    writer.write(sample)

                    if args.w:
                        payload_info = ""
                        if args.additional_payload_size > 0 and additional_payload:
                            payload_info = f" {{{additional_payload[-1]}}}"
                        print(
                            f"{topic_name:<10} {instance_color:<10} {x:03d} {y:03d} [{shapesize}]{payload_info}",
                            flush=True,
                        )

                iteration += 1
                time.sleep(write_period)

        except KeyboardInterrupt:
            pass

    print("Done.")


def run_subscriber(args: argparse.Namespace) -> None:
    """Run shapes demo subscriber — matches Rust run_subscriber() flow."""
    topic_name = args.t or "Square"

    with DomainParticipant(domain_id=args.d) as dp:
        topic = dp.create_topic(topic_name, ShapeType)
        print(f"Create topic: {topic_name}", flush=True)

        subscriber_qos = None
        if args.p:
            subscriber_qos = SubscriberQos(partition=Partition(names=[args.p]))

        sub = Subscriber(dp, qos=subscriber_qos) if subscriber_qos else dp.create_subscriber()

        reader_qos = build_reader_qos(args)
        reader_listener = ShapesReaderListener(topic_name)

        # Determine the topic description (regular or content-filtered)
        topic_desc = topic
        if args.cft:
            cft = dp.create_contentfilteredtopic(
                f"{topic_name}_filtered",
                topic,
                args.cft,
                [],
            )
            topic_desc = cft

        if args.c:
            print(f"Create reader for topic: {topic_name} color: {args.c}", flush=True)
        else:
            print(f"Create reader for topic: {topic_name}", flush=True)
        reader = sub.create_datareader(topic_desc, qos=reader_qos, listener=reader_listener)

        read_period = args.read_period / 1000.0
        iteration = 0

        try:
            while args.num_iterations == 0 or iteration < args.num_iterations:
                time.sleep(read_period)

                samples = reader.read() if args.R else reader.take()
                for sample in samples:
                    if sample.valid_data:
                        data = sample.data
                        if args.c and data.color != args.c:
                            continue
                        payload_info = ""
                        if data.additional_payload_size:
                            payload_info = f" {{{data.additional_payload_size[-1]}}}"
                        print(
                            f"{topic_name:<10} {data.color:<10} "
                            f"{data.x:03d} {data.y:03d} [{data.shapesize}]{payload_info}",
                            flush=True,
                        )
                    # Note: invalid data (instance state changes) not printed
                    # because Python binding's Sample class lacks instance_state field.
                    # Rust prints NOT_ALIVE_NO_WRITERS/DISPOSED based on sample_info().instance_state.

                iteration += 1

        except KeyboardInterrupt:
            pass

    print("Done.")


def main() -> None:
    args = parse_args()

    # Set CDR encoding based on -x option (must be before any serialization)
    set_encoding(xcdr2=(args.x == 2))

    if not args.publisher and not args.subscriber:
        print("Error: specify -P (publisher) or -S (subscriber)", file=sys.stderr)
        sys.exit(1)

    # Handle SIGINT gracefully
    signal.signal(signal.SIGINT, lambda s, f: sys.exit(0))

    if args.publisher:
        run_publisher(args)
    else:
        run_subscriber(args)


if __name__ == "__main__":
    main()
