using System;
using System.Collections.Concurrent;
using System.Runtime.InteropServices;
using Int2Dds.Interop;

namespace Int2Dds.Listeners
{
    // Delegate types matching the native callback signatures
    [UnmanagedFunctionPointer(CallingConvention.Cdecl)]
    internal unsafe delegate void PublicationMatchedCallback(IntPtr writerHandle, NativePublicationMatchedStatus* statusPtr, IntPtr userContext);

    [UnmanagedFunctionPointer(CallingConvention.Cdecl)]
    internal unsafe delegate void OfferedDeadlineMissedCallback(IntPtr writerHandle, NativeOfferedDeadlineMissedStatus* statusPtr, IntPtr userContext);

    [UnmanagedFunctionPointer(CallingConvention.Cdecl)]
    internal unsafe delegate void OfferedIncompatibleQosCallback(IntPtr writerHandle, NativeOfferedIncompatibleQosStatus* statusPtr, IntPtr userContext);

    [UnmanagedFunctionPointer(CallingConvention.Cdecl)]
    internal unsafe delegate void LivelinessLostCallback(IntPtr writerHandle, NativeLivelinessLostStatus* statusPtr, IntPtr userContext);

    [UnmanagedFunctionPointer(CallingConvention.Cdecl)]
    internal delegate void DataAvailableCallback(IntPtr readerHandle, IntPtr userContext);

    [UnmanagedFunctionPointer(CallingConvention.Cdecl)]
    internal unsafe delegate void SubscriptionMatchedCallback(IntPtr readerHandle, NativeSubscriptionMatchedStatus* statusPtr, IntPtr userContext);

    [UnmanagedFunctionPointer(CallingConvention.Cdecl)]
    internal unsafe delegate void SampleRejectedCallback(IntPtr readerHandle, NativeSampleRejectedStatus* statusPtr, IntPtr userContext);

    [UnmanagedFunctionPointer(CallingConvention.Cdecl)]
    internal unsafe delegate void LivelinessChangedCallback(IntPtr readerHandle, NativeLivelinessChangedStatus* statusPtr, IntPtr userContext);

    [UnmanagedFunctionPointer(CallingConvention.Cdecl)]
    internal unsafe delegate void RequestedDeadlineMissedCallback(IntPtr readerHandle, NativeRequestedDeadlineMissedStatus* statusPtr, IntPtr userContext);

    [UnmanagedFunctionPointer(CallingConvention.Cdecl)]
    internal unsafe delegate void RequestedIncompatibleQosCallback(IntPtr readerHandle, NativeRequestedIncompatibleQosStatus* statusPtr, IntPtr userContext);

    [UnmanagedFunctionPointer(CallingConvention.Cdecl)]
    internal unsafe delegate void SampleLostCallback(IntPtr readerHandle, NativeSampleLostStatus* statusPtr, IntPtr userContext);

    /// <summary>
    /// Manages GCHandle-based callback tracking for native listener structs.
    /// Maps managed listener objects to native function pointers so that
    /// unmanaged C callbacks can recover the managed listener and invoke methods.
    /// </summary>
    internal static unsafe class ListenerRegistry
    {
        /// <summary>
        /// Holds the managed listener reference, entity reference, a GCHandle to prevent GC collection,
        /// and the delegate instances that must be kept alive.
        /// </summary>
        private sealed class ListenerContext
        {
            public object Listener { get; }
            public object Entity { get; }
            public GCHandle GcHandle { get; }
            public Delegate[] DelegateRefs { get; set; } = Array.Empty<Delegate>();

            public ListenerContext(object listener, object entity)
            {
                Listener = listener;
                Entity = entity;
                GcHandle = GCHandle.Alloc(this);
            }

            public IntPtr ContextHandle => GCHandle.ToIntPtr(GcHandle);

            public void Free()
            {
                if (GcHandle.IsAllocated)
                    GcHandle.Free();
            }
        }

        private static readonly ConcurrentDictionary<IntPtr, ListenerContext> s_contexts =
            new ConcurrentDictionary<IntPtr, ListenerContext>();

        // ---------------------------------------------------------------
        // Writer listener callbacks
        // ---------------------------------------------------------------

        private static void OnPublicationMatchedCallback(IntPtr writerHandle, NativePublicationMatchedStatus* statusPtr, IntPtr userContext)
        {
            try
            {
                if (!s_contexts.TryGetValue(userContext, out var ctx)) return;
                var listener = (IDataWriterListener)ctx.Listener;
                var status = new PublicationMatchedStatus(
                    statusPtr->TotalCount, statusPtr->TotalCountChange,
                    statusPtr->CurrentCount, statusPtr->CurrentCountChange);
                listener.OnPublicationMatched(ctx.Entity, status);
            }
            catch
            {
                // Exceptions must not propagate across the native boundary.
            }
        }

        private static void OnOfferedDeadlineMissedCallback(IntPtr writerHandle, NativeOfferedDeadlineMissedStatus* statusPtr, IntPtr userContext)
        {
            try
            {
                if (!s_contexts.TryGetValue(userContext, out var ctx)) return;
                var listener = (IDataWriterListener)ctx.Listener;
                var status = new OfferedDeadlineMissedStatus(
                    statusPtr->TotalCount, statusPtr->TotalCountChange);
                listener.OnOfferedDeadlineMissed(ctx.Entity, status);
            }
            catch
            {
                // Exceptions must not propagate across the native boundary.
            }
        }

        private static void OnOfferedIncompatibleQosCallback(IntPtr writerHandle, NativeOfferedIncompatibleQosStatus* statusPtr, IntPtr userContext)
        {
            try
            {
                if (!s_contexts.TryGetValue(userContext, out var ctx)) return;
                var listener = (IDataWriterListener)ctx.Listener;
                var status = new OfferedIncompatibleQosStatus(
                    statusPtr->TotalCount, statusPtr->TotalCountChange,
                    (int)statusPtr->LastPolicyId);
                listener.OnOfferedIncompatibleQos(ctx.Entity, status);
            }
            catch
            {
                // Exceptions must not propagate across the native boundary.
            }
        }

        private static void OnLivelinessLostCallback(IntPtr writerHandle, NativeLivelinessLostStatus* statusPtr, IntPtr userContext)
        {
            try
            {
                if (!s_contexts.TryGetValue(userContext, out var ctx)) return;
                var listener = (IDataWriterListener)ctx.Listener;
                var status = new LivelinessLostStatus(
                    statusPtr->TotalCount, statusPtr->TotalCountChange);
                listener.OnLivelinessLost(ctx.Entity, status);
            }
            catch
            {
                // Exceptions must not propagate across the native boundary.
            }
        }

        // ---------------------------------------------------------------
        // Reader listener callbacks
        // ---------------------------------------------------------------

        private static void OnDataAvailableCallback(IntPtr readerHandle, IntPtr userContext)
        {
            try
            {
                if (!s_contexts.TryGetValue(userContext, out var ctx)) return;
                var listener = (IDataReaderListener)ctx.Listener;
                listener.OnDataAvailable(ctx.Entity);
            }
            catch
            {
                // Exceptions must not propagate across the native boundary.
            }
        }

        private static void OnSubscriptionMatchedCallback(IntPtr readerHandle, NativeSubscriptionMatchedStatus* statusPtr, IntPtr userContext)
        {
            try
            {
                if (!s_contexts.TryGetValue(userContext, out var ctx)) return;
                var listener = (IDataReaderListener)ctx.Listener;
                var status = new SubscriptionMatchedStatus(
                    statusPtr->TotalCount, statusPtr->TotalCountChange,
                    statusPtr->CurrentCount, statusPtr->CurrentCountChange);
                listener.OnSubscriptionMatched(ctx.Entity, status);
            }
            catch
            {
                // Exceptions must not propagate across the native boundary.
            }
        }

        private static void OnSampleRejectedCallback(IntPtr readerHandle, NativeSampleRejectedStatus* statusPtr, IntPtr userContext)
        {
            try
            {
                if (!s_contexts.TryGetValue(userContext, out var ctx)) return;
                var listener = (IDataReaderListener)ctx.Listener;
                var status = new SampleRejectedStatus(
                    statusPtr->TotalCount, statusPtr->TotalCountChange,
                    (int)statusPtr->LastReason);
                listener.OnSampleRejected(ctx.Entity, status);
            }
            catch
            {
                // Exceptions must not propagate across the native boundary.
            }
        }

        private static void OnLivelinessChangedCallback(IntPtr readerHandle, NativeLivelinessChangedStatus* statusPtr, IntPtr userContext)
        {
            try
            {
                if (!s_contexts.TryGetValue(userContext, out var ctx)) return;
                var listener = (IDataReaderListener)ctx.Listener;
                var status = new LivelinessChangedStatus(
                    statusPtr->AliveCount, statusPtr->NotAliveCount,
                    statusPtr->AliveCountChange, statusPtr->NotAliveCountChange);
                listener.OnLivelinessChanged(ctx.Entity, status);
            }
            catch
            {
                // Exceptions must not propagate across the native boundary.
            }
        }

        private static void OnRequestedDeadlineMissedCallback(IntPtr readerHandle, NativeRequestedDeadlineMissedStatus* statusPtr, IntPtr userContext)
        {
            try
            {
                if (!s_contexts.TryGetValue(userContext, out var ctx)) return;
                var listener = (IDataReaderListener)ctx.Listener;
                var status = new RequestedDeadlineMissedStatus(
                    statusPtr->TotalCount, statusPtr->TotalCountChange);
                listener.OnRequestedDeadlineMissed(ctx.Entity, status);
            }
            catch
            {
                // Exceptions must not propagate across the native boundary.
            }
        }

        private static void OnRequestedIncompatibleQosCallback(IntPtr readerHandle, NativeRequestedIncompatibleQosStatus* statusPtr, IntPtr userContext)
        {
            try
            {
                if (!s_contexts.TryGetValue(userContext, out var ctx)) return;
                var listener = (IDataReaderListener)ctx.Listener;
                var status = new RequestedIncompatibleQosStatus(
                    statusPtr->TotalCount, statusPtr->TotalCountChange,
                    (int)statusPtr->LastPolicyId);
                listener.OnRequestedIncompatibleQos(ctx.Entity, status);
            }
            catch
            {
                // Exceptions must not propagate across the native boundary.
            }
        }

        private static void OnSampleLostCallback(IntPtr readerHandle, NativeSampleLostStatus* statusPtr, IntPtr userContext)
        {
            try
            {
                if (!s_contexts.TryGetValue(userContext, out var ctx)) return;
                var listener = (IDataReaderListener)ctx.Listener;
                var status = new SampleLostStatus(
                    statusPtr->TotalCount, statusPtr->TotalCountChange);
                listener.OnSampleLost(ctx.Entity, status);
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
        /// <param name="listener">The managed listener implementation.</param>
        /// <param name="entity">The DataWriter entity to pass to callbacks.</param>
        /// <returns>
        /// A tuple of the native listener struct and the context handle (to pass to <see cref="FreeListener"/>).
        /// </returns>
        public static (NativeDataWriterListener nativeListener, IntPtr contextHandle) CreateWriterListener(IDataWriterListener listener, object entity)
        {
            var ctx = new ListenerContext(listener, entity);
            s_contexts[ctx.ContextHandle] = ctx;

            var d1 = new PublicationMatchedCallback(OnPublicationMatchedCallback);
            var d2 = new OfferedDeadlineMissedCallback(OnOfferedDeadlineMissedCallback);
            var d3 = new OfferedIncompatibleQosCallback(OnOfferedIncompatibleQosCallback);
            var d4 = new LivelinessLostCallback(OnLivelinessLostCallback);

            // Keep delegate instances alive to prevent GC collection
            ctx.DelegateRefs = new Delegate[] { d1, d2, d3, d4 };

            var native = new NativeDataWriterListener
            {
                OnPublicationMatched = Marshal.GetFunctionPointerForDelegate(d1),
                OnOfferedDeadlineMissed = Marshal.GetFunctionPointerForDelegate(d2),
                OnOfferedIncompatibleQos = Marshal.GetFunctionPointerForDelegate(d3),
                OnLivelinessLost = Marshal.GetFunctionPointerForDelegate(d4),
                UserContext = ctx.ContextHandle,
            };

            return (native, ctx.ContextHandle);
        }

        /// <summary>
        /// Creates a native DataReader listener struct backed by a managed <see cref="IDataReaderListener"/>.
        /// </summary>
        /// <param name="listener">The managed listener implementation.</param>
        /// <param name="entity">The DataReader entity to pass to callbacks.</param>
        /// <returns>
        /// A tuple of the native listener struct and the context handle (to pass to <see cref="FreeListener"/>).
        /// </returns>
        public static (NativeDataReaderListener nativeListener, IntPtr contextHandle) CreateReaderListener(IDataReaderListener listener, object entity)
        {
            var ctx = new ListenerContext(listener, entity);
            s_contexts[ctx.ContextHandle] = ctx;

            var d1 = new DataAvailableCallback(OnDataAvailableCallback);
            var d2 = new SubscriptionMatchedCallback(OnSubscriptionMatchedCallback);
            var d3 = new SampleRejectedCallback(OnSampleRejectedCallback);
            var d4 = new LivelinessChangedCallback(OnLivelinessChangedCallback);
            var d5 = new RequestedDeadlineMissedCallback(OnRequestedDeadlineMissedCallback);
            var d6 = new RequestedIncompatibleQosCallback(OnRequestedIncompatibleQosCallback);
            var d7 = new SampleLostCallback(OnSampleLostCallback);

            // Keep delegate instances alive to prevent GC collection
            ctx.DelegateRefs = new Delegate[] { d1, d2, d3, d4, d5, d6, d7 };

            var native = new NativeDataReaderListener
            {
                OnDataAvailable = Marshal.GetFunctionPointerForDelegate(d1),
                OnSubscriptionMatched = Marshal.GetFunctionPointerForDelegate(d2),
                OnSampleRejected = Marshal.GetFunctionPointerForDelegate(d3),
                OnLivelinessChanged = Marshal.GetFunctionPointerForDelegate(d4),
                OnRequestedDeadlineMissed = Marshal.GetFunctionPointerForDelegate(d5),
                OnRequestedIncompatibleQos = Marshal.GetFunctionPointerForDelegate(d6),
                OnSampleLost = Marshal.GetFunctionPointerForDelegate(d7),
                UserContext = ctx.ContextHandle,
            };

            return (native, ctx.ContextHandle);
        }

        /// <summary>
        /// Frees the GCHandle and removes the listener context from the registry.
        /// Call this when the DataWriter or DataReader is disposed.
        /// </summary>
        public static void FreeListener(IntPtr contextHandle)
        {
            if (s_contexts.TryRemove(contextHandle, out var ctx))
            {
                ctx.Free();
            }
        }
    }
}
