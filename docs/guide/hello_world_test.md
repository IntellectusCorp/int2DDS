# Testing the `hello_world` example

`hello_world` is the single, unified Hello World example. One binary runs as a
publisher (`-P`) or subscriber (`-S`), and its QoS comes from one of two sources,
selected automatically:

- **No profile (CLI args)** — quick local runs over plain UDP. QoS is built from
  the command-line flags.
- **Profile (`DDS_QOS_PROFILE`)** — a JSON/XML profile fully drives QoS, including
  the transport (UDP / multicast-TTL / TCP / Hybrid). In this mode the CLI QoS
  flags are ignored.

All commands below are run from the repository root; the crate is the workspace
member `int2dds`.

---

## 1. Quick start

the example runs over UDP and builds QoS from the CLI flags (default).
Open two terminals:

```bash
# Terminal 1 — subscriber
cargo run -p int2dds --example hello_world -- -S

# Terminal 2 — publisher
cargo run -p int2dds --example hello_world -- -P
```

The publisher should match the subscriber and the subscriber should print
`Read sample: ...`.

### CLI flags (args mode only)

| Flag | Meaning | Default |
| --- | --- | --- |
| `-P` / `-S` | Run as publisher / subscriber (required) | — |
| `-T <name>` | Topic name | `hello_world_topic` |
| `-d <id>` | Domain id | `0` |
| `-i <ms>` | Publish interval (publisher) | `1000` |
| `-r` | Reliable reliability | best-effort |
| `-t` | Transient-local durability | volatile |
| `-k <N>` | History: `0` = keep-all, `N` = keep-last N | `1` |
| `-f <ms>` | Deadline period | infinite |
| `-o [strength]` | Exclusive ownership (publisher: `-o <strength>`) | shared |
| `-p <name>` | Partition name | none |
| `-s <bytes>` | Pad the message to this size (publisher) | none |

Example — a reliable, transient-local pair on domain 5:

```bash
cargo run -p int2dds --example hello_world -- -S -d 5 -r -t
cargo run -p int2dds --example hello_world -- -P -d 5 -r -t
```

> The default writer QoS in args mode is **best-effort** (so `-r` is a meaningful
> toggle); this differs from the OMG-spec writer default of reliable.

---

## 2. Profile-driven runs (`DDS_QOS_PROFILE`)

Set `DDS_QOS_PROFILE` to a profile file and the example applies it. Every bundled
file marks its profile `is_default_profile`, so `DDS_QOS_PROFILE` selects
it. The format (JSON or XML) is auto-detected by
file extension, so the `.json` and `.xml` files below are interchangeable.

Bundled profiles live under `dds/examples/hello_world/profiles/`:

| Scenario | Files | Layout |
| --- | --- | --- |
| `udp` | `profile.{json,xml}` | one file, both roles |
| `multicast_ttl` | `profile.{json,xml}` | one file, both roles (UDP + multicast TTL 64) |
| `tcp` | `pub_profile.{json,xml}`, `sub_profile.{json,xml}` | per-role file (bind ports differ) |
| `hybrid` | `pub_profile.{json,xml}`, `sub_profile.{json,xml}` | per-role file |

On a successful load the library logs `Auto-loaded QoS profile: <file>`.

### UDP profile

A single file is shared by both roles:

```bash
P=dds/examples/hello_world/profiles/udp/profile.json   # or .xml
DDS_QOS_PROFILE=$P cargo run -p int2dds --example hello_world -- -S
DDS_QOS_PROFILE=$P cargo run -p int2dds --example hello_world -- -P
```

### Multicast-TTL profile

Same as UDP, but sets `int2dds.transport.UDPv4.multicast_ttl = 64` (the base
`udp` profile leaves it at the default of 1). Useful when discovery must cross
routed/multi-subnet links; on a single host it behaves like UDP.

```bash
P=dds/examples/hello_world/profiles/multicast_ttl/profile.json
DDS_QOS_PROFILE=$P cargo run -p int2dds --example hello_world -- -S
DDS_QOS_PROFILE=$P cargo run -p int2dds --example hello_world -- -P
```

### TCP profile (per-role files)

TCP has no multicast, so each side needs **initial peers** and a distinct
**bind port** — hence two files: `pub_profile` binds `7400`, `sub_profile`
binds `7401`, and they dial each other on loopback.

```bash
# subscriber (binds 7401)
DDS_QOS_PROFILE=dds/examples/hello_world/profiles/tcp/sub_profile.json \
    cargo run -p int2dds --example hello_world -- -S
# publisher (binds 7400)
DDS_QOS_PROFILE=dds/examples/hello_world/profiles/tcp/pub_profile.json \
    cargo run -p int2dds --example hello_world -- -P
```

> **Single host: enable loopback.** Pure TCP can only reach a peer at the address
> it advertises. The bundled profiles dial `127.0.0.1`, so both sides must
> advertise loopback — set `INT2DDS_USE_LOOPBACK_INTERFACE=true` on each process
> (see [env.md](env.md)). To run over a real interface instead, edit the
> `initial_peers` in the profiles to that interface's IP and drop the loopback
> flag.

### Hybrid profile (per-role files)

Hybrid discovers over UDP multicast and carries user data over TCP, so it
matches on a single host **without** the loopback flag. Bind ports still differ
per role:

```bash
DDS_QOS_PROFILE=dds/examples/hello_world/profiles/hybrid/sub_profile.json \
    cargo run -p int2dds --example hello_world -- -S
DDS_QOS_PROFILE=dds/examples/hello_world/profiles/hybrid/pub_profile.json \
    cargo run -p int2dds --example hello_world -- -P
```

---

## 3. Testing with a custom profile

You can point `DDS_QOS_PROFILE` at your own file. A minimal JSON profile that
sets the transport and a reliable writer/reader:

```json
{
  "name": "MyLib",
  "qos_profiles": [
    {
      "name": "MyProfile",
      "is_default_profile": true,
      "domain_participant_qos": {
        "property": { "value": [
          { "name": "int2dds.transport", "value": "udp", "propagate": false }
        ]}
      },
      "datawriter_qos": { "reliability": { "kind": "RELIABLE_RELIABILITY_QOS" } },
      "datareader_qos": { "reliability": { "kind": "RELIABLE_RELIABILITY_QOS" } }
    }
  ]
}
```

```bash
DDS_QOS_PROFILE=/abs/path/my_profile.json \
    cargo run -p int2dds --example hello_world -- -P
```

The XML form uses the RTI/OMG `<qos_library>` / `<qos_profile>` syntax — see the
bundled `*.xml` files for a template.

### Cautions

- **Mark a default profile.** Selection needs either `is_default_profile="true"`
  on a profile, **or** an explicit `DDS_DEFAULT_QOS_PROFILE=MyLib::MyProfile`.
  If neither is set, the file loads but **no profile is selected** — QoS silently
  falls back to the spec default. The example flags this with a warning (below).
- **Use absolute paths.** `DDS_QOS_PROFILE` is loaded as-is, so a relative value
  resolves against the **current working directory** (where you launched
  `cargo`), not the crate/example directory. Prefer an absolute path; a
  repo-root-relative path only works when you launch from the repo root.
- **TCP/Hybrid bind ports must be unique per host.** Two participants on one host
  cannot share a `bind_port`. Pure TCP also requires `initial_peers` on each side.
- **One default per file.** A library can mark only one `is_default_profile`. To
  give a publisher and subscriber different participant QoS (e.g. distinct TCP
  bind ports), split them into two files — as the bundled `tcp`/`hybrid`
  profiles do — and point each process at its own file.

---

## TCP transport in a nutshell

DDS/RTPS is specified over UDP; TCP is an int2DDS **vendor transport** for links
where UDP is unavailable or a reliable, connection-oriented stream is preferred
(crossing a WAN/NAT, strict firewalls, etc.).

- **One multiplexed listener per participant.** Discovery (SPDP/SEDP) and user
  data share a single TCP connection per peer; RTPS "logical ports" are carried
  inside the framed stream instead of separate sockets.
- **No multicast.** Pure-TCP discovery cannot bootstrap on its own — every
  participant must be given `int2dds.initial_peers` pointing at the other side.
- **Per-participant listen port** via `int2dds.transport.TCPv4.bind_port`; when
  omitted it defaults to `7400 + 250 * domain_id`.
- **Configured through QoS, not env** — transport selection and tuning live in
  the participant's `PropertyQosPolicy` (or a QoS profile), e.g.
  `int2dds.transport = tcp`.

### Participant QoS properties that drive the transport

| Property | Meaning | Default |
| --- | --- | --- |
| `int2dds.transport` | `udp` \| `tcp` \| `hybrid` | `udp` |
| `int2dds.transport.TCPv4.bind_port` | This participant's TCP listen port | `7400 + 250 * domain_id` |
| `int2dds.initial_peers` | `ip:port[,ip:port…]` to dial (required for pure TCP) | none |
| `int2dds.transport.UDPv4.multicast_ttl` | IPv4 multicast TTL | `1` |

---

## See also

- [env.md](env.md) — environment variables and CLI overrides
  (`INT2DDS_USE_LOOPBACK_INTERFACE`, `INT2DDS_NETWORK_INTERFACE`, …).
- [dds/examples/hello_world/hello_world.rs](../../dds/examples/hello_world/hello_world.rs) — the example source.
- [dds/examples/hello_world/profiles/](../../dds/examples/hello_world/profiles/) — bundled profiles (JSON + XML).