# int2dds-idl

OMG IDL code generator for int2DDS. Parses `.idl` files and emits ready-to-use type
definitions for multiple targets — Rust, C, Python, C#, plus an XML type representation
and RPC scaffolding. Generated types implement int2DDS's `DdsType` (Rust) / equivalents,
so they serialize/deserialize over DDS out of the box.

The crate provides both a CLI binary (`int2dds-idl`) and a library (`int2dds_idl`)
exposing the parser → resolver → codegen pipeline.

## Build

From the workspace root or the `idl/` directory:

```bash
# Build the generator
cargo build -p int2dds-idl --release

# The binary lands at:
#   target/release/int2dds-idl(.exe)
```

Run it directly via cargo during development:

```bash
cargo run -p int2dds-idl -- <OPTIONS> <INPUT.idl>
```

## Quick start

Given `input/HelloWorld.idl`:

```idl
struct HelloWorld {
    unsigned long index;
    string message;
};
```

Generate Rust and C:

```bash
# Rust
int2dds-idl input/HelloWorld.idl --rust hello_world.rs

# C header
int2dds-idl input/HelloWorld.idl --c-header hello_world.h

# Several targets at once into a directory (files auto-named from the IDL)
int2dds-idl input/HelloWorld.idl --output-dir generated/
```

If no target flag and no `--output-dir` is given, the generator defaults to writing
`<name>.rs` and `<name>.h` in the current directory.

## CLI options

```
Usage: int2dds-idl [OPTIONS] <INPUT.idl>

OPTIONS:
    -r, --rust <PATH>         Generate Rust output to PATH
    -c, --c-header <PATH>     Generate C header output to PATH
    -p, --python <PATH>       Generate Python output to PATH
    -s, --csharp <PATH>       Generate C# output to PATH
    -x, --xml <PATH>          Generate XML type representation to PATH
    -o, --output-dir <DIR>    Output directory (auto-names files)
    -I, --include <DIR>       Add a search dir for #include resolution (repeatable)
    --crate-path <PATH>       Rust crate path (default: int2dds)
    --python-module <PATH>    Python module path (default: int2dds)
    --csharp-namespace <NS>   C# namespace (default: GeneratedTypes)
    --string-bound <N>        Default unbounded string size in C (default: 256)
    --string-pointer          Use char* pointers for strings (OMG standard)
    --rpc <PATH>              Generate RPC types (base types + RPC infrastructure)
    --ros2                    Use ROS2-compatible DDS type naming (scope::dds_::Name_);
                              flat IDL infers the package from a <package>/msg/File.idl path
    --ros2-package <NAME>     Override the package for flat (module-less) IDL under --ros2
    --ros2-kind <KIND>        Interface kind (msg|srv|action) for --ros2-package (default: msg)
    -h, --help                Print help
    -V, --version             Print version
```

Multiple targets may be combined in a single invocation:

```bash
int2dds-idl input/Strings.idl \
  --rust   generated/strings.rs \
  --python generated/strings.py \
  --csharp generated/Strings.cs \
  --crate-path my_dds \
  --csharp-namespace MyApp.Types
```

## Supported IDL

The parser/resolver covers the IDL constructs int2DDS uses on the wire:

| Construct        | Example                                               |
|------------------|-------------------------------------------------------|
| Primitives       | `boolean octet char/wchar short/long/long long (+ unsigned) float double` |
| Strings          | `string`, `string<256>`, `wstring`, `wstring<N>`      |
| Sequences        | `sequence<long>`, `sequence<long, 10>` (bounded)      |
| Arrays           | `long values[16]`                                     |
| Maps             | `map<long, string>`                                   |
| Structs          | nested structs, struct inheritance (`@key` members)   |
| Enums            | `enum Color { RED, GREEN, BLUE };`                    |
| Unions           | `union U switch(long) { case 0: long a; default: ...}`|
| Bitmask / Bitset | `@bit_bound(8) bitmask`, `bitset { bitfield<3> a; }`  |
| Modules          | `module pkg { module msg { struct T {...}; }; };`     |
| Constants        | `const uint8 STATUS_FIX = 0;` (literal values; emitted per target) |
| Includes         | `#include "pkg/msg/Type.idl"` (resolved via `-I <dir>`; resolve-only) |

### Includes (`#include`)

`#include "pkg/msg/Type.idl"` directives are resolved (relative to the including
file, then each `-I <dir>`) and the referenced files are loaded so cross-package
type references resolve. They are **resolve-only**: included types and constants
populate the symbol table but are **not** emitted into the current file's output.
Only the types/constants declared in the input file itself are generated, so each
file's output stays self-contained and free of duplicate or leaf-name-colliding
definitions (e.g. `pkg_a::msg::Status` vs `pkg_b::msg::Status`). Generate each
package's own types from its own `.idl`; references to included types are left as
plain type names for the consuming build to provide.

**Reference contract.** A member whose type comes from an included file is emitted
as a reference to that type in the *dependency's* package, not a re-definition:

- **Rust:** the field uses the full package path, e.g.
  `pub header: std_msgs::msg::Header,`. The output compiles as-is provided the
  consuming build exposes each dependency package at that path (one module/crate
  per package, types at `<pkg>::<kind>::<Type>`). Serialization of the nested type
  travels with its own `#[derive(DdsType)]`, so no extra `use` is needed.
- **C / Python / C#:** still emit the bare leaf name for external references; the
  consuming build must bring the dependency type (and, for C/Python, its
  serialization helpers) into scope. Full path/import emission for these backends
  is pending.

This contract is the integration point for per-package generators such as the ROS2
`rmw` layer, which generates each package separately and wires them together.

Unresolvable includes are reported as a warning and skipped; if a missing include
is actually referenced, resolution then fails with an `unresolved type` error.

### Annotations

| Annotation               | Meaning                                               |
|--------------------------|-------------------------------------------------------|
| `@key`                   | Marks a key member                                    |
| `@extensibility(FINAL\|APPENDABLE\|MUTABLE)` | Type extensibility (default: APPENDABLE) |
| `@id(N)` / `@autoid(...)`| Explicit member id / struct-level auto id policy       |
| `@optional`              | Optional member                                       |
| `@external`              | `@external` (Box-style indirection)                   |
| `@hashid` / `@hashid("name")` | MD5-based 28-bit member id                       |
| `@bit_bound(N)`          | Bitmask bit bound                                     |
| `@position(N)`           | Bitmask flag position                                 |
| `bitfield<N>`            | Bitset field width                                    |

Example with annotations (`input/Strings.idl`):

```idl
@extensibility(APPENDABLE)
struct StringsType {
    @key long id;
    string unbounded_str;
    string<256> bounded_str;
};
```

## Using generated Rust types

The Rust backend emits `#[derive(DdsType)]` types that plug directly into int2DDS:

```rust
// generated hello_world.rs
use int2dds::DdsType;

#[derive(DdsType)]
pub struct HelloWorld {
    pub index: u32,
    pub message: String,
}
```

Reference the generated module from your crate and use it with a `DataWriter` /
`DataReader` as any other `DdsType`. Use `--crate-path` if your int2DDS dependency is
renamed (the default is `int2dds`).

## ROS2-compatible naming (`--ros2`)

How the `--ros2` option works: it rewrites the **registered DDS type
name** to the ROS2-over-DDS convention `scope::dds_::Name_` so int2DDS types interoperate
with ROS2 nodes. Only the registered/type-object name changes — struct fields and
serialization are untouched.

```bash
# Module-based IDL: scope comes from the modules
#   module robot_msgs { module msg { struct Pose {...}; }; };
int2dds-idl Pose.idl --rust pose.rs --ros2
#   -> robot_msgs::msg::dds_::Pose_

# Flat IDL in a ROS2-standard layout: package inferred from the path
int2dds-idl my_pkg/msg/HelloWorld.idl --rust hello.rs --ros2
#   -> my_pkg::msg::dds_::HelloWorld_

# Flat IDL elsewhere: supply the package explicitly
int2dds-idl HelloWorld.idl --rust hello.rs --ros2 --ros2-package my_pkg
#   -> my_pkg::msg::dds_::HelloWorld_
```

For actual ROS2 ↔ DDS communication, also align at runtime: ROS2 prefixes topics with
`rt/`, defaults to XCDR1 data representation, and uses its standard QoS profiles.

> **Note (nested types):** flat ROS2 messages match fully. For nested messages (a struct
> referencing another ROS2-mangled type), only the top-level type's own name is mangled;
> the nested member reference in the TypeObject still uses the plain identifier, so strict
> XTypes TypeObject-hash matching of nested types is not yet covered.

## Library use

The pipeline is also available as a library:

```rust
use int2dds_idl::{parser, resolver, codegen, naming};

let source = std::fs::read_to_string("HelloWorld.idl")?;
let defs   = parser::parse_idl(&source)?;
let model  = resolver::resolve(defs)?;             // -> IdlModel (resolved IR)

let opts = codegen::rust::RustOptions { crate_path: "int2dds".into() };
let code = codegen::rust::generate(&model, "HelloWorld.idl", &opts);
std::fs::write("hello_world.rs", code)?;
```

`IdlModel` (in `int2dds_idl::types`) is the resolved intermediate representation shared by
all backends (`codegen::{rust, c, python, csharp, xml, rpc}`).

## Project layout

```
idl/
├── src/
│   ├── main.rs            CLI entry point / argument parsing
│   ├── lib.rs             Library exports
│   ├── parser/            IDL lexer + grammar -> AST
│   ├── resolver.rs        AST -> resolved IdlModel (type resolution)
│   ├── types.rs           IdlModel / ResolvedType IR
│   ├── naming.rs          Name conversions, keyword escaping, ROS2 mangling
│   ├── keywords.rs        Per-language reserved-word lists
│   └── codegen/           Backends: rust, c, python, csharp, xml, rpc
├── input/                 Sample .idl files
├── output/                Sample generated output
└── tests/                 Integration tests + fixtures
```

## Tests

```bash
cargo test -p int2dds-idl
```
