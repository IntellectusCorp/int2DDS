// Copyright IntellectusCorp. All rights reserved.
// Licensed under the int2DDS license.
//
// Minimal ValueTuple polyfill for .NET Framework 4.5.
//
// .NET Framework 4.5 / 4.6 do not ship System.ValueTuple in mscorlib.
// Microsoft's normal answer is the System.ValueTuple NuGet, but our net45
// build is committed to zero NuGet runtime dependencies. We ship a minimal
// in-assembly polyfill instead. The C# compiler resolves the tuple syntax
// `(T1, T2)` and `(T1 a, T2 b)` to `System.ValueTuple<T1, T2>`, so the
// types must live in the `System` namespace with the exact names below.
//
// `[TupleElementNamesAttribute]` is required when the tuple has named
// elements (e.g. `(uint MemberId, ...)`).
//
// Modern TFMs (netstandard2.0+, .NET 6+, etc.) ship these natively, so
// the entire file is conditionally compiled out on those targets.

#if NET45

namespace System
{
    public struct ValueTuple<T1>
    {
        public T1 Item1;

        public ValueTuple(T1 item1)
        {
            Item1 = item1;
        }
    }

    public struct ValueTuple<T1, T2>
    {
        public T1 Item1;
        public T2 Item2;

        public ValueTuple(T1 item1, T2 item2)
        {
            Item1 = item1;
            Item2 = item2;
        }
    }

    public struct ValueTuple<T1, T2, T3>
    {
        public T1 Item1;
        public T2 Item2;
        public T3 Item3;

        public ValueTuple(T1 item1, T2 item2, T3 item3)
        {
            Item1 = item1;
            Item2 = item2;
            Item3 = item3;
        }
    }

    public struct ValueTuple<T1, T2, T3, T4>
    {
        public T1 Item1;
        public T2 Item2;
        public T3 Item3;
        public T4 Item4;

        public ValueTuple(T1 item1, T2 item2, T3 item3, T4 item4)
        {
            Item1 = item1;
            Item2 = item2;
            Item3 = item3;
            Item4 = item4;
        }
    }

    public struct ValueTuple<T1, T2, T3, T4, T5>
    {
        public T1 Item1;
        public T2 Item2;
        public T3 Item3;
        public T4 Item4;
        public T5 Item5;

        public ValueTuple(T1 item1, T2 item2, T3 item3, T4 item4, T5 item5)
        {
            Item1 = item1;
            Item2 = item2;
            Item3 = item3;
            Item4 = item4;
            Item5 = item5;
        }
    }

    public struct ValueTuple<T1, T2, T3, T4, T5, T6>
    {
        public T1 Item1;
        public T2 Item2;
        public T3 Item3;
        public T4 Item4;
        public T5 Item5;
        public T6 Item6;

        public ValueTuple(T1 item1, T2 item2, T3 item3, T4 item4, T5 item5, T6 item6)
        {
            Item1 = item1;
            Item2 = item2;
            Item3 = item3;
            Item4 = item4;
            Item5 = item5;
            Item6 = item6;
        }
    }

    public struct ValueTuple<T1, T2, T3, T4, T5, T6, T7>
    {
        public T1 Item1;
        public T2 Item2;
        public T3 Item3;
        public T4 Item4;
        public T5 Item5;
        public T6 Item6;
        public T7 Item7;

        public ValueTuple(T1 item1, T2 item2, T3 item3, T4 item4, T5 item5, T6 item6, T7 item7)
        {
            Item1 = item1;
            Item2 = item2;
            Item3 = item3;
            Item4 = item4;
            Item5 = item5;
            Item6 = item6;
            Item7 = item7;
        }
    }
}

namespace System.Runtime.CompilerServices
{
    [System.AttributeUsage(
        System.AttributeTargets.Field
        | System.AttributeTargets.Parameter
        | System.AttributeTargets.Property
        | System.AttributeTargets.ReturnValue
        | System.AttributeTargets.Class
        | System.AttributeTargets.Struct
        | System.AttributeTargets.Event,
        AllowMultiple = false,
        Inherited = false)]
    public sealed class TupleElementNamesAttribute : System.Attribute
    {
        private readonly string[] _transformNames;

        public TupleElementNamesAttribute(string[] transformNames)
        {
            _transformNames = transformNames ?? throw new System.ArgumentNullException(nameof(transformNames));
        }

        public System.Collections.Generic.IList<string> TransformNames => _transformNames;
    }
}

#endif
