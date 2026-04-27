// Copyright IntellectusCorp. All rights reserved.
// Licensed under the int2DDS license.

using System;
using System.Runtime.InteropServices;
using System.Text;

namespace Int2Dds.Cdr
{
    /// <summary>
    /// Pure C# CDR/XCDR2 serialization writer.
    /// Writes CDR-encoded data into an auto-growing internal byte buffer.
    /// </summary>
    public sealed class CdrWriter
    {
        private const int DefaultCapacity = 256;
        private const ushort EncapCdrBe = 0x0000;
        private const ushort EncapCdrLe = 0x0001;
        private const ushort EncapCdr2Be = 0x0006;
        private const ushort EncapCdr2Le = 0x0007;
        private const ushort EncapDcdr2Be = 0x0008;
        private const ushort EncapDcdr2Le = 0x0009;
        private const ushort EncapPlCdr2Be = 0x000A;
        private const ushort EncapPlCdr2Le = 0x000B;
        private const uint MemberIdSentinel = 0x3F02;

        private byte[] _buffer;
        private int _pos;
        private int _headerSize;
        private readonly bool _littleEndian;
        private readonly bool _xcdr2;

        /// <summary>
        /// Creates a new CDR writer that automatically writes the 4-byte encapsulation header.
        /// </summary>
        /// <param name="extensibility">Extensibility kind (Final, Appendable, Mutable).</param>
        /// <param name="littleEndian">True for little-endian data encoding.</param>
        /// <param name="xcdr2">True for XCDR2 (max alignment capped at 4).</param>
        public CdrWriter(Extensibility extensibility = Extensibility.Appendable, bool littleEndian = true, bool xcdr2 = true)
        {
            _littleEndian = littleEndian;
            _xcdr2 = xcdr2;
            _buffer = new byte[DefaultCapacity];
            _pos = 0;
            _headerSize = 0;

            WriteEncapsulationHeader(extensibility);
        }

        /// <summary>
        /// Internal constructor for key writers (no encapsulation header).
        /// </summary>
        internal CdrWriter(bool littleEndian, bool xcdr2, int headerSize)
        {
            _littleEndian = littleEndian;
            _xcdr2 = xcdr2;
            _buffer = new byte[DefaultCapacity];
            _pos = 0;
            _headerSize = headerSize;
        }

        /// <summary>Current number of bytes written.</summary>
        public int Length => _pos;

        /// <summary>Whether this writer uses XCDR2 encoding.</summary>
        public bool IsXcdr2 => _xcdr2;

        // ---- Encapsulation Header -------------------------------------------

        private void WriteEncapsulationHeader(Extensibility extensibility)
        {
            ushort encapId;
            if (_xcdr2)
            {
                encapId = extensibility switch
                {
                    Extensibility.Final => _littleEndian ? EncapCdr2Le : EncapCdr2Be,
                    Extensibility.Appendable => _littleEndian ? EncapDcdr2Le : EncapDcdr2Be,
                    Extensibility.Mutable => _littleEndian ? EncapPlCdr2Le : EncapPlCdr2Be,
                    _ => _littleEndian ? EncapDcdr2Le : EncapDcdr2Be,
                };
            }
            else
            {
                encapId = _littleEndian ? EncapCdrLe : EncapCdrBe;
            }

            EnsureCapacity(4);
            // Encapsulation header is always big-endian
            _buffer[_pos + 0] = (byte)(encapId >> 8);
            _buffer[_pos + 1] = (byte)(encapId);
            _buffer[_pos + 2] = 0; // options
            _buffer[_pos + 3] = 0;
            _pos += 4;
            _headerSize = 4;
        }

        // ---- Alignment ------------------------------------------------------

        private void Align(int alignment)
        {
            if (alignment <= 1) return;

            int actual = _xcdr2 ? Math.Min(alignment, 4) : alignment;
            int streamPos = _pos - _headerSize;
            int aligned = (streamPos + actual - 1) & ~(actual - 1);
            int padding = aligned - streamPos;

            if (padding == 0) return;
            EnsureCapacity(padding);
            Array.Clear(_buffer, _pos, padding);
            _pos += padding;
        }

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

        // ---- Primitive Writes -----------------------------------------------

        /// <summary>Write a boolean (1 byte: 0 or 1).</summary>
        public void WriteBool(bool value)
        {
            EnsureCapacity(1);
            _buffer[_pos++] = value ? (byte)1 : (byte)0;
        }

        /// <summary>Write a signed 8-bit integer.</summary>
        public void WriteI8(sbyte value)
        {
            EnsureCapacity(1);
            _buffer[_pos++] = (byte)value;
        }

        /// <summary>Write an unsigned 8-bit integer.</summary>
        public void WriteU8(byte value)
        {
            EnsureCapacity(1);
            _buffer[_pos++] = value;
        }

        /// <summary>Write a signed 16-bit integer with 2-byte alignment.</summary>
        public void WriteI16(short value)
        {
            Align(2);
            EnsureCapacity(2);
            if (_littleEndian)
                CdrBinaryHelper.WriteInt16LE(_buffer, _pos, value);
            else
                CdrBinaryHelper.WriteInt16BE(_buffer, _pos, value);
            _pos += 2;
        }

        /// <summary>Write an unsigned 16-bit integer with 2-byte alignment.</summary>
        public void WriteU16(ushort value)
        {
            Align(2);
            EnsureCapacity(2);
            if (_littleEndian)
                CdrBinaryHelper.WriteUInt16LE(_buffer, _pos, value);
            else
                CdrBinaryHelper.WriteUInt16BE(_buffer, _pos, value);
            _pos += 2;
        }

        /// <summary>Write a signed 32-bit integer with 4-byte alignment.</summary>
        public void WriteI32(int value)
        {
            Align(4);
            EnsureCapacity(4);
            if (_littleEndian)
                CdrBinaryHelper.WriteInt32LE(_buffer, _pos, value);
            else
                CdrBinaryHelper.WriteInt32BE(_buffer, _pos, value);
            _pos += 4;
        }

        /// <summary>Write an unsigned 32-bit integer with 4-byte alignment.</summary>
        public void WriteU32(uint value)
        {
            Align(4);
            EnsureCapacity(4);
            if (_littleEndian)
                CdrBinaryHelper.WriteUInt32LE(_buffer, _pos, value);
            else
                CdrBinaryHelper.WriteUInt32BE(_buffer, _pos, value);
            _pos += 4;
        }

        /// <summary>Write a signed 64-bit integer with 8-byte alignment.</summary>
        public void WriteI64(long value)
        {
            Align(8);
            EnsureCapacity(8);
            if (_littleEndian)
                CdrBinaryHelper.WriteInt64LE(_buffer, _pos, value);
            else
                CdrBinaryHelper.WriteInt64BE(_buffer, _pos, value);
            _pos += 8;
        }

        /// <summary>Write an unsigned 64-bit integer with 8-byte alignment.</summary>
        public void WriteU64(ulong value)
        {
            Align(8);
            EnsureCapacity(8);
            if (_littleEndian)
                CdrBinaryHelper.WriteUInt64LE(_buffer, _pos, value);
            else
                CdrBinaryHelper.WriteUInt64BE(_buffer, _pos, value);
            _pos += 8;
        }

        /// <summary>Write a 32-bit float with 4-byte alignment.</summary>
        public void WriteF32(float value)
        {
            unsafe
            {
                uint bits = *(uint*)&value;
                WriteU32(bits);
            }
        }

        /// <summary>Write a 64-bit double with 8-byte alignment.</summary>
        public void WriteF64(double value)
        {
            unsafe
            {
                ulong bits = *(ulong*)&value;
                WriteU64(bits);
            }
        }

        // ---- String / Sequence / Bytes --------------------------------------

        /// <summary>
        /// Write a CDR string: uint32 length (including null terminator) + UTF-8 bytes + null byte.
        /// </summary>
        public void WriteString(string s)
        {
            if (s == null) s = string.Empty;
            int byteCount = Encoding.UTF8.GetByteCount(s);
            uint cdrLen = (uint)(byteCount + 1); // includes null terminator
            WriteU32(cdrLen);
            EnsureCapacity(byteCount + 1);
            if (byteCount > 0)
                Encoding.UTF8.GetBytes(s, 0, s.Length, _buffer, _pos);
            _buffer[_pos + byteCount] = 0; // null terminator
            _pos += byteCount + 1;
        }

        /// <summary>
        /// Write a CDR wstring (wide string): uint32 length (number of UTF-16 code units including null)
        /// + UTF-16 code units (each 2 bytes) + null terminator (2 bytes of zero).
        /// </summary>
        public void WriteWString(string s)
        {
            if (s == null) s = string.Empty;
            char[] chars = s.ToCharArray();
            uint cdrLen = (uint)(chars.Length + 1); // number of UTF-16 code units including null
            WriteU32(cdrLen);
            for (int i = 0; i < chars.Length; i++)
            {
                WriteU16((ushort)chars[i]);
            }
            WriteU16(0); // null terminator
        }

        /// <summary>Write a sequence header (uint32 element count).</summary>
        public void WriteSeqHeader(uint count)
        {
            WriteU32(count);
        }

        /// <summary>Write raw bytes (no alignment, no length prefix).</summary>
        public void WriteBytes(byte[] data)
        {
            if (data == null) return;
            EnsureCapacity(data.Length);
            Buffer.BlockCopy(data, 0, _buffer, _pos, data.Length);
            _pos += data.Length;
        }

#if !NET45
        /// <summary>Write raw bytes (no alignment, no length prefix).</summary>
        public void WriteBytes(ReadOnlySpan<byte> data)
        {
            EnsureCapacity(data.Length);
            data.CopyTo(_buffer.AsSpan(_pos));
            _pos += data.Length;
        }
#endif

        /// <summary>Write an enum discriminant as a signed 32-bit integer.</summary>
        public void WriteEnum(int discriminant)
        {
            WriteI32(discriminant);
        }

        // ---- XCDR2 DHEADER --------------------------------------------------

        /// <summary>
        /// Begin writing a DHEADER. Writes a 4-byte placeholder for the object size.
        /// Returns a token to pass to <see cref="DheaderFinalize"/>.
        /// </summary>
        public int DheaderBegin()
        {
            if (!_xcdr2) return -1; // XCDR1: no DHEADER
            Align(4);
            int token = _pos;
            WriteU32(0); // placeholder
            return token;
        }

        /// <summary>
        /// Finalize a DHEADER by back-patching the actual object size.
        /// </summary>
        /// <param name="token">The token returned by <see cref="DheaderBegin"/>.</param>
        public void DheaderFinalize(int token)
        {
            if (!_xcdr2) return; // XCDR1: no DHEADER
            uint objectSize = (uint)(_pos - token - 4);
            if (_littleEndian)
                CdrBinaryHelper.WriteUInt32LE(_buffer, token, objectSize);
            else
                CdrBinaryHelper.WriteUInt32BE(_buffer, token, objectSize);
        }

        // ---- XCDR2 EMHEADER -------------------------------------------------

        /// <summary>
        /// Write an EMHEADER with a known data length.
        /// Uses short encoding (LC=0) if dataLength fits in 16 bits, otherwise extended (LC=4).
        /// </summary>
        public void WriteEmheader(uint memberId, uint dataLength, bool mustUnderstand)
        {
            uint muBit = mustUnderstand ? 0x80000000u : 0;
            if (dataLength <= 0xFFFF)
            {
                // Short encoding: LC=0
                uint header = muBit | ((memberId & 0x3FFF) << 16) | (dataLength & 0xFFFF);
                WriteU32(header);
            }
            else
            {
                // Extended encoding: LC=4
                uint header = muBit | (4u << 28) | ((memberId & 0x3FFF) << 16);
                WriteU32(header);
                WriteU32(dataLength);
            }
        }

        /// <summary>
        /// Begin writing an EMHEADER with unknown data length (uses LC=4 format for back-patching).
        /// Returns a token to pass to <see cref="EmheaderFinalize"/>.
        /// </summary>
        public int EmheaderBegin(uint memberId, bool mustUnderstand)
        {
            uint muBit = mustUnderstand ? 0x80000000u : 0;
            uint header = muBit | (4u << 28) | ((memberId & 0x3FFF) << 16);
            WriteU32(header);
            int token = _pos;
            WriteU32(0); // placeholder for length
            return token;
        }

        /// <summary>
        /// Finalize an EMHEADER by back-patching the actual data length.
        /// </summary>
        /// <param name="token">The token returned by <see cref="EmheaderBegin"/>.</param>
        public void EmheaderFinalize(int token)
        {
            uint dataLength = (uint)(_pos - token - 4);
            if (_littleEndian)
                CdrBinaryHelper.WriteUInt32LE(_buffer, token, dataLength);
            else
                CdrBinaryHelper.WriteUInt32BE(_buffer, token, dataLength);
        }

        /// <summary>
        /// Write a sentinel marker (member_id = 0x3F02, LC=0, length=0).
        /// </summary>
        public void WriteSentinel()
        {
            uint header = MemberIdSentinel << 16;
            WriteU32(header);
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
