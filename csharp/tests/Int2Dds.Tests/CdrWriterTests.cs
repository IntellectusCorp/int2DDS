using Int2Dds.Cdr;
using Xunit;

namespace Int2Dds.Tests
{
    public class CdrWriterTests
    {
        [Fact]
        public void WriteBool_True_RoundTrips()
        {
            var w = new CdrWriter();
            w.WriteBool(true);
            var r = new CdrReader(w.ToBytes());
            Assert.True(r.ReadBool());
        }

        [Fact]
        public void WriteBool_False_RoundTrips()
        {
            var w = new CdrWriter();
            w.WriteBool(false);
            var r = new CdrReader(w.ToBytes());
            Assert.False(r.ReadBool());
        }

        [Theory]
        [InlineData(0)]
        [InlineData(127)]
        [InlineData(-128)]
        public void WriteI8_RoundTrips(sbyte value)
        {
            var w = new CdrWriter();
            w.WriteI8(value);
            var r = new CdrReader(w.ToBytes());
            Assert.Equal(value, r.ReadI8());
        }

        [Theory]
        [InlineData(0)]
        [InlineData(255)]
        public void WriteU8_RoundTrips(byte value)
        {
            var w = new CdrWriter();
            w.WriteU8(value);
            var r = new CdrReader(w.ToBytes());
            Assert.Equal(value, r.ReadU8());
        }

        [Theory]
        [InlineData((short)0)]
        [InlineData(short.MaxValue)]
        [InlineData(short.MinValue)]
        public void WriteI16_RoundTrips(short value)
        {
            var w = new CdrWriter();
            w.WriteI16(value);
            var r = new CdrReader(w.ToBytes());
            Assert.Equal(value, r.ReadI16());
        }

        [Theory]
        [InlineData((ushort)0)]
        [InlineData(ushort.MaxValue)]
        public void WriteU16_RoundTrips(ushort value)
        {
            var w = new CdrWriter();
            w.WriteU16(value);
            var r = new CdrReader(w.ToBytes());
            Assert.Equal(value, r.ReadU16());
        }

        [Theory]
        [InlineData(0)]
        [InlineData(int.MaxValue)]
        [InlineData(int.MinValue)]
        [InlineData(42)]
        public void WriteI32_RoundTrips(int value)
        {
            var w = new CdrWriter();
            w.WriteI32(value);
            var r = new CdrReader(w.ToBytes());
            Assert.Equal(value, r.ReadI32());
        }

        [Theory]
        [InlineData(0u)]
        [InlineData(uint.MaxValue)]
        [InlineData(12345u)]
        public void WriteU32_RoundTrips(uint value)
        {
            var w = new CdrWriter();
            w.WriteU32(value);
            var r = new CdrReader(w.ToBytes());
            Assert.Equal(value, r.ReadU32());
        }

        [Theory]
        [InlineData(0L)]
        [InlineData(long.MaxValue)]
        [InlineData(long.MinValue)]
        public void WriteI64_RoundTrips(long value)
        {
            var w = new CdrWriter();
            w.WriteI64(value);
            var r = new CdrReader(w.ToBytes());
            Assert.Equal(value, r.ReadI64());
        }

        [Theory]
        [InlineData(0UL)]
        [InlineData(ulong.MaxValue)]
        public void WriteU64_RoundTrips(ulong value)
        {
            var w = new CdrWriter();
            w.WriteU64(value);
            var r = new CdrReader(w.ToBytes());
            Assert.Equal(value, r.ReadU64());
        }

        [Theory]
        [InlineData(0.0f)]
        [InlineData(3.14f)]
        [InlineData(float.MaxValue)]
        [InlineData(float.MinValue)]
        public void WriteF32_RoundTrips(float value)
        {
            var w = new CdrWriter();
            w.WriteF32(value);
            var r = new CdrReader(w.ToBytes());
            Assert.Equal(value, r.ReadF32());
        }

        [Theory]
        [InlineData(0.0)]
        [InlineData(3.141592653589793)]
        [InlineData(double.MaxValue)]
        public void WriteF64_RoundTrips(double value)
        {
            var w = new CdrWriter();
            w.WriteF64(value);
            var r = new CdrReader(w.ToBytes());
            Assert.Equal(value, r.ReadF64());
        }

        [Theory]
        [InlineData("")]
        [InlineData("Hello, World!")]
        [InlineData("Unicode: \ud55c\uae00 \u65e5\u672c\u8a9e")]
        public void WriteString_RoundTrips(string value)
        {
            var w = new CdrWriter();
            w.WriteString(value);
            var r = new CdrReader(w.ToBytes());
            Assert.Equal(value, r.ReadString());
        }

        [Fact]
        public void WriteMultipleValues_RoundTrips()
        {
            var w = new CdrWriter();
            w.WriteU32(42);
            w.WriteString("hello");
            w.WriteBool(true);
            w.WriteF64(3.14);

            var r = new CdrReader(w.ToBytes());
            Assert.Equal(42u, r.ReadU32());
            Assert.Equal("hello", r.ReadString());
            Assert.True(r.ReadBool());
            Assert.Equal(3.14, r.ReadF64());
        }

        [Fact]
        public void WriteEnum_RoundTrips()
        {
            var w = new CdrWriter();
            w.WriteEnum(3);
            var r = new CdrReader(w.ToBytes());
            Assert.Equal(3, r.ReadEnum());
        }

        [Fact]
        public void WriteSeqHeader_RoundTrips()
        {
            var w = new CdrWriter();
            w.WriteSeqHeader(5);
            for (uint i = 0; i < 5; i++) w.WriteU32(i);
            var r = new CdrReader(w.ToBytes());
            Assert.Equal(5u, r.ReadSeqHeader());
            for (uint i = 0; i < 5; i++) Assert.Equal(i, r.ReadU32());
        }

        [Fact]
        public void WriteBytes_RoundTrips()
        {
            byte[] data = new byte[] { 1, 2, 3, 4, 5 };
            var w = new CdrWriter();
            w.WriteBytes(data);
            var r = new CdrReader(w.ToBytes());
            var result = r.ReadBytes(5);
            Assert.Equal(data, result);
        }

        [Fact]
        public void EncapsulationHeader_Final_LittleEndian()
        {
            var w = new CdrWriter(Extensibility.Final, littleEndian: true, xcdr2: true);
            var bytes = w.ToBytes();
            // Encapsulation header: CDR2_LE = 0x0007 (big-endian encoded)
            Assert.True(bytes.Length >= 4);
            Assert.Equal(0x00, bytes[0]);
            Assert.Equal(0x07, bytes[1]);
            Assert.Equal(0x00, bytes[2]);
            Assert.Equal(0x00, bytes[3]);
        }

        [Fact]
        public void EncapsulationHeader_Appendable_LittleEndian()
        {
            var w = new CdrWriter(Extensibility.Appendable, littleEndian: true, xcdr2: true);
            var bytes = w.ToBytes();
            Assert.Equal(0x00, bytes[0]);
            Assert.Equal(0x09, bytes[1]); // DCDR2_LE = 0x0009
        }

        [Fact]
        public void EncapsulationHeader_Mutable_LittleEndian()
        {
            var w = new CdrWriter(Extensibility.Mutable, littleEndian: true, xcdr2: true);
            var bytes = w.ToBytes();
            Assert.Equal(0x00, bytes[0]);
            Assert.Equal(0x0B, bytes[1]); // PL_CDR2_LE = 0x000B
        }
    }
}
