// Copyright IntellectusCorp. All rights reserved.
// Licensed under the int2DDS license.

using System;
using System.Buffers.Binary;
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
        private const ushort EncapPlCdrBe = 0x0002; // PL_CDR BE (Mutable, XCDR1)
        private const ushort EncapPlCdrLe = 0x0003; // PL_CDR LE (Mutable, XCDR1)
        private const ushort EncapCdr2Be = 0x0006;
        private const ushort EncapCdr2Le = 0x0007;
        private const ushort EncapDcdr2Be = 0x0008;
        private const ushort EncapDcdr2Le = 0x0009;
        private const ushort EncapPlCdr2Be = 0x000A;
        private const ushort EncapPlCdr2Le = 0x000B;
        private const uint MemberIdSentinel = 0x3F02;
        private const ushort PidExtended = 0x3F01;    // PL_CDR v1 long-form member header marker
        private const uint MaxShortMemberId = 0x3F00; // member ids above this need the long form
        private const int MaxShortLength = 0xFFFF;    // content lengths above this need the long form
        private const ushort MuFlag = 0x4000;         // must-understand bit in a PL_CDR pid

        private byte[] _buffer;
        private int _pos;
        private int _headerSize;
        private readonly bool _littleEndian;
        private readonly bool _xcdr2;
        private readonly Extensibility _extensibility;

        /// <summary>
        /// Creates a new CDR writer that automatically writes the 4-byte encapsulation header.
        /// </summary>
        /// <param name="extensibility">Extensibility kind (Final, Appendable, Mutable).</param>
        /// <param name="littleEndian">True for little-endian data encoding.</param>
        /// <param name="xcdr2">True for XCDR2 (max alignment capped at 4).</param>
        public CdrWriter(Extensibility extensibility = Extensibility.Appendable, bool littleEndian = true, bool xcdr2 = false)
        {
            _littleEndian = littleEndian;
            _xcdr2 = xcdr2;
            _extensibility = extensibility;
            _buffer = new byte[DefaultCapacity];
            _pos = 0;
            _headerSize = 0;

            WriteEncapsulationHeader(extensibility);
        }

        /// <summary>Current number of bytes written.</summary>
        public int Length => _pos;

        /// <summary>
        /// Reset the writer for reuse: discards the content and rewrites the
        /// encapsulation header. The internal buffer is kept.
        /// </summary>
        public void Reset()
        {
            _pos = 0;
            WriteEncapsulationHeader(_extensibility);
        }

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
            else if (extensibility == Extensibility.Mutable)
            {
                // XCDR1 mutable is PL_CDR (PID member headers), not PLAIN_CDR.
                encapId = _littleEndian ? EncapPlCdrLe : EncapPlCdrBe;
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

        // ---- Guards ---------------------------------------------------------

        private void RequireXcdr2(string what)
        {
            if (!_xcdr2)
                throw new InvalidOperationException(
                    $"{what} is XCDR2-only; this writer is XCDR1. XCDR1 mutable types " +
                    "use PL_CDR (PID member headers), not EMHEADER.");
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
            _buffer.AsSpan(_pos, padding).Clear();
            _pos += padding;
        }

        // ---- Buffer Management ----------------------------------------------

        // Array.MaxLength, which is not available on the older target frameworks.
        private const int MaxBufferLength = 0x7FFFFFC7;

        private void EnsureCapacity(int additionalBytes)
        {
            if (additionalBytes < 0)
                throw new ArgumentOutOfRangeException(nameof(additionalBytes), "Negative length.");
            // Subtraction form: `_pos + additionalBytes` can overflow to a negative
            // value and pass an additive check, and doubling can overflow the growth loop.
            if (additionalBytes <= _buffer.Length - _pos) return;
            if (additionalBytes > MaxBufferLength - _pos)
                throw new CdrOverflowException("Buffer would exceed the maximum array length.");

            int required = _pos + additionalBytes;
            int newCapacity = _buffer.Length;
            while (newCapacity < required)
            {
                if (newCapacity > MaxBufferLength / 2)
                {
                    newCapacity = required;
                    break;
                }
                newCapacity *= 2;
            }

            var newBuffer = new byte[newCapacity];
            Buffer.BlockCopy(_buffer, 0, newBuffer, 0, _pos);
            _buffer = newBuffer;
        }

        /// <summary>
        /// Reject a finalize token that does not name a header this writer reserved.
        /// An unchecked token back-patches at the wrong offset and derives a negative
        /// length, which is then written as a huge unsigned value.
        /// </summary>
        private void CheckToken(int token, int headerBytes)
        {
            if (token < 0 || _pos - token < headerBytes)
                throw new ArgumentOutOfRangeException(nameof(token), $"Invalid finalize token: {token}");
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
                BinaryPrimitives.WriteInt16LittleEndian(_buffer.AsSpan(_pos), value);
            else
                BinaryPrimitives.WriteInt16BigEndian(_buffer.AsSpan(_pos), value);
            _pos += 2;
        }

        /// <summary>Write an unsigned 16-bit integer with 2-byte alignment.</summary>
        public void WriteU16(ushort value)
        {
            Align(2);
            EnsureCapacity(2);
            if (_littleEndian)
                BinaryPrimitives.WriteUInt16LittleEndian(_buffer.AsSpan(_pos), value);
            else
                BinaryPrimitives.WriteUInt16BigEndian(_buffer.AsSpan(_pos), value);
            _pos += 2;
        }

        /// <summary>Write a signed 32-bit integer with 4-byte alignment.</summary>
        public void WriteI32(int value)
        {
            Align(4);
            EnsureCapacity(4);
            if (_littleEndian)
                BinaryPrimitives.WriteInt32LittleEndian(_buffer.AsSpan(_pos), value);
            else
                BinaryPrimitives.WriteInt32BigEndian(_buffer.AsSpan(_pos), value);
            _pos += 4;
        }

        /// <summary>Write an unsigned 32-bit integer with 4-byte alignment.</summary>
        public void WriteU32(uint value)
        {
            Align(4);
            EnsureCapacity(4);
            if (_littleEndian)
                BinaryPrimitives.WriteUInt32LittleEndian(_buffer.AsSpan(_pos), value);
            else
                BinaryPrimitives.WriteUInt32BigEndian(_buffer.AsSpan(_pos), value);
            _pos += 4;
        }

        /// <summary>Write a signed 64-bit integer with 8-byte alignment.</summary>
        public void WriteI64(long value)
        {
            Align(8);
            EnsureCapacity(8);
            if (_littleEndian)
                BinaryPrimitives.WriteInt64LittleEndian(_buffer.AsSpan(_pos), value);
            else
                BinaryPrimitives.WriteInt64BigEndian(_buffer.AsSpan(_pos), value);
            _pos += 8;
        }

        /// <summary>Write an unsigned 64-bit integer with 8-byte alignment.</summary>
        public void WriteU64(ulong value)
        {
            Align(8);
            EnsureCapacity(8);
            if (_littleEndian)
                BinaryPrimitives.WriteUInt64LittleEndian(_buffer.AsSpan(_pos), value);
            else
                BinaryPrimitives.WriteUInt64BigEndian(_buffer.AsSpan(_pos), value);
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
        /// Write a CDR wstring (wide string): uint32 length (number of UTF-16 code units,
        /// no terminator) + UTF-16 code units (each 2 bytes). Matches the Rust core format.
        /// </summary>
        public void WriteWString(string s)
        {
            if (s == null) s = string.Empty;
            WriteU32((uint)s.Length);
            if (s.Length == 0) return;
            Align(2);
            int byteLen = s.Length * 2;
            EnsureCapacity(byteLen);
            var enc = _littleEndian ? Encoding.Unicode : Encoding.BigEndianUnicode;
            enc.GetBytes(s, 0, s.Length, _buffer, _pos);
            _pos += byteLen;
        }

        /// <summary>Write a sequence header (uint32 element count).</summary>
        public void WriteSeqHeader(uint count)
        {
            WriteU32(count);
        }

        /// <summary>Write raw bytes (no alignment, no length prefix).</summary>
        public void WriteBytes(ReadOnlySpan<byte> data)
        {
            EnsureCapacity(data.Length);
            data.CopyTo(_buffer.AsSpan(_pos));
            _pos += data.Length;
        }

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
            CheckToken(token, 4);
            uint objectSize = (uint)(_pos - token - 4);
            if (_littleEndian)
                BinaryPrimitives.WriteUInt32LittleEndian(_buffer.AsSpan(token), objectSize);
            else
                BinaryPrimitives.WriteUInt32BigEndian(_buffer.AsSpan(token), objectSize);
        }

        // ---- XCDR2 EMHEADER -------------------------------------------------

        /// <summary>
        /// Write an EMHEADER with a known data length using LC=4 (NEXTINT) encoding.
        /// </summary>
        public void WriteEmheader(uint memberId, uint dataLength, bool mustUnderstand)
        {
            RequireXcdr2("EMHEADER");
            if (memberId > 0x0FFFFFFFu)
                throw new ArgumentOutOfRangeException(nameof(memberId), $"EMHEADER member_id exceeds 28 bits: 0x{memberId:X}");
            uint muBit = mustUnderstand ? 0x80000000u : 0;
            uint header = muBit | (4u << 28) | (memberId & 0x0FFFFFFFu);
            WriteU32(header);
            WriteU32(dataLength);
        }

        /// <summary>
        /// Begin writing an EMHEADER with unknown data length (uses LC=4 format for back-patching).
        /// Returns a token to pass to <see cref="EmheaderFinalize"/>.
        /// </summary>
        public int EmheaderBegin(uint memberId, bool mustUnderstand)
        {
            RequireXcdr2("EMHEADER");
            if (memberId > 0x0FFFFFFFu)
                throw new ArgumentOutOfRangeException(nameof(memberId), $"EMHEADER member_id exceeds 28 bits: 0x{memberId:X}");
            uint muBit = mustUnderstand ? 0x80000000u : 0;
            uint header = muBit | (4u << 28) | (memberId & 0x0FFFFFFFu);
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
            CheckToken(token, 4);
            uint dataLength = (uint)(_pos - token - 4);
            if (_littleEndian)
                BinaryPrimitives.WriteUInt32LittleEndian(_buffer.AsSpan(token), dataLength);
            else
                BinaryPrimitives.WriteUInt32BigEndian(_buffer.AsSpan(token), dataLength);
        }

        /// <summary>
        /// Write a sentinel marker (member_id = 0x3F02, LC=0, length=0).
        /// </summary>
        public void WriteSentinel()
        {
            RequireXcdr2("Sentinel");
            WriteU32(MemberIdSentinel);
        }

        // ---- XCDR1 PL_CDR member headers (Mutable types under XCDR1) --------

        /// <summary>
        /// Begin a PL_CDR v1 member (XCDR1 mutable): 4-align and reserve the header.
        /// Mirrors the Rust core's <c>write_member_with_v1</c>. Returns a token.
        /// </summary>
        public int MemberV1Begin(uint memberId)
        {
            if (memberId > 0x0FFFFFFFu)
                throw new ArgumentOutOfRangeException(nameof(memberId), $"member_id exceeds 28 bits: 0x{memberId:X}");
            Align(4);
            int headerPos = _pos;
            int reserve = memberId <= MaxShortMemberId ? 4 : 12;
            EnsureCapacity(reserve);
            _buffer.AsSpan(_pos, reserve).Clear();
            _pos += reserve;
            return headerPos;
        }

        /// <summary>Backpatch a PL_CDR v1 member header, promoting to long form if needed.</summary>
        public void MemberV1Finalize(int headerPos, uint memberId, bool mustUnderstand)
        {
            ushort flags = mustUnderstand ? MuFlag : (ushort)0;
            bool shortReserved = memberId <= MaxShortMemberId;
            CheckToken(headerPos, shortReserved ? 4 : 12);
            int contentStart = headerPos + (shortReserved ? 4 : 12);
            int contentLen = _pos - contentStart;
            if (shortReserved && contentLen <= MaxShortLength)
            {
                WriteU16At(headerPos, (ushort)(flags | (memberId & 0x3FFFu)));
                WriteU16At(headerPos + 2, (ushort)contentLen);
                return;
            }
            if (shortReserved)
            {
                // Content too large for the short form: make room for 8 more header bytes.
                // Array.Copy documents correct handling of overlapping source/dest ranges.
                EnsureCapacity(8);
                Array.Copy(_buffer, headerPos + 4, _buffer, headerPos + 12, _pos - (headerPos + 4));
                _buffer.AsSpan(headerPos + 4, 8).Clear();
                _pos += 8;
            }
            WriteU16At(headerPos, (ushort)(flags | PidExtended));
            WriteU16At(headerPos + 2, 8);
            WriteU32At(headerPos + 4, memberId);
            WriteU32At(headerPos + 8, (uint)contentLen);
        }

        /// <summary>Write the PL_CDR sentinel that terminates an XCDR1 mutable struct.</summary>
        public void EndMutableStruct()
        {
            Align(4);
            WriteU16((ushort)MemberIdSentinel);
            WriteU16(0);
        }

        private void WriteU16At(int pos, ushort value)
        {
            if (_littleEndian)
                BinaryPrimitives.WriteUInt16LittleEndian(_buffer.AsSpan(pos), value);
            else
                BinaryPrimitives.WriteUInt16BigEndian(_buffer.AsSpan(pos), value);
        }

        private void WriteU32At(int pos, uint value)
        {
            if (_littleEndian)
                BinaryPrimitives.WriteUInt32LittleEndian(_buffer.AsSpan(pos), value);
            else
                BinaryPrimitives.WriteUInt32BigEndian(_buffer.AsSpan(pos), value);
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

        /// <summary>
        /// The backing buffer for zero-copy interop; only [0, Length) is valid,
        /// and any write invalidates it.
        /// </summary>
        internal byte[] InternalBuffer => _buffer;
    }
}
