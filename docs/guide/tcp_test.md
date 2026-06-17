# Testing the TCP Transport

int2DDS ships a TCP transport alongside the default UDP. This guide explains
what TCP mode is, where its tests live, and how to exercise it — from the
automated unit/integration suites to a hands-on publisher/subscriber run.

## TCP mode in a nutshell

DDS/RTPS is specified over UDP; TCP is an int2DDS **vendor transport** for links
where UDP is unavailable or where a reliable, connection-oriented stream is
preferred (crossing a WAN/NAT, strict firewalls, etc.).

Key differences from UDP:

- **One multiplexed listener per participant.** Discovery (SPDP/SEDP) and user
  data share a single TCP connection per peer; RTPS "logical ports" are carried
  inside the framed stream instead of separate sockets.
- **No multicast.** TCP cannot multicast, so discovery cannot bootstrap on its
  own — every pure-TCP participant must be given **initial peers**
  (`int2dds.initial_peers`) pointing at the other side.
- **Per-participant listen port.** Each participant binds its own TCP port, set
  with the `int2dds.transport.TCPv4.bind_port` QoS property. When omitted it
  defaults to the domain formula `7400 + 250 * domain_id`. Running several TCP
  participants on one host therefore requires a distinct `bind_port` per
  participant.
- **Configured through QoS, not env.** Transport selection and TCP tuning live
  in the participant's `PropertyQosPolicy` (or a QoS profile JSON), e.g.
  `int2dds.transport = tcp`.

## Tests

| Location | Kind | Covers |
| --- | --- | --- |
| `dds/examples/hello_world/hello_world_tcp.rs` | example | end-to-end pub/sub over TCP |
| `dds/examples/route_gateway/route_gateway.rs` | example | Topic-relaying in WAN (LAN UDP ⇄ WAN TCP) |

All commands below are run from the repository root; the crate is the workspace
member `int2dds`.

## End-to-end with the `hello_world_tcp` example

`hello_world_tcp` runs the Hello World publisher/subscriber over TCP. By default
the transport is loaded from a QoS profile; the publisher binds `7400` and the
subscriber `7401` on a single host over loopback.

Open two terminals:

```bash
# Terminal 1 — subscriber (binds 7401)
cargo run -p int2dds --example hello_world_tcp -- -S

# Terminal 2 — publisher (binds 7400)
cargo run -p int2dds --example hello_world_tcp -- -P
```

### Choosing the QoS source

- **Bundled profile (default).** With no env set, the example loads its bundled
  `dds/examples/hello_world/hello_world_tcp_qos.json`. The path is baked in at
  compile time (`CARGO_MANIFEST_DIR`), so it works from any working directory.

- **Custom profile via `DDS_QOS_PROFILE`.** Point it at your own profile file;
  the factory auto-loads it. It must define the `HelloWorldTcp::TcpPub` and
  `HelloWorldTcp::TcpSub` profiles.

  ```bash
  DDS_QOS_PROFILE=/abs/path/my_tcp_qos.json \
      cargo run -p int2dds --example hello_world_tcp -- -P
  ```

  > **Caution — relative paths.** `DDS_QOS_PROFILE` is loaded as-is, so a
  > relative value resolves against the **current working directory** (wherever
  > you launched `cargo` from), **not** the crate or example directory. Because
  > `cargo run` does not change directory, a relative path must be relative to
  > that working directory — e.g. from the repo root:
  >
  > ```bash
  > DDS_QOS_PROFILE=dds/examples/hello_world/hello_world_tcp_qos.json \
  >     cargo run -p int2dds --example hello_world_tcp -- -P
  > ```
  >
  > The same value breaks if you `cd` elsewhere first. **Prefer an absolute
  > path.** (Only the bundled fallback avoids this, because it is resolved at
  > compile time.)

- **Code-level QoS (`--inline-qos` / `-c`).** Builds the same TCP QoS in code,
  so no JSON file is needed — handy for a quick check:

  ```bash
  cargo run -p int2dds --example hello_world_tcp -- -S -c
  cargo run -p int2dds --example hello_world_tcp -- -P -c
  ```

## WAN bridging with `route_gateway`

`route_gateway` bridges a LAN (UDP) participant and a WAN (TCP) participant via
`AutoRelay`. Its config — including the remote node's `transport: "tcp"`,
`bind_port`, and `initial_peers` — is read from `DDS_QOS_PROFILE`, else the
bundled `dds/examples/route_gateway/gateway.example.json`:

```bash
cargo run -p int2dds --example route_gateway
# or with your own config (absolute path recommended — same caution as above):
export DDS_QOS_PROFILE=/abs/path/gateway.json
cargo run -p int2dds --example route_gateway
```

## TCP QoS reference

The participant-level properties that drive TCP:

| Property | Meaning | Default |
| --- | --- | --- |
| `int2dds.transport` | Transport selection — set to `tcp` (or `hybrid`) | `udp` |
| `int2dds.transport.TCPv4.bind_port` | This participant's TCP listen port | `7400 + 250 * domain_id` |
| `int2dds.initial_peers` | `ip:port[,ip:port…]` of peers to dial (required for pure TCP) | none |

Minimal QoS profile JSON (a single publisher-side profile):

```json
{
  "name": "MyTcp",
  "qos_profiles": [
    {
      "name": "Pub",
      "domain_participant_qos": {
        "property": { "value": [
          { "name": "int2dds.transport", "value": "tcp", "propagate": false },
          { "name": "int2dds.transport.TCPv4.bind_port", "value": "7400", "propagate": false },
          { "name": "int2dds.initial_peers", "value": "127.0.0.1:7401", "propagate": false }
        ]}
      }
    }
  ]
}
```

> Single-host runs also need a working NIC or loopback. The `hello_world_tcp`
> example forces loopback (`set_use_loopback_interface(true)`) so both sides
> advertise `127.0.0.1`, matching the `initial_peers` they dial.

## Related Files

- [dds/examples/hello_world/hello_world_tcp.rs](../../dds/examples/hello_world/hello_world_tcp.rs) - TCP pub/sub example
- [dds/examples/hello_world/hello_world_tcp_qos.json](../../dds/examples/hello_world/hello_world_tcp_qos.json) - bundled TCP QoS profile
- [dds/examples/route_gateway/route_gateway.rs](../../dds/examples/route_gateway/route_gateway.rs) - WAN gateway example
- [dds/src/rtps/transport/transport_config.rs](../../dds/src/rtps/transport/transport_config.rs) - `TcpConfig` resolution from QoS
- [dds/src/rtps/transport/tcp/](../../dds/src/rtps/transport/tcp/) - TCP transport implementation and unit tests
