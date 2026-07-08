#!/usr/bin/env python3
"""
QoS Profile Publisher Example (Python)

Loads QoS settings from an XML profile file (OMG <qos_library> syntax) and
publishes HelloWorld samples with the profile applied. int2dds auto-loads the
profiles named by the DDS_QOS_PROFILE environment variable when the participant
factory is first created; the profile marked is_default_profile="true" is then
applied to every entity created with default QoS.

This script points DDS_QOS_PROFILE at the shared qos_profiles.xml (the same file
the Rust example uses) unless it is already set. Override the file with --xml.

Usage:
    python qos_profile_publisher.py [--domain N] [--xml PATH]
"""

import argparse
import os
import sys
import time

sys.path.insert(0, os.path.join(os.path.dirname(__file__), ".."))

from hello_world_type import HelloWorld

DEFAULT_XML = os.path.normpath(
    os.path.join(
        os.path.dirname(__file__),
        "..", "..", "dds", "examples", "qos_profile", "xml", "qos_profiles.xml",
    )
)


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--domain", type=int, default=0)
    parser.add_argument("--xml", default=None, help="path to the QoS profile XML")
    args = parser.parse_args()

    # The factory reads DDS_QOS_PROFILE when it is first created, so set it
    # before any DomainParticipant is constructed. A pre-set value wins.
    if "DDS_QOS_PROFILE" not in os.environ:
        os.environ["DDS_QOS_PROFILE"] = args.xml or DEFAULT_XML

    # Imported after the env var is set (kept lazy for clarity, not required).
    from int2dds import DomainParticipant, WaitSet

    print("=== QoS Profile Publisher (Python) ===")
    print(f"Domain: {args.domain}")
    print(f"DDS_QOS_PROFILE: {os.environ['DDS_QOS_PROFILE']}")
    print("The default profile from the XML is applied to default-QoS entities.\n")

    with DomainParticipant(domain_id=args.domain, name="qos_profile_publisher") as dp:
        topic = dp.create_topic("hello_world_topic", HelloWorld)
        pub = dp.create_publisher()
        writer = pub.create_datawriter(topic)
        print("Created publisher and data writer (QoS from XML profile)")

        print("Waiting for subscriber...")
        waitset = WaitSet()
        waitset.attach(writer)
        while writer.matched_readers == 0:
            try:
                waitset.wait(timeout=1.0)
            except Exception:
                pass

        print(f"Matched {writer.matched_readers} reader(s)")

        for i in range(20):
            sample = HelloWorld(index=i, message=f"Hello from QoS profile (Python)! ({i})")
            writer.write(sample)
            print(f"Published: index={sample.index}, message='{sample.message}'")
            time.sleep(0.5)

        print("Done publishing")
    return 0


if __name__ == "__main__":
    sys.exit(main())
