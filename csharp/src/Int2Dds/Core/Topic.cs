using System;
using System.Reflection;
using System.Text;
using Int2Dds.Cdr;
using Int2Dds.Exceptions;
using Int2Dds.Interop;
using Int2Dds.Listeners;
using Int2Dds.Qos;
using Int2Dds.Types;

namespace Int2Dds.Core
{
    /// <summary>
    /// Topic - associates a name with a data type for publish/subscribe.
    ///
    /// Topics are created through DomainParticipant.CreateTopic.
    /// </summary>
    /// <typeparam name="T">The DDS data type.</typeparam>
    public sealed class Topic<T> : IDisposable where T : class, IDdsType, new()
    {
        private readonly IntPtr _handle;
        private readonly string _name;
        private readonly string _typeName;
        private bool _disposed;

        /// <summary>
        /// Creates a new Topic. Normally called via DomainParticipant.CreateTopic.
        /// </summary>
        internal Topic(DomainParticipant participant, string topicName, TopicQos? qos = null)
        {
            _name = topicName;
            var attr = typeof(T).GetCustomAttribute<DdsTypeAttribute>();
            _typeName = attr?.TypeName ?? typeof(T).Name;
            var extensibility = attr?.Extensibility ?? NativeMethods.int2dds_default_extensibility();
            var hasKey = attr?.HasKey ?? false;

            // Create Topic QoS if provided
            IntPtr qosHandle = IntPtr.Zero;
            if (qos != null)
            {
                ReturnCodeHelper.CheckReturn(NativeMethods.int2dds_topic_qos_create_default(out qosHandle));
                try
                {
                    ApplyTopicQos(qosHandle, qos);
                }
                catch
                {
                    NativeMethods.int2dds_topic_qos_destroy(qosHandle);
                    throw;
                }
            }

            try
            {
                // If the generator emitted flat-type advertisement metadata, build a conformant
                // TypeObject and advertise it during discovery (matching the Rust derive so
                // strict XTypes peers can structurally match). Otherwise fall back to the
                // name-based keyed path.
                var adFieldsInfo = typeof(T).GetField(
                    "DdsTypeInfoFields", BindingFlags.Public | BindingFlags.Static);
                var adFields = adFieldsInfo?.GetValue(null) as DdsTypeInfoField[];

                if (adFields != null && adFields.Length > 0)
                {
                    IntPtr typeInfo = BuildTypeInfoForType(typeof(T));
                    try
                    {
                        unsafe
                        {
                            var topicNameBytes = Encoding.UTF8.GetBytes(topicName + '\0');
                            fixed (byte* pTopicName = topicNameBytes)
                            {
                                ReturnCodeHelper.CheckReturn(
                                    NativeMethods.int2dds_create_topic_with_type_info(
                                        participant.Handle, pTopicName, typeInfo, qosHandle, out _handle));
                            }
                        }
                    }
                    finally
                    {
                        NativeMethods.int2dds_type_info_destroy(typeInfo);
                    }
                }
                else
                {
                    // A keyed topic needs field metadata to advertise a TypeObject for a
                    // spec-conformant KeyHash; the name-only keyed path was removed (#334).
                    if (hasKey)
                        throw new NotSupportedException(
                            $"Keyed topic '{topicName}' (type '{_typeName}') needs DdsTypeInfoFields " +
                            "metadata to advertise a TypeObject for a spec-conformant KeyHash (#334).");

                    unsafe
                    {
                        var topicNameBytes = Encoding.UTF8.GetBytes(topicName + '\0');
                        var typeNameBytes = Encoding.UTF8.GetBytes(_typeName + '\0');

                        fixed (byte* pTopicName = topicNameBytes)
                        fixed (byte* pTypeName = typeNameBytes)
                        {
                            ReturnCodeHelper.CheckReturn(
                                NativeMethods.int2dds_create_topic(
                                    participant.Handle,
                                    pTopicName,
                                    pTypeName,
                                    extensibility,
                                    qosHandle,
                                    out _handle));
                        }
                    }
                }
            }
            finally
            {
                if (qosHandle != IntPtr.Zero)
                    NativeMethods.int2dds_topic_qos_destroy(qosHandle);
            }
        }

        /// <summary>
        /// Creates a new Topic using a QoS profile path.
        /// Normally called via DomainParticipant.CreateTopicWithProfile.
        /// </summary>
        internal Topic(DomainParticipant participant, string topicName, string qosPath)
        {
            _name = topicName;
            var attr = typeof(T).GetCustomAttribute<DdsTypeAttribute>();
            _typeName = attr?.TypeName ?? typeof(T).Name;
            var extensibility = attr?.Extensibility ?? NativeMethods.int2dds_default_extensibility();
            var hasKey = attr?.HasKey ?? false;

            // Keyed topics need a full TypeObject, which the profile path cannot build (#334).
            if (hasKey)
                throw new NotSupportedException(
                    $"Keyed topic '{topicName}' cannot be created from a QoS profile; keyed topics " +
                    "need field metadata for a spec-conformant KeyHash (#334).");

            unsafe
            {
                var topicNameBytes = Encoding.UTF8.GetBytes(topicName + '\0');
                var typeNameBytes = Encoding.UTF8.GetBytes(_typeName + '\0');
                var qosPathBytes = Encoding.UTF8.GetBytes(qosPath + '\0');

                fixed (byte* pTopicName = topicNameBytes)
                fixed (byte* pTypeName = typeNameBytes)
                fixed (byte* pQos = qosPathBytes)
                {
                    ReturnCodeHelper.CheckReturn(
                        NativeMethods.int2dds_create_topic_with_profile(
                            participant.Handle,
                            pTopicName,
                            pTypeName,
                            (int)extensibility,
                            pQos,
                            out _handle));
                }
            }
        }

        /// <summary>
        /// Wraps a native handle returned by <c>find_topic</c> without re-creating
        /// the topic. The native topic already carries its type registration; this
        /// only attaches the managed type <typeparamref name="T"/> for serialization.
        /// </summary>
        internal Topic(DomainParticipant participant, IntPtr foundHandle, string topicName)
        {
            _name = topicName;
            var attr = typeof(T).GetCustomAttribute<DdsTypeAttribute>();
            _typeName = attr?.TypeName ?? typeof(T).Name;
            _handle = foundHandle;
        }

        /// <summary>
        /// Builds a generated type's Int2DdsTypeInfo so a conformant TypeObject can be
        /// advertised during discovery. A struct reflects its <c>DdsTypeInfoFields</c> and
        /// <c>DdsTypeAttribute</c>; a union (<c>DdsUnionAttribute</c>) and a bitset
        /// (<c>DdsBitsetAttribute</c>) replay the same field array onto their own builders;
        /// enums and bitmasks (<c>DdsBitmaskAttribute</c>) are built by reflection. The
        /// returned handle is owned by the caller and must be freed with
        /// int2dds_type_info_destroy.
        /// </summary>
        private static unsafe IntPtr BuildTypeInfoForType(Type type)
        {
            if (type.IsEnum)
            {
                var bitmask = type.GetCustomAttribute<DdsBitmaskAttribute>();
                return bitmask != null ? BuildBitmaskTypeInfo(type, bitmask) : BuildEnumTypeInfo(type);
            }

            var attr = type.GetCustomAttribute<DdsTypeAttribute>();
            string name = attr?.TypeName ?? type.Name;
            int ext = attr?.Extensibility ?? NativeMethods.int2dds_default_extensibility();
            var fieldsInfo = type.GetField(
                "DdsTypeInfoFields", BindingFlags.Public | BindingFlags.Static);
            var fields = fieldsInfo?.GetValue(null) as DdsTypeInfoField[]
                ?? new DdsTypeInfoField[0];

            var nameBytes = Encoding.UTF8.GetBytes(name + '\0');
            IntPtr ti;
            fixed (byte* pName = nameBytes)
            {
                var union = type.GetCustomAttribute<DdsUnionAttribute>();
                if (union != null)
                    ReturnCodeHelper.CheckReturn(
                        NativeMethods.int2dds_type_info_create_union(pName, ext, union.DiscriminatorType, out ti));
                else if (type.GetCustomAttribute<DdsBitsetAttribute>() != null)
                    ReturnCodeHelper.CheckReturn(
                        NativeMethods.int2dds_type_info_create_bitset(pName, out ti));
                else
                    ReturnCodeHelper.CheckReturn(
                        NativeMethods.int2dds_type_info_create(pName, ext, out ti));
            }

            try
            {
                AddTypeInfoFields(ti, fields);
            }
            catch
            {
                NativeMethods.int2dds_type_info_destroy(ti);
                throw;
            }

            return ti;
        }

        /// <summary>
        /// Replays generator-emitted field descriptors onto the builder <paramref name="ti"/>:
        /// scalar/string/collection members by their kind, nested named types by recursively
        /// building their own type_info (referenced by content-hash), maps by key/value kind,
        /// bitset <c>bitfield</c> entries and union <c>label</c> entries.
        /// </summary>
        private static unsafe void AddTypeInfoFields(IntPtr ti, DdsTypeInfoField[] fields)
        {
            foreach (var f in fields)
            {
                var fieldBytes = Encoding.UTF8.GetBytes(f.Name + '\0');
                fixed (byte* pField = fieldBytes)
                {
                    int rc;
                    switch (f.Op)
                    {
                        case "field":
                            rc = NativeMethods.int2dds_type_info_add_field(ti, pField, f.TypeConst, f.Flags);
                            break;
                        case "string":
                            rc = NativeMethods.int2dds_type_info_add_string_field(ti, pField, f.Size, f.Flags);
                            break;
                        case "wstring":
                            rc = NativeMethods.int2dds_type_info_add_wstring_field(ti, pField, f.Size, f.Flags);
                            break;
                        case "seq":
                            rc = NativeMethods.int2dds_type_info_add_sequence_field(ti, pField, f.TypeConst, f.Size, f.Flags);
                            break;
                        case "arr":
                            rc = NativeMethods.int2dds_type_info_add_array_field(ti, pField, f.TypeConst, f.Size, f.Flags);
                            break;
                        case "map":
                            rc = NativeMethods.int2dds_type_info_add_map_field(
                                ti, pField, f.KeyType, f.KeyBound, f.TypeConst, f.ValueBound, f.Size, f.Flags);
                            break;
                        case "bitfield":
                            rc = NativeMethods.int2dds_type_info_add_bitfield(ti, pField, (byte)f.Size, f.TypeConst);
                            break;
                        case "label":
                            rc = NativeMethods.int2dds_type_info_add_union_label(ti, pField, f.TypeConst);
                            break;
                        case "nested":
                        case "seq_nested":
                        case "arr_nested":
                        case "map_nested":
                            if (f.NestedType == null)
                                throw new InvalidOperationException(
                                    $"Nested type_info field '{f.Name}' has no NestedType.");
                            IntPtr nestedTi = BuildTypeInfoForType(f.NestedType);
                            try
                            {
                                switch (f.Op)
                                {
                                    case "seq_nested":
                                        rc = NativeMethods.int2dds_type_info_add_sequence_of_nested_field(ti, pField, nestedTi, f.Size, f.Flags);
                                        break;
                                    case "arr_nested":
                                        rc = NativeMethods.int2dds_type_info_add_array_of_nested_field(ti, pField, nestedTi, f.Size, f.Flags);
                                        break;
                                    case "map_nested":
                                        rc = NativeMethods.int2dds_type_info_add_map_of_nested_field(
                                            ti, pField, f.KeyType, f.KeyBound, nestedTi, f.Size, f.Flags);
                                        break;
                                    default:
                                        rc = NativeMethods.int2dds_type_info_add_nested_field(ti, pField, nestedTi, f.Flags);
                                        break;
                                }
                            }
                            finally
                            {
                                NativeMethods.int2dds_type_info_destroy(nestedTi);
                            }
                            break;
                        default:
                            rc = 0;
                            break;
                    }
                    ReturnCodeHelper.CheckReturn(rc);
                }
            }
        }

        /// <summary>
        /// Builds a bitmask's Int2DdsTypeInfo from its <c>[Flags]</c> enum: the IDL
        /// <c>@bit_bound</c> comes from <see cref="DdsBitmaskAttribute"/>, each member (read in
        /// declaration order, PascalCase like the Rust derive) becomes a flag at the bit position
        /// of its value. The returned handle is owned by the caller and must be freed with
        /// int2dds_type_info_destroy.
        /// </summary>
        private static unsafe IntPtr BuildBitmaskTypeInfo(Type enumType, DdsBitmaskAttribute bitmask)
        {
            var nameBytes = Encoding.UTF8.GetBytes(bitmask.TypeName + '\0');
            IntPtr ti;
            fixed (byte* pName = nameBytes)
            {
                ReturnCodeHelper.CheckReturn(
                    NativeMethods.int2dds_type_info_create_bitmask(pName, (ushort)bitmask.BitBound, out ti));
            }
            try
            {
                foreach (var field in enumType.GetFields(BindingFlags.Public | BindingFlags.Static))
                {
                    ulong value = Convert.ToUInt64(field.GetRawConstantValue());
                    ushort position = 0;
                    while (position < 64 && ((value >> position) & 1UL) == 0)
                        position++;
                    var flagBytes = Encoding.UTF8.GetBytes(field.Name + '\0');
                    fixed (byte* pFlag = flagBytes)
                    {
                        ReturnCodeHelper.CheckReturn(
                            NativeMethods.int2dds_type_info_add_bitmask_flag(ti, pFlag, position));
                    }
                }
            }
            catch
            {
                NativeMethods.int2dds_type_info_destroy(ti);
                throw;
            }
            return ti;
        }

        /// <summary>
        /// Builds an enum's Int2DdsTypeInfo from its CLR metadata. Literals are read via
        /// <c>Type.GetFields</c> so they stay in IDL declaration order (unlike
        /// <c>Enum.GetNames</c>, which sorts by value and would reorder a non-ascending enum,
        /// breaking byte-parity with the Rust derive). Member names are PascalCase (matching the
        /// derive variant names); IDL enums are i32, so bit_bound is 32 and no literal is @default.
        /// </summary>
        private static unsafe IntPtr BuildEnumTypeInfo(Type enumType)
        {
            var attr = enumType.GetCustomAttribute<DdsTypeAttribute>();
            string name = attr?.TypeName ?? enumType.Name;
            var nameBytes = Encoding.UTF8.GetBytes(name + '\0');
            IntPtr ti;
            fixed (byte* pName = nameBytes)
            {
                ReturnCodeHelper.CheckReturn(
                    NativeMethods.int2dds_type_info_create_enum(pName, 32, out ti));
            }
            try
            {
                // Static value fields, in declaration order (excludes the special __value field).
                foreach (var field in enumType.GetFields(BindingFlags.Public | BindingFlags.Static))
                {
                    int value = Convert.ToInt32(field.GetRawConstantValue());
                    var litBytes = Encoding.UTF8.GetBytes(field.Name + '\0');
                    fixed (byte* pLit = litBytes)
                    {
                        ReturnCodeHelper.CheckReturn(
                            NativeMethods.int2dds_type_info_add_enum_literal(ti, pLit, value, 0));
                    }
                }
            }
            catch
            {
                NativeMethods.int2dds_type_info_destroy(ti);
                throw;
            }
            return ti;
        }

        /// <summary>
        /// Gets the native handle. For internal use by other Core types.
        /// </summary>
        internal IntPtr Handle => _handle;

        /// <summary>
        /// Gets the StatusCondition associated with this topic.
        /// </summary>
        public Int2Dds.Conditions.StatusCondition GetStatusCondition()
        {
            if (_disposed) throw new ObjectDisposedException(GetType().Name);
            ReturnCodeHelper.CheckReturn(
                NativeMethods.int2dds_topic_get_statuscondition(_handle, out var conditionHandle));
            return new Int2Dds.Conditions.StatusCondition(conditionHandle);
        }

        /// <summary>
        /// Gets the current status change bitmask of this topic.
        /// </summary>
        public uint GetStatusChanges()
        {
            if (_disposed) throw new ObjectDisposedException(GetType().Name);
            ReturnCodeHelper.CheckReturn(
                NativeMethods.int2dds_topic_get_status_changes(_handle, out var mask));
            return mask;
        }

        /// <summary>
        /// Gets the topic name.
        /// </summary>
        public string Name => _name;

        /// <summary>
        /// Gets the DDS type name.
        /// </summary>
        public string TypeName => _typeName;

        /// <summary>
        /// Gets the CLR type associated with this topic.
        /// </summary>
        public Type TypeClass => typeof(T);

        /// <summary>
        /// Gets the inconsistent topic status for this Topic. Reports how many times a
        /// remote topic with the same name but an incompatible type was discovered.
        /// Reading the status resets its <c>TotalCountChange</c>.
        /// </summary>
        public InconsistentTopicStatus GetInconsistentTopicStatus()
        {
            if (_disposed) throw new ObjectDisposedException(GetType().Name);

            unsafe
            {
                NativeInconsistentTopicStatus native;
                ReturnCodeHelper.CheckReturn(
                    NativeMethods.int2dds_topic_get_inconsistent_topic_status(_handle, &native));
                return new InconsistentTopicStatus(native.TotalCount, native.TotalCountChange);
            }
        }

        /// <summary>
        /// Sets new QoS policies on this Topic.
        /// Some policies can only be changed before the entity is enabled.
        /// </summary>
        /// <param name="qos">The new QoS policies to apply.</param>
        public void SetQos(TopicQos qos)
        {
            if (_disposed) throw new ObjectDisposedException(GetType().Name);

            // Get current QoS as base, then apply user overrides on top
            ReturnCodeHelper.CheckReturn(NativeMethods.int2dds_topic_get_qos(_handle, out var qosHandle));
            try
            {
                ApplyTopicQos(qosHandle, qos);
                ReturnCodeHelper.CheckReturn(NativeMethods.int2dds_topic_set_qos(_handle, qosHandle));
            }
            finally
            {
                NativeMethods.int2dds_topic_qos_destroy(qosHandle);
            }
        }

        private static void ApplyTopicQos(IntPtr qosHandle, TopicQos qos)
        {
            if (qos.Reliability != null)
                ReturnCodeHelper.CheckReturn(NativeMethods.int2dds_topic_qos_set_reliability(
                    qosHandle, (int)qos.Reliability.Kind, qos.Reliability.MaxBlockingTimeNs));

            if (qos.Durability != null)
                ReturnCodeHelper.CheckReturn(NativeMethods.int2dds_topic_qos_set_durability(
                    qosHandle, (int)qos.Durability.Kind));

            if (qos.History != null)
                ReturnCodeHelper.CheckReturn(NativeMethods.int2dds_topic_qos_set_history(
                    qosHandle, (int)qos.History.Kind, qos.History.Depth));

            if (qos.Deadline != null)
                ReturnCodeHelper.CheckReturn(NativeMethods.int2dds_topic_qos_set_deadline(
                    qosHandle, qos.Deadline.PeriodNs));

            if (qos.Liveliness != null)
                ReturnCodeHelper.CheckReturn(NativeMethods.int2dds_topic_qos_set_liveliness(
                    qosHandle, (int)qos.Liveliness.Kind, qos.Liveliness.LeaseDurationNs));

            if (qos.DestinationOrder != null)
                ReturnCodeHelper.CheckReturn(NativeMethods.int2dds_topic_qos_set_destination_order(
                    qosHandle, (int)qos.DestinationOrder.Kind));

            if (qos.ResourceLimits != null)
                ReturnCodeHelper.CheckReturn(NativeMethods.int2dds_topic_qos_set_resource_limits(
                    qosHandle, qos.ResourceLimits.MaxSamples, qos.ResourceLimits.MaxInstances,
                    qos.ResourceLimits.MaxSamplesPerInstance));

            if (qos.TransportPriority != null)
                ReturnCodeHelper.CheckReturn(NativeMethods.int2dds_topic_qos_set_transport_priority(
                    qosHandle, qos.TransportPriority.Value));

            if (qos.Lifespan != null)
                ReturnCodeHelper.CheckReturn(NativeMethods.int2dds_topic_qos_set_lifespan(
                    qosHandle, qos.Lifespan.DurationNs));

            if (qos.Ownership != null)
                ReturnCodeHelper.CheckReturn(NativeMethods.int2dds_topic_qos_set_ownership(
                    qosHandle, (int)qos.Ownership.Kind));

            if (qos.DataRepresentation != null)
                ReturnCodeHelper.CheckReturn(NativeMethods.int2dds_topic_qos_set_data_representation(
                    qosHandle, (int)qos.DataRepresentation.Kind));
        }

        /// <summary>
        /// Releases all resources used by the Topic.
        /// </summary>
        public void Dispose()
        {
            if (_disposed) return;
            _disposed = true;
            GC.SuppressFinalize(this);
            NativeMethods.int2dds_delete_topic(_handle);
        }

        ~Topic()
        {
            if (!_disposed)
            {
                try { Dispose(); }
                catch { /* suppress errors during finalization */ }
            }
        }
    }
}
