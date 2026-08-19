namespace Int2Dds.Types
{
    /// <summary>
    /// Bits of the <c>flags</c> word carried by a type member, as the header's
    /// <c>INT2DDS_MEMBER_*</c> constants define them. Both <see cref="DdsTypeInfoField.Flags"/>
    /// and <c>Xtypes.MemberInfo.Flags</c> are masks of these.
    /// </summary>
    public static class MemberFlags
    {
        public const int Key = 1 << 0;
        public const int Optional = 1 << 1;
        public const int MustUnderstand = 1 << 2;
        public const int External = 1 << 3;
    }
}
