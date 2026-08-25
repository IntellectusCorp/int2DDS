# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- Java code generation in `int2dds-idl` (`-j`/`--java <DIR>`, `--java-package <PKG>`).
  Unlike the other language flags it takes a directory, because Java requires one
  public top-level class per file. Generated classes implement the Java binding's
  `IDdsType`; a keyed struct also gets a static `ddsFields()` for the keyed
  `createTopic` overload. Batch `--output-dir` now emits Java alongside the other
  targets.
- `scripts/check-idl-java.sh`, which generates every `idl/input/*.idl` file and
  compiles the result with `javac -Xlint:all`, then verifies the committed
  `CdrGolden.java` still matches the generator. Wired into the `java` CI status.
- `GeneratedTypeConformanceTest`, which hands bytes produced by a generated Java
  type to the core's own deserializer rather than round-tripping against our own
  reader.
- `java/README.md`, documenting the binding's build, examples, and IDL workflow.
- `gen-jni --count`, printing the size of the exported C ABI surface.

### Changed

- `java/examples`' `HelloWorld` type is now generated from `idl/input/HelloWorld.idl`
  instead of hand-written. Verified byte-identical on the wire under both XCDR1 and
  XCDR2 before the swap.

### Fixed

- The JNI generator's count guards no longer hardcode the FFI surface size in five
  places and the CI symbol check in a sixth. Four of them now derive from the parsed
  surface, so adding a C ABI function requires updating exactly one constant.

## [0.1.1] - TBD

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

<!--
No version has been tagged yet, so there are no link definitions here: any
compare/ or releases/tag/ URL would 404. When v0.1.1 is tagged, replace the TBD
above with the release date and append:

[unreleased]: https://github.com/IntellectusCorp/int2DDS/compare/v0.1.1...HEAD
[0.1.1]: https://github.com/IntellectusCorp/int2DDS/releases/tag/v0.1.1
-->

