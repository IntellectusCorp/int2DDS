// Copyright IntellectusCorp. All rights reserved.
// Licensed under the int2DDS license.

using System;

namespace Int2Dds.Cdr
{
    // byte[]+offset endian-aware primitives. Replacement for System.Buffers.Binary.BinaryPrimitives,
    // which is not available on net45 (System.Memory NuGet) — and we keep net45 deps zero.
    // Same implementation is used on every TFM so behavior stays identical across builds.
    internal static class CdrBinaryHelper
    {
        // ---- 16-bit ---------------------------------------------------------

        public static void WriteInt16LE(byte[] buf, int offset, short value)
        {
            buf[offset]     = (byte)value;
            buf[offset + 1] = (byte)(value >> 8);
        }

        public static void WriteInt16BE(byte[] buf, int offset, short value)
        {
            buf[offset]     = (byte)(value >> 8);
            buf[offset + 1] = (byte)value;
        }

        public static void WriteUInt16LE(byte[] buf, int offset, ushort value)
        {
            buf[offset]     = (byte)value;
            buf[offset + 1] = (byte)(value >> 8);
        }

        public static void WriteUInt16BE(byte[] buf, int offset, ushort value)
        {
            buf[offset]     = (byte)(value >> 8);
            buf[offset + 1] = (byte)value;
        }

        public static short ReadInt16LE(byte[] buf, int offset)
        {
            return (short)(buf[offset] | (buf[offset + 1] << 8));
        }

        public static short ReadInt16BE(byte[] buf, int offset)
        {
            return (short)((buf[offset] << 8) | buf[offset + 1]);
        }

        public static ushort ReadUInt16LE(byte[] buf, int offset)
        {
            return (ushort)(buf[offset] | (buf[offset + 1] << 8));
        }

        public static ushort ReadUInt16BE(byte[] buf, int offset)
        {
            return (ushort)((buf[offset] << 8) | buf[offset + 1]);
        }

        // ---- 32-bit ---------------------------------------------------------

        public static void WriteInt32LE(byte[] buf, int offset, int value)
        {
            buf[offset]     = (byte)value;
            buf[offset + 1] = (byte)(value >> 8);
            buf[offset + 2] = (byte)(value >> 16);
            buf[offset + 3] = (byte)(value >> 24);
        }

        public static void WriteInt32BE(byte[] buf, int offset, int value)
        {
            buf[offset]     = (byte)(value >> 24);
            buf[offset + 1] = (byte)(value >> 16);
            buf[offset + 2] = (byte)(value >> 8);
            buf[offset + 3] = (byte)value;
        }

        public static void WriteUInt32LE(byte[] buf, int offset, uint value)
        {
            buf[offset]     = (byte)value;
            buf[offset + 1] = (byte)(value >> 8);
            buf[offset + 2] = (byte)(value >> 16);
            buf[offset + 3] = (byte)(value >> 24);
        }

        public static void WriteUInt32BE(byte[] buf, int offset, uint value)
        {
            buf[offset]     = (byte)(value >> 24);
            buf[offset + 1] = (byte)(value >> 16);
            buf[offset + 2] = (byte)(value >> 8);
            buf[offset + 3] = (byte)value;
        }

        public static int ReadInt32LE(byte[] buf, int offset)
        {
            return buf[offset]
                 | (buf[offset + 1] << 8)
                 | (buf[offset + 2] << 16)
                 | (buf[offset + 3] << 24);
        }

        public static int ReadInt32BE(byte[] buf, int offset)
        {
            return (buf[offset] << 24)
                 | (buf[offset + 1] << 16)
                 | (buf[offset + 2] << 8)
                 |  buf[offset + 3];
        }

        public static uint ReadUInt32LE(byte[] buf, int offset)
        {
            return (uint)buf[offset]
                 | ((uint)buf[offset + 1] << 8)
                 | ((uint)buf[offset + 2] << 16)
                 | ((uint)buf[offset + 3] << 24);
        }

        public static uint ReadUInt32BE(byte[] buf, int offset)
        {
            return ((uint)buf[offset] << 24)
                 | ((uint)buf[offset + 1] << 16)
                 | ((uint)buf[offset + 2] << 8)
                 |  (uint)buf[offset + 3];
        }

        // ---- 64-bit ---------------------------------------------------------

        public static void WriteInt64LE(byte[] buf, int offset, long value)
        {
            buf[offset]     = (byte)value;
            buf[offset + 1] = (byte)(value >> 8);
            buf[offset + 2] = (byte)(value >> 16);
            buf[offset + 3] = (byte)(value >> 24);
            buf[offset + 4] = (byte)(value >> 32);
            buf[offset + 5] = (byte)(value >> 40);
            buf[offset + 6] = (byte)(value >> 48);
            buf[offset + 7] = (byte)(value >> 56);
        }

        public static void WriteInt64BE(byte[] buf, int offset, long value)
        {
            buf[offset]     = (byte)(value >> 56);
            buf[offset + 1] = (byte)(value >> 48);
            buf[offset + 2] = (byte)(value >> 40);
            buf[offset + 3] = (byte)(value >> 32);
            buf[offset + 4] = (byte)(value >> 24);
            buf[offset + 5] = (byte)(value >> 16);
            buf[offset + 6] = (byte)(value >> 8);
            buf[offset + 7] = (byte)value;
        }

        public static void WriteUInt64LE(byte[] buf, int offset, ulong value)
        {
            buf[offset]     = (byte)value;
            buf[offset + 1] = (byte)(value >> 8);
            buf[offset + 2] = (byte)(value >> 16);
            buf[offset + 3] = (byte)(value >> 24);
            buf[offset + 4] = (byte)(value >> 32);
            buf[offset + 5] = (byte)(value >> 40);
            buf[offset + 6] = (byte)(value >> 48);
            buf[offset + 7] = (byte)(value >> 56);
        }

        public static void WriteUInt64BE(byte[] buf, int offset, ulong value)
        {
            buf[offset]     = (byte)(value >> 56);
            buf[offset + 1] = (byte)(value >> 48);
            buf[offset + 2] = (byte)(value >> 40);
            buf[offset + 3] = (byte)(value >> 32);
            buf[offset + 4] = (byte)(value >> 24);
            buf[offset + 5] = (byte)(value >> 16);
            buf[offset + 6] = (byte)(value >> 8);
            buf[offset + 7] = (byte)value;
        }

        public static long ReadInt64LE(byte[] buf, int offset)
        {
            uint lo = (uint)buf[offset]
                    | ((uint)buf[offset + 1] << 8)
                    | ((uint)buf[offset + 2] << 16)
                    | ((uint)buf[offset + 3] << 24);
            uint hi = (uint)buf[offset + 4]
                    | ((uint)buf[offset + 5] << 8)
                    | ((uint)buf[offset + 6] << 16)
                    | ((uint)buf[offset + 7] << 24);
            return (long)(((ulong)hi << 32) | lo);
        }

        public static long ReadInt64BE(byte[] buf, int offset)
        {
            uint hi = ((uint)buf[offset] << 24)
                    | ((uint)buf[offset + 1] << 16)
                    | ((uint)buf[offset + 2] << 8)
                    |  (uint)buf[offset + 3];
            uint lo = ((uint)buf[offset + 4] << 24)
                    | ((uint)buf[offset + 5] << 16)
                    | ((uint)buf[offset + 6] << 8)
                    |  (uint)buf[offset + 7];
            return (long)(((ulong)hi << 32) | lo);
        }

        public static ulong ReadUInt64LE(byte[] buf, int offset)
        {
            uint lo = (uint)buf[offset]
                    | ((uint)buf[offset + 1] << 8)
                    | ((uint)buf[offset + 2] << 16)
                    | ((uint)buf[offset + 3] << 24);
            uint hi = (uint)buf[offset + 4]
                    | ((uint)buf[offset + 5] << 8)
                    | ((uint)buf[offset + 6] << 16)
                    | ((uint)buf[offset + 7] << 24);
            return ((ulong)hi << 32) | lo;
        }

        public static ulong ReadUInt64BE(byte[] buf, int offset)
        {
            uint hi = ((uint)buf[offset] << 24)
                    | ((uint)buf[offset + 1] << 16)
                    | ((uint)buf[offset + 2] << 8)
                    |  (uint)buf[offset + 3];
            uint lo = ((uint)buf[offset + 4] << 24)
                    | ((uint)buf[offset + 5] << 16)
                    | ((uint)buf[offset + 6] << 8)
                    |  (uint)buf[offset + 7];
            return ((ulong)hi << 32) | lo;
        }
    }
}
