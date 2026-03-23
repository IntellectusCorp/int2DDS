using System.Runtime.CompilerServices;
using System.Runtime.InteropServices;

namespace Int2Dds.Core;

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
    /// Creates an InstanceHandle from a 16-byte span.
    /// </summary>
    public InstanceHandle(ReadOnlySpan<byte> bytes)
    {
        if (bytes.Length < 16)
            throw new ArgumentException("InstanceHandle requires exactly 16 bytes.", nameof(bytes));

        _lo = Unsafe.ReadUnaligned<long>(ref MemoryMarshal.GetReference(bytes));
        _hi = Unsafe.ReadUnaligned<long>(ref Unsafe.Add(ref MemoryMarshal.GetReference(bytes), 8));
    }

    /// <summary>
    /// Creates an InstanceHandle from a 16-byte array.
    /// </summary>
    public InstanceHandle(byte[] bytes) : this(new ReadOnlySpan<byte>(bytes))
    {
    }

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
        Unsafe.WriteUnaligned(ref result[0], _lo);
        Unsafe.WriteUnaligned(ref result[8], _hi);
        return result;
    }

    /// <summary>
    /// Writes this handle into a 16-byte span.
    /// </summary>
    public void WriteTo(Span<byte> destination)
    {
        if (destination.Length < 16)
            throw new ArgumentException("Destination must be at least 16 bytes.", nameof(destination));

        Unsafe.WriteUnaligned(ref MemoryMarshal.GetReference(destination), _lo);
        Unsafe.WriteUnaligned(ref Unsafe.Add(ref MemoryMarshal.GetReference(destination), 8), _hi);
    }

    /// <summary>
    /// Creates an InstanceHandle from a byte array. The array must be exactly 16 bytes.
    /// </summary>
    public static InstanceHandle FromBytes(byte[] bytes) => new(bytes);

    /// <summary>
    /// Creates an InstanceHandle from a read-only span of bytes.
    /// </summary>
    public static InstanceHandle FromBytes(ReadOnlySpan<byte> bytes) => new(bytes);

    public bool Equals(InstanceHandle other) => _lo == other._lo && _hi == other._hi;

    public override bool Equals(object? obj) => obj is InstanceHandle other && Equals(other);

    public override int GetHashCode() => HashCode.Combine(_lo, _hi);

    public static bool operator ==(InstanceHandle left, InstanceHandle right) => left.Equals(right);

    public static bool operator !=(InstanceHandle left, InstanceHandle right) => !left.Equals(right);

    public override string ToString()
    {
        var bytes = ToByteArray();
        return Convert.ToHexString(bytes);
    }

    /// <summary>
    /// Implicit conversion from byte[] to InstanceHandle.
    /// </summary>
    public static implicit operator InstanceHandle(byte[] bytes) => new(bytes);

    /// <summary>
    /// Explicit conversion from InstanceHandle to byte[].
    /// </summary>
    public static explicit operator byte[](InstanceHandle handle) => handle.ToByteArray();
}
