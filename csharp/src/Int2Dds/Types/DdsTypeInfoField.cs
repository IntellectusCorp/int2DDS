using System;

namespace Int2Dds.Types
{
    /// <summary>
    /// One field descriptor emitted by the IDL generator for flat types (all members are
    /// primitives, strings, sequences/arrays of primitives, or nested structs that are
    /// themselves flat). The runtime reads the generated <c>DdsTypeInfoFields</c> array to
    /// build an <c>Int2DdsTypeInfo</c> and advertise a conformant TypeObject during discovery,
    /// matching the Rust derive.
    /// </summary>
    public readonly struct DdsTypeInfoField
    {
        /// <summary>Builder operation: "field", "string", "wstring", "seq", "arr", or "nested".</summary>
        public string Op { get; }

        /// <summary>DDS member name (the IDL name, used in the TypeObject).</summary>
        public string Name { get; }

        /// <summary>INT2DDS_FIELD_* constant; the element type for "seq"/"arr".</summary>
        public int TypeConst { get; }

        /// <summary>String/sequence bound or array size (0 = unbounded).</summary>
        public uint Size { get; }

        /// <summary>INT2DDS_MEMBER_* flag bitmask (KEY=1, OPTIONAL=2, MUST_UNDERSTAND=4, EXTERNAL=8).</summary>
        public int Flags { get; }

        /// <summary>
        /// For the "nested"/"seq_nested"/"arr_nested" ops: the generated CLR type of the nested
        /// struct/enum member (the element type for the collection ops). The runtime recursively
        /// builds its type_info and references it by content-hash so composite keys resolve. Null
        /// for all other ops.
        /// </summary>
        public Type? NestedType { get; }

        public DdsTypeInfoField(string op, string name, int typeConst, uint size, int flags)
        {
            Op = op;
            Name = name;
            TypeConst = typeConst;
            Size = size;
            Flags = flags;
            NestedType = null;
        }

        public DdsTypeInfoField(string op, string name, Type nestedType, int flags)
        {
            Op = op;
            Name = name;
            TypeConst = 0;
            Size = 0;
            Flags = flags;
            NestedType = nestedType;
        }

        /// <summary>Collection-of-nested ("seq_nested"/"arr_nested"): element type + bound/size.</summary>
        public DdsTypeInfoField(string op, string name, Type elementType, uint size, int flags)
        {
            Op = op;
            Name = name;
            TypeConst = 0;
            Size = size;
            Flags = flags;
            NestedType = elementType;
        }
    }
}
