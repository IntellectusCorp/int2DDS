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
                    if (IsWindows())
                    {
                        // On Windows, use SetDllDirectory or prepend to PATH
                        var currentPath = Environment.GetEnvironmentVariable("PATH") ?? "";
                        Environment.SetEnvironmentVariable("PATH", envPath + ";" + currentPath);
                    }
                    else
                    {
                        // On Linux/macOS, prepend to LD_LIBRARY_PATH / DYLD_LIBRARY_PATH
                        var varName = IsMacOS() ? "DYLD_LIBRARY_PATH" : "LD_LIBRARY_PATH";
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

        // OS detection. RuntimeInformation/OSPlatform are .NET 4.7.1+ / netstandard2.0+ only;
        // net45 falls back to the classic Environment.OSVersion.Platform check.
        private static bool IsWindows()
        {
#if NET45
            var p = Environment.OSVersion.Platform;
            return p == PlatformID.Win32NT
                || p == PlatformID.Win32S
                || p == PlatformID.Win32Windows
                || p == PlatformID.WinCE;
#else
            return RuntimeInformation.IsOSPlatform(OSPlatform.Windows);
#endif
        }

        private static bool IsMacOS()
        {
#if NET45
            // .NET Framework 4.5 reports macOS as PlatformID.Unix and lacks a clean
            // detection path. The net45 deployment scenario for this library is embedded
            // Windows; treat non-Windows as Linux-style to avoid a false DYLD path.
            return false;
#else
            return RuntimeInformation.IsOSPlatform(OSPlatform.OSX);
#endif
        }
    }
}
