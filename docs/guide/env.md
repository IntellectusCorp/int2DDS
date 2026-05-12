# Environment Variables

This document describes the environment variables available in int2dds. All environment variables can also be set via CLI arguments.

## Environment Variables Summary

| Environment Variable                 | CLI Argument                           | Description                                | Default                 |
| ------------------------------------ | -------------------------------------- | ------------------------------------------ | ----------------------- |
| `INT2DDS_TRANSPORT`                  | `--int2dds-transport`                  | Transport protocol (udp, tcp, hybrid, shm) | udp                     |
| `INT2DDS_DISCOVERY_MODE`             | `--int2dds-discovery-mode`             | Discovery mode (udp, tcp, hybrid)          | udp                     |
| `INT2DDS_LOG_TYPE`                   | `--int2dds-log-type`                   | Log output type (console, file, all, none) | none                    |
| `INT2DDS_CONSOLE_LOG_LEVEL`          | `--int2dds-console-log-level`          | Console log level                          | info                    |
| `INT2DDS_FILE_LOG_LEVEL`             | `--int2dds-file-log-level`             | File log level                             | info                    |
| `INT2DDS_NETWORK_INTERFACE`          | `--int2dds-network-interface`          | Network interface name                     | auto                    |
| `INT2DDS_NETWORK_IP`                 | `--int2dds-network-ip`                 | Network IP address                         | auto                    |
| `INT2DDS_USE_LOOPBACK_INTERFACE`     | `--int2dds-use-loopback-interface`     | Enable loopback interface                  | false                   |
| `INT2DDS_UDP_SOCKET_BUFFER`          | `--int2dds-udp-socket-buffer`          | UDP socket buffer size (bytes)             | OS default              |
| `INT2DDS_MULTICAST_TTL`              | `--int2dds-multicast-ttl`              | IPv4 multicast TTL fallback (0-255)        | 1                       |
| `INT2DDS_EXTENDED_DISCOVERY`         | `--int2dds-extended-discovery`         | Enable extended discovery                  | false                   |
| `INT2DDS_TCP_CONNECT_TIMEOUT`        | `--int2dds-tcp-connect-timeout`        | TCP connection timeout (ms)                | 5000                    |
| `INT2DDS_TCP_WRITE_TIMEOUT`          | `--int2dds-tcp-write-timeout`          | TCP write timeout (ms)                     | 10000                   |
| `INT2DDS_TCP_NODELAY`                | `--int2dds-tcp-nodelay`                | Enable TCP Nodelay                         | true                    |
| `INT2DDS_INITIAL_PEERS`              | `--int2dds-initial-peers`              | Initial peer list                          | none                    |
| `INT2DDS_THREAD_MONITORING`          | `--int2dds-thread-monitoring`          | Enable thread monitoring                   | false                   |
| `INT2DDS_THREAD_MONITORING_LOG_PATH` | `--int2dds-thread-monitoring-log-path` | Thread monitoring log path                 | ./thread_monitoring.log |
| `INT2DDS_FUNCTION_TIMING`            | `--int2dds-function-timing`            | Enable function timing                     | false                   |
| `INT2DDS_FUNCTION_TIMING_LOG_PATH`   | `--int2dds-function-timing-log-path`   | Function timing log path                   | ./function_timing.log   |
| `INT2DDS_EXTERNAL_ADDRESS`           | `--int2dds-external-address`           | Public IPv4 advertised in SPDP (NAT/WAN)   | none                    |
| `INT2DDS_META_PORT`                  | `--int2dds-meta-port`                  | Pinned metatraffic unicast port            | RTPS standard           |
| `INT2DDS_USER_PORT`                  | `--int2dds-user-port`                  | Pinned user-traffic unicast port           | RTPS standard           |

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

# CLI argument
cargo run --example hello_world -- --int2dds-transport tcp
```

```bash
# Linux/macOS - Environment variable
export INT2DDS_TRANSPORT=tcp

# CLI argument
cargo run --example hello_world -- --int2dds-transport tcp
```

### INT2DDS_DISCOVERY_MODE

Sets the DDS Participant Discovery mode.

| Value    | Discovery Method            | User Data Transport | Initial Peers Required |
| -------- | --------------------------- | ------------------- | ---------------------- |
| `udp`    | UDP multicast               | UDP                 | No                     |
| `tcp`    | TCP unicast                 | TCP                 | **Yes**                |
| `hybrid` | UDP multicast + TCP unicast | TCP                 | Optional               |

#### Configuration

```powershell
# Windows PowerShell
$env:INT2DDS_DISCOVERY_MODE = "hybrid"

# CLI argument
cargo run --example hello_world -- --int2dds-discovery-mode hybrid
```

```bash
# Linux/macOS
export INT2DDS_DISCOVERY_MODE=hybrid

# CLI argument
cargo run --example hello_world -- --int2dds-discovery-mode hybrid
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

# CLI argument
cargo run --example hello_world -- --int2dds-log-type console
```

```bash
# Linux/macOS
export INT2DDS_LOG_TYPE=console

# CLI argument
cargo run --example hello_world -- --int2dds-log-type console
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

# CLI argument
cargo run --example hello_world -- --int2dds-console-log-level debug
```

```bash
# Linux/macOS
export INT2DDS_CONSOLE_LOG_LEVEL=debug

# CLI argument
cargo run --example hello_world -- --int2dds-console-log-level debug
```

### INT2DDS_FILE_LOG_LEVEL

Sets the file log level. Values are the same as `INT2DDS_CONSOLE_LOG_LEVEL`.

#### Configuration

```powershell
# Windows PowerShell
$env:INT2DDS_FILE_LOG_LEVEL = "trace"

# CLI argument
cargo run --example hello_world -- --int2dds-file-log-level trace
```

```bash
# Linux/macOS
export INT2DDS_FILE_LOG_LEVEL=trace

# CLI argument
cargo run --example hello_world -- --int2dds-file-log-level trace
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

# CLI argument
cargo run --example hello_world -- --int2dds-network-interface Ethernet
```

```bash
# Linux/macOS
export INT2DDS_NETWORK_INTERFACE=eth0

# CLI argument
cargo run --example hello_world -- --int2dds-network-interface eth0
```

### INT2DDS_NETWORK_IP

Directly specifies the network IP address to use.
If not specified, all available addresses will be used.

#### Configuration

```powershell
# Windows PowerShell
$env:INT2DDS_NETWORK_IP = "192.168.1.100"

# CLI argument
cargo run --example hello_world -- --int2dds-network-ip 192.168.1.100
```

```bash
# Linux/macOS
export INT2DDS_NETWORK_IP=192.168.1.100

# CLI argument
cargo run --example hello_world -- --int2dds-network-ip 192.168.1.100
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

# CLI argument (flag type)
cargo run --example hello_world -- --int2dds-use-loopback-interface
```

```bash
# Linux/macOS - Environment variable
export INT2DDS_USE_LOOPBACK_INTERFACE=true

# CLI argument (flag type)
cargo run --example hello_world -- --int2dds-use-loopback-interface
```

### INT2DDS_UDP_SOCKET_BUFFER

Sets the UDP socket buffer size in bytes. Default uses OS default value.

#### Configuration

```powershell
# Windows PowerShell
$env:INT2DDS_UDP_SOCKET_BUFFER = "1048576"  # 1MB

# CLI argument
cargo run --example hello_world -- --int2dds-udp-socket-buffer 1048576
```

```bash
# Linux/macOS
export INT2DDS_UDP_SOCKET_BUFFER=1048576

# CLI argument
cargo run --example hello_world -- --int2dds-udp-socket-buffer 1048576
```

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
2. `INT2DDS_MULTICAST_TTL` env var (or `--int2dds-multicast-ttl` CLI flag)
3. Default `1`

#### Configuration

```powershell
# Windows PowerShell
$env:INT2DDS_MULTICAST_TTL = "32"

# CLI argument
cargo run --example hello_world -- --int2dds-multicast-ttl 32
```

```bash
# Linux/macOS
export INT2DDS_MULTICAST_TTL=32

# CLI argument
cargo run --example hello_world -- --int2dds-multicast-ttl 32
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

### INT2DDS_EXTENDED_DISCOVERY

Controls the ability to send DDS discovery messages via extended discovery.

#### Configuration

```powershell
# Windows PowerShell - Environment variable
$env:INT2DDS_EXTENDED_DISCOVERY = "true"

# CLI argument (flag type)
cargo run --example hello_world -- --int2dds-extended-discovery
```

```bash
# Linux/macOS - Environment variable
export INT2DDS_EXTENDED_DISCOVERY=true

# CLI argument (flag type)
cargo run --example hello_world -- --int2dds-extended-discovery
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

---

## TCP Settings

### INT2DDS_TCP_CONNECT_TIMEOUT

Sets the TCP connection timeout in milliseconds.

- Default: 5000ms (5 seconds)

#### Configuration

```powershell
# Windows PowerShell
$env:INT2DDS_TCP_CONNECT_TIMEOUT = "10000"  # 10 seconds

# CLI argument
cargo run --example hello_world -- --int2dds-tcp-connect-timeout 10000
```

```bash
# Linux/macOS
export INT2DDS_TCP_CONNECT_TIMEOUT=10000

# CLI argument
cargo run --example hello_world -- --int2dds-tcp-connect-timeout 10000
```

### INT2DDS_TCP_WRITE_TIMEOUT

Sets the TCP write timeout in milliseconds.

- Default: 10000ms (10 seconds)

#### Configuration

```powershell
# Windows PowerShell
$env:INT2DDS_TCP_WRITE_TIMEOUT = "5000"  # 5 seconds

# CLI argument
cargo run --example hello_world -- --int2dds-tcp-write-timeout 5000
```

```bash
# Linux/macOS
export INT2DDS_TCP_WRITE_TIMEOUT=5000

# CLI argument
cargo run --example hello_world -- --int2dds-tcp-write-timeout 5000
```

### INT2DDS_TCP_NODELAY

Sets TCP Nodelay (disables Nagle algorithm).

- Default: true (Nagle algorithm disabled, low latency)

#### Configuration

```powershell
# Windows PowerShell - Environment variable
$env:INT2DDS_TCP_NODELAY = "true"

# CLI argument (flag type)
cargo run --example hello_world -- --int2dds-tcp-nodelay
```

```bash
# Linux/macOS - Environment variable
export INT2DDS_TCP_NODELAY=true

# CLI argument (flag type)
cargo run --example hello_world -- --int2dds-tcp-nodelay
```

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

# CLI argument
cargo run --example hello_world -- --int2dds-initial-peers "192.168.1.100:17410,192.168.1.100:17412"
```

```bash
# Linux/macOS
export INT2DDS_INITIAL_PEERS="192.168.1.100:17410,192.168.1.100:17412"

# CLI argument
cargo run --example hello_world -- --int2dds-initial-peers "192.168.1.100:17410,192.168.1.100:17412"
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

# CLI argument (flag type)
cargo run --example hello_world -- --int2dds-thread-monitoring
```

```bash
# Linux/macOS - Environment variable
export INT2DDS_THREAD_MONITORING=true

# CLI argument (flag type)
cargo run --example hello_world -- --int2dds-thread-monitoring
```

### INT2DDS_THREAD_MONITORING_LOG_PATH

Sets the thread monitoring log file path.

- Default: `./thread_monitoring.log`

#### Configuration

```powershell
# Windows PowerShell
$env:INT2DDS_THREAD_MONITORING_LOG_PATH = "C:\logs\thread.log"

# CLI argument
cargo run --example hello_world -- --int2dds-thread-monitoring-log-path "C:\logs\thread.log"
```

```bash
# Linux/macOS
export INT2DDS_THREAD_MONITORING_LOG_PATH=/var/log/thread_monitoring.log

# CLI argument
cargo run --example hello_world -- --int2dds-thread-monitoring-log-path /var/log/thread_monitoring.log
```

### INT2DDS_FUNCTION_TIMING

Enables function execution time measurement.

- Default: false

#### Configuration

```powershell
# Windows PowerShell - Environment variable
$env:INT2DDS_FUNCTION_TIMING = "true"

# CLI argument (flag type)
cargo run --example hello_world -- --int2dds-function-timing
```

```bash
# Linux/macOS - Environment variable
export INT2DDS_FUNCTION_TIMING=true

# CLI argument (flag type)
cargo run --example hello_world -- --int2dds-function-timing
```

### INT2DDS_FUNCTION_TIMING_LOG_PATH

Sets the function timing log file path.

- Default: `./function_timing.log`

#### Configuration

```powershell
# Windows PowerShell
$env:INT2DDS_FUNCTION_TIMING_LOG_PATH = "C:\logs\timing.log"

# CLI argument
cargo run --example hello_world -- --int2dds-function-timing-log-path "C:\logs\timing.log"
```

```bash
# Linux/macOS
export INT2DDS_FUNCTION_TIMING_LOG_PATH=/var/log/function_timing.log

# CLI argument
cargo run --example hello_world -- --int2dds-function-timing-log-path /var/log/function_timing.log
```

---

## Related Files

- [dds/src/common/env.rs](../../dds/src/common/env.rs) - Environment variable parsing and application
- [dds/src/common/log.rs](../../dds/src/common/log.rs) - Logging configuration
- [dds/src/rtps/transport/mod.rs](../../dds/src/rtps/transport/mod.rs) - Transport type definition
- [dds/src/rtps/transport/transport_config.rs](../../dds/src/rtps/transport/transport_config.rs) - Multicast TTL resolution
- [dds/src/rtps/transport/udp/udp_sender.rs](../../dds/src/rtps/transport/udp/udp_sender.rs) - UDP transport settings
- [dds/src/rtps/transport/tcp/tcp_sender.rs](../../dds/src/rtps/transport/tcp/tcp_sender.rs) - TCP sender and connection management
