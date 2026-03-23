using System.Collections.Concurrent;
using System.Runtime.CompilerServices;
using System.Runtime.InteropServices;
using Int2Dds.Interop;

namespace Int2Dds.Listeners;

/// <summary>
/// Manages GCHandle-based callback tracking for native listener structs.
/// Maps managed listener objects to native function pointers so that
/// unmanaged C callbacks can recover the managed listener and invoke methods.
/// </summary>
internal static unsafe class ListenerRegistry
{
    /// <summary>
    /// Holds the managed listener reference and a GCHandle to prevent GC collection.
    /// </summary>
    private sealed class ListenerContext
    {
        public object Listener { get; }
        public GCHandle GcHandle { get; }

        public ListenerContext(object listener)
        {
            Listener = listener;
            GcHandle = GCHandle.Alloc(this);
        }

        public nint ContextHandle => GCHandle.ToIntPtr(GcHandle);

        public void Free()
        {
            if (GcHandle.IsAllocated)
                GcHandle.Free();
        }
    }

    private static readonly ConcurrentDictionary<nint, ListenerContext> s_contexts = new();

    // ---------------------------------------------------------------
    // Writer listener callbacks
    // ---------------------------------------------------------------

    [UnmanagedCallersOnly(CallConvs = [typeof(CallConvCdecl)])]
    private static void OnPublicationMatchedCallback(nint writerHandle, NativePublicationMatchedStatus* statusPtr, nint userContext)
    {
        try
        {
            if (!s_contexts.TryGetValue(userContext, out var ctx)) return;
            var listener = (IDataWriterListener)ctx.Listener;
            var status = new PublicationMatchedStatus(
                statusPtr->TotalCount, statusPtr->TotalCountChange,
                statusPtr->CurrentCount, statusPtr->CurrentCountChange);
            listener.OnPublicationMatched(writerHandle, status);
        }
        catch
        {
            // Exceptions must not propagate across the native boundary.
        }
    }

    [UnmanagedCallersOnly(CallConvs = [typeof(CallConvCdecl)])]
    private static void OnOfferedDeadlineMissedCallback(nint writerHandle, NativeOfferedDeadlineMissedStatus* statusPtr, nint userContext)
    {
        try
        {
            if (!s_contexts.TryGetValue(userContext, out var ctx)) return;
            var listener = (IDataWriterListener)ctx.Listener;
            var status = new OfferedDeadlineMissedStatus(
                statusPtr->TotalCount, statusPtr->TotalCountChange);
            listener.OnOfferedDeadlineMissed(writerHandle, status);
        }
        catch
        {
            // Exceptions must not propagate across the native boundary.
        }
    }

    [UnmanagedCallersOnly(CallConvs = [typeof(CallConvCdecl)])]
    private static void OnOfferedIncompatibleQosCallback(nint writerHandle, NativeOfferedIncompatibleQosStatus* statusPtr, nint userContext)
    {
        try
        {
            if (!s_contexts.TryGetValue(userContext, out var ctx)) return;
            var listener = (IDataWriterListener)ctx.Listener;
            var status = new OfferedIncompatibleQosStatus(
                statusPtr->TotalCount, statusPtr->TotalCountChange,
                (int)statusPtr->LastPolicyId);
            listener.OnOfferedIncompatibleQos(writerHandle, status);
        }
        catch
        {
            // Exceptions must not propagate across the native boundary.
        }
    }

    [UnmanagedCallersOnly(CallConvs = [typeof(CallConvCdecl)])]
    private static void OnLivelinessLostCallback(nint writerHandle, NativeLivelinessLostStatus* statusPtr, nint userContext)
    {
        try
        {
            if (!s_contexts.TryGetValue(userContext, out var ctx)) return;
            var listener = (IDataWriterListener)ctx.Listener;
            var status = new LivelinessLostStatus(
                statusPtr->TotalCount, statusPtr->TotalCountChange);
            listener.OnLivelinessLost(writerHandle, status);
        }
        catch
        {
            // Exceptions must not propagate across the native boundary.
        }
    }

    // ---------------------------------------------------------------
    // Reader listener callbacks
    // ---------------------------------------------------------------

    [UnmanagedCallersOnly(CallConvs = [typeof(CallConvCdecl)])]
    private static void OnDataAvailableCallback(nint readerHandle, nint userContext)
    {
        try
        {
            if (!s_contexts.TryGetValue(userContext, out var ctx)) return;
            var listener = (IDataReaderListener)ctx.Listener;
            listener.OnDataAvailable(readerHandle);
        }
        catch
        {
            // Exceptions must not propagate across the native boundary.
        }
    }

    [UnmanagedCallersOnly(CallConvs = [typeof(CallConvCdecl)])]
    private static void OnSubscriptionMatchedCallback(nint readerHandle, NativeSubscriptionMatchedStatus* statusPtr, nint userContext)
    {
        try
        {
            if (!s_contexts.TryGetValue(userContext, out var ctx)) return;
            var listener = (IDataReaderListener)ctx.Listener;
            var status = new SubscriptionMatchedStatus(
                statusPtr->TotalCount, statusPtr->TotalCountChange,
                statusPtr->CurrentCount, statusPtr->CurrentCountChange);
            listener.OnSubscriptionMatched(readerHandle, status);
        }
        catch
        {
            // Exceptions must not propagate across the native boundary.
        }
    }

    [UnmanagedCallersOnly(CallConvs = [typeof(CallConvCdecl)])]
    private static void OnSampleRejectedCallback(nint readerHandle, NativeSampleRejectedStatus* statusPtr, nint userContext)
    {
        try
        {
            if (!s_contexts.TryGetValue(userContext, out var ctx)) return;
            var listener = (IDataReaderListener)ctx.Listener;
            var status = new SampleRejectedStatus(
                statusPtr->TotalCount, statusPtr->TotalCountChange,
                (int)statusPtr->LastReason);
            listener.OnSampleRejected(readerHandle, status);
        }
        catch
        {
            // Exceptions must not propagate across the native boundary.
        }
    }

    [UnmanagedCallersOnly(CallConvs = [typeof(CallConvCdecl)])]
    private static void OnLivelinessChangedCallback(nint readerHandle, NativeLivelinessChangedStatus* statusPtr, nint userContext)
    {
        try
        {
            if (!s_contexts.TryGetValue(userContext, out var ctx)) return;
            var listener = (IDataReaderListener)ctx.Listener;
            var status = new LivelinessChangedStatus(
                statusPtr->AliveCount, statusPtr->NotAliveCount,
                statusPtr->AliveCountChange, statusPtr->NotAliveCountChange);
            listener.OnLivelinessChanged(readerHandle, status);
        }
        catch
        {
            // Exceptions must not propagate across the native boundary.
        }
    }

    [UnmanagedCallersOnly(CallConvs = [typeof(CallConvCdecl)])]
    private static void OnRequestedDeadlineMissedCallback(nint readerHandle, NativeRequestedDeadlineMissedStatus* statusPtr, nint userContext)
    {
        try
        {
            if (!s_contexts.TryGetValue(userContext, out var ctx)) return;
            var listener = (IDataReaderListener)ctx.Listener;
            var status = new RequestedDeadlineMissedStatus(
                statusPtr->TotalCount, statusPtr->TotalCountChange);
            listener.OnRequestedDeadlineMissed(readerHandle, status);
        }
        catch
        {
            // Exceptions must not propagate across the native boundary.
        }
    }

    [UnmanagedCallersOnly(CallConvs = [typeof(CallConvCdecl)])]
    private static void OnRequestedIncompatibleQosCallback(nint readerHandle, NativeRequestedIncompatibleQosStatus* statusPtr, nint userContext)
    {
        try
        {
            if (!s_contexts.TryGetValue(userContext, out var ctx)) return;
            var listener = (IDataReaderListener)ctx.Listener;
            var status = new RequestedIncompatibleQosStatus(
                statusPtr->TotalCount, statusPtr->TotalCountChange,
                (int)statusPtr->LastPolicyId);
            listener.OnRequestedIncompatibleQos(readerHandle, status);
        }
        catch
        {
            // Exceptions must not propagate across the native boundary.
        }
    }

    [UnmanagedCallersOnly(CallConvs = [typeof(CallConvCdecl)])]
    private static void OnSampleLostCallback(nint readerHandle, NativeSampleLostStatus* statusPtr, nint userContext)
    {
        try
        {
            if (!s_contexts.TryGetValue(userContext, out var ctx)) return;
            var listener = (IDataReaderListener)ctx.Listener;
            var status = new SampleLostStatus(
                statusPtr->TotalCount, statusPtr->TotalCountChange);
            listener.OnSampleLost(readerHandle, status);
        }
        catch
        {
            // Exceptions must not propagate across the native boundary.
        }
    }

    // ---------------------------------------------------------------
    // Public API
    // ---------------------------------------------------------------

    /// <summary>
    /// Creates a native DataWriter listener struct backed by a managed <see cref="IDataWriterListener"/>.
    /// </summary>
    /// <returns>
    /// A tuple of the native listener struct and the context handle (to pass to <see cref="FreeListener"/>).
    /// </returns>
    public static (NativeDataWriterListener nativeListener, nint contextHandle) CreateWriterListener(IDataWriterListener listener)
    {
        var ctx = new ListenerContext(listener);
        s_contexts[ctx.ContextHandle] = ctx;

        var native = new NativeDataWriterListener
        {
            OnPublicationMatched = &OnPublicationMatchedCallback,
            OnOfferedDeadlineMissed = &OnOfferedDeadlineMissedCallback,
            OnOfferedIncompatibleQos = &OnOfferedIncompatibleQosCallback,
            OnLivelinessLost = &OnLivelinessLostCallback,
            UserContext = ctx.ContextHandle,
        };

        return (native, ctx.ContextHandle);
    }

    /// <summary>
    /// Creates a native DataReader listener struct backed by a managed <see cref="IDataReaderListener"/>.
    /// </summary>
    /// <returns>
    /// A tuple of the native listener struct and the context handle (to pass to <see cref="FreeListener"/>).
    /// </returns>
    public static (NativeDataReaderListener nativeListener, nint contextHandle) CreateReaderListener(IDataReaderListener listener)
    {
        var ctx = new ListenerContext(listener);
        s_contexts[ctx.ContextHandle] = ctx;

        var native = new NativeDataReaderListener
        {
            OnDataAvailable = &OnDataAvailableCallback,
            OnSubscriptionMatched = &OnSubscriptionMatchedCallback,
            OnSampleRejected = &OnSampleRejectedCallback,
            OnLivelinessChanged = &OnLivelinessChangedCallback,
            OnRequestedDeadlineMissed = &OnRequestedDeadlineMissedCallback,
            OnRequestedIncompatibleQos = &OnRequestedIncompatibleQosCallback,
            OnSampleLost = &OnSampleLostCallback,
            UserContext = ctx.ContextHandle,
        };

        return (native, ctx.ContextHandle);
    }

    /// <summary>
    /// Frees the GCHandle and removes the listener context from the registry.
    /// Call this when the DataWriter or DataReader is disposed.
    /// </summary>
    public static void FreeListener(nint contextHandle)
    {
        if (s_contexts.TryRemove(contextHandle, out var ctx))
        {
            ctx.Free();
        }
    }
}
