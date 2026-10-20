// Copyright IntellectusCorp. All rights reserved.
// Licensed under the int2DDS license.

using System;
using System.Globalization;
using System.Text;
using Int2Dds.Exceptions;
using Int2Dds.Interop;

namespace Int2Dds.Cdr
{
    /// <summary>
    /// IDL <c>float128</c> (<c>long double</c>) value.
    /// .NET has no binary128 type, so the value is carried as its raw IEEE 754 bits; going through
    /// <see cref="double"/> would drop the low 60 mantissa bits that peers compare.
    /// Equality is bitwise.
    /// </summary>
    public readonly struct Float128 : IEquatable<Float128>
    {
        /// <summary>Low 64 bits of the binary128 encoding.</summary>
        public ulong Lo { get; }

        /// <summary>High 64 bits (sign, exponent, top of the mantissa).</summary>
        public ulong Hi { get; }

        public Float128(ulong lo, ulong hi)
        {
            Lo = lo;
            Hi = hi;
        }

        /// <summary>Decimal literal correctly rounded to binary128 (by the native core parser).</summary>
        public static unsafe Float128 Parse(string text)
        {
            var bytes = new byte[16];
            var t = Encoding.UTF8.GetBytes(text + '\0');
            fixed (byte* pt = t)
            fixed (byte* pb = bytes)
                ReturnCodeHelper.CheckReturn(NativeMethods.int2dds_float128_parse(pt, pb));
            return FromBytes(bytes, 0);
        }

        /// <summary>Exact widening of a double.</summary>
        public static unsafe Float128 FromDouble(double value)
        {
            var bytes = new byte[16];
            fixed (byte* pb = bytes)
                ReturnCodeHelper.CheckReturn(NativeMethods.int2dds_float128_from_f64(value, pb));
            return FromBytes(bytes, 0);
        }

        /// <summary>Nearest double.</summary>
        public unsafe double ToDouble()
        {
            var bytes = ToBytes();
            fixed (byte* pb = bytes)
            {
                ReturnCodeHelper.CheckReturn(NativeMethods.int2dds_float128_to_f64(pb, out double v));
                return v;
            }
        }

        /// <summary>16 bytes, little-endian.</summary>
        public byte[] ToBytes()
        {
            var bytes = new byte[16];
            for (int i = 0; i < 8; i++)
            {
                bytes[i] = (byte)(Lo >> (8 * i));
                bytes[8 + i] = (byte)(Hi >> (8 * i));
            }
            return bytes;
        }

        /// <summary>Reads 16 little-endian bytes starting at <paramref name="offset"/>.</summary>
        public static Float128 FromBytes(byte[] bytes, int offset)
        {
            ulong lo = 0, hi = 0;
            for (int i = 7; i >= 0; i--)
            {
                lo = (lo << 8) | bytes[offset + i];
                hi = (hi << 8) | bytes[offset + 8 + i];
            }
            return new Float128(lo, hi);
        }

        public bool Equals(Float128 other) => Lo == other.Lo && Hi == other.Hi;
        public override bool Equals(object? obj) => obj is Float128 other && Equals(other);
        public override int GetHashCode() => Lo.GetHashCode() ^ (Hi.GetHashCode() * 31);
        public static bool operator ==(Float128 a, Float128 b) => a.Equals(b);
        public static bool operator !=(Float128 a, Float128 b) => !a.Equals(b);

        public override string ToString() =>
            string.Format(CultureInfo.InvariantCulture, "{0:R} (0x{1:x16}{2:x16})", ToDouble(), Hi, Lo);
    }
}
