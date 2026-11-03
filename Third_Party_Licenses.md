# Third-Party Licenses

This document describes how **int2DDS** handles the licenses of the third-party
components it depends on. int2DDS itself is licensed under the Apache License 2.0
(see `LICENSE` and `NOTICE`).

## 1. Source distributions

The source repository and the source packages built from it (the `int2dds`,
`int2dds-derive`, `int2dds-idl` and `int2dds-rpc` crates on crates.io, and the
Python sdist) bundle no third-party code. Rust dependencies are declared in the
`Cargo.toml` files and fetched from crates.io by the consumer's own build.

## 2. Binary distributions

The binary artifacts published with each release statically link third-party
Rust crates:

- the native archives (`int2dds-<version>-<platform>.tar.gz` / `.zip`),
- the Python wheels,
- the Java JAR,
- the C# NuGet package,
- the ROS 2 vendor tarball (`int2dds-ffi-<version>-linux.tar.gz`).

Each of these ships `LICENSE-THIRD-PARTY.md` next to `LICENSE` and `NOTICE`. The
file is generated at release time by
[cargo-about](https://github.com/EmbarkStudios/cargo-about) from `about.toml` and
`about.hbs` in this repository. It lists every crate in the dependency graph of
the binaries together with the full text of its license, including the copyright
notices that the MIT, ISC, BSD and Unicode licenses require to accompany binary
copies. Procedural-macro crates run only at build time but are listed as well. A
crate that ships no license file is represented by the standard text of its
declared license.

`Int2Dds-<version>-dotnet-libs.zip` contains managed assemblies only and carries
the same three files for consistency.

## 3. License policy

- A dependency may only be used under one of the licenses listed in `accepted`
  in `about.toml`. `cargo about generate --fail` runs in the pull request CI and
  in the release workflow, so a dependency that introduces any other license
  fails the build.
- Where a crate offers a choice of licenses (for example `MIT OR Apache-2.0`),
  the first match in the order of `accepted` is used. Apache-2.0 comes first.
- None of the crates in the binaries' dependency graph ships an Apache `NOTICE`
  file, so there is nothing to pass on under section 4(d) of the license.
  cargo-about collects license texts only, so the pull request CI also fails
  when such a crate starts shipping a `NOTICE` file; its contents then have to
  be carried into `LICENSE-THIRD-PARTY.md`.
