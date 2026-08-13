# Environment Variables

This document describes the environment variables available in int2dds.

## Environment Variables Summary

| Environment Variable                 | Description                                | Default                 |
| ------------------------------------ | ------------------------------------------ | ----------------------- |
| `INT2DDS_TRANSPORT`                  | Transport protocol (udp, tcp, hybrid, shm) | udp                     |
| `INT2DDS_LOG_TYPE`                   | Log output type (console, file, all, none) | none                    |
| `INT2DDS_CONSOLE_LOG_LEVEL`          | Console log level                          | info                    |
| `INT2DDS_FILE_LOG_LEVEL`             | File log level                             | info                    |
| `INT2DDS_NETWORK_INTERFACE`          | Network interface name                     | auto                    |
| `INT2DDS_NETWORK_IP`                 | Network IP address                         | auto                    |
| `INT2DDS_USE_LOOPBACK_INTERFACE`     | Enable loopback interface                  | false                   |
| `INT2DDS_FORCE_LOOPBACK_MULTICAST`   | Force multicast egress via 127.0.0.1       | false                   |
| `INT2DDS_UDP_SOCKET_BUFFER`          | UDP socket buffer size (bytes)             | OS default              |
| `INT2DDS_SHM_BUFFER_SIZE`            | Shared-memory ring buffer size (bytes)     | 1048576 (1MB)           |
| `INT2DDS_DATA_FRAG_SIZE`             | DATA_FRAG fragment size (bytes)            | 65000                   |
| `INT2DDS_MAX_MESSAGE_SIZE`           | Max RTPS message size (bytes)              | 65000                   |
| `INT2DDS_MULTICAST_TTL`              | IPv4 multicast TTL fallback (0-255)        | 1                       |
| `INT2DDS_DISABLE_PREEMPTIVE`         | Disable preemptive ACKNACK/HEARTBEAT       | false                   |
| `INT2DDS_EXTENDED_DISCOVERY`         | Enable extended discovery                  | false                   |
| `INT2DDS_INITIAL_PEERS`              | Initial peer list                          | none                    |
| `INT2DDS_THREAD_MONITORING`          | Enable thread monitoring                   | false                   |
| `INT2DDS_THREAD_MONITORING_LOG_PATH` | Thread monitoring log path                 | ./thread_monitoring.log |
| `INT2DDS_FUNCTION_TIMING`            | Enable function timing                     | false                   |
| `INT2DDS_FUNCTION_TIMING_LOG_PATH`   | Function timing log path                   | ./function_timing.log   |
| `INT2DDS_EXTERNAL_ADDRESS`           | Public IPv4 advertised in SPDP (NAT/WAN)   | none                    |
| `INT2DDS_META_PORT`                  | Pinned metatraffic unicast port            | RTPS standard           |
| `INT2DDS_USER_PORT`                  | Pinned user-traffic unicast port           | RTPS standard           |

---

## Transport Settings

### INT2DDS_TRANSPORT

Sets the transport protocol type.

| Value    | Description                                      |
| -------- | ------------------------------------------------ |
| `udp`    | UDP transport (default)                          |
| `tcp`    | TCP transport                                    |
| `hybrid` | UDP + TCP simultaneous use                       |
| `shm`    | Shared memory for high-performance intra-host    |

#### Configuration

```powershell
# Windows PowerShell - Environment variable
$env:INT2DDS_TRANSPORT = "tcp"

cargo run --example hello_world_pub
```

```bash
# Linux/macOS - Environment variable
export INT2DDS_TRANSPORT=tcp

cargo run --example hello_world_pub
```

---

## Logging Settings

### INT2DDS_LOG_TYPE

Sets the log output type.

| Value     | Description               |
| --------- | ------------------------- |
| `none`    | Disable logging (default) |
| `console` | Console output only       |
| `file`    | File output only          |
| `all`     | Console + file output     |

#### Configuration

```powershell
# Windows PowerShell
$env:INT2DDS_LOG_TYPE = "console"

cargo run --example hello_world_pub
```

```bash
# Linux/macOS
export INT2DDS_LOG_TYPE=console

cargo run --example hello_world_pub
```

### INT2DDS_CONSOLE_LOG_LEVEL

Sets the console log level.

| Value   | Description              |
| ------- | ------------------------ |
| `error` | Error only               |
| `warn`  | Warning and above        |
| `info`  | Info and above (default) |
| `debug` | Debug and above          |
| `trace` | All logs                 |

#### Configuration

```powershell
# Windows PowerShell
$env:INT2DDS_CONSOLE_LOG_LEVEL = "debug"

cargo run --example hello_world_pub
```

```bash
# Linux/macOS
export INT2DDS_CONSOLE_LOG_LEVEL=debug

cargo run --example hello_world_pub
```

### INT2DDS_FILE_LOG_LEVEL

Sets the file log level. Values are the same as `INT2DDS_CONSOLE_LOG_LEVEL`.

#### Configuration

```powershell
# Windows PowerShell
$env:INT2DDS_FILE_LOG_LEVEL = "trace"

cargo run --example hello_world_pub
```

```bash
# Linux/macOS
export INT2DDS_FILE_LOG_LEVEL=trace

cargo run --example hello_world_pub
```

---

## Network Settings

### INT2DDS_NETWORK_INTERFACE

Specifies the network interface name to use (e.g., eth0, wlan0, en0).
If not specified, all available interfaces will be used.

#### Configuration

```powershell
# Windows PowerShell
$env:INT2DDS_NETWORK_INTERFACE = "Ethernet"

cargo run --example hello_world_pub
```

```bash
# Linux/macOS
export INT2DDS_NETWORK_INTERFACE=eth0

cargo run --example hello_world_pub
```

### INT2DDS_NETWORK_IP

Directly specifies the network IP address to use.
If not specified, all available addresses will be used.

#### Configuration

```powershell
# Windows PowerShell
$env:INT2DDS_NETWORK_IP = "192.168.1.100"

cargo run --example hello_world_pub
```

```bash
# Linux/macOS
export INT2DDS_NETWORK_IP=192.168.1.100

cargo run --example hello_world_pub
```

### INT2DDS_USE_LOOPBACK_INTERFACE

Enables the loopback interface for endpoint communication. <br>
When enabled, the loopback address (127.0.0.1) is added to the available IP address list.

This is useful when the NIC may go down during communication—previously matched entities can continue exchanging data via loopback.
However, new participants will not be discovered since loopback multicast discovery is still not supported in int2DDS.

#### Interaction with Other Settings

- When int2DDS-feature provides the working IP, this setting is ignored. The feature-specified NIC takes full control of the network interface selection.
- If no network interfaces are available (e.g., WiFi and Ethernet disconnected), loopback is automatically used without setting this variable.

#### Configuration

```powershell
# Windows PowerShell - Environment variable
$env:INT2DDS_USE_LOOPBACK_INTERFACE = "true"

cargo run --example hello_world_pub
```

```bash
# Linux/macOS - Environment variable
export INT2DDS_USE_LOOPBACK_INTERFACE=true

cargo run --example hello_world_pub
```

### INT2DDS_FORCE_LOOPBACK_MULTICAST

Forces multicast egress to go out through the loopback interface (127.0.0.1) instead of the interface
resolved from the OS routing table. Intended for local-only testing, where every participant runs on the
same host and multicast traffic must not leave the machine.

#### Interaction with Other Settings

- When int2DDS-feature provides the working IP, this setting is ignored. The feature-specified NIC takes full control of the network interface selection.
- Use together with `INT2DDS_USE_LOOPBACK_INTERFACE=true`. The multicast group is joined on each working IP, so 127.0.0.1 must be in the working IP list for the loopback-sent multicast to be received.

#### Configuration

```powershell
# Windows PowerShell - Environment variable
$env:INT2DDS_USE_LOOPBACK_INTERFACE = "true"
$env:INT2DDS_FORCE_LOOPBACK_MULTICAST = "true"

cargo run --example hello_world_pub
```

```bash
# Linux/macOS - Environment variable
export INT2DDS_USE_LOOPBACK_INTERFACE=true
export INT2DDS_FORCE_LOOPBACK_MULTICAST=true

cargo run --example hello_world_pub
```

### INT2DDS_UDP_SOCKET_BUFFER

Sets the UDP socket buffer size in bytes. Default uses OS default value.

#### Configuration

```powershell
# Windows PowerShell
$env:INT2DDS_UDP_SOCKET_BUFFER = "1048576"  # 1MB

cargo run --example hello_world_pub
```

```bash
# Linux/macOS
export INT2DDS_UDP_SOCKET_BUFFER=1048576

cargo run --example hello_world_pub
```

### INT2DDS_SHM_BUFFER_SIZE

Sets the shared-memory transport's ring buffer size in bytes. Only applies when
the SHM transport is in use. Default: 1048576 (1MB).

#### Configuration

```powershell
# Windows PowerShell
$env:INT2DDS_SHM_BUFFER_SIZE = "2097152"  # 2MB

cargo run --example hello_world_pub
```

```bash
# Linux/macOS
export INT2DDS_SHM_BUFFER_SIZE=2097152

cargo run --example hello_world_pub
```


### INT2DDS_DATA_FRAG_SIZE

Sets the RTPS DATA_FRAG fragment size, in bytes, used by DataWriters whose QoS
does not specify one. A `data_frag.max_size` of `1` or greater, set in code or in
a QoS profile, always wins over this fallback.

- Valid range: `1` - `65000`
- Values outside the range, or values that do not parse as an integer, are
  logged at warn level and ignored — the built-in default `65000` is used.
  The value is **not** clamped.

#### Resolution order

1. DataWriter QoS `data_frag.max_size` (code) / QoS profile entry, when `>= 1`
2. `INT2DDS_DATA_FRAG_SIZE` env var
3. Default `65000`

A `max_size` of `0` — the default — means "unspecified". Negative values are
treated the same way, so they do not override the env var. `int2dds_datawriter_qos_get_data_frag()`
and the Python/C# `data_frag` accessors return the value as set, not the resolved
size, so they report `0` for a QoS that never set it.

#### Configuration

```powershell
# Windows PowerShell
$env:INT2DDS_DATA_FRAG_SIZE = "8000"

cargo run --example hello_world_pub
```

```bash
# Linux/macOS
export INT2DDS_DATA_FRAG_SIZE=8000

cargo run --example hello_world_pub
```

The Rust core consumes the env var inside `DataFragQosPolicy::effective_max_size`
when the RTPS writer is created, so the value must be set **before** the
DataWriter is created.

### INT2DDS_MAX_MESSAGE_SIZE

Sets the maximum RTPS message size, in bytes: the threshold above which a
DataWriter fragments a sample. A sample whose serialized payload exceeds this
size is split into DATA_FRAG fragments of `INT2DDS_DATA_FRAG_SIZE` bytes each. A
sample at or below it is sent as a single DATA submessage.

`INT2DDS_DATA_FRAG_SIZE` sizes each fragment. This variable decides when
fragmentation starts and how much payload one message carries, so keep it at or
above `INT2DDS_DATA_FRAG_SIZE`.

- Valid range: `1` - `65000`
- Values outside the range, or values that do not parse as an integer, are
  logged at warn level and ignored — the built-in default `65000` is used.

#### Configuration

```powershell
# Windows PowerShell
$env:INT2DDS_MAX_MESSAGE_SIZE = "14720"

cargo run --example hello_world_pub
```

```bash
# Linux/macOS
export INT2DDS_MAX_MESSAGE_SIZE=14720

cargo run --example hello_world_pub
```

The Rust core reads this via `env::get_max_message_size` each time a sample is
written, so a new value takes effect for samples written afterward.

### INT2DDS_MULTICAST_TTL

Sets the IPv4 multicast TTL (Time-To-Live) used when no explicit
`int2dds.transport.UDPv4.multicast_ttl` `PropertyQosPolicy` entry is present.
Explicit code or JSON-profile QoS settings always win over this fallback.

| Value   | Description                                          |
| ------- | ---------------------------------------------------- |
| `0`     | Restricted to the local host (no network forwarding) |
| `1`     | Same subnet only (default, RFC 1112 link-local)      |
| `2-255` | Cross-router multicast hop limit                     |

#### Resolution order

1. `property.set_multicast_ttl(N)` (code) / JSON profile entry
2. `INT2DDS_MULTICAST_TTL` env var
3. Default `1`

#### Configuration

```powershell
# Windows PowerShell
$env:INT2DDS_MULTICAST_TTL = "32"

cargo run --example hello_world_pub
```

```bash
# Linux/macOS
export INT2DDS_MULTICAST_TTL=32

cargo run --example hello_world_pub
```

#### Language bindings

```python
from int2dds import env
env.set_multicast_ttl(32)
```

```csharp
Int2Dds.Core.Env.SetMulticastTtl(32);
```

The Rust core consumes the env var inside `TransportConfig::from_property` at
participant creation, so the value must be set **before** the first
`DomainParticipant` is created.

### INT2DDS_DISABLE_PREEMPTIVE

Disables the two one-shot messages sent when a new endpoint match is made: the
reader's preemptive ACKNACK and the writer's preemptive HEARTBEAT. Accepts
`true`/`false`/`1`/`0`, case-insensitive; anything else is logged at warn level
and treated as `false`.

Both remain enabled by default. Disabling them removes discovery-time traffic at
the cost of first-sample latency: a reader then learns the writer's sequence
range from the first periodic HEARTBEAT instead of from an immediate one.
Reliability is unaffected — periodic HEARTBEAT and the normal
HEARTBEAT/ACKNACK repair loop still run, and a HEARTBEAT arriving from a peer is
still answered.

#### Configuration

```powershell
# Windows PowerShell
$env:INT2DDS_DISABLE_PREEMPTIVE = "true"

cargo run --example hello_world_sub
```

```bash
# Linux/macOS
export INT2DDS_DISABLE_PREEMPTIVE=true

cargo run --example hello_world_sub
```

The Rust core reads the env var when an endpoint match is made, so the value must
be set **before** the DataReader/DataWriter that should skip it is matched.

### INT2DDS_EXTENDED_DISCOVERY

Controls the ability to send DDS discovery messages via extended discovery.

#### Configuration

```powershell
# Windows PowerShell - Environment variable
$env:INT2DDS_EXTENDED_DISCOVERY = "true"

cargo run --example hello_world_pub
```

```bash
# Linux/macOS - Environment variable
export INT2DDS_EXTENDED_DISCOVERY=true

cargo run --example hello_world_pub
```

#### Behavior

- **When extended discovery is disabled** (default):

  - Discovery messages: Sent via multicast only
  - Send interval: 2 seconds

- **When extended discovery is enabled**:
  - Discovery messages: Sent via both multicast and extended discovery
  - Send interval: Same as multicast (TODO: needs adjustment)

#### Valid Values

- `"true"`, `"1"`: Enabled
- `"false"`, `"0"`, not set: Disabled (default)
- Case-insensitive

### INT2DDS_INITIAL_PEERS

Sets the initial peer list for SPDP unicast discovery. When set, SPDP messages are sent via unicast to these peers in addition to the default multicast. Works with all transport modes. Format is comma-separated socket addresses.

- Format: `ip:port,ip:port,...`
- The port must be the remote participant's **metatraffic unicast port** (discovery unicast port)
- Port calculation: `7400 + (250 * domain_id) + 10 + (2 * participant_id)`
  - `participant_id` is assigned sequentially starting from 0 for each participant created on the same host
- When the remote peer is behind NAT, point this at its `INT2DDS_EXTERNAL_ADDRESS:META_PORT` (see [NAT / WAN Traversal Settings](#nat--wan-traversal-settings))

#### Configuration

```powershell
# Windows PowerShell
$env:INT2DDS_INITIAL_PEERS = "192.168.1.100:17410,192.168.1.100:17412"

cargo run --example hello_world_pub
```

```bash
# Linux/macOS
export INT2DDS_INITIAL_PEERS="192.168.1.100:17410,192.168.1.100:17412"

cargo run --example hello_world_pub
```

---

## NAT / WAN Traversal Settings

When two int2DDS hosts need to communicate across the public Internet (WAN)
and at least one of them sits behind a NAT, the SPDP-advertised locator must
carry the *publicly reachable* address rather than the local NIC IP. LAN-only
deployments do not need any of these settings — the default behavior works.

| Variable                    | Effect                                        |
| --------------------------- | --------------------------------------------- |
| `INT2DDS_EXTERNAL_ADDRESS`  | Replaces the IP advertised in SPDP locators   |
| `INT2DDS_META_PORT`         | Pins the metatraffic unicast port             |
| `INT2DDS_USER_PORT`         | Pins the user-traffic unicast port            |
| `INT2DDS_INITIAL_PEERS`     | Sends SPDP via unicast to the listed peers    |

Sockets still bind to the local NIC IP (or `0.0.0.0`); only the *advertised*
address changes. When `INT2DDS_META_PORT` / `INT2DDS_USER_PORT` are set, the
RTPS standard port formula is bypassed entirely and `domain_id` no longer
affects the port — the value you provide is used directly for the first
participant on the host.

### INT2DDS_INITIAL_PEERS (required for WAN)

SPDP defaults to multicast, which does not cross the public Internet. Each
side must list the *other* side's `EXTERNAL_ADDRESS:META_PORT` so the initial
discovery packet is sent via unicast. Without this, no participant data
(DATA(p)) is ever exchanged. See the full description in
[INT2DDS_INITIAL_PEERS](#int2dds_initial_peers).

### INT2DDS_EXTERNAL_ADDRESS

Public IPv4 advertised in unicast SPDP locators in place of every local NIC
IP. When unset, every working NIC IP is advertised (default behavior).

- Format: a single IPv4 address (e.g. `203.0.113.50`)
- An invalid value is logged at error level and the default is used.

### INT2DDS_META_PORT / INT2DDS_USER_PORT

Pin the metatraffic and user-traffic unicast ports. When set, the RTPS
standard formula `7400 + 250*domain_id + 10/11 + 2*pid` is replaced with:

```
metatraffic_port(pid) = INT2DDS_META_PORT + 2 * pid
user_port(pid)        = INT2DDS_USER_PORT + 2 * pid
```

`pid` is the participant_id, assigned sequentially starting from 0 for each
DomainParticipant created in the same process.

#### Recommended: USER = META + 1

Set `INT2DDS_USER_PORT = INT2DDS_META_PORT + 1`. This mirrors the RTPS
standard offsets (`+10` for metatraffic, `+11` for user-traffic), so meta
comes first and user immediately follows. With N participants on the host,
every port used falls inside the single contiguous range:

```
[INT2DDS_META_PORT, INT2DDS_META_PORT + 2N - 1]
```

so a NAT router needs only **one range-forwarding rule**.

### Example: AWS EC2 talking to a remote peer at `198.51.100.7`

On the EC2 instance:

```bash
export INT2DDS_EXTERNAL_ADDRESS=3.34.X.Y
export INT2DDS_META_PORT=55000
export INT2DDS_USER_PORT=55001               # META + 1
export INT2DDS_INITIAL_PEERS=198.51.100.7:55000   # remote peer's META port
```

Security Group inbound rule: allow UDP `55000-55001` (or `55000-55009` for
up to 5 participants).

### Example: Home/office router talking to a remote peer at `3.34.X.Y`

```bash
export INT2DDS_EXTERNAL_ADDRESS=203.0.113.50
export INT2DDS_META_PORT=55000
export INT2DDS_USER_PORT=55001
export INT2DDS_INITIAL_PEERS=3.34.X.Y:55000       # remote peer's META port
```

NAT rule on the router (single host, single participant):

```
WAN 203.0.113.50:55000-55001 (UDP) -> LAN <private IP>:55000-55001
```

For N participants on the same host, widen the range to `55000` through
`55000 + 2N - 1`.

### Example: Multiple int2DDS processes on the same host

Environment variables are per-OS-process. Two int2DDS programs on the same
host read identical env values and would otherwise compete for the same port
range. Give each program its own META/USER values:

```bash
# Process A
export INT2DDS_META_PORT=55000
export INT2DDS_USER_PORT=55001
./my_app_a

# Process B (same host, separate shell/script)
export INT2DDS_META_PORT=56000
export INT2DDS_USER_PORT=56001
./my_app_b
```

Within a single process, multiple DomainParticipants are spaced automatically
by `2 * pid`, so you only need to plan port ranges *between* processes.


---

## Performance Monitoring

### INT2DDS_THREAD_MONITORING

Enables thread monitoring.

- Default: false

#### Configuration

```powershell
# Windows PowerShell - Environment variable
$env:INT2DDS_THREAD_MONITORING = "true"

cargo run --example hello_world_pub
```

```bash
# Linux/macOS - Environment variable
export INT2DDS_THREAD_MONITORING=true

cargo run --example hello_world_pub
```

### INT2DDS_THREAD_MONITORING_LOG_PATH

Sets the thread monitoring log file path.

- Default: `./thread_monitoring.log`

#### Configuration

```powershell
# Windows PowerShell
$env:INT2DDS_THREAD_MONITORING_LOG_PATH = "C:\logs\thread.log"

cargo run --example hello_world_pub
```

```bash
# Linux/macOS
export INT2DDS_THREAD_MONITORING_LOG_PATH=/var/log/thread_monitoring.log

cargo run --example hello_world_pub
```

### INT2DDS_FUNCTION_TIMING

Enables function execution time measurement.

- Default: false

#### Configuration

```powershell
# Windows PowerShell - Environment variable
$env:INT2DDS_FUNCTION_TIMING = "true"

cargo run --example hello_world_pub
```

```bash
# Linux/macOS - Environment variable
export INT2DDS_FUNCTION_TIMING=true

cargo run --example hello_world_pub
```

### INT2DDS_FUNCTION_TIMING_LOG_PATH

Sets the function timing log file path.

- Default: `./function_timing.log`

#### Configuration

```powershell
# Windows PowerShell
$env:INT2DDS_FUNCTION_TIMING_LOG_PATH = "C:\logs\timing.log"

cargo run --example hello_world_pub
```

```bash
# Linux/macOS
export INT2DDS_FUNCTION_TIMING_LOG_PATH=/var/log/function_timing.log

cargo run --example hello_world_pub
```

---

## Related Files

- [dds/src/common/env.rs](../../dds/src/common/env.rs) - Environment variable parsing and application
- [dds/src/common/log.rs](../../dds/src/common/log.rs) - Logging configuration
- [dds/src/rtps/transport/mod.rs](../../dds/src/rtps/transport/mod.rs) - Transport type definition
- [dds/src/rtps/transport/transport_config.rs](../../dds/src/rtps/transport/transport_config.rs) - Multicast TTL resolution
- [dds/src/dcps/infrastructure/qos_policy.rs](../../dds/src/dcps/infrastructure/qos_policy.rs) - DATA_FRAG fragment size resolution
- [dds/src/rtps/transport/udp/udp_sender.rs](../../dds/src/rtps/transport/udp/udp_sender.rs) - UDP transport settings
