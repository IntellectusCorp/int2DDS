using System;
using System.Runtime.InteropServices;

namespace Int2Dds.Interop
{
    /// <summary>
    /// Field kinds for <see cref="NativeCFieldLayout"/>. Mirrors the header's
    /// <c>Int2DdsCFieldKind</c>; declaration-only (the C layout path is consumed
    /// by generated C code, not by this binding).
    /// </summary>
    internal enum CFieldKind
    {
        CFieldNone = 0,
        CFieldBool = 1,
        CFieldInt8 = 2,
        CFieldUInt8 = 3,
        CFieldInt16 = 4,
        CFieldUInt16 = 5,
        CFieldInt32 = 6,
        CFieldUInt32 = 7,
        CFieldInt64 = 8,
        CFieldUInt64 = 9,
        CFieldFloat32 = 10,
        CFieldFloat64 = 11,
        CFieldChar8 = 12,
        CFieldEnum = 13,
        CFieldBitmask = 14,
        CFieldString = 15,
        CFieldStringPtr = 16,
        CFieldWString = 17,
        CFieldStruct = 18,
        CFieldArray = 19,
        CFieldSequence = 20,
        CFieldSequencePtr = 21,
    }

    /// <summary>Mirrors the header's <c>Int2DdsCFieldLayout</c>.</summary>
    [StructLayout(LayoutKind.Sequential)]
    internal struct NativeCFieldLayout
    {
        public IntPtr Name;
        public CFieldKind Kind;
        public uint Offset;
        public uint LengthOffset;
        public uint Size;
        public uint Count;
        public CFieldKind ElemKind;
        public uint ElemSize;
        public IntPtr Nested;
    }

    /// <summary>Mirrors the header's <c>Int2DdsCTypeLayout</c>.</summary>
    [StructLayout(LayoutKind.Sequential)]
    internal struct NativeCTypeLayout
    {
        public IntPtr TypeName;
        public uint StructSize;
        public uint FieldCount;
        public IntPtr Fields;
    }
}
