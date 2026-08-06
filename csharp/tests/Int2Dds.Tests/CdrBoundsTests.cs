using System;
using Int2Dds.Cdr;
using Xunit;

namespace Int2Dds.Tests
{
    /// <summary>Bounds-check hardening (issue #378).</summary>
    public class CdrBoundsTests
    {
        private static byte[] Raw(params uint[] words)
        {
            var bytes = new byte[words.Length * 4];
            for (int i = 0; i < words.Length; i++)
                BitConverter.GetBytes(words[i]).CopyTo(bytes, i * 4);
            return bytes;
        }

        [Fact]
        public void ReadString_LengthAboveIntMax_ThrowsUnderflow()
        {
            var r = CdrReader.FromRaw(Raw(0x80000000u, 0, 0, 0));
            Assert.Throws<CdrUnderflowException>(() => r.ReadString());
        }

        [Fact]
        public void ReadString_OversizedLength_ThrowsUnderflow()
        {
            var r = CdrReader.FromRaw(Raw(0xFFFFFFF0u, 0, 0, 0));
            Assert.Throws<CdrUnderflowException>(() => r.ReadString());
        }

        [Fact]
        public void ReadWString_OversizedLength_ThrowsInsteadOfAllocating()
        {
            // Without the remaining-bytes check this sizes a char[] straight from
            // the wire and tries to allocate roughly 2 GB.
            var r = CdrReader.FromRaw(Raw(0x40000001u, 0, 0, 0));
            Assert.Throws<CdrUnderflowException>(() => r.ReadWString());
        }

        [Fact]
        public void ReadDheaderEnd_OverflowingObjectSize_ThrowsUnderflow()
        {
            var r = CdrReader.FromRaw(Raw(0, 0, 0, 0), littleEndian: true, xcdr2: true);
            Assert.Throws<CdrUnderflowException>(() => r.ReadDheaderEnd(0xFFFFFFF0u, 8));
        }

        [Fact]
        public void ReadDheaderEnd_NegativeStart_ThrowsUnderflow()
        {
            var r = CdrReader.FromRaw(Raw(0, 0, 0, 0), littleEndian: true, xcdr2: true);
            Assert.Throws<CdrUnderflowException>(() => r.ReadDheaderEnd(4u, -8));
        }

        [Fact]
        public void DheaderFinalize_NegativeToken_Throws()
        {
            var w = new CdrWriter(xcdr2: true);
            w.WriteU32(0xAAAAAAAAu);
            Assert.Throws<ArgumentOutOfRangeException>(() => w.DheaderFinalize(-4));
        }

        [Fact]
        public void DheaderFinalize_TokenPastPosition_Throws()
        {
            var w = new CdrWriter(xcdr2: true);
            w.WriteU32(0xAAAAAAAAu);
            Assert.Throws<ArgumentOutOfRangeException>(() => w.DheaderFinalize(w.Length + 16));
        }

        [Fact]
        public void EmheaderFinalize_BadToken_Throws()
        {
            var w = new CdrWriter(xcdr2: true);
            w.WriteU32(0xAAAAAAAAu);
            Assert.Throws<ArgumentOutOfRangeException>(() => w.EmheaderFinalize(-4));
        }

        [Fact]
        public void MemberV1Finalize_BadToken_Throws()
        {
            var w = new CdrWriter();
            w.WriteU32(0xAAAAAAAAu);
            Assert.Throws<ArgumentOutOfRangeException>(() => w.MemberV1Finalize(-4, 5, false));
        }

        [Fact]
        public void DheaderRoundTrip_StillWorks()
        {
            var w = new CdrWriter(xcdr2: true);
            int token = w.DheaderBegin();
            w.WriteU32(0xDEADBEEFu);
            w.DheaderFinalize(token);

            var r = new CdrReader(w.ToBytes());
            var (size, start) = r.ReadDheader();
            Assert.Equal(4u, size);
            Assert.Equal(0xDEADBEEFu, r.ReadU32());
            r.ReadDheaderEnd(size, start);
        }

        [Fact]
        public void MemberV1RoundTrip_StillWorks()
        {
            var w = new CdrWriter();
            int token = w.MemberV1Begin(5);
            w.WriteU32(0xCAFEBABEu);
            w.MemberV1Finalize(token, 5, false);
            Assert.True(w.ToBytes().Length > 4);
        }

        [Fact]
        public void StringRoundTrip_StillWorks()
        {
            var w = new CdrWriter();
            w.WriteString("hello");
            w.WriteWString("world");

            var r = new CdrReader(w.ToBytes());
            Assert.Equal("hello", r.ReadString());
            Assert.Equal("world", r.ReadWString());
        }
    }
}
