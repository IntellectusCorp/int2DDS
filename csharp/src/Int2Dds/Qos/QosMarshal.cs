using System;
using System.Linq;
using System.Runtime.InteropServices;
using System.Text;
using Int2Dds.Exceptions;
using Int2Dds.Interop;

namespace Int2Dds.Qos
{
    /// <summary>
    /// Conversion between the managed QoS objects and a native QoS handle.
    ///
    /// These live outside <c>DataWriter&lt;T&gt;</c> / <c>DataReader&lt;T&gt;</c> on purpose:
    /// the dynamic endpoints (<c>DynamicTypeWriter</c> / <c>DynamicTypeReader</c>) are not
    /// generic and could not otherwise reach them, and static members of a generic class
    /// are emitted once per closed type. One copy, used by both paths.
    /// </summary>
    internal static class QosMarshal
    {
        /// <summary>Write the non-null policies of <paramref name="qos"/> onto a native writer QoS handle.</summary>
        internal static void ApplyWriterQos(IntPtr qosHandle, DataWriterQos qos)
        {
            if (qos.Reliability != null)
                ReturnCodeHelper.CheckReturn(NativeMethods.int2dds_datawriter_qos_set_reliability(
                    qosHandle, (int)qos.Reliability.Kind, qos.Reliability.MaxBlockingTimeNs));

            if (qos.Durability != null)
                ReturnCodeHelper.CheckReturn(NativeMethods.int2dds_datawriter_qos_set_durability(
                    qosHandle, (int)qos.Durability.Kind));

            if (qos.History != null)
                ReturnCodeHelper.CheckReturn(NativeMethods.int2dds_datawriter_qos_set_history(
                    qosHandle, (int)qos.History.Kind, qos.History.Depth));

            if (qos.Ownership != null)
                ReturnCodeHelper.CheckReturn(NativeMethods.int2dds_datawriter_qos_set_ownership(
                    qosHandle, (int)qos.Ownership.Kind));

            if (qos.OwnershipStrength != null)
                ReturnCodeHelper.CheckReturn(NativeMethods.int2dds_datawriter_qos_set_ownership_strength(
                    qosHandle, qos.OwnershipStrength.Value));

            if (qos.ResourceLimits != null)
                ReturnCodeHelper.CheckReturn(NativeMethods.int2dds_datawriter_qos_set_resource_limits(
                    qosHandle, qos.ResourceLimits.MaxSamples, qos.ResourceLimits.MaxInstances,
                    qos.ResourceLimits.MaxSamplesPerInstance));

            if (qos.Lifespan != null)
                ReturnCodeHelper.CheckReturn(NativeMethods.int2dds_datawriter_qos_set_lifespan(
                    qosHandle, qos.Lifespan.DurationNs));

            if (qos.DestinationOrder != null)
                ReturnCodeHelper.CheckReturn(NativeMethods.int2dds_datawriter_qos_set_destination_order(
                    qosHandle, (int)qos.DestinationOrder.Kind));

            if (qos.LatencyBudget != null)
                ReturnCodeHelper.CheckReturn(NativeMethods.int2dds_datawriter_qos_set_latency_budget(
                    qosHandle, qos.LatencyBudget.DurationNs));

            if (qos.TransportPriority != null)
                ReturnCodeHelper.CheckReturn(NativeMethods.int2dds_datawriter_qos_set_transport_priority(
                    qosHandle, qos.TransportPriority.Value));

            if (qos.UserData != null && qos.UserData.Data != null)
            {
                unsafe
                {
                    fixed (byte* pData = qos.UserData.Data)
                    {
                        ReturnCodeHelper.CheckReturn(NativeMethods.int2dds_datawriter_qos_set_user_data(
                            qosHandle, pData, (UIntPtr)qos.UserData.Data.Length));
                    }
                }
            }

            if (qos.WriterDataLifecycle != null)
                ReturnCodeHelper.CheckReturn(NativeMethods.int2dds_datawriter_qos_set_writer_data_lifecycle(
                    qosHandle, qos.WriterDataLifecycle.AutodisposeUnregisteredInstances));

            if (qos.DataRepresentation != null)
                ReturnCodeHelper.CheckReturn(NativeMethods.int2dds_datawriter_qos_set_data_representation(
                    qosHandle, (int)qos.DataRepresentation.Kind));

            if (qos.Deadline != null)
                ReturnCodeHelper.CheckReturn(NativeMethods.int2dds_datawriter_qos_set_deadline(
                    qosHandle, qos.Deadline.PeriodNs));

            if (qos.Liveliness != null)
                ReturnCodeHelper.CheckReturn(NativeMethods.int2dds_datawriter_qos_set_liveliness(
                    qosHandle, (int)qos.Liveliness.Kind, qos.Liveliness.LeaseDurationNs));

            if (qos.DataFrag != null)
                ReturnCodeHelper.CheckReturn(NativeMethods.int2dds_datawriter_qos_set_data_frag(
                    qosHandle, qos.DataFrag.Value));
        }

        /// <summary>Read every policy off a native writer QoS handle.</summary>
        internal static unsafe DataWriterQos ReadWriterQos(IntPtr h)
        {
            NativeMethods.int2dds_datawriter_qos_get_reliability(h, out var relKind, out var relTime);
            NativeMethods.int2dds_datawriter_qos_get_durability(h, out var durKind);
            NativeMethods.int2dds_datawriter_qos_get_history(h, out var histKind, out var histDepth);
            NativeMethods.int2dds_datawriter_qos_get_ownership(h, out var ownKind);
            NativeMethods.int2dds_datawriter_qos_get_ownership_strength(h, out var ownStr);
            NativeMethods.int2dds_datawriter_qos_get_resource_limits(h, out var maxS, out var maxI, out var maxPI);
            NativeMethods.int2dds_datawriter_qos_get_lifespan(h, out var lifespanNs);
            NativeMethods.int2dds_datawriter_qos_get_destination_order(h, out var destKind);
            NativeMethods.int2dds_datawriter_qos_get_deadline(h, out var deadlineNs);
            NativeMethods.int2dds_datawriter_qos_get_liveliness(h, out var liveKind, out var liveNs);
            NativeMethods.int2dds_datawriter_qos_get_data_representation(h, out var reprKind);
            NativeMethods.int2dds_datawriter_qos_get_transport_priority(h, out var transPri);
            NativeMethods.int2dds_datawriter_qos_get_latency_budget(h, out var latNs);
            NativeMethods.int2dds_datawriter_qos_get_writer_data_lifecycle(h, out var autoDispose);
            NativeMethods.int2dds_datawriter_qos_get_data_frag(h, out var dataFrag);

            return new DataWriterQos
            {
                Reliability = new Reliability((ReliabilityKind)relKind, TimeSpan.FromTicks(relTime / 100)),
                Durability = new Durability((DurabilityKind)durKind),
                History = new History((HistoryKind)histKind, histDepth),
                Ownership = new Ownership((OwnershipKind)ownKind),
                OwnershipStrength = new OwnershipStrength(ownStr),
                ResourceLimits = new ResourceLimits(maxS, maxI, maxPI),
                Lifespan = new Lifespan(TimeSpan.FromTicks(lifespanNs / 100)),
                DestinationOrder = new DestinationOrder((DestinationOrderKind)destKind),
                Deadline = new Deadline(TimeSpan.FromTicks(deadlineNs / 100)),
                Liveliness = new Liveliness((LivelinessKind)liveKind, TimeSpan.FromTicks(liveNs / 100)),
                DataRepresentation = new DataRepresentation((DataRepresentationKind)reprKind),
                TransportPriority = new TransportPriority(transPri),
                LatencyBudget = new LatencyBudget(TimeSpan.FromTicks(latNs / 100)),
                WriterDataLifecycle = new WriterDataLifecycle(autoDispose),
                DataFrag = dataFrag,
                UserData = new UserData(ReadBytes(h, NativeMethods.int2dds_datawriter_qos_get_user_data)),
            };
        }

        /// <summary>Write the non-null policies of <paramref name="qos"/> onto a native reader QoS handle.</summary>
        internal static void ApplyReaderQos(IntPtr qosHandle, DataReaderQos qos)
        {
            if (qos.Reliability != null)
                ReturnCodeHelper.CheckReturn(NativeMethods.int2dds_datareader_qos_set_reliability(
                    qosHandle, (int)qos.Reliability.Kind, qos.Reliability.MaxBlockingTimeNs));

            if (qos.Durability != null)
                ReturnCodeHelper.CheckReturn(NativeMethods.int2dds_datareader_qos_set_durability(
                    qosHandle, (int)qos.Durability.Kind));

            if (qos.History != null)
                ReturnCodeHelper.CheckReturn(NativeMethods.int2dds_datareader_qos_set_history(
                    qosHandle, (int)qos.History.Kind, qos.History.Depth));

            if (qos.Ownership != null)
                ReturnCodeHelper.CheckReturn(NativeMethods.int2dds_datareader_qos_set_ownership(
                    qosHandle, (int)qos.Ownership.Kind));

            if (qos.ResourceLimits != null)
                ReturnCodeHelper.CheckReturn(NativeMethods.int2dds_datareader_qos_set_resource_limits(
                    qosHandle, qos.ResourceLimits.MaxSamples, qos.ResourceLimits.MaxInstances,
                    qos.ResourceLimits.MaxSamplesPerInstance));

            if (qos.DestinationOrder != null)
                ReturnCodeHelper.CheckReturn(NativeMethods.int2dds_datareader_qos_set_destination_order(
                    qosHandle, (int)qos.DestinationOrder.Kind));

            if (qos.LifespanReference != null)
                ReturnCodeHelper.CheckReturn(NativeMethods.int2dds_datareader_qos_set_lifespan_reference(
                    qosHandle, (int)qos.LifespanReference.Kind));

            if (qos.TimeBasedFilter != null)
                ReturnCodeHelper.CheckReturn(NativeMethods.int2dds_datareader_qos_set_time_based_filter(
                    qosHandle, qos.TimeBasedFilter.MinimumSeparationNs));

            if (qos.LatencyBudget != null)
                ReturnCodeHelper.CheckReturn(NativeMethods.int2dds_datareader_qos_set_latency_budget(
                    qosHandle, qos.LatencyBudget.DurationNs));

            if (qos.UserData != null && qos.UserData.Data != null)
            {
                unsafe
                {
                    fixed (byte* pData = qos.UserData.Data)
                    {
                        ReturnCodeHelper.CheckReturn(NativeMethods.int2dds_datareader_qos_set_user_data(
                            qosHandle, pData, (UIntPtr)qos.UserData.Data.Length));
                    }
                }
            }

            if (qos.ReaderDataLifecycle != null)
                ReturnCodeHelper.CheckReturn(NativeMethods.int2dds_datareader_qos_set_reader_data_lifecycle(
                    qosHandle, qos.ReaderDataLifecycle.AutopurgeNowriterNs,
                    qos.ReaderDataLifecycle.AutopurgeDisposedNs));

            if (qos.DataRepresentation != null)
                ReturnCodeHelper.CheckReturn(NativeMethods.int2dds_datareader_qos_set_data_representation(
                    qosHandle, (int)qos.DataRepresentation.Kind));

            if (qos.Deadline != null)
                ReturnCodeHelper.CheckReturn(NativeMethods.int2dds_datareader_qos_set_deadline(
                    qosHandle, qos.Deadline.PeriodNs));

            if (qos.Liveliness != null)
                ReturnCodeHelper.CheckReturn(NativeMethods.int2dds_datareader_qos_set_liveliness(
                    qosHandle, (int)qos.Liveliness.Kind, qos.Liveliness.LeaseDurationNs));
        }

        /// <summary>Read every policy off a native reader QoS handle.</summary>
        internal static unsafe DataReaderQos ReadReaderQos(IntPtr h)
        {
            NativeMethods.int2dds_datareader_qos_get_reliability(h, out var relKind, out var relTime);
            NativeMethods.int2dds_datareader_qos_get_durability(h, out var durKind);
            NativeMethods.int2dds_datareader_qos_get_history(h, out var histKind, out var histDepth);
            NativeMethods.int2dds_datareader_qos_get_ownership(h, out var ownKind);
            NativeMethods.int2dds_datareader_qos_get_resource_limits(h, out var maxS, out var maxI, out var maxPI);
            NativeMethods.int2dds_datareader_qos_get_destination_order(h, out var destKind);
            NativeMethods.int2dds_datareader_qos_get_lifespan_reference(h, out var lifespanRefKind);
            NativeMethods.int2dds_datareader_qos_get_deadline(h, out var deadlineNs);
            NativeMethods.int2dds_datareader_qos_get_liveliness(h, out var liveKind, out var liveNs);
            NativeMethods.int2dds_datareader_qos_get_data_representation(h, out var reprKind);
            NativeMethods.int2dds_datareader_qos_get_latency_budget(h, out var latNs);
            NativeMethods.int2dds_datareader_qos_get_time_based_filter(h, out var tbfNs);
            NativeMethods.int2dds_datareader_qos_get_reader_data_lifecycle(h, out var purgeNowriterNs, out var purgeDisposedNs);

            return new DataReaderQos
            {
                Reliability = new Reliability((ReliabilityKind)relKind, TimeSpan.FromTicks(relTime / 100)),
                Durability = new Durability((DurabilityKind)durKind),
                History = new History((HistoryKind)histKind, histDepth),
                Ownership = new Ownership((OwnershipKind)ownKind),
                ResourceLimits = new ResourceLimits(maxS, maxI, maxPI),
                DestinationOrder = new DestinationOrder((DestinationOrderKind)destKind),
                LifespanReference = new LifespanReference((LifespanReferenceKind)lifespanRefKind),
                Deadline = new Deadline(TimeSpan.FromTicks(deadlineNs / 100)),
                Liveliness = new Liveliness((LivelinessKind)liveKind, TimeSpan.FromTicks(liveNs / 100)),
                DataRepresentation = new DataRepresentation((DataRepresentationKind)reprKind),
                LatencyBudget = new LatencyBudget(TimeSpan.FromTicks(latNs / 100)),
                TimeBasedFilter = new TimeBasedFilter(TimeSpan.FromTicks(tbfNs / 100)),
                ReaderDataLifecycle = new ReaderDataLifecycle(
                    TimeSpan.FromTicks(purgeNowriterNs / 100),
                    TimeSpan.FromTicks(purgeDisposedNs / 100)),
                UserData = new UserData(ReadBytes(h, NativeMethods.int2dds_datareader_qos_get_user_data)),
            };
        }

        /// <summary>True when <paramref name="qos"/> carries a policy that must reach a native publisher QoS handle.</summary>
        internal static bool HasPublisherPolicies(PublisherQos qos) =>
            qos.Partition is { Names: { Length: var len } } && len > 0 || qos.Presentation != null
            || qos.GroupData is { Data: { Length: var dataLen } } && dataLen > 0;

        /// <summary>Write the non-null policies of <paramref name="qos"/> onto a native publisher QoS handle.</summary>
        internal static unsafe void ApplyPublisherQos(IntPtr qosHandle, PublisherQos qos)
        {
            if (qos.Partition is { Names: { Length: var len } } partition && len > 0)
                ApplyPartition(qosHandle, partition, NativeMethods.int2dds_publisher_qos_set_partition);

            if (qos.Presentation != null)
                ReturnCodeHelper.CheckReturn(NativeMethods.int2dds_publisher_qos_set_presentation(
                    qosHandle, (int)qos.Presentation.AccessScope, qos.Presentation.CoherentAccess,
                    qos.Presentation.OrderedAccess));

            if (qos.GroupData != null && qos.GroupData.Data != null)
            {
                fixed (byte* pData = qos.GroupData.Data)
                {
                    ReturnCodeHelper.CheckReturn(NativeMethods.int2dds_publisher_qos_set_group_data(
                        qosHandle, pData, (UIntPtr)qos.GroupData.Data.Length));
                }
            }
        }

        /// <summary>Read the policies that have a native getter off a publisher QoS handle. Partition is left null.</summary>
        internal static unsafe PublisherQos ReadPublisherQos(IntPtr h)
        {
            NativeMethods.int2dds_publisher_qos_get_presentation(h, out var scope, out var coherent, out var ordered);

            return new PublisherQos
            {
                Presentation = new Presentation((PresentationAccessScopeKind)scope, coherent, ordered),
                GroupData = new GroupData(ReadBytes(h, NativeMethods.int2dds_publisher_qos_get_group_data)),
            };
        }

        /// <summary>True when <paramref name="qos"/> carries a policy that must reach a native subscriber QoS handle.</summary>
        internal static bool HasSubscriberPolicies(SubscriberQos qos) =>
            qos.Partition is { Names: { Length: var len } } && len > 0 || qos.Presentation != null
            || qos.GroupData is { Data: { Length: var dataLen } } && dataLen > 0;

        /// <summary>Write the non-null policies of <paramref name="qos"/> onto a native subscriber QoS handle.</summary>
        internal static unsafe void ApplySubscriberQos(IntPtr qosHandle, SubscriberQos qos)
        {
            if (qos.Partition is { Names: { Length: var len } } partition && len > 0)
                ApplyPartition(qosHandle, partition, NativeMethods.int2dds_subscriber_qos_set_partition);

            if (qos.Presentation != null)
                ReturnCodeHelper.CheckReturn(NativeMethods.int2dds_subscriber_qos_set_presentation(
                    qosHandle, (int)qos.Presentation.AccessScope, qos.Presentation.CoherentAccess,
                    qos.Presentation.OrderedAccess));

            if (qos.GroupData != null && qos.GroupData.Data != null)
            {
                fixed (byte* pData = qos.GroupData.Data)
                {
                    ReturnCodeHelper.CheckReturn(NativeMethods.int2dds_subscriber_qos_set_group_data(
                        qosHandle, pData, (UIntPtr)qos.GroupData.Data.Length));
                }
            }
        }

        /// <summary>Read the policies that have a native getter off a subscriber QoS handle. Partition is left null.</summary>
        internal static unsafe SubscriberQos ReadSubscriberQos(IntPtr h)
        {
            NativeMethods.int2dds_subscriber_qos_get_presentation(h, out var scope, out var coherent, out var ordered);

            return new SubscriberQos
            {
                Presentation = new Presentation((PresentationAccessScopeKind)scope, coherent, ordered),
                GroupData = new GroupData(ReadBytes(h, NativeMethods.int2dds_subscriber_qos_get_group_data)),
            };
        }

        internal unsafe delegate int GetBytesFn(IntPtr qos, byte* buf, UIntPtr capacity, out UIntPtr sizeOut);

        /// <summary>Read a byte sequence policy: query the size with a null buffer, then copy.</summary>
        internal static unsafe byte[] ReadBytes(IntPtr qosHandle, GetBytesFn getBytes)
        {
            ReturnCodeHelper.CheckReturn(getBytes(qosHandle, null, UIntPtr.Zero, out var size));
            int len = (int)(uint)size;
            if (len == 0) return new byte[0];

            var buf = new byte[len];
            fixed (byte* p = buf)
            {
                ReturnCodeHelper.CheckReturn(getBytes(qosHandle, p, (UIntPtr)len, out size));
            }
            return buf;
        }

        private unsafe delegate int SetPartitionFn(IntPtr qos, byte** partitions, UIntPtr partitionCount);

        private static unsafe void ApplyPartition(IntPtr qosHandle, Partition partition, SetPartitionFn setPartition)
        {
            var partitionByteArrays = partition.Names
                .Select(n => Encoding.UTF8.GetBytes(n + '\0'))
                .ToArray();
            var pinnedArrays = new GCHandle[partitionByteArrays.Length];
            for (int i = 0; i < partitionByteArrays.Length; i++)
                pinnedArrays[i] = GCHandle.Alloc(partitionByteArrays[i], GCHandleType.Pinned);

            try
            {
                var ptrs = new byte*[partitionByteArrays.Length];
                for (int i = 0; i < ptrs.Length; i++)
                    ptrs[i] = (byte*)pinnedArrays[i].AddrOfPinnedObject();
                fixed (byte** pPartitions = ptrs)
                {
                    ReturnCodeHelper.CheckReturn(setPartition(qosHandle, pPartitions, (UIntPtr)partition.Names.Length));
                }
            }
            finally
            {
                foreach (var pin in pinnedArrays)
                    pin.Free();
            }
        }
    }
}
