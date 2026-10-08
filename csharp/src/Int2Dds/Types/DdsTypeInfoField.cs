using System;

namespace Int2Dds.Types
{
    /// <summary>
    /// One field descriptor emitted by the IDL generator for advertisable types (members are
    /// primitives, strings, enums, bitmasks, bitsets, nested structs/unions that are themselves
    /// advertisable, sequences/arrays of those, or maps with a scalar key). The runtime reads
    /// the generated <c>DdsTypeInfoFields</c> array to build an <c>Int2DdsTypeInfo</c> and
    /// advertise a conformant TypeObject during discovery, matching the Rust derive.
    /// </summary>
    public readonly struct DdsTypeInfoField
    {
        /// <summary>
        /// Builder operation: "field", "string", "wstring", "seq", "arr", "nested",
        /// "seq_nested", "arr_nested", "map", "map_nested", "bitfield" (bitset classes) or
        /// "label" (union classes; attaches a case label to the member named <see cref="Name"/>).
        /// </summary>
        public string Op { get; }

        /// <summary>DDS member name (the IDL name, used in the TypeObject).</summary>
        public string Name { get; }

        /// <summary>
        /// INT2DDS_FIELD_* constant; the element type for "seq"/"arr", the value type for
        /// "map", the holder kind for "bitfield", the label value for "label".
        /// </summary>
        public int TypeConst { get; }

        /// <summary>
        /// String/sequence/map bound or array size (0 = unbounded); the bit width for
        /// "bitfield".
        /// </summary>
        public uint Size { get; }

        /// <summary>INT2DDS_MEMBER_* flag bitmask (KEY=1, OPTIONAL=2, MUST_UNDERSTAND=4, EXTERNAL=8, DEFAULT=16).</summary>
        public int Flags { get; }

        /// <summary>
        /// For the "nested"/"seq_nested"/"arr_nested"/"map_nested" ops: the generated CLR type
        /// of the nested member (the element type for the collection ops, the value type for
        /// "map_nested"). The runtime recursively builds its type_info and references it by
        /// content-hash so composite keys resolve. Null for all other ops.
        /// </summary>
        public Type? NestedType { get; }

        /// <summary>For the "map"/"map_nested" ops: the INT2DDS_FIELD_* kind of the key.</summary>
        public int KeyType { get; }

        /// <summary>For the "map"/"map_nested" ops: the key's string bound (0 = unbounded).</summary>
        public uint KeyBound { get; }

        /// <summary>For the "map" op: the value's string bound (0 = unbounded).</summary>
        public uint ValueBound { get; }

        public DdsTypeInfoField(string op, string name, int typeConst, uint size, int flags)
        {
            Op = op;
            Name = name;
            TypeConst = typeConst;
            Size = size;
            Flags = flags;
            NestedType = null;
            KeyType = 0;
            KeyBound = 0;
            ValueBound = 0;
        }

        public DdsTypeInfoField(string op, string name, Type nestedType, int flags)
        {
            Op = op;
            Name = name;
            TypeConst = 0;
            Size = 0;
            Flags = flags;
            NestedType = nestedType;
            KeyType = 0;
            KeyBound = 0;
            ValueBound = 0;
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
            KeyType = 0;
            KeyBound = 0;
            ValueBound = 0;
        }

        /// <summary>Scalar map ("map"): key kind/bound, value kind/bound and the map bound.</summary>
        public DdsTypeInfoField(string op, string name, int keyType, uint keyBound, int valueType, uint valueBound, uint bound, int flags)
        {
            Op = op;
            Name = name;
            TypeConst = valueType;
            Size = bound;
            Flags = flags;
            NestedType = null;
            KeyType = keyType;
            KeyBound = keyBound;
            ValueBound = valueBound;
        }

        /// <summary>Map of nested ("map_nested"): key kind/bound, the value's CLR type and the map bound.</summary>
        public DdsTypeInfoField(string op, string name, int keyType, uint keyBound, Type valueType, uint bound, int flags)
        {
            Op = op;
            Name = name;
            TypeConst = 0;
            Size = bound;
            Flags = flags;
            NestedType = valueType;
            KeyType = keyType;
            KeyBound = keyBound;
            ValueBound = 0;
        }
    }
}
