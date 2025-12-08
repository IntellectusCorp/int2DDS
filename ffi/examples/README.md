# int2dds FFI Examples

This directory contains C examples demonstrating how to use the int2dds FFI bindings.

## Directory Structure

```
examples/
├── CMakeLists.txt          # Build configuration
├── README.md               # This file
├── hello_world/            # Basic pub-sub examples
│   ├── hello_world_publisher.c
│   └── hello_world_subscriber.c
└── multiple_participant/   # Multi-participant examples
    └── multi_pub_sub.c
```

## Prerequisites

1. Build the int2dds-ffi library first:
   ```bash
   cargo build --package int2dds-ffi
   ```

2. Install CMake (3.10 or later)

3. Install a C compiler (MSVC on Windows, GCC/Clang on Linux/macOS)

## Building Examples

### Windows (Visual Studio)

```powershell
cd ffi/examples
mkdir build
cd build
cmake ..
cmake --build .
```

### Linux/macOS

```bash
cd ffi/examples
mkdir build && cd build
cmake ..
make
```

## Running Examples

### Hello World Examples

Basic publisher-subscriber communication between two processes.

**Best Effort QoS (default)**

Terminal 1:
```bash
./hello_world_subscriber
```

Terminal 2:
```bash
./hello_world_publisher
```

**Reliable QoS**

Terminal 1:
```bash
./hello_world_subscriber --reliable
```

Terminal 2:
```bash
./hello_world_publisher --reliable
```

**Custom Domain ID**

```bash
./hello_world_publisher --domain 10
./hello_world_subscriber --domain 10
```

### Multiple Participant Example

Demonstrates two DomainParticipants communicating in the same process.

```bash
./multi_pub_sub
```

**Options:**
- `--domain N`: Set domain ID (default: 0)
- `--count N`: Number of messages to send (default: 10)

**Example:**
```bash
./multi_pub_sub --domain 5 --count 20
```

This example creates:
- Participant 1: Publisher that sends messages
- Participant 2: Subscriber that receives messages

Both participants run in the same process, demonstrating intra-process DDS communication.

## Data Format

The examples use a simple serialization format:
- 4 bytes: index (uint32_t, little-endian)
- N bytes: null-terminated message string

This is a simplified format for demonstration. In production, you would use CDR (Common Data Representation) serialization for DDS interoperability.

## Notes

- Make sure publisher and subscriber use matching QoS settings
- The hello_world subscriber will wait for 100 messages before exiting
- The hello_world publisher sends 100 messages with 1-second intervals
- The multi_pub_sub example uses Reliable QoS for guaranteed delivery
