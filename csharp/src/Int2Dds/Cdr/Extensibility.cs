// Copyright IntellectusCorp. All rights reserved.
// Licensed under the int2DDS license.

namespace Int2Dds.Cdr;

/// <summary>
/// XCDR2 extensibility kinds used to select the encapsulation encoding.
/// </summary>
public enum Extensibility
{
    /// <summary>PLAIN_CDR2 — no DHEADER, no EMHEADER.</summary>
    Final = 0,

    /// <summary>DELIMITED_CDR2 — DHEADER around each aggregate.</summary>
    Appendable = 1,

    /// <summary>PL_CDR2 — EMHEADER per member + sentinel.</summary>
    Mutable = 2,
}
