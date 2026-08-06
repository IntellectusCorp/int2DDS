using System;
using Int2Dds.Cdr;
using Int2Dds.Types;

namespace Int2Dds.Tests.Helpers
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
            SerializeCdr(w);
            return w.ToBytes();
        }

        public void SerializeCdr(CdrWriter w)
        {
            w.WriteU32(Index);
            w.WriteString(Message);
        }

        public static HelloWorld DeserializeCdr(byte[] data)
        {
            var r = new CdrReader(data);
            var index = r.ReadU32();
            var message = r.ReadString();
            return new HelloWorld(index, message);
        }

        private static readonly byte[] s_emptyKey = new byte[0];
        public byte[] SerializeKey() => s_emptyKey;
    }
}
