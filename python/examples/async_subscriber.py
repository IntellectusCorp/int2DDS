#!/usr/bin/env python3
"""
Async HelloWorld Subscriber Example

Demonstrates async/await usage with int2dds Python bindings.

Usage:
    python async_subscriber.py
"""

import asyncio

from hello_world_type import HelloWorld

from int2dds import AsyncDataReader, DomainParticipant, DdsTimeout


async def main() -> None:
    print("=== Async HelloWorld Subscriber ===")

    with DomainParticipant(domain_id=0, name="AsyncPythonSubscriber") as dp:
        print(f"Created participant on domain {dp.domain_id}")

        topic = dp.create_topic("hello_world_topic", HelloWorld)
        print(f"Created topic: {topic.name} ({topic.type_name})")

        sub = dp.create_subscriber()
        reader = sub.create_datareader(topic)
        print("Created subscriber and data reader")

        # Wrap reader with async support
        async_reader = AsyncDataReader(reader)

        print("Waiting for publisher...")

        # Wait for publisher to connect
        while reader.matched_writers == 0:
            await asyncio.sleep(0.5)

        print(f"Matched {reader.matched_writers} writer(s)")

        # Receive samples using async iteration
        print("Waiting for data (Ctrl+C to stop)...")
        samples_received = 0
        timeout_count = 0

        try:
            while timeout_count < 3:
                # Wait for data with timeout
                has_data = await async_reader.wait_for_data(timeout=2.0)

                if has_data:
                    samples = await async_reader.take()
                    for sample in samples:
                        if sample.valid_data:
                            data = sample.data
                            print(f"Received: index={data.index}, message='{data.message}'")
                            samples_received += 1
                        else:
                            print("Received dispose/unregister notification")
                    timeout_count = 0
                else:
                    timeout_count += 1
                    print(f"No data received (timeout {timeout_count}/3)")

        except KeyboardInterrupt:
            print("\nInterrupted by user")
        finally:
            async_reader.close()

        print(f"Done. Received {samples_received} samples.")


if __name__ == "__main__":
    asyncio.run(main())
