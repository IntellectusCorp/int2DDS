# int2dds-java

Java binding for int2DDS. A JNI layer (`int2dds-java`, a Rust `cdylib`) exposes the
core's C ABI, and the `api` module wraps it in a DDS-shaped Java API.

## Requirements

- **JDK 17** to run Gradle itself (the wrapper is included — no separate Gradle install).
- The published classes target **Java 8** (`options.release.set(8)`), with
  multi-release source sets for 9 and 22. The test suite runs on 8, 11, 17, 21 or 25
  via `./gradlew test -PtestJavaVersion=<N>`.
- A Rust toolchain to build the native library.

## Building

The native library must exist before any test or example runs:

```bash
# From the repository root
cargo build --release -p int2dds-java
# -> target/release/libint2dds_java.{so,dylib} / int2dds_java.dll
```

Then:

```bash
cd java
./gradlew build
./gradlew test
```

The native loader looks in this order: the `INT2DDS_JAVA_LIB` environment variable →
bundled JAR resources → `java.library.path`. Gradle's own test tasks set
`INT2DDS_JAVA_LIB` for you; set it by hand only when running outside Gradle:

```bash
export INT2DDS_JAVA_LIB=/abs/path/to/target/release/libint2dds_java.so
```

## Running the examples

```bash
cd java
./gradlew :examples:runSub    # subscriber, in one terminal
./gradlew :examples:run       # publisher, in another
./gradlew :examples:runDynamic # XTypes dynamic-type variant
```

Both take `-d`/`--domain <id>` (default 0) and `--reliable` (default `BEST_EFFORT`);
pass the same reliability on both sides for the two to match. They use the same topic
and type name as the C# and Rust `HelloWorldPub` examples, so any pair of them
interoperates on the wire.

## Quick start

### 1. Define your type

Types implement `IDdsType`. Write one by hand, or generate it — see
[Using IDL code generation](#using-idl-code-generation) below.

```java
public final class HelloWorld implements IDdsType {

    public int index;
    public String message = "";

    @Override public String typeName() { return "HelloWorld"; }
    @Override public Extensibility extensibility() { return Extensibility.APPENDABLE; }

    @Override
    public void serializeCdr(CdrWriter writer) {
        int token = writer.dheaderBegin();
        writer.writeU32(index);
        writer.writeString(message);
        writer.dheaderFinalize(token);
    }

    @Override
    public void deserializeCdr(CdrReader reader) {
        CdrReader.Dheader d = reader.readDheader();
        index = reader.readU32();
        message = reader.readString();
        reader.readDheaderEnd(d);
    }
}
```

`createTopic` takes an *instance* rather than a `Class<T>`, because that instance
supplies the type name and extensibility without a reflection lookup on every call.

### 2. Publisher

```java
try (DomainParticipant participant = new DomainParticipant(0)) {
    Topic<HelloWorld> topic = participant.createTopic("hello_world_topic", new HelloWorld());
    Publisher publisher = participant.createPublisher();

    DataWriterQos qos = new DataWriterQos();
    qos.setReliability(new Reliability(ReliabilityKind.RELIABLE));
    DataWriter<HelloWorld> writer = publisher.createDataWriter(topic, qos);

    HelloWorld sample = new HelloWorld();
    sample.index = 0;
    sample.message = "Hello from Java!";
    writer.write(sample);
}
```

`DomainParticipant.close()` cascades to every live child — topic, publisher, writer —
so one try-with-resources on the participant is enough.

### 3. Subscriber

```java
try (DomainParticipant participant = new DomainParticipant(0)) {
    Topic<HelloWorld> topic = participant.createTopic("hello_world_topic", new HelloWorld());
    Subscriber subscriber = participant.createSubscriber();

    DataReaderQos qos = new DataReaderQos();
    qos.setReliability(new Reliability(ReliabilityKind.RELIABLE));
    DataReader<HelloWorld> reader = subscriber.createDataReader(topic, HelloWorld::new, qos);

    try (WaitSet waitSet = new WaitSet();
            StatusCondition condition = reader.getStatusCondition()) {
        condition.setEnabledStatuses(StatusMask.of(StatusMask.DATA_AVAILABLE));
        waitSet.attach(condition);

        List<Condition> triggered = waitSet.await(1000L);
        Sample<HelloWorld> sample;
        while ((sample = reader.take()) != null) {
            HelloWorld data = sample.data();
            if (data != null) {           // null when info().validData() is false
                System.out.println(data.message);
            }
        }
    }
}
```

`createDataReader` takes a supplier (`HelloWorld::new`) so the reader can construct
samples without reflection.

## Using IDL code generation

Generate Java from an `.idl` file:

```bash
# From the repository root
cargo run -p int2dds-idl -- idl/input/HelloWorld.idl \
    -j java/examples/src/main/java \
    --java-package com.intellectus.int2dds.examples
```

Unlike every other language flag, `-j` takes a **directory**: Java requires one public
top-level class per file, so an `.idl` declaring several types produces several
`.java` files, nested under the `--java-package` path.

> The bundled `examples/.../HelloWorld.java` is generated exactly this way from
> `idl/input/HelloWorld.idl`; it carries a `DO NOT EDIT` header. The command is
> also recorded in `examples/build.gradle.kts`. The build does **not** run it — an
> example that needed a Rust toolchain to compile would not be one anybody could copy.

Generated types are ordinary `IDdsType` implementations. Two mapping decisions are
worth knowing:

- **Unsigned integers keep their width and wrap.** `unsigned long` becomes `int`, not
  `long`. Use `Integer.toUnsignedLong` / `Short.toUnsignedInt` at the edges where you
  need the numeric value.
- **Sequences and arrays both become Java arrays**, not `List<T>`, so no element is
  boxed on the serialization path.

Every generated struct also overrides `typeInfo()`, a full description of its members
(kinds, bounds, `@key` flags, nested types). `createTopic` uses it to advertise the
same TypeObject the Rust, C# and Python bindings advertise for that IDL, which is what
lets a keyed Java topic match a keyed peer in another language and lets the core
resolve instance keys. A hand-written `IDdsType` that leaves `typeInfo()` at its
default (`null`) gets a key-less topic matched by type name alone.

Because the C ABI has no profile variant of that path, `createTopic(name, prototype,
profilePath)` refuses a keyed type; pass a `TopicQos` instead.

The backend refuses several IDL constructs rather than emitting Java that will not
compile — or that compiles and encodes the wrong bytes. See
[What the Java backend refuses](../idl/README.md#what-the-java-backend-refuses).

## Layout

Gradle modules (`settings.gradle.kts`):

| Module | Contents |
|---|---|
| `api` | The public Java API — packages `core`, `qos`, `cdr`, `types`, `conditions`, `listeners`, `discovery`, `xtypes`, `async`, `status`, `exceptions`, plus internal `internal` |
| `examples` | `HelloWorldPub` / `HelloWorldSub` / `DynamicHelloWorld` |
| `bench` | JMH benchmarks |

`native/` is not a Gradle module — it is the Rust crate (`int2dds-java`) that builds
the JNI library, and is built with `cargo`.

The JNI layer and both `Ffi.java` variants are **generated** from the FFI surface:

```bash
cargo run -p int2dds-java --bin gen-jni
./scripts/check-jni-drift.sh    # fails if the committed output is stale
```

## Tests

```bash
cd java && ./gradlew test          # the Java suite
cargo test -p int2dds-java         # the JNI crate's own tests
./scripts/check-idl-java.sh        # generated Java compiles; committed golden type is current
```

`GeneratedTypeConformanceTest` hands bytes produced by a **generated** type to the
Rust core's own deserializer and asserts what the core decodes. That is deliberately
not a round trip against our own reader: a round trip cannot fail when the writer and
reader share a mistake.
