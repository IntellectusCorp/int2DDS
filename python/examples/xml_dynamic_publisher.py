#!/usr/bin/env python3
"""
XML Dynamic Type Publisher (Python)

Loads a type defined in XML at runtime — the same XmlTypeRegistry workflow as
the Rust and C examples — and publishes it without any compile-time IDL. The
companion xml_dynamic_subscriber.py loads the same XML and receives it.

Usage:
    python xml_dynamic_publisher.py [--domain N] [--xml PATH] [--type NAME]
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
    parser.add_argument("--seconds", type=float, default=15.0, help="publish duration")
    args = parser.parse_args()

    print("=== XML Dynamic Type Publisher (Python) ===")
    print(f"Domain: {args.domain}")
    print(f"XML : {args.xml}")
    print(f"Type: {args.type_name}\n")

    registry = XmlTypeRegistry.from_file(args.xml)
    support = registry.get_type_support(args.type_name)

    with DomainParticipant(domain_id=args.domain, name="xml_dynamic_publisher") as dp:
        topic = dp.create_topic_dynamic("SensorTopic", support)
        pub = dp.create_publisher()
        writer = pub.create_datawriter_dynamic(topic, support)

        print("Waiting for a subscriber to match...", flush=True)
        for _ in range(400):
            if writer.publication_matched_count() > 0:
                break
            time.sleep(0.05)

        deadline = time.monotonic() + args.seconds
        n = 0
        while time.monotonic() < deadline:
            data = support.create_data()
            data.set_i32("sensor_id", 42)
            data.set_f64("temperature", 23.5)
            data.set_f64("humidity", 48.0)
            writer.write(data)
            n += 1
            print(f"[SEND] #{n} sensor_id=42 temperature=23.5 humidity=48.0", flush=True)
            time.sleep(0.3)
    return 0


if __name__ == "__main__":
    sys.exit(main())
