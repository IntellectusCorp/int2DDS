using System.Reflection;
using System.Runtime.CompilerServices;
using System.Runtime.InteropServices;

namespace Int2Dds.Interop;

internal static class NativeLibraryLoader
{
    internal const string LibraryName = "int2dds_ffi";

    [ModuleInitializer]
    internal static void Initialize()
    {
        NativeLibrary.SetDllImportResolver(
            typeof(NativeLibraryLoader).Assembly,
            ResolveLibrary);
    }

    private static nint ResolveLibrary(string libraryName, Assembly assembly, DllImportSearchPath? searchPath)
    {
        if (libraryName != LibraryName)
            return nint.Zero;

        // Platform-specific library file name
        string libFileName;
        if (RuntimeInformation.IsOSPlatform(OSPlatform.Windows))
            libFileName = "int2dds_ffi.dll";
        else if (RuntimeInformation.IsOSPlatform(OSPlatform.OSX))
            libFileName = "libint2dds_ffi.dylib";
        else
            libFileName = "libint2dds_ffi.so";

        // 1. Environment variable
        var envPath = Environment.GetEnvironmentVariable("INT2DDS_FFI_PATH");
        if (!string.IsNullOrEmpty(envPath))
        {
            if (NativeLibrary.TryLoad(envPath, out var handle))
                return handle;
        }

        // 2. Relative to assembly location
        var assemblyDir = Path.GetDirectoryName(assembly.Location) ?? ".";
        string[] relativePaths =
        [
            Path.Combine(assemblyDir, libFileName),
            Path.Combine(assemblyDir, "..", "..", "..", "..", "..", "target", "release", libFileName),
            Path.Combine(assemblyDir, "..", "..", "..", "..", "..", "target", "debug", libFileName),
            Path.Combine(assemblyDir, "..", "..", "..", "..", "..", "ffi", "target", "release", libFileName),
            Path.Combine(assemblyDir, "..", "..", "..", "..", "..", "ffi", "target", "debug", libFileName),
        ];

        foreach (var path in relativePaths)
        {
            if (NativeLibrary.TryLoad(path, out var handle))
                return handle;
        }

        // 3. System default search
        if (NativeLibrary.TryLoad(libFileName, assembly, searchPath, out var systemHandle))
            return systemHandle;

        return nint.Zero;
    }
}
