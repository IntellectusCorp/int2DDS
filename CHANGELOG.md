# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Changed

- **BREAKING (FFI)**: Consolidated entity creator functions along the QoS/listener axes.
  Every creator now takes a nullable `qos` (NULL = default-QoS resolution chain), and
  writer/reader creators always take `listener`/`mask` (NULL + 0 = no listener):
  - `int2dds_create_participant(factory, domain_id, qos, out)` — unused `name` argument
    removed; absorbs `int2dds_create_participant_with_qos`
  - `int2dds_create_participant_with_profile(factory, domain_id, qos_path, out)` — `name` removed
  - `int2dds_create_publisher` / `int2dds_create_subscriber` gained a `qos` argument;
    `_with_qos` variants removed
  - `int2dds_create_datawriter` / `int2dds_create_datareader` /
    `int2dds_create_datareader_cft` and their `_with_profile` variants gained
    `listener`/`mask`; `_with_listener` and `_with_profile_and_listener` variants removed
  - `int2dds_create_topic_keyed` and `int2dds_create_topic_keyed_with_key_fields` removed
    (keyed raw topics always returned UNSUPPORTED since #334; use
    `int2dds_create_topic_with_type_info` / `_with_field_descriptors`);
    `int2dds_create_topic_with_profile` lost its dead `has_key` argument
- **BREAKING (FFI)**: Removed duplicate/deprecated symbols:
  `int2dds_get_default_participant_qos` (use
  `int2dds_domain_participant_factory_get_default_participant_qos`),
  `int2dds_waitset_wait` (use `_wait_ex`/`_wait_ex_ns`), and the
  `int2dds_waitset_attach/detach_datareader/datawriter` convenience wrappers
  (use `<entity>_get_statuscondition` + `attach_statuscondition`)
- **BREAKING (FFI)**: Normalized entity-first naming:
  - `int2dds_write_serialized*`, `prepare/commit/abort_serialized_write` → `int2dds_datawriter_*`
  - `int2dds_take/read_serialized*`, `*_instance_serialized_batch`,
    `take_serialized_loaned`, `return_serialized_loan` → `int2dds_datareader_*`
  - `*_serialized*_w_condition` → `*_w_states` (arguments are state masks, not condition handles)
  - `int2dds_get_publication/subscription_matched_status` →
    `int2dds_datawriter/datareader_get_*_matched_status`, now returning the full
    `Int2DdsPublicationMatchedStatus` / `Int2DdsSubscriptionMatchedStatus` structs
  - `int2dds_get_builtin_subscriber`, `int2dds_wait_for_type_object`,
    `int2dds_take_discovered_*_snapshot` → `int2dds_participant_*`;
    `int2dds_take_publication_data` → `int2dds_subscriber_take_publication_data`
  - `int2dds_waitset_attach/detach_condition` → `attach/detach_statuscondition`
- **BREAKING (FFI)**: Second consolidation pass (duplicate declarations, redundant
  arguments, naming stragglers):
  - `int2dds_create_topic_with_field_descriptors` lost its redundant `has_key`
    argument — it is now derived from the `field_is_key` array
  - Removed the duplicate publication-data accessor family
    (`int2dds_publication_data_topic_name`/`_type_name`/`_take_type_object`/`_destroy`
    over `Int2DdsPublicationBuiltinData`); `int2dds_subscriber_take_publication_data`
    now returns `Int2DdsPublicationBuiltinTopicData` — use the
    `int2dds_publication_builtin_topic_data_*` accessors plus the new
    `int2dds_publication_builtin_topic_data_take_type_object`
  - `int2dds_guard_condition_*` → `int2dds_guardcondition_*` and
    `int2dds_waitset_attach/detach_guard_condition` → `attach/detach_guardcondition`
    (aligns with `statuscondition`/`readcondition`)
  - `int2dds_publication/subscription_builtin_topic_data_seq_len`/`_seq_destroy` →
    `_seq_length`/`_seq_delete` (aligns with `sample_seq`/`condition_seq`)
  - `int2dds_datareader_take/read_w_readcondition` →
    `take/read_serialized_batch_w_readcondition` (they return an `Int2DdsSampleSeq`
    like the other `_serialized_batch` functions)
  - `int2dds_load_profiles` / `int2dds_get_dynamic_type_support` dropped their unused
    `factory` argument (module-level utilities; participant creators keep the factory)
- Added `int2dds_subscriber_get_instance_handle` (pairs with the publisher variant),
  exposed as `Subscriber.get_instance_handle()` (Python) and
  `Subscriber.GetInstanceHandle()` (C#)
- Fixed: `int2dds_create_datareader_cft` with NULL qos now engages the default-QoS
  resolution chain (registered default → default profile → spec default) instead of
  bypassing it with a bare spec default
- Python/C# bindings updated to the new FFI surface (high-level APIs unchanged;
  the `name` parameter of participant constructors is retained as a no-op label)

### Planned

- Public release preparation
- API stabilization
- Performance optimizations
- Additional interoperability testing

## [0.0.1] - TBD

### Added

#### Core DDS Features

- RTPS 2.5 protocol implementation
- DomainParticipant entity for domain management
- Publisher and DataWriter for data publication
- Subscriber and DataReader for data subscription
- Topic management with type support
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
- Network interface and IP address selection
- Configurable socket buffer sizes

#### Type System

- `#[derive(DdsType)]` procedural macro for automatic type support
- Support for primitive types (bool, integers, floats)
- Support for complex types (String, Vec, HashMap)
- Nested struct support
- Array and bounded sequence support
- Key field support with `#[dds(key)]` attribute
- XCDR2 serialization support (Final, Appendable, Mutable types)

#### FFI Support

- C language bindings (int2DDS-ffi)
- Static and dynamic library builds
- C header generation with cbindgen

#### Developer Tools

- Thread monitoring and logging (INT2DDS_THREAD_MONITORING)
- Function timing and performance profiling (INT2DDS_FUNCTION_TIMING)
- Comprehensive logging with RUST_LOG
- Environment variable configuration

#### Examples

- Hello World

#### Interoperability

- Tested with CycloneDDS
- Tested with FastDDS
- Shapes demo for cross-vendor testing
- RTPS protocol compliance

#### Platform Support

- Windows support
- Linux support
- macOS support

#### Documentation

- API documentation with examples
- Build and usage instructions
- Environment variable documentation
- Performance tuning guide
- Function timing guide

### Changed

- N/A (initial release)

### Deprecated

- N/A (initial release)

### Removed

- N/A (initial release)

### Fixed

- N/A (initial release)

### Security

- N/A (initial release)

## Release Notes

### Version 0.0.1 - Initial Development Release

This is the initial development version of int2DDS, a Rust implementation of the DDS middleware standard. While functional and tested, this version is not yet production-ready and APIs may change before 1.0.0.

**Status**: Pre-release / Development

**Key Highlights**:

- Full RTPS 2.5 protocol support
- working examples
- Interoperability with major DDS implementations
- Comprehensive QoS policy support
- Multi-transport support (UDP, TCP, hybrid)
- Cross-platform support

**Known Limitations**:

- APIs are not yet stabilized and may change
- Some advanced DDS features are not yet implemented
- Performance optimizations are ongoing
- Documentation is being expanded

**Next Steps**:

- API stabilization for 0.1.0 release
- Performance benchmarking and optimization
- Expanded test coverage
- Production hardening

---

[unreleased]: https://github.com/IntellectusCorp/int2DDS/compare/v0.0.1...HEAD
[0.0.1]: https://github.com/IntellectusCorp/int2DDS/releases/tag/v0.0.1
