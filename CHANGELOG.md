# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Changed

-

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
- UDP broadcast support (configurable)
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
