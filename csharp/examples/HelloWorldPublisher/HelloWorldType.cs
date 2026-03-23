using Int2Dds.Cdr;
using Int2Dds.Types;

namespace HelloWorldPublisher;

/// <summary>
/// IDL struct: HelloWorld { unsigned long index; string message; };
/// Extensibility: FINAL, non-keyed.
/// </summary>
public class HelloWorld : IDdsType<HelloWorld>
{
    public static string DdsTypeName => "HelloWorld";
    public static Extensibility TypeExtensibility => Extensibility.Final;
    public static bool HasKey => false;

    public uint Index { get; set; }
    public string Message { get; set; } = "";

    public HelloWorld() { }

    public HelloWorld(uint index, string message)
    {
        Index = index;
        Message = message;
    }

    public byte[] SerializeCdr()
    {
        var w = new CdrWriter(TypeExtensibility);
        w.WriteU32(Index);
        w.WriteString(Message);
        return w.ToBytes();
    }

    public static HelloWorld DeserializeCdr(ReadOnlySpan<byte> data)
    {
        var r = new CdrReader(data);
        return new HelloWorld(r.ReadU32(), r.ReadString());
    }

    public byte[] SerializeKey() => [];
}
