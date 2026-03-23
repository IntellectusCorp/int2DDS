// Copyright IntellectusCorp. All rights reserved.
// Licensed under the int2DDS license.

using System;
using System.Buffers.Binary;
using System.Text;

namespace Int2Dds.Cdr;

/// <summary>
/// CDR key reader: big-endian, XCDR2, no encapsulation header.
/// Used for deserializing key fields from keyhash data.
/// </summary>
public sealed class CdrKeyReader
{
    private readonly byte[] _data;
    private int _pos;

    public CdrKeyReader(ReadOnlySpan<byte> data)
    {
        _data = data.ToArray();
        _pos = 0;
    }

    /// <summary>Number of bytes remaining.</summary>
    public int Remaining => Math.Max(0, _data.Length - _pos);

    /// <summary>Current read position.</summary>
    public int Position => _pos;

    // ---- Alignment (XCDR2: max 4, headerSize=0, big-endian) -------------

    private void Align(int alignment)
    {
        if (alignment <= 1) return;

        int actual = Math.Min(alignment, 4); // XCDR2 caps at 4
        int aligned = (_pos + actual - 1) & ~(actual - 1);

        if (aligned > _data.Length)
            throw new CdrUnderflowException("Alignment would exceed buffer.");
        _pos = aligned;
    }

    private void EnsureRemaining(int count)
    {
        if (_pos + count > _data.Length)
            throw new CdrUnderflowException(
                $"Need {count} bytes but only {Remaining} remaining.");
    }

    // ---- Primitive Reads (big-endian) -----------------------------------

    public bool ReadBool()
    {
        EnsureRemaining(1);
        return _data[_pos++] != 0;
    }

    public sbyte ReadI8()
    {
        EnsureRemaining(1);
        return (sbyte)_data[_pos++];
    }

    public byte ReadU8()
    {
        EnsureRemaining(1);
        return _data[_pos++];
    }

    public short ReadI16()
    {
        Align(2);
        EnsureRemaining(2);
        short value = BinaryPrimitives.ReadInt16BigEndian(_data.AsSpan(_pos));
        _pos += 2;
        return value;
    }

    public ushort ReadU16()
    {
        Align(2);
        EnsureRemaining(2);
        ushort value = BinaryPrimitives.ReadUInt16BigEndian(_data.AsSpan(_pos));
        _pos += 2;
        return value;
    }

    public int ReadI32()
    {
        Align(4);
        EnsureRemaining(4);
        int value = BinaryPrimitives.ReadInt32BigEndian(_data.AsSpan(_pos));
        _pos += 4;
        return value;
    }

    public uint ReadU32()
    {
        Align(4);
        EnsureRemaining(4);
        uint value = BinaryPrimitives.ReadUInt32BigEndian(_data.AsSpan(_pos));
        _pos += 4;
        return value;
    }

    public long ReadI64()
    {
        Align(8);
        EnsureRemaining(8);
        long value = BinaryPrimitives.ReadInt64BigEndian(_data.AsSpan(_pos));
        _pos += 8;
        return value;
    }

    public ulong ReadU64()
    {
        Align(8);
        EnsureRemaining(8);
        ulong value = BinaryPrimitives.ReadUInt64BigEndian(_data.AsSpan(_pos));
        _pos += 8;
        return value;
    }

    public float ReadF32()
    {
        uint bits = ReadU32();
        return BitConverter.UInt32BitsToSingle(bits);
    }

    public double ReadF64()
    {
        ulong bits = ReadU64();
        return BitConverter.UInt64BitsToDouble(bits);
    }

    // ---- String ---------------------------------------------------------

    /// <summary>
    /// Read a CDR string: uint32 length (including null terminator) + UTF-8 bytes + null byte.
    /// </summary>
    public string ReadString()
    {
        uint cdrLen = ReadU32();
        if (cdrLen == 0)
            return string.Empty;

        EnsureRemaining((int)cdrLen);
        int strLen = (int)(cdrLen - 1);
        string result = strLen > 0
            ? Encoding.UTF8.GetString(_data, _pos, strLen)
            : string.Empty;
        _pos += (int)cdrLen;
        return result;
    }

    public int ReadEnum()
    {
        return ReadI32();
    }

    /// <summary>Skip a specified number of bytes.</summary>
    public void Skip(int count)
    {
        EnsureRemaining(count);
        _pos += count;
    }
}
