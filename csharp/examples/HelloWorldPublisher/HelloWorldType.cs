using System;
using Int2Dds.Cdr;
using Int2Dds.Types;

namespace HelloWorldPublisher
{
    /// <summary>
    /// IDL struct: HelloWorld { unsigned long index; string message; };
    /// Extensibility: FINAL, non-keyed.
    /// </summary>
    [DdsType("HelloWorld", 0, false)]
    public class HelloWorld : IDdsType
    {
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
            return SerializeCdr(false);
        }

        public byte[] SerializeCdr(bool xcdr2)
        {
            var w = new CdrWriter(Extensibility.Final);
            w.WriteU32(Index);
            w.WriteString(Message);
            return w.ToBytes();
        }

        public static HelloWorld DeserializeCdr(byte[] data)
        {
            var r = new CdrReader(data);
            return new HelloWorld(r.ReadU32(), r.ReadString());
        }

        public byte[] SerializeKey() => Array.Empty<byte>();
    }
}
