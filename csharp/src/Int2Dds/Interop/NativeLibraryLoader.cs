using System;
using System.IO;
using System.Reflection;
using System.Runtime.InteropServices;

namespace Int2Dds.Interop
{
    internal static class NativeLibraryLoader
    {
        internal const string LibraryName = "int2dds_ffi";

        /// <summary>
        /// On netstandard2.1, we rely on the OS's native library search.
        /// Users should ensure the native library is on PATH (Windows)
        /// or LD_LIBRARY_PATH (Linux), or in the application directory.
        ///
        /// The INT2DDS_FFI_PATH environment variable can be used by setting
        /// the directory on the platform search path before loading the assembly.
        /// </summary>
        static NativeLibraryLoader()
        {
            Initialize();
        }

        internal static void Initialize()
        {
            // Try to add INT2DDS_FFI_PATH to the DLL search path if set.
            var envPath = Environment.GetEnvironmentVariable("INT2DDS_FFI_PATH");
            if (!string.IsNullOrEmpty(envPath))
            {
                try
                {
                    if (RuntimeInformation.IsOSPlatform(OSPlatform.Windows))
                    {
                        // On Windows, use SetDllDirectory or prepend to PATH
                        var currentPath = Environment.GetEnvironmentVariable("PATH") ?? "";
                        Environment.SetEnvironmentVariable("PATH", envPath + ";" + currentPath);
                    }
                    else
                    {
                        // On Linux/macOS, prepend to LD_LIBRARY_PATH / DYLD_LIBRARY_PATH
                        var varName = RuntimeInformation.IsOSPlatform(OSPlatform.OSX)
                            ? "DYLD_LIBRARY_PATH"
                            : "LD_LIBRARY_PATH";
                        var currentPath = Environment.GetEnvironmentVariable(varName) ?? "";
                        Environment.SetEnvironmentVariable(varName, envPath + ":" + currentPath);
                    }
                }
                catch
                {
                    // Best effort - if we can't set the path, the user can set it manually
                }
            }
        }
    }
}
