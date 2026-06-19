#!/usr/bin/env python3
"""
XML Dynamic Type Subscriber (Python)

Loads a type defined in XML at runtime — the same XmlTypeRegistry workflow as
the Rust and C examples — and subscribes to it without any compile-time IDL.
The companion xml_dynamic_publisher.py loads the same XML and publishes it.

Usage:
    python xml_dynamic_subscriber.py [--domain N] [--xml PATH] [--type NAME]
"""

import argparse
import os
import sys
import time

sys.path.insert(0, os.path.join(os.path.dirname(__file__), ".."))

from int2dds import DomainParticipant
from int2dds.types import XmlTypeRegistry

DEFAULT_XML = os.path.join(
    os.path.dirname(__file__), "..", "..", "dds", "examples", "xtypes", "sensor_data.xml"
)


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--domain", type=int, default=0)
    parser.add_argument("--xml", default=DEFAULT_XML)
    parser.add_argument("--type", dest="type_name", default="SensorData")
    parser.add_argument("--timeout", type=float, default=25.0, help="receive timeout")
    args = parser.parse_args()

    print("=== XML Dynamic Type Subscriber (Python) ===")
    print(f"Domain: {args.domain}")
    print(f"XML : {args.xml}")
    print(f"Type: {args.type_name}\n")

    registry = XmlTypeRegistry.from_file(args.xml)
    support = registry.get_type_support(args.type_name)

    with DomainParticipant(domain_id=args.domain, name="xml_dynamic_subscriber") as dp:
        topic = dp.create_topic_dynamic("SensorTopic", support)
        sub = dp.create_subscriber()
        reader = sub.create_datareader_dynamic(topic, support)

        print("Waiting for a publisher and samples...", flush=True)
        deadline = time.monotonic() + args.timeout
        received_any = False
        while time.monotonic() < deadline:
            sample = reader.take()
            if sample is not None:
                received_any = True
                sid = sample.get_i32("sensor_id")
                temp = sample.get_f64("temperature")
                hum = sample.get_f64("humidity")
                print(f"[RECV] sensor_id={sid} temperature={temp:.1f} humidity={hum:.1f}", flush=True)
                deadline = time.monotonic() + args.timeout
            else:
                time.sleep(0.05)

    if not received_any:
        print("no sample received", file=sys.stderr, flush=True)
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
