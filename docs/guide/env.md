# Environment Variables

This document describes the environment variables available in int2dds. All environment variables can also be set via CLI arguments.

## Environment Variables Summary

| Environment Variable                 | CLI Argument                           | Description                                | Default                 |
| ------------------------------------ | -------------------------------------- | ------------------------------------------ | ----------------------- |
| `INT2DDS_TRANSPORT`                  | `--int2dds-transport`                  | Transport protocol (udp, tcp, hybrid)      | udp                     |
| `INT2DDS_DISCOVERY_MODE`             | `--int2dds-discovery-mode`             | Discovery mode (udp, tcp, hybrid)          | udp                     |
| `INT2DDS_LOG_TYPE`                   | `--int2dds-log-type`                   | Log output type (console, file, all, none) | none                    |
| `INT2DDS_CONSOLE_LOG_LEVEL`          | `--int2dds-console-log-level`          | Console log level                          | info                    |
| `INT2DDS_FILE_LOG_LEVEL`             | `--int2dds-file-log-level`             | File log level                             | info                    |
| `INT2DDS_NETWORK_INTERFACE`          | `--int2dds-network-interface`          | Network interface name                     | auto                    |
| `INT2DDS_NETWORK_IP`                 | `--int2dds-network-ip`                 | Network IP address                         | auto                    |
| `INT2DDS_UDP_SOCKET_BUFFER`          | `--int2dds-udp-socket-buffer`          | UDP socket buffer size (bytes)             | OS default              |
| `INT2DDS_EXTENDED_DISCOVERY`         | `--int2dds-extended-discovery`         | Enable extended discovery                  | false                   |
| `INT2DDS_TCP_CONNECT_TIMEOUT`        | `--int2dds-tcp-connect-timeout`        | TCP connection timeout (ms)                | 5000                    |
| `INT2DDS_TCP_WRITE_TIMEOUT`          | `--int2dds-tcp-write-timeout`          | TCP write timeout (ms)                     | 10000                   |
| `INT2DDS_TCP_NODELAY`                | `--int2dds-tcp-nodelay`                | Enable TCP Nodelay                         | true                    |
| `INT2DDS_INITIAL_PEERS`              | `--int2dds-initial-peers`              | Initial peer list                          | none                    |
| `INT2DDS_THREAD_MONITORING`          | `--int2dds-thread-monitoring`          | Enable thread monitoring                   | false                   |
| `INT2DDS_THREAD_MONITORING_LOG_PATH` | `--int2dds-thread-monitoring-log-path` | Thread monitoring log path                 | ./thread_monitoring.log |
| `INT2DDS_FUNCTION_TIMING`            | `--int2dds-function-timing`            | Enable function timing                     | false                   |
| `INT2DDS_FUNCTION_TIMING_LOG_PATH`   | `--int2dds-function-timing-log-path`   | Function timing log path                   | ./function_timing.log   |

---

## Transport Settings

### INT2DDS_TRANSPORT

Sets the transport protocol type.

| Value    | Description                |
| -------- | -------------------------- |
| `udp`    | UDP transport (default)    |
| `tcp`    | TCP transport              |
| `hybrid` | UDP + TCP simultaneous use |

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

Sets the initial peer list for TCP/Hybrid mode. Format is comma-separated socket addresses.

- Format: `ip:port,ip:port,...`
- Discovery port calculation: `7400 + (250 * domain_id) + 10 + (2 * participant_id)`
  - For Domain 40: participant 0 = 17410, participant 1 = 17412

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
- [dds/src/rtps/transport/udp/udp_sender.rs](../../dds/src/rtps/transport/udp/udp_sender.rs) - UDP transport settings
- [dds/src/rtps/transport/tcp/tcp_sender.rs](../../dds/src/rtps/transport/tcp/tcp_sender.rs) - TCP sender and connection management
