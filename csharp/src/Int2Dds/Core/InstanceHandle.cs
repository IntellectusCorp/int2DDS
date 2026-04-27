using System;
using System.Runtime.InteropServices;

namespace Int2Dds.Core
{
    /// <summary>
    /// A 16-byte value type representing a DDS instance handle.
    /// </summary>
    [StructLayout(LayoutKind.Sequential)]
    public readonly struct InstanceHandle : IEquatable<InstanceHandle>
    {
        private readonly long _lo;
        private readonly long _hi;

        /// <summary>
        /// The nil (all-zero) instance handle.
        /// </summary>
        public static readonly InstanceHandle Nil = default;

        /// <summary>
        /// Creates an InstanceHandle from a 16-byte array.
        /// </summary>
        public InstanceHandle(byte[] bytes) : this(bytes, 0)
        {
        }

        /// <summary>
        /// Creates an InstanceHandle from 16 bytes within an array starting at offset.
        /// </summary>
        public InstanceHandle(byte[] bytes, int offset)
        {
            if (bytes == null) throw new ArgumentNullException(nameof(bytes));
            if (offset < 0 || offset + 16 > bytes.Length)
                throw new ArgumentException("InstanceHandle requires 16 bytes starting at offset.", nameof(offset));

            // Platform-native byte order (matches the previous Unsafe.ReadUnaligned semantics).
            _lo = BitConverter.ToInt64(bytes, offset);
            _hi = BitConverter.ToInt64(bytes, offset + 8);
        }

#if !NET45
        /// <summary>
        /// Creates an InstanceHandle from a 16-byte span.
        /// </summary>
        public InstanceHandle(ReadOnlySpan<byte> bytes)
        {
            if (bytes.Length < 16)
                throw new ArgumentException("InstanceHandle requires exactly 16 bytes.", nameof(bytes));

            var arr = new byte[16];
            bytes.Slice(0, 16).CopyTo(arr);
            _lo = BitConverter.ToInt64(arr, 0);
            _hi = BitConverter.ToInt64(arr, 8);
        }
#endif

        /// <summary>
        /// Returns true if this handle is the nil handle (all zeros).
        /// </summary>
        public bool IsNil => _lo == 0 && _hi == 0;

        /// <summary>
        /// Converts this handle to a 16-byte array.
        /// </summary>
        public byte[] ToByteArray()
        {
            var result = new byte[16];
            Buffer.BlockCopy(BitConverter.GetBytes(_lo), 0, result, 0, 8);
            Buffer.BlockCopy(BitConverter.GetBytes(_hi), 0, result, 8, 8);
            return result;
        }

#if !NET45
        /// <summary>
        /// Writes this handle into a 16-byte span.
        /// </summary>
        public void WriteTo(Span<byte> destination)
        {
            if (destination.Length < 16)
                throw new ArgumentException("Destination must be at least 16 bytes.", nameof(destination));

            ToByteArray().CopyTo(destination);
        }
#endif

        /// <summary>
        /// Creates an InstanceHandle from a byte array. The array must be exactly 16 bytes.
        /// </summary>
        public static InstanceHandle FromBytes(byte[] bytes) => new InstanceHandle(bytes);

#if !NET45
        /// <summary>
        /// Creates an InstanceHandle from a read-only span of bytes.
        /// </summary>
        public static InstanceHandle FromBytes(ReadOnlySpan<byte> bytes) => new InstanceHandle(bytes);
#endif

        public bool Equals(InstanceHandle other) => _lo == other._lo && _hi == other._hi;

        public override bool Equals(object? obj) => obj is InstanceHandle other && Equals(other);

        public override int GetHashCode()
        {
            unchecked
            {
                int hash = 17;
                hash = hash * 31 + _lo.GetHashCode();
                hash = hash * 31 + _hi.GetHashCode();
                return hash;
            }
        }

        public static bool operator ==(InstanceHandle left, InstanceHandle right) => left.Equals(right);

        public static bool operator !=(InstanceHandle left, InstanceHandle right) => !left.Equals(right);

        public override string ToString()
        {
            var bytes = ToByteArray();
            return BitConverter.ToString(bytes).Replace("-", "");
        }

        /// <summary>
        /// Implicit conversion from byte[] to InstanceHandle.
        /// </summary>
        public static implicit operator InstanceHandle(byte[] bytes) => new InstanceHandle(bytes);

        /// <summary>
        /// Explicit conversion from InstanceHandle to byte[].
        /// </summary>
        public static explicit operator byte[](InstanceHandle handle) => handle.ToByteArray();
    }
}
