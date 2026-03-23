using Int2Dds.Cdr;
using Xunit;

namespace Int2Dds.Tests;

public class CdrReaderTests
{
    [Fact]
    public void Remaining_AfterRead_Decreases()
    {
        var w = new CdrWriter();
        w.WriteU32(1);
        w.WriteU32(2);

        var r = new CdrReader(w.ToBytes());
        var initialRemaining = r.Remaining;
        r.ReadU32();
        Assert.True(r.Remaining < initialRemaining);
    }

    [Fact]
    public void Position_AfterEncapHeader_Is4()
    {
        var w = new CdrWriter();
        var r = new CdrReader(w.ToBytes());
        Assert.Equal(4, r.Position);
    }

    [Fact]
    public void Skip_MovesPosition()
    {
        var w = new CdrWriter();
        w.WriteU32(42);
        w.WriteU32(99);

        var r = new CdrReader(w.ToBytes());
        r.Skip(4); // skip first u32
        Assert.Equal(99u, r.ReadU32());
    }

    [Fact]
    public void ReadUnderflow_ThrowsCdrUnderflowException()
    {
        var w = new CdrWriter();
        w.WriteU8(1);

        var r = new CdrReader(w.ToBytes());
        r.ReadU8(); // consume the byte
        Assert.Throws<CdrUnderflowException>(() => r.ReadU32()); // not enough data
    }

    [Fact]
    public void Appendable_DheaderRoundTrip()
    {
        var w = new CdrWriter(Extensibility.Appendable);
        var token = w.DheaderBegin();
        w.WriteU32(42);
        w.WriteString("hello");
        w.DheaderFinalize(token);

        var r = new CdrReader(w.ToBytes());
        var (objectSize, startPos) = r.ReadDheader();
        Assert.True(objectSize > 0);
        var val = r.ReadU32();
        Assert.Equal(42u, val);
        var str = r.ReadString();
        Assert.Equal("hello", str);
    }

    [Fact]
    public void Mutable_EmheaderAndSentinel_RoundTrip()
    {
        var w = new CdrWriter(Extensibility.Mutable);
        var token = w.DheaderBegin();

        // Write member 0: a single u32
        var emToken1 = w.EmheaderBegin(0, mustUnderstand: true);
        w.WriteU32(100);
        w.EmheaderFinalize(emToken1);

        // Write sentinel
        w.WriteSentinel();
        w.DheaderFinalize(token);

        var r = new CdrReader(w.ToBytes());
        var (_, startPos) = r.ReadDheader();

        // Read member 0
        var (memberId1, dataLen1, mustUnderstand1) = r.ReadEmheader();
        Assert.Equal(0u, memberId1);
        Assert.True(mustUnderstand1);
        Assert.Equal(100u, r.ReadU32());

        // Reader should now be at sentinel
        Assert.True(r.IsSentinel, $"Expected sentinel at position {r.Position}, remaining={r.Remaining}");
    }

    [Fact]
    public void HelloWorld_SerializeDeserialize()
    {
        // Manually serialize HelloWorld { index: 42, message: "Hello" }
        var w = new CdrWriter(Extensibility.Final);
        w.WriteU32(42);
        w.WriteString("Hello");

        var data = w.ToBytes();
        var r = new CdrReader(data);

        Assert.Equal(42u, r.ReadU32());
        Assert.Equal("Hello", r.ReadString());
    }

    [Fact]
    public void Sequence_WriteAndRead()
    {
        var w = new CdrWriter();
        // Sequence of 3 u32 values
        w.WriteSeqHeader(3);
        w.WriteU32(10);
        w.WriteU32(20);
        w.WriteU32(30);

        var r = new CdrReader(w.ToBytes());
        var count = r.ReadSeqHeader();
        Assert.Equal(3u, count);

        var values = new uint[count];
        for (int i = 0; i < count; i++)
            values[i] = r.ReadU32();

        Assert.Equal([10u, 20u, 30u], values);
    }

    [Fact]
    public void EmptySequence_RoundTrips()
    {
        var w = new CdrWriter();
        w.WriteSeqHeader(0);

        var r = new CdrReader(w.ToBytes());
        Assert.Equal(0u, r.ReadSeqHeader());
    }
}
