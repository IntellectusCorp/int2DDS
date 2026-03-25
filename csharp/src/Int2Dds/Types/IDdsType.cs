using System;

namespace Int2Dds.Types
{
    [AttributeUsage(AttributeTargets.Class)]
    public class DdsTypeAttribute : Attribute
    {
        public string TypeName { get; }
        public int Extensibility { get; }  // 0=Final, 1=Appendable, 2=Mutable
        public bool HasKey { get; }

        public DdsTypeAttribute(string typeName, int extensibility, bool hasKey)
        {
            TypeName = typeName;
            Extensibility = extensibility;
            HasKey = hasKey;
        }
    }

    public interface IDdsType
    {
        byte[] SerializeCdr();
        byte[] SerializeCdr(bool xcdr2);
        byte[] SerializeKey();
    }
}
