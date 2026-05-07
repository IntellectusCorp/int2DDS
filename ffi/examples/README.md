# int2dds FFI Examples

This directory contains C examples demonstrating how to use the int2dds FFI bindings.

## Directory Structure

```
examples/
├── CMakeLists.txt          # Build configuration
├── README.md               # This file
├── hello_world/            # Basic pub-sub examples
│   ├── hello_world_publisher.c
│   ├── hello_world_subscriber.c
│   ├── hello_world_with_ttl_publisher.c
│   ├── hello_world_with_ttl_subscriber.c
│   ├── waitset_publisher.c
│   ├── waitset_subscriber.c
│   ├── keyed_publisher.c
│   └── keyed_subscriber.c
├── listener/               # Listener callback examples
│   ├── listener_publisher.c
│   └── listener_subscriber.c
└── multiple_participant/   # Multi-participant examples
    ├── multi_pub_sub.c
    ├── multi_participant_1.c
    └── multi_participant_2.c
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
# Debug
cmake ..
cmake --build .
# or cmake --build . --config Debug

#Release
cmake .. -DCMAKE_BUILD_TYPE=Release
cmake --build . --config Release
# or cmake --build .
```

**Note:** Visual Studio is a multi-configuration generator, so you must specify `--config Debug` or `--config Release` when building.

### Linux/macOS

```bash
cd ffi/examples
mkdir build && cd build
cmake ..
make
```

## Running Examples

**Windows Note:** Executables are in `build/Debug` or `build/Release` directory depending on your build configuration.

**Linux/macOS Note:** Executables are directly in the `build` directory.

### Hello World Examples

Basic publisher-subscriber communication between two processes.

**Best Effort QoS (default)**

Terminal 1:
```bash
# Windows
cd build/Debug
./hello_world_subscriber

# Linux/macOS
cd build
./hello_world_subscriber
```

Terminal 2:
```bash
# Windows
cd build/Debug
./hello_world_publisher

# Linux/macOS
cd build
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

**Hello World with Multicast TTL**

`hello_world_with_ttl_publisher` and `hello_world_with_ttl_subscriber` mirror
the basic hello_world pair but apply `int2dds.transport.UDPv4.multicast_ttl`
via `PropertyQosPolicy`. The `INT2DDS_MULTICAST_TTL` env var serves as a
fallback when no QoS entry is set.

```bash
# Windows PowerShell
$env:INT2DDS_MULTICAST_TTL = "32"
./hello_world_with_ttl_subscriber
./hello_world_with_ttl_publisher

# Linux/macOS (Git Bash on Windows works the same)
INT2DDS_MULTICAST_TTL=32 ./hello_world_with_ttl_subscriber
INT2DDS_MULTICAST_TTL=32 ./hello_world_with_ttl_publisher
```

See [docs/guide/env.md](../../docs/guide/env.md#int2dds_multicast_ttl) for the
full TTL configuration reference.

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

### Listener Examples

Demonstrates event-driven callbacks for DataReader and DataWriter.

**Basic Usage:**

Terminal 1 (Subscriber with callbacks):
```bash
# Windows
cd build/Debug
./listener_subscriber

# Linux/macOS
cd build
./listener_subscriber
```

Terminal 2 (Publisher with callbacks):
```bash
# Windows
cd build/Debug
./listener_publisher

# Linux/macOS
cd build
./listener_publisher
```

**What happens:**
- Publisher's `on_publication_matched` callback fires when subscriber connects/disconnects
- Subscriber's `on_subscription_matched` callback fires when publisher connects/disconnects
- Subscriber's `on_data_available` callback fires automatically when data arrives
- No polling needed - callbacks handle everything!

**Options:**
- `--domain N`: Set domain ID (default: 0)
- `--wait N`: Subscriber wait time in seconds (default: 30)
- `--best-effort`: Use BEST_EFFORT QoS instead of RELIABLE (default: reliable)

**Example:**
```bash
./listener_subscriber --domain 5 --wait 60
./listener_publisher --domain 5
```

**Key Features:**
- Event-driven architecture (no polling)
- User context example (message counter)
- Thread-safe callbacks from DDS background threads
- Demonstrates discovery events (publication/subscription matched)

## Data Format

The examples use a simple serialization format:
- 4 bytes: index (uint32_t, little-endian)
- N bytes: null-terminated message string

This is a simplified format for demonstration. In production, you would use CDR (Common Data Representation) serialization for DDS interoperability.

## Notes

- **Build Configuration:** On Windows, remember to use `--config Debug` or `--config Release` when building
- **QoS Matching:** Make sure publisher and subscriber use matching QoS settings (reliability, durability, etc.)
- **Hello World Examples:**
  - Subscriber waits for 100 messages before exiting
  - Publisher sends 100 messages with 1-second intervals
- **Multi-Participant Example:** Uses Reliable QoS for guaranteed delivery
- **Listener Examples:**
  - Callbacks are invoked from DDS background threads - must be thread-safe
  - User context pointer must remain valid until entity deletion
  - Do not delete entities inside their own callbacks
  - Callbacks should return quickly to avoid blocking DDS internal operations
