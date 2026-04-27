// Copyright IntellectusCorp. All rights reserved.
// Licensed under the int2DDS license.

using System;
using System.Buffers.Binary;
using Int2Dds.Cdr;
using Xunit;

namespace Int2Dds.Tests
{
    // CdrBinaryHelper replaces System.Buffers.Binary.BinaryPrimitives because the latter
    // requires Span<T> (System.Memory NuGet on net45 — banned by our zero-deps policy).
    // The helper is byte[]+offset based and was hand-written from scratch, so its
    // correctness is the highest single risk introduced by the net45 work.
    // These tests pin its output to BinaryPrimitives across the relevant value space
    // and offset positions, which transitively guarantees the same behavior on net45
    // (the helper is pure managed code with no TFM-conditional paths).
    public class CdrBinaryHelperTests
    {
        private const int OffsetMargin = 5;

        // ---- 16-bit ---------------------------------------------------------

        [Theory]
        [InlineData((short)0)]
        [InlineData((short)1)]
        [InlineData((short)-1)]
        [InlineData(short.MaxValue)]
        [InlineData(short.MinValue)]
        [InlineData((short)0x1234)]
        [InlineData((short)unchecked((short)0xABCD))]
        public void WriteInt16LE_MatchesBinaryPrimitives(short value)
        {
            AssertWriteMatches(2,
                (buf, off) => CdrBinaryHelper.WriteInt16LE(buf, off, value),
                (buf, off) => BinaryPrimitives.WriteInt16LittleEndian(buf.AsSpan(off), value));
        }

        [Theory]
        [InlineData((short)0)]
        [InlineData((short)1)]
        [InlineData((short)-1)]
        [InlineData(short.MaxValue)]
        [InlineData(short.MinValue)]
        [InlineData((short)0x1234)]
        public void WriteInt16BE_MatchesBinaryPrimitives(short value)
        {
            AssertWriteMatches(2,
                (buf, off) => CdrBinaryHelper.WriteInt16BE(buf, off, value),
                (buf, off) => BinaryPrimitives.WriteInt16BigEndian(buf.AsSpan(off), value));
        }

        [Theory]
        [InlineData((ushort)0)]
        [InlineData((ushort)1)]
        [InlineData(ushort.MaxValue)]
        [InlineData((ushort)0x1234)]
        [InlineData((ushort)0xABCD)]
        public void WriteUInt16LE_MatchesBinaryPrimitives(ushort value)
        {
            AssertWriteMatches(2,
                (buf, off) => CdrBinaryHelper.WriteUInt16LE(buf, off, value),
                (buf, off) => BinaryPrimitives.WriteUInt16LittleEndian(buf.AsSpan(off), value));
        }

        [Theory]
        [InlineData((ushort)0)]
        [InlineData((ushort)1)]
        [InlineData(ushort.MaxValue)]
        [InlineData((ushort)0x1234)]
        [InlineData((ushort)0xABCD)]
        public void WriteUInt16BE_MatchesBinaryPrimitives(ushort value)
        {
            AssertWriteMatches(2,
                (buf, off) => CdrBinaryHelper.WriteUInt16BE(buf, off, value),
                (buf, off) => BinaryPrimitives.WriteUInt16BigEndian(buf.AsSpan(off), value));
        }

        [Theory]
        [InlineData((short)0)]
        [InlineData((short)1)]
        [InlineData((short)-1)]
        [InlineData(short.MaxValue)]
        [InlineData(short.MinValue)]
        [InlineData((short)0x1234)]
        public void ReadInt16_RoundTripsAcrossEndianness(short value)
        {
            var buf = new byte[2 + OffsetMargin * 2];
            CdrBinaryHelper.WriteInt16LE(buf, OffsetMargin, value);
            Assert.Equal(value, CdrBinaryHelper.ReadInt16LE(buf, OffsetMargin));
            Assert.Equal(BinaryPrimitives.ReadInt16LittleEndian(buf.AsSpan(OffsetMargin)),
                         CdrBinaryHelper.ReadInt16LE(buf, OffsetMargin));

            CdrBinaryHelper.WriteInt16BE(buf, OffsetMargin, value);
            Assert.Equal(value, CdrBinaryHelper.ReadInt16BE(buf, OffsetMargin));
            Assert.Equal(BinaryPrimitives.ReadInt16BigEndian(buf.AsSpan(OffsetMargin)),
                         CdrBinaryHelper.ReadInt16BE(buf, OffsetMargin));
        }

        [Theory]
        [InlineData((ushort)0)]
        [InlineData((ushort)1)]
        [InlineData(ushort.MaxValue)]
        [InlineData((ushort)0x1234)]
        [InlineData((ushort)0xABCD)]
        public void ReadUInt16_RoundTripsAcrossEndianness(ushort value)
        {
            var buf = new byte[2 + OffsetMargin * 2];
            CdrBinaryHelper.WriteUInt16LE(buf, OffsetMargin, value);
            Assert.Equal(value, CdrBinaryHelper.ReadUInt16LE(buf, OffsetMargin));
            Assert.Equal(BinaryPrimitives.ReadUInt16LittleEndian(buf.AsSpan(OffsetMargin)),
                         CdrBinaryHelper.ReadUInt16LE(buf, OffsetMargin));

            CdrBinaryHelper.WriteUInt16BE(buf, OffsetMargin, value);
            Assert.Equal(value, CdrBinaryHelper.ReadUInt16BE(buf, OffsetMargin));
            Assert.Equal(BinaryPrimitives.ReadUInt16BigEndian(buf.AsSpan(OffsetMargin)),
                         CdrBinaryHelper.ReadUInt16BE(buf, OffsetMargin));
        }

        // ---- 32-bit ---------------------------------------------------------

        [Theory]
        [InlineData(0)]
        [InlineData(1)]
        [InlineData(-1)]
        [InlineData(int.MaxValue)]
        [InlineData(int.MinValue)]
        [InlineData(0x12345678)]
        [InlineData(unchecked((int)0xFEDCBA98))]
        public void WriteInt32LE_MatchesBinaryPrimitives(int value)
        {
            AssertWriteMatches(4,
                (buf, off) => CdrBinaryHelper.WriteInt32LE(buf, off, value),
                (buf, off) => BinaryPrimitives.WriteInt32LittleEndian(buf.AsSpan(off), value));
        }

        [Theory]
        [InlineData(0)]
        [InlineData(1)]
        [InlineData(-1)]
        [InlineData(int.MaxValue)]
        [InlineData(int.MinValue)]
        [InlineData(0x12345678)]
        [InlineData(unchecked((int)0xFEDCBA98))]
        public void WriteInt32BE_MatchesBinaryPrimitives(int value)
        {
            AssertWriteMatches(4,
                (buf, off) => CdrBinaryHelper.WriteInt32BE(buf, off, value),
                (buf, off) => BinaryPrimitives.WriteInt32BigEndian(buf.AsSpan(off), value));
        }

        [Theory]
        [InlineData(0u)]
        [InlineData(1u)]
        [InlineData(uint.MaxValue)]
        [InlineData(0x12345678u)]
        [InlineData(0xFEDCBA98u)]
        public void WriteUInt32LE_MatchesBinaryPrimitives(uint value)
        {
            AssertWriteMatches(4,
                (buf, off) => CdrBinaryHelper.WriteUInt32LE(buf, off, value),
                (buf, off) => BinaryPrimitives.WriteUInt32LittleEndian(buf.AsSpan(off), value));
        }

        [Theory]
        [InlineData(0u)]
        [InlineData(1u)]
        [InlineData(uint.MaxValue)]
        [InlineData(0x12345678u)]
        [InlineData(0xFEDCBA98u)]
        public void WriteUInt32BE_MatchesBinaryPrimitives(uint value)
        {
            AssertWriteMatches(4,
                (buf, off) => CdrBinaryHelper.WriteUInt32BE(buf, off, value),
                (buf, off) => BinaryPrimitives.WriteUInt32BigEndian(buf.AsSpan(off), value));
        }

        [Theory]
        [InlineData(0)]
        [InlineData(1)]
        [InlineData(-1)]
        [InlineData(int.MaxValue)]
        [InlineData(int.MinValue)]
        [InlineData(0x12345678)]
        public void ReadInt32_RoundTripsAcrossEndianness(int value)
        {
            var buf = new byte[4 + OffsetMargin * 2];
            CdrBinaryHelper.WriteInt32LE(buf, OffsetMargin, value);
            Assert.Equal(value, CdrBinaryHelper.ReadInt32LE(buf, OffsetMargin));
            Assert.Equal(BinaryPrimitives.ReadInt32LittleEndian(buf.AsSpan(OffsetMargin)),
                         CdrBinaryHelper.ReadInt32LE(buf, OffsetMargin));

            CdrBinaryHelper.WriteInt32BE(buf, OffsetMargin, value);
            Assert.Equal(value, CdrBinaryHelper.ReadInt32BE(buf, OffsetMargin));
            Assert.Equal(BinaryPrimitives.ReadInt32BigEndian(buf.AsSpan(OffsetMargin)),
                         CdrBinaryHelper.ReadInt32BE(buf, OffsetMargin));
        }

        [Theory]
        [InlineData(0u)]
        [InlineData(1u)]
        [InlineData(uint.MaxValue)]
        [InlineData(0x12345678u)]
        [InlineData(0xFEDCBA98u)]
        public void ReadUInt32_RoundTripsAcrossEndianness(uint value)
        {
            var buf = new byte[4 + OffsetMargin * 2];
            CdrBinaryHelper.WriteUInt32LE(buf, OffsetMargin, value);
            Assert.Equal(value, CdrBinaryHelper.ReadUInt32LE(buf, OffsetMargin));
            Assert.Equal(BinaryPrimitives.ReadUInt32LittleEndian(buf.AsSpan(OffsetMargin)),
                         CdrBinaryHelper.ReadUInt32LE(buf, OffsetMargin));

            CdrBinaryHelper.WriteUInt32BE(buf, OffsetMargin, value);
            Assert.Equal(value, CdrBinaryHelper.ReadUInt32BE(buf, OffsetMargin));
            Assert.Equal(BinaryPrimitives.ReadUInt32BigEndian(buf.AsSpan(OffsetMargin)),
                         CdrBinaryHelper.ReadUInt32BE(buf, OffsetMargin));
        }

        // ---- 64-bit ---------------------------------------------------------

        [Theory]
        [InlineData(0L)]
        [InlineData(1L)]
        [InlineData(-1L)]
        [InlineData(long.MaxValue)]
        [InlineData(long.MinValue)]
        [InlineData(0x123456789ABCDEF0L)]
        [InlineData(unchecked((long)0xFEDCBA9876543210L))]
        public void WriteInt64LE_MatchesBinaryPrimitives(long value)
        {
            AssertWriteMatches(8,
                (buf, off) => CdrBinaryHelper.WriteInt64LE(buf, off, value),
                (buf, off) => BinaryPrimitives.WriteInt64LittleEndian(buf.AsSpan(off), value));
        }

        [Theory]
        [InlineData(0L)]
        [InlineData(1L)]
        [InlineData(-1L)]
        [InlineData(long.MaxValue)]
        [InlineData(long.MinValue)]
        [InlineData(0x123456789ABCDEF0L)]
        [InlineData(unchecked((long)0xFEDCBA9876543210L))]
        public void WriteInt64BE_MatchesBinaryPrimitives(long value)
        {
            AssertWriteMatches(8,
                (buf, off) => CdrBinaryHelper.WriteInt64BE(buf, off, value),
                (buf, off) => BinaryPrimitives.WriteInt64BigEndian(buf.AsSpan(off), value));
        }

        [Theory]
        [InlineData(0UL)]
        [InlineData(1UL)]
        [InlineData(ulong.MaxValue)]
        [InlineData(0x123456789ABCDEF0UL)]
        [InlineData(0xFEDCBA9876543210UL)]
        public void WriteUInt64LE_MatchesBinaryPrimitives(ulong value)
        {
            AssertWriteMatches(8,
                (buf, off) => CdrBinaryHelper.WriteUInt64LE(buf, off, value),
                (buf, off) => BinaryPrimitives.WriteUInt64LittleEndian(buf.AsSpan(off), value));
        }

        [Theory]
        [InlineData(0UL)]
        [InlineData(1UL)]
        [InlineData(ulong.MaxValue)]
        [InlineData(0x123456789ABCDEF0UL)]
        [InlineData(0xFEDCBA9876543210UL)]
        public void WriteUInt64BE_MatchesBinaryPrimitives(ulong value)
        {
            AssertWriteMatches(8,
                (buf, off) => CdrBinaryHelper.WriteUInt64BE(buf, off, value),
                (buf, off) => BinaryPrimitives.WriteUInt64BigEndian(buf.AsSpan(off), value));
        }

        [Theory]
        [InlineData(0L)]
        [InlineData(1L)]
        [InlineData(-1L)]
        [InlineData(long.MaxValue)]
        [InlineData(long.MinValue)]
        [InlineData(0x123456789ABCDEF0L)]
        public void ReadInt64_RoundTripsAcrossEndianness(long value)
        {
            var buf = new byte[8 + OffsetMargin * 2];
            CdrBinaryHelper.WriteInt64LE(buf, OffsetMargin, value);
            Assert.Equal(value, CdrBinaryHelper.ReadInt64LE(buf, OffsetMargin));
            Assert.Equal(BinaryPrimitives.ReadInt64LittleEndian(buf.AsSpan(OffsetMargin)),
                         CdrBinaryHelper.ReadInt64LE(buf, OffsetMargin));

            CdrBinaryHelper.WriteInt64BE(buf, OffsetMargin, value);
            Assert.Equal(value, CdrBinaryHelper.ReadInt64BE(buf, OffsetMargin));
            Assert.Equal(BinaryPrimitives.ReadInt64BigEndian(buf.AsSpan(OffsetMargin)),
                         CdrBinaryHelper.ReadInt64BE(buf, OffsetMargin));
        }

        [Theory]
        [InlineData(0UL)]
        [InlineData(1UL)]
        [InlineData(ulong.MaxValue)]
        [InlineData(0x123456789ABCDEF0UL)]
        [InlineData(0xFEDCBA9876543210UL)]
        public void ReadUInt64_RoundTripsAcrossEndianness(ulong value)
        {
            var buf = new byte[8 + OffsetMargin * 2];
            CdrBinaryHelper.WriteUInt64LE(buf, OffsetMargin, value);
            Assert.Equal(value, CdrBinaryHelper.ReadUInt64LE(buf, OffsetMargin));
            Assert.Equal(BinaryPrimitives.ReadUInt64LittleEndian(buf.AsSpan(OffsetMargin)),
                         CdrBinaryHelper.ReadUInt64LE(buf, OffsetMargin));

            CdrBinaryHelper.WriteUInt64BE(buf, OffsetMargin, value);
            Assert.Equal(value, CdrBinaryHelper.ReadUInt64BE(buf, OffsetMargin));
            Assert.Equal(BinaryPrimitives.ReadUInt64BigEndian(buf.AsSpan(OffsetMargin)),
                         CdrBinaryHelper.ReadUInt64BE(buf, OffsetMargin));
        }

        // ---- Offset isolation -----------------------------------------------

        [Fact]
        public void Write_DoesNotTouchBytesOutsideTheTargetRange()
        {
            // Sentinel-fill the buffer, write at the middle, then verify only the target
            // bytes changed. Catches off-by-one regressions in offset arithmetic.
            const int total = 32;
            const int offset = 8;

            var buf = new byte[total];
            for (int i = 0; i < total; i++) buf[i] = 0xAA;

            CdrBinaryHelper.WriteInt64BE(buf, offset, 0x0102030405060708L);

            for (int i = 0; i < offset; i++)
                Assert.Equal((byte)0xAA, buf[i]);
            for (int i = offset + 8; i < total; i++)
                Assert.Equal((byte)0xAA, buf[i]);
        }

        // ---- Helper ---------------------------------------------------------

        private static void AssertWriteMatches(
            int width,
            Action<byte[], int> writeWithHelper,
            Action<byte[], int> writeWithPrimitives)
        {
            // Offset 0
            var helperBuf0 = new byte[width];
            var primitivesBuf0 = new byte[width];
            writeWithHelper(helperBuf0, 0);
            writeWithPrimitives(primitivesBuf0, 0);
            Assert.Equal(primitivesBuf0, helperBuf0);

            // Non-zero offset (catches off-by-one in offset arithmetic)
            var helperBuf = new byte[width + OffsetMargin * 2];
            var primitivesBuf = new byte[width + OffsetMargin * 2];
            writeWithHelper(helperBuf, OffsetMargin);
            writeWithPrimitives(primitivesBuf, OffsetMargin);
            Assert.Equal(primitivesBuf, helperBuf);
        }
    }
}
