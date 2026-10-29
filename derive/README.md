# int2dds-derive

Procedural macros for [int2dds](https://crates.io/crates/int2dds), a Rust implementation of the
OMG Data Distribution Service (DDS) with RTPS protocol support.

This crate provides `#[derive(DdsType)]`, which generates the type support a Rust type needs to be
used as a DDS topic type: XCDR serialization, key handling, and the XTypes TypeObject.

The generated code depends on `int2dds`, so both crates must be added as dependencies.

## Installation

```toml
[dependencies]
int2dds = "1.0.0"
int2dds-derive = "1.0.0"
speedy = "0.8"
log = "0.4"
```

## Usage

```rust,ignore
use int2dds_derive::DdsType;

#[derive(DdsType)]
#[dds_type(final)] // extensibility: final, appendable (default) or mutable
struct SensorReading {
    #[dds(key)] // key member
    sensor_id: u32,
    value: f64,
    #[dds(optional)] // optional member
    unit: Option<String>,
}
```

The derive also implements `Debug`, `Clone`, `PartialEq` and `Default` for the type.
Use `#[dds_type(no_default)]`, `#[dds_type(no_partialeq)]` or `#[dds_type(no_additional_derives)]`
to opt out.

For more examples and usage with int2dds, see the [int2DDS repository](https://github.com/IntellectusCorp/int2DDS).

## License

Licensed under the [Apache License 2.0](https://github.com/IntellectusCorp/int2DDS/blob/main/LICENSE).
