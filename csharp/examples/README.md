# int2Dds C# Examples

Minimal C# hello_world pub/sub examples for the int2Dds .NET bindings.

## Structure

```
examples/
├── HelloWorldPub/
│   ├── HelloWorld.cs   # Generated from idl/input/HelloWorld.idl by int2dds-idl
│   └── Program.cs
└── HelloWorldSub/
    ├── HelloWorld.cs   # Generated from idl/input/HelloWorld.idl by int2dds-idl
    └── Program.cs
```

> The `HelloWorld.cs` type in each project is generated from
> `idl/input/HelloWorld.idl` with int2dds-idl (once per project, with a matching
> `--csharp-namespace`):
>
> ```bash
> int2dds-idl -s HelloWorld.cs --csharp-namespace HelloWorldPub idl/input/HelloWorld.idl
> int2dds-idl -s HelloWorld.cs --csharp-namespace HelloWorldSub idl/input/HelloWorld.idl
> ```
>
> Do not edit it by hand.

## Prerequisites

1. Build the native `int2dds_ffi` library — the C# bindings P/Invoke into it:
   ```bash
   cargo build --package int2dds-ffi
   ```
2. Install the .NET SDK (the projects target net6.0/net8.0/net10.0; net45/net48 too).

The native `int2dds_ffi` library must be resolvable at runtime (e.g. on `PATH`
or copied next to the built executable).

## Running

Start the subscriber first, then the publisher. Arguments after `--` go to the
example: `-d`/`--domain <id>` (default 0) and `--reliable` (default BEST_EFFORT).

```bash
dotnet run --project HelloWorldSub
dotnet run --project HelloWorldPub

# custom domain + reliable QoS
dotnet run --project HelloWorldSub -- --domain 10 --reliable
dotnet run --project HelloWorldPub -- --domain 10 --reliable
```
