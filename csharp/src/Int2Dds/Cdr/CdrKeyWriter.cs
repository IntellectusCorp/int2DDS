// Copyright IntellectusCorp. All rights reserved.
// Licensed under the int2DDS license.

using System;
using System.Buffers.Binary;
using System.Text;

namespace Int2Dds.Cdr
{
    /// <summary>
    /// CDR key writer: big-endian, XCDR2, no encapsulation header.
    /// Used for serializing key fields for keyhash computation.
    /// </summary>
    public sealed class CdrKeyWriter
    {
        private const int DefaultCapacity = 128;

        private byte[] _buffer;
        private int _pos;

        public CdrKeyWriter()
        {
            _buffer = new byte[DefaultCapacity];
            _pos = 0;
        }

        /// <summary>Current number of bytes written.</summary>
        public int Length => _pos;

        // ---- Buffer Management ----------------------------------------------

        private void EnsureCapacity(int additionalBytes)
        {
            int required = _pos + additionalBytes;
            if (required <= _buffer.Length) return;

            int newCapacity = _buffer.Length;
            while (newCapacity < required)
                newCapacity *= 2;

            var newBuffer = new byte[newCapacity];
            Buffer.BlockCopy(_buffer, 0, newBuffer, 0, _pos);
            _buffer = newBuffer;
        }

        // ---- Alignment (XCDR2: max 4, headerSize=0, big-endian) -------------

        private void Align(int alignment)
        {
            if (alignment <= 1) return;

            int actual = Math.Min(alignment, 4); // XCDR2 caps at 4
            int aligned = (_pos + actual - 1) & ~(actual - 1);
            int padding = aligned - _pos;

            if (padding == 0) return;
            EnsureCapacity(padding);
            _buffer.AsSpan(_pos, padding).Clear();
            _pos += padding;
        }

        // ---- Primitive Writes (big-endian) ----------------------------------

        public void WriteBool(bool value)
        {
            EnsureCapacity(1);
            _buffer[_pos++] = value ? (byte)1 : (byte)0;
        }

        public void WriteI8(sbyte value)
        {
            EnsureCapacity(1);
            _buffer[_pos++] = (byte)value;
        }

        public void WriteU8(byte value)
        {
            EnsureCapacity(1);
            _buffer[_pos++] = value;
        }

        public void WriteI16(short value)
        {
            Align(2);
            EnsureCapacity(2);
            BinaryPrimitives.WriteInt16BigEndian(_buffer.AsSpan(_pos), value);
            _pos += 2;
        }

        public void WriteU16(ushort value)
        {
            Align(2);
            EnsureCapacity(2);
            BinaryPrimitives.WriteUInt16BigEndian(_buffer.AsSpan(_pos), value);
            _pos += 2;
        }

        public void WriteI32(int value)
        {
            Align(4);
            EnsureCapacity(4);
            BinaryPrimitives.WriteInt32BigEndian(_buffer.AsSpan(_pos), value);
            _pos += 4;
        }

        public void WriteU32(uint value)
        {
            Align(4);
            EnsureCapacity(4);
            BinaryPrimitives.WriteUInt32BigEndian(_buffer.AsSpan(_pos), value);
            _pos += 4;
        }

        public void WriteI64(long value)
        {
            Align(8);
            EnsureCapacity(8);
            BinaryPrimitives.WriteInt64BigEndian(_buffer.AsSpan(_pos), value);
            _pos += 8;
        }

        public void WriteU64(ulong value)
        {
            Align(8);
            EnsureCapacity(8);
            BinaryPrimitives.WriteUInt64BigEndian(_buffer.AsSpan(_pos), value);
            _pos += 8;
        }

        public void WriteF32(float value)
        {
            unsafe
            {
                uint bits = *(uint*)&value;
                WriteU32(bits);
            }
        }

        public void WriteF64(double value)
        {
            unsafe
            {
                ulong bits = *(ulong*)&value;
                WriteU64(bits);
            }
        }

        // ---- String ---------------------------------------------------------

        /// <summary>
        /// Write a CDR string: uint32 length (including null terminator) + UTF-8 bytes + null byte.
        /// </summary>
        public void WriteString(string s)
        {
            if (s == null) s = string.Empty;
            int byteCount = Encoding.UTF8.GetByteCount(s);
            uint cdrLen = (uint)(byteCount + 1);
            WriteU32(cdrLen);
            EnsureCapacity(byteCount + 1);
            if (byteCount > 0)
                Encoding.UTF8.GetBytes(s.AsSpan(), _buffer.AsSpan(_pos));
            _buffer[_pos + byteCount] = 0;
            _pos += byteCount + 1;
        }

        public void WriteEnum(int discriminant)
        {
            WriteI32(discriminant);
        }

        public void WriteBytes(ReadOnlySpan<byte> data)
        {
            EnsureCapacity(data.Length);
            data.CopyTo(_buffer.AsSpan(_pos));
            _pos += data.Length;
        }

        // ---- Output ---------------------------------------------------------

        /// <summary>
        /// Returns a copy of the written data as a new byte array.
        /// </summary>
        public byte[] ToBytes()
        {
            var result = new byte[_pos];
            Buffer.BlockCopy(_buffer, 0, result, 0, _pos);
            return result;
        }
    }
}
