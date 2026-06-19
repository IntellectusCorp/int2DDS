# Dynamic XTypes Examples (FFI PHASE 1)

This example demonstrates **runtime type discovery in C**: a subscriber that
receives data without compile-time knowledge of the type, by discovering the
publisher's TypeObject through DDS-XTypes discovery and decoding samples
field-by-field through the dynamic FFI.

The publisher uses an IDL-generated header (`sensor_data.h`); the subscriber
intentionally does **not** include it.

## Files

- `sensor_data.h` - auto-generated from `idl/input/SensorData.idl`. Used only
  by the publisher.
- `dynamic_type_publisher.c` - publishes `SensorData` on topic `SensorTopic`.
- `dynamic_type_subscriber.c` - discovers the type at runtime, prints the
  introspected layout, and decodes samples by field name.

## Regenerating `sensor_data.h`

From the `idl/` directory:

```
cargo run -- -c ../ffi/examples/xtypes/sensor_data.h input/SensorData.idl
```

## Building

From `ffi/examples`:

```
cargo build --package int2dds-ffi
mkdir build && cd build
cmake ..
cmake --build .
```

## Running - Scenario 1: C <-> C

Terminal 1:

```
./dynamic_type_publisher --domain 0
```

Terminal 2:

```
./dynamic_type_subscriber --domain 0
```

Expected subscriber output (after match):

```
Dynamic XTypes Subscriber - discovers type at runtime
Waiting for publisher on 'SensorTopic'...
Discovered type: SensorData
Extensibility: Appendable, 4 members:
  [0] id=0 kind=INT32 sensor_id [KEY]
  [1] id=1 kind=FLOAT64 temperature
  [2] id=2 kind=FLOAT64 humidity
  [3] id=3 kind=STRING location

Receiving samples...
[RECV] id=1 temp=20.0 hum=40.0 loc=Lab A
[RECV] id=1 temp=20.5 hum=40.3 loc=Lab B
...
```

## What this phase covers

PHASE 1 supports: primitive members, string members, all three extensibilities
(Final / Appendable / Mutable). See
`docs/superpowers/specs/2026-04-09-ffi-xtypes-parity-roadmap.md` for the full
multi-phase plan.

## XML runtime types

These newer examples load types **defined in XML at runtime** — the same
`XmlTypeRegistry` workflow used from Rust — and publish/subscribe them through
the dynamic FFI without any compile-time IDL.

- `xml_dynamic_publisher.c` / `xml_dynamic_subscriber.c` — both load
  `dds/examples/xtypes/sensor_data.xml` and publish/subscribe `SensorData`
  through `int2dds_xml_type_registry_*` +
  `int2dds_create_{topic,datawriter,datareader}_dynamic` + the
  `int2dds_dynamic_data_*` setters/getters. Run them in two terminals:

  ```
  ./xml_dynamic_subscriber [--domain N] [--xml PATH] [--type NAME]
  ./xml_dynamic_publisher  [--domain N] [--xml PATH] [--type NAME]
  # defaults: --xml ../../dds/examples/xtypes/sensor_data.xml  --type SensorData
  ```

- `xml_parse_check.c` — loads the sample file (`sensor_data.xml`) and confirms
  every declared type resolves to a dynamic type support, demonstrating that XML
  type definitions parse through the C API.

  ```
  ./xml_parse_check [xml_dir]
  # default dir: ../../dds/examples/xtypes
  ```

For richer payloads (nested structs, sequences, maps, unions, enums,
bitmask/bitset, wide strings), build values with the `int2dds_dynamic_value_*`
tree API and attach them via `int2dds_dynamic_data_set_value`.
