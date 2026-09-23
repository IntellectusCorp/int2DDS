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

    /// <summary>
    /// Marks a generated union class. The runtime advertises it with a union TypeObject whose
    /// discriminator is the given <c>INT2DDS_FIELD_*</c> scalar kind; the case members and
    /// their labels come from the class's <c>DdsTypeInfoFields</c>.
    /// </summary>
    [AttributeUsage(AttributeTargets.Class)]
    public class DdsUnionAttribute : Attribute
    {
        /// <summary>The <c>INT2DDS_FIELD_*</c> kind of the switch discriminator.</summary>
        public int DiscriminatorType { get; }

        public DdsUnionAttribute(int discriminatorType)
        {
            DiscriminatorType = discriminatorType;
        }
    }

    /// <summary>
    /// Marks a generated bitset class. The runtime advertises it with a bitset TypeObject
    /// whose bitfields come from the class's <c>DdsTypeInfoFields</c> (<c>bitfield</c> ops).
    /// </summary>
    [AttributeUsage(AttributeTargets.Class)]
    public class DdsBitsetAttribute : Attribute
    {
    }

    /// <summary>
    /// Marks a generated bitmask enum. The runtime advertises it with a bitmask TypeObject of
    /// the given IDL <c>@bit_bound</c>; each enum member becomes a flag at the bit position of
    /// its value.
    /// </summary>
    [AttributeUsage(AttributeTargets.Enum)]
    public class DdsBitmaskAttribute : Attribute
    {
        public string TypeName { get; }
        public int BitBound { get; }

        public DdsBitmaskAttribute(string typeName, int bitBound)
        {
            TypeName = typeName;
            BitBound = bitBound;
        }
    }

    public interface IDdsType
    {
        byte[] SerializeCdr();
        byte[] SerializeCdr(bool xcdr2);
    }
}
