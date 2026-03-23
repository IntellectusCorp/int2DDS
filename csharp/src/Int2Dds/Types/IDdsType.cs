namespace Int2Dds.Types;

using Int2Dds.Cdr;

public interface IDdsType<T> where T : IDdsType<T>
{
    static abstract string DdsTypeName { get; }
    static abstract Extensibility TypeExtensibility { get; }
    static abstract bool HasKey { get; }

    byte[] SerializeCdr();
    static abstract T DeserializeCdr(ReadOnlySpan<byte> data);
    byte[] SerializeKey();
}
