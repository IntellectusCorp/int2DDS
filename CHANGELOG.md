# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.1.5] - 2026-09-10

Entity deletion no longer races with in-flight operations, and the TCP transport
gets a peer list that settles on the peers that actually answer.

### Added

- The TCP peer search width can be set through the environment, both for an
  explicit peer list and for a wildcard (`ip:0`) entry
- A TCP peer list that narrows from what was declared to what answers, with a
  peer that is gone leaving the announcement list

### Fixed

- Deleting an entity now waits for its in-flight operations instead of racing
  them: callbacks are drained, readers and writers are unregistered before being
  marked deleted, and public operations are gated on the lifecycle
- Deleting a Publisher, Subscriber, Topic or DomainParticipant from inside a
  listener is refused rather than deadlocking
- A WaitSet waiter is woken when its condition's entity is deleted, and a reader
  with an outstanding ReadCondition is not deleted underneath it
- Each TCP Participant takes its own listen port within its domain block, so two
  Participants on one host no longer collide
- A same-host peer is accepted at the loopback address discovery substitutes for
  it, and the dial gate opens for a same-host peer named by the wildcard

## [0.1.4] - 2026-09-04

Same-host communication now settles on a single loopback address, plus a
workspace-wide clippy pass.

### Added

- `INT2DDS_DISABLE_SAME_HOST_LOOPBACK` to turn the same-host narrowing off and
  address a co-located peer at every address it announced
- Integration coverage for the pure-TCP path, where the peer is named rather
  than discovered

### Changed

- A co-located peer is reached at one loopback address instead of one locator
  per interface; co-location is judged against every address of this host, and
  locators are narrowed one at a time rather than by replacing the list
- The same-host locator fallback table was dropped
- The unused chained payload path was dropped from `CacheChange` and
  `DataSample`
- QoS default resolution simplified, and clippy findings cleared across the
  CDR, XTypes, derive, DCPS, RTPS, FFI and IDL paths

### Fixed

- Same-host is settled from the first datagram instead of the SPDP that may
  follow it
- Interfaces that appear after the address set was first read are now learned

## [0.1.3] - 2026-09-02

Large-data and discovery hardening, a redesigned TCP transport, and the first
release built against a fixed glibc floor. All changes are against 0.1.1; no
0.1.2 was tagged.

### Added

#### Fragmentation and large data

- Packing of several fragments into one `DATA_FRAG` submessage, on both the
  first send and the repair path
- Batched `NACK_FRAG` windows so a reader requests every missing fragment at once
- A per-peer fragment send window that is carried across calls and bounded by
  the peer's advertised receive-buffer size
- `DATA_FRAG` batching per subscriber-side participant
- QoS and environment knobs for the reader's `NACK_FRAG` repair timing

#### Discovery

- SEDP endpoint-discovery push callback
- SEDP dispose propagation, reported as disposed instances
- Advertisement of the participant's receive-buffer size in SPDP
- `INT2DDS_SEDP_HEARTBEAT_MS` to override the SEDP heartbeat period
- SEDP `DATA` submessages bundled within `INT2DDS_MAX_MESSAGE_SIZE`

#### Type system and codegen

- XCDR version derived from writer QoS, with `PL_CDR1` support on the C raw path
- Bulk byte copy for `sequence<octet>` in the C code generator

#### Configuration

- `INT2DDS_DISABLE_PREEMPTIVE` to gate preemptive `ACKNACK` and `HEARTBEAT`
- Environment override for `disable_piggyback_heartbeat`
- Environment override for the `DATA_FRAG` size

#### Build and release

- Linux GNU release artifacts built against glibc 2.28 (`manylinux_2_28`)
- A glibc floor smoke-test loader for release artifacts
- FFI vendor tarball produced in CI
- Static enterprise-hooks seam module

### Changed

#### Transport layer redesign

The TCP layer was rebuilt on synchronous I/O: frames are capped at 64 KiB, the
frame length is carried in band as an RTPS submessage, connection I/O tasks are
consolidated, and hybrid TCP defaults to an ephemeral port. Connect is the only
bounded operation; a send is never bounded. Refusals before the wire now carry
their own transport error codes and are reported to the caller instead of being
swallowed.

#### Threading

- Discovery processing moved to a worker thread
- User data processing moved to a worker thread

#### Performance

- `DATA` to readers behind one participant batched into a single message, as are
  the `ACKNACK`s owed to one participant
- Parent handles are no longer deep-cloned on the writer write path or the
  per-sample path
- `StatusCondition` masks stored in atomics instead of mutexes
- Timers indexed by deadline instead of scanning every wake
- Reply-locator lists inlined rather than heap-allocated
- Diagnostic strings are no longer built for log levels that cannot emit them

#### Discovery timing

- The SEDP heartbeat period is 200 ms and stops once fully acknowledged
- The NACK response delay is 100 ms and applies to `NACK_FRAG`

### Fixed

Over a hundred fixes. The largest clusters:

- **Fragment reassembly** — buffers keyed by reader, freed when the reader or
  writer they wait on is gone, evicted by last-updated time; partially received
  samples stay NACK-able; a completed sample is delivered to every waiting
  reader; `DATA_FRAG` payloads with an out-of-range offset or an implausible
  fragment count are rejected and padded to a 4-byte boundary
- **WaitSet** — the lost-wakeup window is closed, conditions are identified by
  pointer and keyed by identity, and `set_enabled_statuses` notifies when it
  makes a condition triggered
- **TCP** — what the send buffer cannot take is queued instead of dropped,
  inbound discovery is backpressured instead of dropped on a full channel, a
  stalled handshake is bounded by a timeout and closed, and one failed
  connection no longer disrupts the others
- **SEDP** — history pushed to a newly discovered participant, the ack position
  recorded so idle heartbeats stop, disposes kept in the builtin writer history,
  and announcements naming no endpoint discarded
- **Entity lifecycle** — in-flight reader callbacks drained before deletion
  returns, a panicking user listener can no longer kill the RTPS receive thread,
  and the matched-writer lock is released before listeners are notified
- **Wire decoding** — `octetsToNextHeader` decoded with the endianness its own
  header announces, parameter lengths read as unsigned shorts, and the locator
  reservation in `InfoReply` bounded by the bytes available

### Removed

- RMW profiling scaffolding
- Outbound buffer pooling
- The `dlopen` feature-FFI path, with core call sites routed through hooks

## [0.1.1] - 2026-07-31

First tagged release of int2DDS. Development started on 2025-12-01 and no earlier
version was ever tagged or published, so this entry is cumulative — it describes
what the release contains rather than a delta against a predecessor.

APIs are not yet stabilized and may change before 1.0.0.

### Added

#### Core DDS Features

- RTPS 2.5 protocol implementation
- DomainParticipant entity for domain management
- Publisher and DataWriter for data publication
- Subscriber and DataReader for data subscription
- Topic management with type support
- ContentFilteredTopic for subscriber-side content filtering
- WaitSet with status, read, and guard conditions
- Comprehensive QoS policy support:
  - Reliability (RELIABLE, BEST_EFFORT)
  - Durability (VOLATILE, TRANSIENT_LOCAL)
  - History (KEEP_LAST, KEEP_ALL)
  - Liveliness (AUTOMATIC, MANUAL_BY_PARTICIPANT, MANUAL_BY_TOPIC)
  - Deadline, Latency Budget, Ownership, and more

#### RTPS Protocol Layer

- RTPS message serialization and deserialization
- Built-in endpoint discovery (SPDP and SEDP)
- Reliable communication with acknowledgments and retransmissions
- Best-effort communication
- Data fragmentation for large messages
- Heartbeat and AckNack mechanisms

#### Transport Layer

- UDP multicast transport
- TCP transport support
- Hybrid transport (UDP + TCP)
- TLS over TCP, backed by rustls (TLS 1.3 capable, no OpenSSL dependency), with
  CA / certificate / key configuration, peer verification, and SNI
- Network interface and IP address selection
- Configurable socket buffer sizes
- Loopback-only multicast for local testing
  (`INT2DDS_USE_LOOPBACK_INTERFACE`, `INT2DDS_FORCE_LOOPBACK_MULTICAST`)

#### Type System

- `#[derive(DdsType)]` procedural macro for automatic type support
- Support for primitive types (bool, integers, floats)
- Support for complex types (String, Vec, HashMap, BTreeMap)
- Nested struct support
- Array and bounded sequence support
- Key field support with `#[dds(key)]` attribute
- XCDR2 serialization support (Final, Appendable, Mutable types)
- Dynamic XTypes: runtime type discovery, TypeObject introspection, and dynamic
  endpoint creation without compile-time type definitions

#### Language Bindings

- C bindings (`int2dds-ffi`) with static and dynamic library builds and
  cbindgen-generated headers
- Python bindings (`int2dds`), cffi-based, requiring Python 3.10+
- C# bindings (`Int2Dds`), versioned in lockstep with the Rust workspace

#### IDL Toolchain

- `int2dds-idl`, an OMG IDL compiler available as both a CLI binary and a library
- Code generation targets: Rust, C, Python, C#, an XML type representation, and
  RPC scaffolding

#### DDS-RPC

- `int2dds-rpc`, an implementation of the OMG DDS-RPC request-reply model on top
  of int2DDS, with service interfaces defined in OMG IDL
- Basic Service Mapping profile (Enhanced Service Mapping is not yet implemented)

#### Configuration

- XML and JSON QoS profile files, applied to both statically and dynamically
  created entities
- Environment variable configuration

#### Route Gateway

- Gateway service that bridges DDS traffic between a LAN (UDP) and a WAN (TCP)
- Explicit topic relay and automatic relay modes

#### Developer Tools

- Thread monitoring and logging (INT2DDS_THREAD_MONITORING)
- Function timing and performance profiling (INT2DDS_FUNCTION_TIMING)
- Comprehensive logging with RUST_LOG

#### Examples

- Hello World for Rust, C, Python, and C#
- QoS profile publisher/subscriber for C#
- Robot control service for DDS-RPC

#### Interoperability

- Tested against CycloneDDS, FastDDS, and OpenDDS in CI
- Shapes demo for cross-vendor testing
- RTPS protocol compliance

#### Platform Support

- Windows, Linux (x86_64 and arm64), and macOS, all covered by CI

#### Documentation

- API documentation with examples
- Build and usage instructions
- Environment variable documentation
- Performance tuning guide
- Function timing guide

### Changed

#### FFI API consolidation (breaking)

Immediately before this release the C FFI surface was consolidated into a single,
consistently named API. Nothing prior to 0.1.1 was ever published, so this is not
a break against a released version — it is recorded for anyone who was tracking
pre-release development builds. There are no compatibility shims: every C, C#, and
Python caller must be updated. All bundled examples, bindings, and headers in this
repository were migrated together.

- Entity-prefixed naming for all operations:
  - `int2dds_write_serialized()` → `int2dds_datawriter_write_serialized()`
    (and `_w_timestamp` likewise)
  - `int2dds_take_serialized()` / `int2dds_read_serialized()` /
    `_w_info` / `_batch` → `int2dds_datareader_*` equivalents
  - `int2dds_get_publication_matched_status()` →
    `int2dds_datawriter_get_publication_matched_status()`
  - `int2dds_get_subscription_matched_status()` →
    `int2dds_datareader_get_subscription_matched_status()`
  - `int2dds_guard_condition_*()` → `int2dds_guardcondition_*()`
  - `int2dds_waitset_attach_condition()` / `_detach_condition()` →
    `int2dds_waitset_attach_statuscondition()` / `_detach_statuscondition()`
  - `int2dds_waitset_attach_guard_condition()` / `_detach_guard_condition()` →
    `int2dds_waitset_attach_guardcondition()` / `_detach_guardcondition()`
  - `int2dds_get_builtin_subscriber()` →
    `int2dds_participant_get_builtin_subscriber()`
  - `int2dds_take_publication_data()` →
    `int2dds_subscriber_take_publication_data()`
  - `int2dds_wait_for_type_object()` →
    `int2dds_participant_wait_for_type_object()`
- Optional arguments replace `_with_*` variants. Creation functions now take
  QoS and listener arguments directly: a `NULL` QoS means "use the default",
  and a `NULL` listener with mask `0` means "no listener".
  - `int2dds_create_publisher_with_qos()` / `int2dds_create_subscriber_with_qos()`
    folded into `int2dds_create_publisher()` / `int2dds_create_subscriber()`
  - `int2dds_create_datawriter_with_listener()` /
    `int2dds_create_datareader_with_listener()` folded into
    `int2dds_create_datawriter()` / `int2dds_create_datareader()`
  - `int2dds_create_topic_keyed()` removed; keyed types now go through
    `int2dds_create_topic_with_type_info()`, while `int2dds_create_topic()`
    covers the keyless raw-bytes path
- Argument order normalized on `int2dds_create_participant()`: the domain ID now
  precedes the QoS pointer (`factory, domain_id, qos, &participant`).
- QoS-profile creation paths added for every entity:
  `int2dds_create_participant_with_profile()`,
  `int2dds_create_publisher_with_profile()`,
  `int2dds_create_subscriber_with_profile()`,
  `int2dds_create_datawriter_with_profile()`,
  `int2dds_create_datareader_with_profile()`,
  `int2dds_create_topic_with_profile()`.
- `int2dds_create_datareader_cft()` added for creating a reader on a
  ContentFilteredTopic.
- Publication discovery data is now returned as
  `Int2DdsPublicationBuiltinTopicData` and read through the standard builtin
  topic data accessors plus
  `int2dds_publication_builtin_topic_data_take_type_object()`.

### Removed

- `int2dds_waitset_wait()` — deprecated because it discarded the triggered
  conditions. Use `int2dds_waitset_wait_ex()` or `int2dds_waitset_wait_ex_ns()`.
- `int2dds_publication_data_topic_name()`,
  `int2dds_publication_data_type_name()`,
  `int2dds_publication_data_take_type_object()`, and
  `int2dds_publication_data_destroy()` — superseded by the builtin topic data
  accessors above.
- Unused C code-generation output in `idl` and unused declarations in the
  `hello_world` FFI example header.

[unreleased]: https://github.com/IntellectusCorp/int2DDS/compare/v0.1.5...HEAD
[0.1.5]: https://github.com/IntellectusCorp/int2DDS/compare/v0.1.4...v0.1.5
[0.1.4]: https://github.com/IntellectusCorp/int2DDS/compare/v0.1.3...v0.1.4
[0.1.3]: https://github.com/IntellectusCorp/int2DDS/compare/v0.1.1...v0.1.3
[0.1.1]: https://github.com/IntellectusCorp/int2DDS/releases/tag/v0.1.1

