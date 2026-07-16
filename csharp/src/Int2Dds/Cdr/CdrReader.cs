// Copyright IntellectusCorp. All rights reserved.
// Licensed under the int2DDS license.

using System;
using System.Buffers.Binary;
using System.Text;

namespace Int2Dds.Cdr
{
    /// <summary>
    /// Pure C# CDR/XCDR2 deserialization reader.
    /// Reads CDR-encoded data from a byte buffer.
    /// </summary>
    public sealed class CdrReader
    {
        private const ushort EncapCdrBe = 0x0000;
        private const ushort EncapCdrLe = 0x0001;
        private const ushort EncapPlCdrBe = 0x0002;
        private const ushort EncapPlCdrLe = 0x0003;
        private const ushort EncapCdr2Be = 0x0006;
        private const ushort EncapCdr2Le = 0x0007;
        private const ushort EncapDcdr2Be = 0x0008;
        private const ushort EncapDcdr2Le = 0x0009;
        private const ushort EncapPlCdr2Be = 0x000A;
        private const ushort EncapPlCdr2Le = 0x000B;
        private const uint MemberIdSentinel = 0x3F02;

        private readonly byte[] _data;
        private int _pos;
        private readonly int _headerSize;
        private readonly bool _littleEndian;
        private readonly bool _xcdr2;

        /// <summary>
        /// Create a reader from a byte array, parsing the 4-byte encapsulation header.
        /// </summary>
        public CdrReader(byte[] data) : this((ReadOnlySpan<byte>)data)
        {
        }

        /// <summary>
        /// Create a reader that parses the 4-byte encapsulation header to detect
        /// endianness and XCDR version.
        /// </summary>
        public CdrReader(ReadOnlySpan<byte> data)
        {
            if (data.Length < 4)
                throw new CdrUnderflowException("Data too short for encapsulation header.");

            _data = data.ToArray();

            // Encapsulation header is always big-endian
            ushort encapId = (ushort)((_data[0] << 8) | _data[1]);

            switch (encapId)
            {
                case EncapCdrLe:
                    _littleEndian = true; _xcdr2 = false; break;
                case EncapCdrBe:
                    _littleEndian = false; _xcdr2 = false; break;
                case EncapPlCdrLe:
                    _littleEndian = true; _xcdr2 = false; break;
                case EncapPlCdrBe:
                    _littleEndian = false; _xcdr2 = false; break;
                case EncapCdr2Le:
                case EncapDcdr2Le:
                case EncapPlCdr2Le:
                    _littleEndian = true; _xcdr2 = true; break;
                case EncapCdr2Be:
                case EncapDcdr2Be:
                case EncapPlCdr2Be:
                    _littleEndian = false; _xcdr2 = true; break;
                default:
                    throw new CdrInvalidEncapsulationException(
                        $"Unrecognized encapsulation ID: 0x{encapId:X4}");
            }

            _headerSize = 4;
            _pos = 4; // skip encapsulation header
        }

        /// <summary>
        /// Internal constructor for raw data without encapsulation header.
        /// </summary>
        private CdrReader(byte[] data, bool littleEndian, bool xcdr2)
        {
            _data = data;
            _littleEndian = littleEndian;
            _xcdr2 = xcdr2;
            _headerSize = 0;
            _pos = 0;
        }

        /// <summary>
        /// Create a reader from raw data (no encapsulation header).
        /// </summary>
        public static CdrReader FromRaw(ReadOnlySpan<byte> data, bool littleEndian = true, bool xcdr2 = false)
        {
            return new CdrReader(data.ToArray(), littleEndian, xcdr2);
        }

        /// <summary>Number of bytes remaining to be read.</summary>
        public int Remaining => Math.Max(0, _data.Length - _pos);

        /// <summary>Current read position in the buffer.</summary>
        public int Position => _pos;

        // ---- Alignment ------------------------------------------------------

        private void Align(int alignment)
        {
            if (alignment <= 1) return;

            int actual = _xcdr2 ? Math.Min(alignment, 4) : alignment;
            int streamPos = _pos - _headerSize;
            int aligned = (streamPos + actual - 1) & ~(actual - 1);
            int newPos = aligned + _headerSize;

            if (newPos > _data.Length)
                throw new CdrUnderflowException("Alignment would exceed buffer.");
            _pos = newPos;
        }

        private void EnsureRemaining(int count)
        {
            if (_pos + count > _data.Length)
                throw new CdrUnderflowException(
                    $"Need {count} bytes but only {Remaining} remaining.");
        }

        // ---- Primitive Reads ------------------------------------------------

        /// <summary>Read a boolean (1 byte).</summary>
        public bool ReadBool()
        {
            EnsureRemaining(1);
            return _data[_pos++] != 0;
        }

        /// <summary>Read a signed 8-bit integer.</summary>
        public sbyte ReadI8()
        {
            EnsureRemaining(1);
            return (sbyte)_data[_pos++];
        }

        /// <summary>Read an unsigned 8-bit integer.</summary>
        public byte ReadU8()
        {
            EnsureRemaining(1);
            return _data[_pos++];
        }

        /// <summary>Read a signed 16-bit integer with 2-byte alignment.</summary>
        public short ReadI16()
        {
            Align(2);
            EnsureRemaining(2);
            short value = _littleEndian
                ? BinaryPrimitives.ReadInt16LittleEndian(_data.AsSpan(_pos))
                : BinaryPrimitives.ReadInt16BigEndian(_data.AsSpan(_pos));
            _pos += 2;
            return value;
        }

        /// <summary>Read an unsigned 16-bit integer with 2-byte alignment.</summary>
        public ushort ReadU16()
        {
            Align(2);
            EnsureRemaining(2);
            ushort value = _littleEndian
                ? BinaryPrimitives.ReadUInt16LittleEndian(_data.AsSpan(_pos))
                : BinaryPrimitives.ReadUInt16BigEndian(_data.AsSpan(_pos));
            _pos += 2;
            return value;
        }

        /// <summary>Read a signed 32-bit integer with 4-byte alignment.</summary>
        public int ReadI32()
        {
            Align(4);
            EnsureRemaining(4);
            int value = _littleEndian
                ? BinaryPrimitives.ReadInt32LittleEndian(_data.AsSpan(_pos))
                : BinaryPrimitives.ReadInt32BigEndian(_data.AsSpan(_pos));
            _pos += 4;
            return value;
        }

        /// <summary>Read an unsigned 32-bit integer with 4-byte alignment.</summary>
        public uint ReadU32()
        {
            Align(4);
            EnsureRemaining(4);
            uint value = _littleEndian
                ? BinaryPrimitives.ReadUInt32LittleEndian(_data.AsSpan(_pos))
                : BinaryPrimitives.ReadUInt32BigEndian(_data.AsSpan(_pos));
            _pos += 4;
            return value;
        }

        /// <summary>Read a signed 64-bit integer with 8-byte alignment.</summary>
        public long ReadI64()
        {
            Align(8);
            EnsureRemaining(8);
            long value = _littleEndian
                ? BinaryPrimitives.ReadInt64LittleEndian(_data.AsSpan(_pos))
                : BinaryPrimitives.ReadInt64BigEndian(_data.AsSpan(_pos));
            _pos += 8;
            return value;
        }

        /// <summary>Read an unsigned 64-bit integer with 8-byte alignment.</summary>
        public ulong ReadU64()
        {
            Align(8);
            EnsureRemaining(8);
            ulong value = _littleEndian
                ? BinaryPrimitives.ReadUInt64LittleEndian(_data.AsSpan(_pos))
                : BinaryPrimitives.ReadUInt64BigEndian(_data.AsSpan(_pos));
            _pos += 8;
            return value;
        }

        /// <summary>Read a 32-bit float with 4-byte alignment.</summary>
        public float ReadF32()
        {
            uint bits = ReadU32();
            unsafe
            {
                return *(float*)&bits;
            }
        }

        /// <summary>Read a 64-bit double with 8-byte alignment.</summary>
        public double ReadF64()
        {
            ulong bits = ReadU64();
            unsafe
            {
                return *(double*)&bits;
            }
        }

        // ---- String / Sequence / Bytes --------------------------------------

        /// <summary>
        /// Read a CDR string: uint32 length (including null terminator) + UTF-8 bytes + null byte.
        /// Returns the decoded string (without null terminator).
        /// </summary>
        public string ReadString()
        {
            uint cdrLen = ReadU32();
            if (cdrLen == 0)
                return string.Empty;

            EnsureRemaining((int)cdrLen);
            // cdrLen includes the null terminator; string length is cdrLen - 1
            int strLen = (int)(cdrLen - 1);
            string result = strLen > 0
                ? Encoding.UTF8.GetString(_data, _pos, strLen)
                : string.Empty;
            _pos += (int)cdrLen;
            return result;
        }

        /// <summary>
        /// Read a CDR wstring (wide string): uint32 length (number of UTF-16 code units including null)
        /// + UTF-16 code units (each 2 bytes) + null terminator.
        /// Returns the decoded string (without null terminator).
        /// </summary>
        public string ReadWString()
        {
            uint cdrLen = ReadU32(); // number of UTF-16 code units including null
            if (cdrLen == 0)
                return string.Empty;

            int strLen = (int)(cdrLen - 1); // exclude null terminator
            var chars = new char[strLen];
            for (int i = 0; i < strLen; i++)
            {
                chars[i] = (char)ReadU16();
            }
            ReadU16(); // consume null terminator
            return new string(chars);
        }

        /// <summary>Read a sequence header (uint32 element count).</summary>
        public uint ReadSeqHeader()
        {
            uint count = ReadU32();
            
            if (count > (uint)Remaining)
                throw new CdrUnderflowException(
                    $"Sequence length {count} exceeds {Remaining} remaining bytes.");
            return count;
        }

        /// <summary>Read raw bytes (no alignment).</summary>
        public byte[] ReadBytes(int length)
        {
            EnsureRemaining(length);
            var result = new byte[length];
            Buffer.BlockCopy(_data, _pos, result, 0, length);
            _pos += length;
            return result;
        }

        /// <summary>Read an enum discriminant as a signed 32-bit integer.</summary>
        public int ReadEnum()
        {
            return ReadI32();
        }

        // ---- XCDR2 DHEADER --------------------------------------------------

        /// <summary>
        /// Read a DHEADER: returns the object size and the start position after the header.
        /// </summary>
        public (uint ObjectSize, int StartPos) ReadDheader()
        {
            if (!_xcdr2) return (0, _pos); // XCDR1: no DHEADER
            uint objectSize = ReadU32();
            return (objectSize, _pos);
        }

        /// <summary>
        /// Validate/skip to the end of a DHEADER region.
        /// Advances position to startPos + objectSize, skipping any unknown trailing fields.
        /// </summary>
        public void ReadDheaderEnd(uint objectSize, int startPos)
        {
            if (!_xcdr2) return; // XCDR1: no DHEADER
            int expectedEnd = startPos + (int)objectSize;
            if (expectedEnd > _data.Length)
                throw new CdrUnderflowException("DHEADER end exceeds buffer.");
            _pos = expectedEnd;
        }

        // ---- XCDR2 EMHEADER -------------------------------------------------

        /// <summary>
        /// Read an EMHEADER: returns member ID, data length, and must-understand flag.
        /// </summary>
        public (uint MemberId, uint DataLength, bool MustUnderstand) ReadEmheader()
        {
            uint header = ReadU32();
            bool mustUnderstand = (header & 0x80000000u) != 0;
            byte lc = (byte)((header >> 28) & 0x07);
            uint memberId = header & 0x0FFFFFFFu;
            uint dataLength;

            switch (lc)
            {
                case 0:
                    dataLength = 1;
                    break;
                case 1:
                    dataLength = 2;
                    break;
                case 2:
                    dataLength = 4;
                    break;
                case 3:
                    dataLength = 8;
                    break;
                case 4:
                    dataLength = ReadU32();
                    break;
                case 5:
                case 6:
                case 7:
                {
                    if (_pos + 4 > _data.Length)
                        throw new CdrUnderflowException("NEXTINT extends beyond buffer.");
                    uint nextInt = _littleEndian
                        ? BinaryPrimitives.ReadUInt32LittleEndian(_data.AsSpan(_pos))
                        : BinaryPrimitives.ReadUInt32BigEndian(_data.AsSpan(_pos));
                    dataLength = lc switch
                    {
                        5 => nextInt,
                        6 => 4u + 4u * nextInt,
                        _ => 4u + 8u * nextInt, // lc == 7
                    };
                    break;
                }
                default:
                    dataLength = 0;
                    break;
            }

            return (memberId, dataLength, mustUnderstand);
        }

        /// <summary>
        /// Check if the current position holds a sentinel marker without consuming it.
        /// </summary>
        public bool IsSentinel
        {
            get
            {
                if (_pos + 4 > _data.Length) return false;
                uint header = _littleEndian
                    ? BinaryPrimitives.ReadUInt32LittleEndian(_data.AsSpan(_pos))
                    : BinaryPrimitives.ReadUInt32BigEndian(_data.AsSpan(_pos));
                uint memberId = header & 0x0FFFFFFFu;
                return memberId == MemberIdSentinel;
            }
        }

        /// <summary>Skip a specified number of bytes.</summary>
        public void Skip(int count)
        {
            EnsureRemaining(count);
            _pos += count;
        }
    }
}
