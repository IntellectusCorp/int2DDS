using System;
using System.Runtime.InteropServices;

namespace Int2Dds.Interop
{
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
        public unsafe fixed byte LastSubscriptionHandle[16];
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
        public unsafe fixed byte LastPublicationHandle[16];
    }

    /// <summary>
    /// C-compatible offered deadline missed status.
    /// </summary>
    [StructLayout(LayoutKind.Sequential)]
    internal struct NativeOfferedDeadlineMissedStatus
    {
        public int TotalCount;
        public int TotalCountChange;
        public unsafe fixed byte LastInstanceHandle[16];
    }

    /// <summary>
    /// C-compatible requested deadline missed status.
    /// </summary>
    [StructLayout(LayoutKind.Sequential)]
    internal struct NativeRequestedDeadlineMissedStatus
    {
        public int TotalCount;
        public int TotalCountChange;
        public unsafe fixed byte LastInstanceHandle[16];
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
        public unsafe fixed byte LastPublicationHandle[16];
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
        public unsafe fixed byte LastInstanceHandle[16];
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
        public unsafe fixed byte InstanceHandle[16];
        public unsafe fixed byte PublicationHandle[16];
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
    internal struct NativeDataWriterListener
    {
        public IntPtr OnPublicationMatched;
        public IntPtr OnOfferedDeadlineMissed;
        public IntPtr OnOfferedIncompatibleQos;
        public IntPtr OnLivelinessLost;
        public IntPtr UserContext;
    }

    /// <summary>
    /// C-compatible DataReader listener struct with function pointers.
    /// Matches Int2DdsDataReaderListener in int2dds-ffi.h.
    /// </summary>
    [StructLayout(LayoutKind.Sequential)]
    internal struct NativeDataReaderListener
    {
        public IntPtr OnDataAvailable;
        public IntPtr OnSubscriptionMatched;
        public IntPtr OnSampleRejected;
        public IntPtr OnLivelinessChanged;
        public IntPtr OnRequestedDeadlineMissed;
        public IntPtr OnRequestedIncompatibleQos;
        public IntPtr OnSampleLost;
        public IntPtr UserContext;
    }
}
