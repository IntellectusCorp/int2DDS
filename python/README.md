# int2dds-py

Python bindings for int2DDS - a high-performance DDS (Data Distribution Service) implementation.

## Installation

```bash
# Install dependencies
pip install cffi

# Install the package in development mode
pip install -e .
```

## Requirements

- Python 3.10+
- cffi
- int2dds_ffi library (built from ../ffi)

### Building the FFI Library

```bash
cargo build --package int2dds-ffi
```

Set the library path:

```bash
# Linux/macOS
export INT2DDS_FFI_PATH=/path/to/libint2dds_ffi.so

# Windows
set INT2DDS_FFI_PATH=C:\path\to\int2dds_ffi.dll
```

## Running the examples

Bundled runnable pub/sub examples live in `examples/`. Start the subscriber
first, then the publisher:

```bash
python examples/hello_world_sub.py
python examples/hello_world_pub.py

# custom domain + reliable QoS
python examples/hello_world_sub.py --domain 10 --reliable
python examples/hello_world_pub.py --domain 10 --reliable
```

Both accept `-d`/`--domain <id>` (default 0) and `--reliable` (default
BEST_EFFORT).

## Quick Start

### 1. Define your data type

Create a Python dataclass with CDR serialization methods (or use int2dds-idl to generate from IDL):

```python
from dataclasses import dataclass
from typing import ClassVar
from int2dds.cdr import CdrWriter, CdrReader, Extensibility

@dataclass
class HelloWorld:
    index: int = 0
    message: str = ""

    _dds_type_name: ClassVar[str] = "HelloWorld"
    _extensibility: ClassVar[Extensibility] = Extensibility.APPENDABLE
    _has_key: ClassVar[bool] = False

    def _serialize_cdr(self) -> bytes:
        w = CdrWriter(extensibility=self._extensibility)
        w.write_u32(self.index)
        w.write_string(self.message)
        return w.to_bytes()

    @classmethod
    def _deserialize_cdr(cls, data: bytes) -> "HelloWorld":
        r = CdrReader(data)
        return cls(
            index=r.read_u32(),
            message=r.read_string(),
        )

    def _serialize_key(self) -> bytes:
        return b""
```

### 2. Publisher

```python
from int2dds import DomainParticipant, WaitSet

# Create participant
with DomainParticipant(domain_id=0) as dp:
    # Create topic
    topic = dp.create_topic("hello_world_topic", HelloWorld)

    # Create publisher and writer
    pub = dp.create_publisher()
    writer = pub.create_datawriter(topic)

    # Wait for a reader to connect
    waitset = WaitSet()
    waitset.attach(writer)
    waitset.wait(timeout=10.0)

    # Write samples
    for i in range(10):
        sample = HelloWorld(index=i, message=f"Hello {i}")
        writer.write(sample)
        print(f"Sent: {sample}")
```

### 3. Subscriber

```python
from int2dds import DomainParticipant, WaitSet, DdsTimeout

with DomainParticipant(domain_id=0) as dp:
    topic = dp.create_topic("hello_world_topic", HelloWorld)

    sub = dp.create_subscriber()
    reader = sub.create_datareader(topic)

    waitset = WaitSet()
    waitset.attach(reader)

    while True:
        try:
            waitset.wait(timeout=5.0)
            for sample in reader.take():
                if sample.valid_data:
                    print(f"Received: {sample.data}")
        except DdsTimeout:
            print("No data received")
            break
```

## Using IDL Code Generation

Generate Python code from IDL:

```bash
int2dds-idl --python hello_world.py HelloWorld.idl
```

> The bundled `examples/hello_world_type.py` is generated this way from
> `idl/input/HelloWorld.idl` (`int2dds-idl --python examples/hello_world_type.py
> idl/input/HelloWorld.idl`); do not edit it by hand.

Example IDL file:

```idl
struct HelloWorld {
    unsigned long index;
    string message;
};
```

## QoS Configuration

```python
from int2dds import DataWriterQos, DataReaderQos, Reliability, Durability, History

# Reliable writer with transient-local durability
writer_qos = DataWriterQos(
    reliability=Reliability("RELIABLE", max_blocking_time=1.0),
    durability=Durability("TRANSIENT_LOCAL"),
    history=History("KEEP_LAST", depth=10),
)
writer = pub.create_datawriter(topic, qos=writer_qos)

# Best-effort reader
reader_qos = DataReaderQos(
    reliability=Reliability("BEST_EFFORT"),
)
reader = sub.create_datareader(topic, qos=reader_qos)
```

## XML-defined runtime types

Load a type from an XML file at runtime — the same `XmlTypeRegistry` workflow as
the Rust and C APIs — and publish/subscribe it with **no compile-time IDL**. The
other DDS vendor XML dialects both parse through the registry.

```python
from int2dds import DomainParticipant
from int2dds.types import XmlTypeRegistry

reg = XmlTypeRegistry.from_file("sensor_data.xml")
support = reg.get_type_support("SensorData")

with DomainParticipant(domain_id=0) as dp:
    topic = dp.create_topic_dynamic("SensorTopic", support)
    writer = dp.create_publisher().create_datawriter_dynamic(topic, support)
    reader = dp.create_subscriber().create_datareader_dynamic(topic, support)

    data = support.create_data()
    data.set_i32("sensor_id", 42)
    data.set_f64("temperature", 23.5)
    writer.write(data)

    sample = reader.take()              # DynamicData or None
    if sample is not None:
        print(sample.get_i32("sensor_id"), sample.get_f64("temperature"))
```

`DynamicData` exposes flat `set_*`/`get_*` accessors (by dotted/indexed path,
e.g. `"pos.x"`, `"items[2]"`). For nested structs, sequences, arrays, maps,
unions, enums, bitmask/bitset and wide strings, build a `DynamicValue` tree and
attach it with `set_value`; read it back with `get_value`:

```python
from int2dds.types import DynamicValue

seq = DynamicValue.sequence()
seq.push(DynamicValue.i32(100)).push(DynamicValue.i32(200))
data.set_value("samples", seq)
data.set_value("mode", DynamicValue.enum("ON", 1))

got = sample.get_value("samples")
print(got.len(), got.element(0).as_i32())
print(sample.get_value("mode").as_enum())   # -> ("ON", 1)
```

See the `xtypes` examples in [int2DDS-examples](https://github.com/IntellectusCorp/int2DDS-examples)
for a runnable pub/sub pair.

## Environment Configuration

The `int2dds.env` module wraps the underlying `INT2DDS_*` environment variables
the Rust core consults during participant creation. Mutate them **before**
creating the first `DomainParticipant`.

```python
from int2dds import env, DomainParticipant

# Equivalent to INT2DDS_MULTICAST_TTL=32 in the process environment.
# Used as a fallback only when QoS does not set multicast_ttl explicitly.
env.set_multicast_ttl(32)
ttl = env.get_multicast_ttl()  # -> 32 or None

with DomainParticipant(domain_id=0) as dp:
    ...
```

For per-participant override, use `Property.set_multicast_ttl(...)` on
`ParticipantQos.property` — explicit QoS always wins over the env var.
See [docs/guide/env.md](../docs/guide/env.md) for the full env-var reference.

## API Reference

### DomainParticipant

- `DomainParticipant(domain_id: int = 0, name: str | None = None)` - Create a participant
- `create_publisher() -> Publisher` - Create a publisher
- `create_subscriber() -> Subscriber` - Create a subscriber
- `create_topic(name: str, type_class: type) -> Topic` - Create a topic

### Publisher / DataWriter

- `Publisher.create_datawriter(topic, qos=None) -> DataWriter`
- `DataWriter.write(sample)` - Write a sample
- `DataWriter.matched_readers` - Number of matched readers

### Subscriber / DataReader

- `Subscriber.create_datareader(topic, qos=None) -> DataReader`
- `DataReader.take() -> list[Sample]` - Take all available samples
- `DataReader.read() -> list[Sample]` - Read samples without removing
- `DataReader.matched_writers` - Number of matched writers

### WaitSet

- `WaitSet()` - Create a waitset
- `attach(condition)` - Attach a condition (GuardCondition, StatusCondition, DataReader, DataWriter)
- `wait(timeout: float | None)` - Wait for conditions (raises DdsTimeout)
