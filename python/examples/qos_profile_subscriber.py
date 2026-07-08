#!/usr/bin/env python3
"""
QoS Profile Subscriber Example (Python)

Companion to qos_profile_publisher.py. Loads QoS settings from an XML profile
file (OMG <qos_library> syntax) via the DDS_QOS_PROFILE environment variable
and subscribes to HelloWorld samples with the profile applied. The profile
marked is_default_profile="true" is applied to every entity created with
default QoS.

This script points DDS_QOS_PROFILE at the shared qos_profiles.xml (the same file
the Rust example uses) unless it is already set. Override the file with --xml.

Usage:
    python qos_profile_subscriber.py [--domain N] [--xml PATH]
"""

import argparse
import os
import sys

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

    from int2dds import DomainParticipant, DdsTimeout, WaitSet
    from int2dds.core.conditions import STATUS_DATA_AVAILABLE, STATUS_SUBSCRIPTION_MATCHED

    print("=== QoS Profile Subscriber (Python) ===")
    print(f"Domain: {args.domain}")
    print(f"DDS_QOS_PROFILE: {os.environ['DDS_QOS_PROFILE']}")
    print("The default profile from the XML is applied to default-QoS entities.\n")

    with DomainParticipant(domain_id=args.domain, name="qos_profile_subscriber") as dp:
        topic = dp.create_topic("hello_world_topic", HelloWorld)
        sub = dp.create_subscriber()
        reader = sub.create_datareader(topic)
        print("Created subscriber and data reader (QoS from XML profile)")

        status_cond = reader.get_statuscondition()
        status_cond.set_enabled_statuses(STATUS_SUBSCRIPTION_MATCHED)

        print("Waiting for publisher...")
        waitset = WaitSet()
        waitset.attach(status_cond)
        while reader.matched_writers == 0:
            try:
                waitset.wait(timeout=1.0)
            except DdsTimeout:
                pass

        print(f"Matched {reader.matched_writers} writer(s)")

        status_cond.set_enabled_statuses(STATUS_DATA_AVAILABLE)

        print("Waiting for data...")
        samples_received = 0
        timeout_count = 0

        while timeout_count < 3:
            for sample in reader.take():
                if sample.valid_data:
                    data = sample.data
                    print(f"Received: index={data.index}, message='{data.message}'")
                    samples_received += 1
                timeout_count = 0

            try:
                waitset.wait(timeout=2.0)
                for sample in reader.take():
                    if sample.valid_data:
                        data = sample.data
                        print(f"Received: index={data.index}, message='{data.message}'")
                        samples_received += 1
                timeout_count = 0
            except DdsTimeout:
                timeout_count += 1
                print(f"No data received (timeout {timeout_count}/3)")

        print(f"Done. Received {samples_received} samples.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
