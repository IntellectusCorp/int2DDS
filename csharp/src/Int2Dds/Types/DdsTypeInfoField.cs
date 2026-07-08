namespace Int2Dds.Types
{
    /// <summary>
    /// One field descriptor emitted by the IDL generator for flat types (all members are
    /// primitives, strings, or sequences/arrays of primitives). The runtime reads the
    /// generated <c>DdsTypeInfoFields</c> array to build an <c>Int2DdsTypeInfo</c> and
    /// advertise a conformant TypeObject during discovery, matching the Rust derive.
    /// </summary>
    public readonly struct DdsTypeInfoField
    {
        /// <summary>Builder operation: "field", "string", "wstring", "seq", or "arr".</summary>
        public string Op { get; }

        /// <summary>DDS member name (the IDL name, used in the TypeObject).</summary>
        public string Name { get; }

        /// <summary>INT2DDS_FIELD_* constant; the element type for "seq"/"arr".</summary>
        public int TypeConst { get; }

        /// <summary>String/sequence bound or array size (0 = unbounded).</summary>
        public uint Size { get; }

        /// <summary>INT2DDS_MEMBER_* flag bitmask (KEY=1, OPTIONAL=2, MUST_UNDERSTAND=4, EXTERNAL=8).</summary>
        public int Flags { get; }

        public DdsTypeInfoField(string op, string name, int typeConst, uint size, int flags)
        {
            Op = op;
            Name = name;
            TypeConst = typeConst;
            Size = size;
            Flags = flags;
        }
    }
}
