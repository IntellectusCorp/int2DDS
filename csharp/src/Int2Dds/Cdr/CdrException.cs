// Copyright IntellectusCorp. All rights reserved.
// Licensed under the int2DDS license.

using System;

namespace Int2Dds.Cdr;

/// <summary>
/// Base exception for CDR serialization/deserialization errors.
/// </summary>
public class CdrException : Exception
{
    public CdrException() { }
    public CdrException(string message) : base(message) { }
    public CdrException(string message, Exception innerException) : base(message, innerException) { }
}

/// <summary>
/// Thrown when a CDR writer exceeds its buffer capacity.
/// </summary>
public class CdrOverflowException : CdrException
{
    public CdrOverflowException() : base("CDR writer buffer overflow.") { }
    public CdrOverflowException(string message) : base(message) { }
    public CdrOverflowException(string message, Exception innerException) : base(message, innerException) { }
}

/// <summary>
/// Thrown when a CDR reader has insufficient data remaining.
/// </summary>
public class CdrUnderflowException : CdrException
{
    public CdrUnderflowException() : base("CDR reader buffer underflow.") { }
    public CdrUnderflowException(string message) : base(message) { }
    public CdrUnderflowException(string message, Exception innerException) : base(message, innerException) { }
}

/// <summary>
/// Thrown when a CDR reader encounters an unrecognized encapsulation ID.
/// </summary>
public class CdrInvalidEncapsulationException : CdrException
{
    public CdrInvalidEncapsulationException() : base("Unrecognized CDR encapsulation ID.") { }
    public CdrInvalidEncapsulationException(string message) : base(message) { }
    public CdrInvalidEncapsulationException(string message, Exception innerException) : base(message, innerException) { }
}
