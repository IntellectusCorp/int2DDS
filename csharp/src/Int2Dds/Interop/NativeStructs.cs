using System.Runtime.CompilerServices;
using System.Runtime.InteropServices;

namespace Int2Dds.Interop;

/// <summary>
/// Fixed-size 16-byte inline array for instance handles.
/// </summary>
[InlineArray(16)]
internal struct Handle16
{
    private byte _element;
}

/// <summary>
/// Fixed-size 12-byte inline array for builtin topic data keys.
/// </summary>
[InlineArray(12)]
internal struct Key12
{
    private byte _element;
}

/// <summary>
/// C-compatible publication matched status.
/// Matches Int2DdsPublicationMatchedStatus in int2dds-ffi.h.
/// </summary>
[StructLayout(LayoutKind.Sequential)]
internal struct NativePublicationMatchedStatus
{
    public int TotalCount;
    public int TotalCountChange;
    public int CurrentCount;
    public int CurrentCountChange;
    public Handle16 LastSubscriptionHandle;
}

/// <summary>
/// C-compatible subscription matched status.
/// Matches Int2DdsSubscriptionMatchedStatus in int2dds-ffi.h.
/// </summary>
[StructLayout(LayoutKind.Sequential)]
internal struct NativeSubscriptionMatchedStatus
{
    public int TotalCount;
    public int TotalCountChange;
    public int CurrentCount;
    public int CurrentCountChange;
    public Handle16 LastPublicationHandle;
}

/// <summary>
/// C-compatible offered deadline missed status.
/// </summary>
[StructLayout(LayoutKind.Sequential)]
internal struct NativeOfferedDeadlineMissedStatus
{
    public int TotalCount;
    public int TotalCountChange;
    public Handle16 LastInstanceHandle;
}

/// <summary>
/// C-compatible requested deadline missed status.
/// </summary>
[StructLayout(LayoutKind.Sequential)]
internal struct NativeRequestedDeadlineMissedStatus
{
    public int TotalCount;
    public int TotalCountChange;
    public Handle16 LastInstanceHandle;
}

/// <summary>
/// C-compatible offered incompatible QoS status.
/// </summary>
[StructLayout(LayoutKind.Sequential)]
internal struct NativeOfferedIncompatibleQosStatus
{
    public int TotalCount;
    public int TotalCountChange;
    public QosPolicyId LastPolicyId;
    public uint PoliciesCount;
}

/// <summary>
/// C-compatible requested incompatible QoS status.
/// </summary>
[StructLayout(LayoutKind.Sequential)]
internal struct NativeRequestedIncompatibleQosStatus
{
    public int TotalCount;
    public int TotalCountChange;
    public QosPolicyId LastPolicyId;
    public uint PoliciesCount;
}

/// <summary>
/// C-compatible liveliness lost status.
/// </summary>
[StructLayout(LayoutKind.Sequential)]
internal struct NativeLivelinessLostStatus
{
    public int TotalCount;
    public int TotalCountChange;
}

/// <summary>
/// C-compatible liveliness changed status.
/// </summary>
[StructLayout(LayoutKind.Sequential)]
internal struct NativeLivelinessChangedStatus
{
    public int AliveCount;
    public int NotAliveCount;
    public int AliveCountChange;
    public int NotAliveCountChange;
    public Handle16 LastPublicationHandle;
}

/// <summary>
/// C-compatible sample lost status.
/// </summary>
[StructLayout(LayoutKind.Sequential)]
internal struct NativeSampleLostStatus
{
    public int TotalCount;
    public int TotalCountChange;
}

/// <summary>
/// C-compatible sample rejected status.
/// </summary>
[StructLayout(LayoutKind.Sequential)]
internal struct NativeSampleRejectedStatus
{
    public int TotalCount;
    public int TotalCountChange;
    public SampleRejectedStatusKind LastReason;
    public Handle16 LastInstanceHandle;
}

/// <summary>
/// FFI-safe SampleInfo returned to C callers.
/// Matches Int2DdsSampleInfo in int2dds-ffi.h.
/// </summary>
[StructLayout(LayoutKind.Sequential)]
internal struct NativeSampleInfo
{
    public int SourceTimestampSec;
    public uint SourceTimestampNanosec;
    public uint SampleState;
    public uint ViewState;
    public uint InstanceState;
    public Handle16 InstanceHandle;
    public Handle16 PublicationHandle;
    public int DisposedGenerationCount;
    public int NoWritersGenerationCount;
    public int SampleRank;
    public int GenerationRank;
    public int AbsoluteGenerationRank;
    [MarshalAs(UnmanagedType.U1)]
    public bool ValidData;
}

/// <summary>
/// C-compatible DataWriter listener struct with function pointers.
/// Matches Int2DdsDataWriterListener in int2dds-ffi.h.
/// </summary>
[StructLayout(LayoutKind.Sequential)]
internal unsafe struct NativeDataWriterListener
{
    public delegate* unmanaged[Cdecl]<nint, NativePublicationMatchedStatus*, nint, void> OnPublicationMatched;
    public delegate* unmanaged[Cdecl]<nint, NativeOfferedDeadlineMissedStatus*, nint, void> OnOfferedDeadlineMissed;
    public delegate* unmanaged[Cdecl]<nint, NativeOfferedIncompatibleQosStatus*, nint, void> OnOfferedIncompatibleQos;
    public delegate* unmanaged[Cdecl]<nint, NativeLivelinessLostStatus*, nint, void> OnLivelinessLost;
    public nint UserContext;
}

/// <summary>
/// C-compatible DataReader listener struct with function pointers.
/// Matches Int2DdsDataReaderListener in int2dds-ffi.h.
/// </summary>
[StructLayout(LayoutKind.Sequential)]
internal unsafe struct NativeDataReaderListener
{
    public delegate* unmanaged[Cdecl]<nint, nint, void> OnDataAvailable;
    public delegate* unmanaged[Cdecl]<nint, NativeSubscriptionMatchedStatus*, nint, void> OnSubscriptionMatched;
    public delegate* unmanaged[Cdecl]<nint, NativeSampleRejectedStatus*, nint, void> OnSampleRejected;
    public delegate* unmanaged[Cdecl]<nint, NativeLivelinessChangedStatus*, nint, void> OnLivelinessChanged;
    public delegate* unmanaged[Cdecl]<nint, NativeRequestedDeadlineMissedStatus*, nint, void> OnRequestedDeadlineMissed;
    public delegate* unmanaged[Cdecl]<nint, NativeRequestedIncompatibleQosStatus*, nint, void> OnRequestedIncompatibleQos;
    public delegate* unmanaged[Cdecl]<nint, NativeSampleLostStatus*, nint, void> OnSampleLost;
    public nint UserContext;
}
