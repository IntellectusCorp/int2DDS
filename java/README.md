# int2dds-java

Java binding for int2DDS. The `api` module wraps the core's C ABI in a DDS-shaped Java
API, with one of two native backends underneath, picked by the multi-release JAR:

| Runtime | Backend | Native library |
|---|---|---|
| JDK 8 – 21 | JNI | `int2dds_java` (a Rust `cdylib`, `native/`) |
| JDK 22+ | FFM (Panama) downcalls and upcalls — no JNI at all | `int2dds_ffi`, the plain C ABI library the C# and Python bindings load |

The public API is identical on both; only `kr.co.intellectus.int2dds.internal` classes
differ. `internal.ffi.FfiHandwritten.backend()` returns `"jni"` or `"ffm"` for the backend
a runtime selected.

### JNI and FFM

- **JNI** (Java Native Interface) can only call C functions written against the JNI
  calling convention, so it needs a glue library: `int2dds_java` wraps each C ABI
  function in a JNI export, and listener callbacks reach Java through JNI upcalls.
  It is the only option before JDK 22.
- **FFM** (Foreign Function & Memory API, JEP 454, final in JDK 22) calls C functions
  directly from Java through `MethodHandle` downcalls, and turns Java listeners into
  C function pointers with upcall stubs. No glue library is involved: the JAR loads
  the same `int2dds_ffi` the C, C# and Python bindings use.

Both sides are generated from `ffi/src` (see [Layout](#layout)), so they expose the
same functions. Choosing between them takes no code: the JVM picks the classes for its
own version from the JAR.

## Requirements

- **JDK 17** to run Gradle itself (the wrapper is included — no separate Gradle install).
- The published classes target **Java 8** (`options.release.set(8)`), with
  multi-release source sets for 9 and 22. The test suite runs on 8, 11, 17, 21 or 25
  via `./gradlew test -PtestJavaVersion=<N>`; 22 and later test the FFM backend, with
  the JNI library deliberately made unloadable so nothing can fall back to it.
- The 22 source set is compiled with a JDK 24 toolchain, which Gradle downloads on the
  first build if none is installed.
- A Rust toolchain to build the native library.

## Using the release JAR

Each GitHub release attaches `int2dds-api-<version>.jar`. It is not published to a
Maven repository; add it to the class path directly, or install it into the local
Maven repository with `./gradlew :api:publishToMavenLocal` when building from source.
The JAR has no runtime dependencies.

It bundles both native libraries for six platforms, so no environment variable is
needed on them:

| OS | Architectures |
|---|---|
| Windows | x86_64, aarch64 |
| Linux (glibc) | x86_64, aarch64 |
| macOS | x86_64, aarch64 |

The other platforms the release archives cover (Linux armhf, musl, Windows i686 and
`-gnu`) are not bundled. On JDK 22+, extract `int2dds-<version>-<dist>` and point
`INT2DDS_FFI_PATH` at its `lib/` (or `bin/` on Windows) directory. Those archives do not
contain `int2dds_java`, so on JDK 8 – 21 build it with `cargo` for the target.

## Building

The native libraries must exist before any test or example runs:

```bash
# From the repository root
cargo build --release -p int2dds-java -p int2dds-ffi
# -> target/release/libint2dds_java.{so,dylib} / int2dds_java.dll   (JDK 8-21)
# -> target/release/libint2dds_ffi.{so,dylib}  / int2dds_ffi.dll    (JDK 22+)
```

Then:

```bash
cd java
./gradlew build      # Windows: .\gradlew.bat build
./gradlew test       # Windows: .\gradlew.bat test
```

The native loader looks in this order: an environment variable → bundled JAR resources
(`/native/<os>-<arch>/`) → `java.library.path`, plus, on JDK 22+, the platform loader's own
search (`PATH`, `LD_LIBRARY_PATH`, ...) last. The environment variable depends on the
backend:

| Runtime | Variable | Value |
|---|---|---|
| JDK 8 – 21 | `INT2DDS_JAVA_LIB` | path of `int2dds_java` |
| JDK 22+ | `INT2DDS_FFI_PATH` | path of `int2dds_ffi`, or of its directory — the same variable the C# and Python bindings read |

Gradle's own test and example tasks set both for you, picking the platform's
file names; set them by hand only when running outside Gradle:

```bash
export INT2DDS_JAVA_LIB=/abs/path/to/target/release/libint2dds_java.so
export INT2DDS_FFI_PATH=/abs/path/to/target/release/libint2dds_ffi.so
```

```powershell
$env:INT2DDS_JAVA_LIB = "D:\abs\path\to\target\release\int2dds_java.dll"
$env:INT2DDS_FFI_PATH = "D:\abs\path\to\target\release\int2dds_ffi.dll"
```

Either backend refuses a native library whose product version differs from the JAR's,
since struct layouts and signatures are compiled into the Java side.

FFM is a restricted API: on JDK 22 and later the JVM warns the first time the binding
calls into native code, and so does JNI from JDK 24 (JEP 472). Pass
`--enable-native-access=ALL-UNNAMED` to `java` to acknowledge it; the JAR's own
`Enable-Native-Access` manifest attribute only takes effect with `java -jar`.

The JDK 22+ backend is selected by the JAR's `Multi-Release: true` manifest attribute,
which only applies when the classes run from the JAR. Two setups lose it and fall back to
the JNI classes, which then need `int2dds_java`: running from an exploded class directory
(an IDE launch configuration), and repackaging into a fat JAR without carrying the
attribute over (for example a `maven-shade-plugin` setup that does not set
`Multi-Release: true`).

## Running the examples

Linux / macOS / Git Bash:

```bash
cd java
./gradlew :examples:runSub  # subscriber, in one terminal
./gradlew :examples:runPub  # publisher, in another
```

Windows (PowerShell / cmd):

```bat
cd java
.\gradlew.bat :examples:runSub
.\gradlew.bat :examples:runPub
```

Both take `-d`/`--domain <id>` (default 0) and `--reliable` (default `BEST_EFFORT`);
pass the same reliability on both sides for the two to match. They use the same topic
and type name as the Rust, C# and Python hello_world examples, so any pair of them
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

### 4. Listener instead of a WaitSet

```java
reader.setListener(new DataReaderListenerBase() {
    @Override
    public void onDataAvailable(DataReader<?> r) {
        Sample<HelloWorld> sample;
        while ((sample = reader.take()) != null) {
            if (sample.data() != null) {
                System.out.println(sample.data().message);
            }
        }
    }
}, StatusMask.of(StatusMask.DATA_AVAILABLE));
```

Callbacks run on DDS threads and receive the entity they are installed on. Closing the
reader (or its participant) clears the listener.

## Using IDL code generation

Generate Java from an `.idl` file:

```bash
# From the repository root
cargo run -p int2dds-idl -- idl/input/HelloWorld.idl \
    -j java/examples/src/main/java \
    --java-package kr.co.intellectus.int2dds.examples
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
| `examples` | `HelloWorldPub` / `HelloWorldSub` |

`native/` is not a Gradle module — it is the Rust crate (`int2dds-java`) that builds
the JNI library, and is built with `cargo`. The JDK 22+ backend needs no crate of its
own: it binds `int2dds_ffi` (`../ffi`) directly.

| | JDK 8 – 21 (`src/main/java`) | JDK 22+ (`src/main/java22`) |
|---|---|---|
| `internal.ffi.Ffi` | generated `native` declarations | generated FFM downcalls |
| `internal.ffi.FfiHandwritten` | JNI natives in `native/src` (listeners, QoS properties) | FFM upcalls (`FfmListeners`) and downcalls |
| `internal.NativeLoader` | `System.load` of `int2dds_java` | `SymbolLookup.libraryLookup` of `int2dds_ffi` |

The JNI layer and both `Ffi.java` variants are **generated** from the FFI surface:

```bash
cargo run -p int2dds-java --bin gen-jni
./.github/scripts/ci/check-jni-drift.sh    # fails if the committed output is stale
```

## Tests

```bash
cd java && ./gradlew test          # the Java suite
cargo test -p int2dds-java         # the JNI crate's own tests
./.github/scripts/ci/check-idl-java.sh   # generated Java compiles; committed golden type is current
```

`GeneratedTypeConformanceTest` hands bytes produced by a **generated** type to the
Rust core's own deserializer and asserts what the core decodes. That is deliberately
not a round trip against our own reader: a round trip cannot fail when the writer and
reader share a mistake.
