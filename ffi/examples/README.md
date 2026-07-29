# int2dds FFI Examples

Minimal C hello_world examples for the int2dds FFI bindings. The full C example
suite (listener, waitset, keyed data, multiple participants, XTypes, QoS
profiles) lives in the separate
[int2DDS-examples](https://github.com/IntellectusCorp/int2DDS-examples) repository.

## Directory Structure

```
examples/
├── CMakeLists.txt          # Build configuration
├── README.md               # This file
└── hello_world/            # Basic pub-sub smoke examples
    ├── hello_world_pub.c
    ├── hello_world_sub.c
    └── hello_world.h        # Generated from idl/input/HelloWorld.idl by int2dds-idl
```

> `hello_world.h` is generated from `idl/input/HelloWorld.idl` with int2dds-idl
> (`int2dds-idl -c hello_world.h idl/input/HelloWorld.idl`); do not edit it by hand.

## Prerequisites

1. Build the int2dds-ffi library first:
   ```bash
   cargo build --package int2dds-ffi
   ```
2. Install CMake (3.10 or later)
3. Install a C compiler (MSVC on Windows, GCC/Clang on Linux/macOS)

## Building

### Windows (Visual Studio)

```powershell
cd ffi/examples
mkdir build
cd build
cmake ..
cmake --build .                 # Debug (default)
# Release:
cmake .. -DCMAKE_BUILD_TYPE=Release
cmake --build . --config Release
```

**Note:** Visual Studio is a multi-configuration generator, so specify
`--config Debug` or `--config Release` when building.

### Linux/macOS

```bash
cd ffi/examples
mkdir build && cd build
cmake ..
make
```

## Running

Executables are in `build/Debug` (or `build/Release`) on Windows, and directly in
`build` on Linux/macOS.

**Best Effort QoS (default)** — two terminals:

```bash
./hello_world_sub      # terminal 1
./hello_world_pub      # terminal 2
```

**Reliable QoS:**

```bash
./hello_world_sub --reliable
./hello_world_pub --reliable
```

**Custom domain** (`-d` is an alias for `--domain`):

```bash
./hello_world_pub --domain 10
./hello_world_sub --domain 10
```

## Data Format

Samples are serialized with the IDL-generated CDR functions in `hello_world.h`
(`HelloWorld_serialize_cdr` / `HelloWorld_deserialize_cdr`), the same wire format
the Rust, C#, and Python HelloWorld examples use — so the C examples interoperate
with them on the same domain.

## Notes

- **Build configuration:** on Windows, use `--config Debug`/`--config Release`.
- **QoS matching:** publisher and subscriber must use matching QoS (reliability, durability, …).
- Both run until interrupted (Ctrl-C): the publisher sends one message per second; the subscriber prints samples as they arrive.
