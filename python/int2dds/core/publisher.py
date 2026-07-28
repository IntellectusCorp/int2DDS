"""
Publisher and DataWriter for publishing DDS data.
"""

from __future__ import annotations

from typing import TYPE_CHECKING, Generic, TypeVar

from int2dds._ffi import CData, ffi, lib
from int2dds.core.conditions import StatusCondition
from int2dds.core.listeners import (
    DataWriterListener,
    _create_writer_listener_struct,
    _remove_listener,
    STATUS_MASK_ALL,
)
from int2dds.exceptions import check_ret

if TYPE_CHECKING:
    from int2dds.core.participant import DomainParticipant
    from int2dds.core.qos import DataWriterQos
    from int2dds.core.topic import Topic
    from int2dds.types.base import DdsType

T = TypeVar("T", bound="DdsType")


def _apply_datawriter_qos(handle: CData, qos: "DataWriterQos") -> None:
    """Apply DataWriterQos policies onto a native writer QoS handle.

    Shared by writer creation and set_qos. Every policy is optional; only the ones
    explicitly set on `qos` are applied, so unset policies retain the base handle's
    value (the native default at creation, or the current QoS on set_qos merge).
    """
    if qos.reliability is not None:
        check_ret(lib.int2dds_datawriter_qos_set_reliability(
            handle, qos.reliability._kind_int, qos.reliability._max_blocking_time_ns))
    if qos.durability is not None:
        check_ret(lib.int2dds_datawriter_qos_set_durability(handle, qos.durability._kind_int))
    if qos.history is not None:
        check_ret(lib.int2dds_datawriter_qos_set_history(
            handle, qos.history._kind_int, qos.history.depth))
    if qos.ownership is not None:
        check_ret(lib.int2dds_datawriter_qos_set_ownership(handle, qos.ownership._kind_int))
    if qos.ownership_strength is not None:
        check_ret(lib.int2dds_datawriter_qos_set_ownership_strength(
            handle, qos.ownership_strength.value))
    if qos.resource_limits is not None:
        check_ret(lib.int2dds_datawriter_qos_set_resource_limits(
            handle,
            qos.resource_limits.max_samples,
            qos.resource_limits.max_instances,
            qos.resource_limits.max_samples_per_instance))
    if qos.lifespan is not None:
        check_ret(lib.int2dds_datawriter_qos_set_lifespan(handle, qos.lifespan._duration_ns))
    if qos.destination_order is not None:
        check_ret(lib.int2dds_datawriter_qos_set_destination_order(
            handle, qos.destination_order._kind_int))
    if qos.latency_budget is not None:
        check_ret(lib.int2dds_datawriter_qos_set_latency_budget(
            handle, qos.latency_budget._duration_ns))
    if qos.transport_priority is not None:
        check_ret(lib.int2dds_datawriter_qos_set_transport_priority(
            handle, qos.transport_priority.value))
    if qos.user_data is not None and qos.user_data.data:
        data_ptr = ffi.from_buffer(qos.user_data.data)
        check_ret(lib.int2dds_datawriter_qos_set_user_data(
            handle, data_ptr, len(qos.user_data.data)))
    if qos.writer_data_lifecycle is not None:
        check_ret(lib.int2dds_datawriter_qos_set_writer_data_lifecycle(
            handle, qos.writer_data_lifecycle.autodispose_unregistered_instances))
    if qos.data_representation is not None:
        check_ret(lib.int2dds_datawriter_qos_set_data_representation(
            handle, qos.data_representation._kind_int))
    if qos.deadline is not None:
        check_ret(lib.int2dds_datawriter_qos_set_deadline(handle, qos.deadline._period_ns))
    if qos.liveliness is not None:
        check_ret(lib.int2dds_datawriter_qos_set_liveliness(
            handle, qos.liveliness._kind_int, qos.liveliness._lease_duration_ns))
    if qos.data_frag is not None:
        check_ret(lib.int2dds_datawriter_qos_set_data_frag(handle, qos.data_frag))


class Publisher:
    """
    Publisher - groups DataWriters for coherent publication.

    Publishers are created through DomainParticipant.create_publisher().
    """

    __slots__ = ("_handle", "_participant", "_closed")

    def __init__(self, participant: DomainParticipant, qos: "PublisherQos | None" = None,
                 profile: str | None = None) -> None:
        self._participant = participant
        self._closed = False

        publisher_ptr = ffi.new("Int2DdsPublisher **")
        if profile is not None:
            check_ret(lib.int2dds_create_publisher_with_profile(
                participant._handle, profile.encode(), publisher_ptr))
        elif qos is not None and qos.partition is not None and qos.partition.names:
            qos_handle_ptr = ffi.new("Int2DdsPublisherQos **")
            check_ret(lib.int2dds_publisher_qos_create_default(qos_handle_ptr))
            qos_handle = qos_handle_ptr[0]
            c_strings = [ffi.new("char[]", n.encode()) for n in qos.partition.names]
            c_array = ffi.new("char*[]", c_strings)
            check_ret(lib.int2dds_publisher_qos_set_partition(
                qos_handle, c_array, len(qos.partition.names)))
            check_ret(lib.int2dds_create_publisher_with_qos(
                participant._handle, qos_handle, publisher_ptr))
            lib.int2dds_publisher_qos_destroy(qos_handle)
        else:
            check_ret(lib.int2dds_create_publisher(participant._handle, publisher_ptr))
        self._handle = publisher_ptr[0]

    def create_datawriter(
        self,
        topic: Topic[T],
        qos: DataWriterQos | None = None,
        listener: DataWriterListener | None = None,
        status_mask: int | None = None,
    ) -> DataWriter[T]:
        """
        Create a DataWriter for the given topic.

        Args:
            topic: The topic to write to
            qos: Optional QoS settings
            listener: Optional listener for event callbacks
            status_mask: Bitmask of statuses to listen for

        Returns:
            A new DataWriter instance
        """
        return DataWriter(self, topic, qos, listener, status_mask)

    def create_datawriter_with_profile(self, topic: Topic[T], profile: str) -> DataWriter[T]:
        """Create a DataWriter whose QoS comes from the named XML profile."""
        return DataWriter(self, topic, profile=profile)

    def create_datawriter_dynamic(self, topic, support):
        """Create a DataWriter for a runtime XML/dynamic-typed topic."""
        from int2dds.types.dynamic import create_datawriter_dynamic

        return create_datawriter_dynamic(self, topic, support)

    def delete_contained_entities(self) -> None:
        """Delete all DataWriters created by this publisher."""
        check_ret(lib.int2dds_publisher_delete_contained_entities(self._handle))

    def get_instance_handle(self) -> bytes:
        """Get this publisher's 16-byte instance handle."""
        buf = ffi.new("uint8_t[16]")
        check_ret(lib.int2dds_publisher_get_instance_handle(
            self._handle, ffi.cast("uint8_t(*)[16]", buf)))
        return bytes(ffi.buffer(buf, 16))

    def get_statuscondition(self) -> StatusCondition:
        """Get the StatusCondition associated with this publisher."""
        cond_ptr = ffi.new("Int2DdsStatusCondition **")
        check_ret(lib.int2dds_publisher_get_statuscondition(self._handle, cond_ptr))
        return StatusCondition(cond_ptr[0], owner=self)

    def get_status_changes(self) -> int:
        """Get the current status change bitmask of this publisher."""
        mask_out = ffi.new("uint32_t *")
        check_ret(lib.int2dds_publisher_get_status_changes(self._handle, mask_out))
        return mask_out[0]

    def set_qos(self, qos: "PublisherQos") -> None:
        """Set this publisher's QoS.

        The current QoS is used as the merge base, then the ``partition`` policy
        from ``qos`` (the only mutable PublisherQos policy) is applied on top.
        """
        qos_ptr = ffi.new("Int2DdsPublisherQos **")
        check_ret(lib.int2dds_publisher_get_qos(self._handle, qos_ptr))
        handle = qos_ptr[0]
        try:
            if qos is not None and qos.partition is not None and qos.partition.names:
                c_strings = [ffi.new("char[]", n.encode()) for n in qos.partition.names]
                c_array = ffi.new("char*[]", c_strings)
                check_ret(lib.int2dds_publisher_qos_set_partition(
                    handle, c_array, len(qos.partition.names)))
            check_ret(lib.int2dds_publisher_set_qos(self._handle, handle))
        finally:
            lib.int2dds_publisher_qos_destroy(handle)

    def close(self) -> None:
        """Delete the publisher."""
        if not self._closed and self._handle is not None:
            self.delete_contained_entities()
            check_ret(lib.int2dds_delete_publisher(self._handle))
            self._handle = None
            self._closed = True

    def __enter__(self) -> Publisher:
        return self

    def __exit__(self, exc_type: object, exc_val: object, exc_tb: object) -> None:
        self.close()

    def __del__(self) -> None:
        if not getattr(self, "_closed", True):
            try:
                self.close()
            except Exception:
                pass  # Suppress errors during cleanup


class DataWriter(Generic[T]):
    """
    DataWriter - publishes data samples to a topic.

    DataWriters are created through Publisher.create_datawriter().

    Example:
        >>> writer = publisher.create_datawriter(topic)
        >>> writer.write(MyType(value=42))
    """

    __slots__ = ("_handle", "_publisher", "_topic", "_qos_handle", "_closed", "_listener_ctx_id", "_xcdr2")

    def __init__(
        self,
        publisher: Publisher,
        topic: Topic[T],
        qos: DataWriterQos | None = None,
        listener: DataWriterListener | None = None,
        status_mask: int | None = None,
        profile: str | None = None,
    ) -> None:
        self._publisher = publisher
        self._topic = topic
        self._closed = False
        self._qos_handle: CData | None = None
        self._listener_ctx_id: int | None = None

        # Create QoS if provided
        qos_ptr = ffi.NULL
        if qos is not None:
            qos_handle_ptr = ffi.new("Int2DdsDataWriterQos **")
            check_ret(lib.int2dds_datawriter_qos_create_default(qos_handle_ptr))
            self._qos_handle = qos_handle_ptr[0]
            _apply_datawriter_qos(self._qos_handle, qos)
            qos_ptr = self._qos_handle

        # Effective representation = caller's choice, else the core default
        # (single source of truth in the Rust core, not hardcoded here).
        if qos is not None and qos.data_representation is not None:
            effective_repr = qos.data_representation._kind_int
        else:
            effective_repr = lib.int2dds_default_data_representation()
        # INT2DDS_QOS_DATA_REPR_XCDR2 == 2
        self._xcdr2 = (effective_repr == 2)

        writer_ptr = ffi.new("Int2DdsDataWriter **")

        if profile is not None:
            check_ret(
                lib.int2dds_create_datawriter_with_profile(
                    publisher._handle, topic._handle, profile.encode(), writer_ptr
                )
            )
        elif listener is not None:
            mask = status_mask if status_mask is not None else STATUS_MASK_ALL
            c_listener, ctx_id = _create_writer_listener_struct(listener, self)
            check_ret(
                lib.int2dds_create_datawriter_with_listener(
                    publisher._handle, topic._handle, qos_ptr,
                    c_listener, mask, writer_ptr
                )
            )
            self._listener_ctx_id = ctx_id
        else:
            check_ret(
                lib.int2dds_create_datawriter(
                    publisher._handle, topic._handle, qos_ptr, writer_ptr
                )
            )

        self._handle = writer_ptr[0]

        # Clean up QoS handle after use
        if self._qos_handle is not None:
            lib.int2dds_datawriter_qos_destroy(self._qos_handle)
            self._qos_handle = None

    @property
    def topic(self) -> Topic[T]:
        """Get the topic this writer publishes to."""
        return self._topic

    def get_qos(self) -> "DataWriterQos":
        """Return the effective QoS (reliability, durability, history) in force."""
        from int2dds.core.qos import (
            DataWriterQos, Reliability, Durability, History, Lifespan,
            ReliabilityKind, DurabilityKind, HistoryKind,
        )

        qos_ptr = ffi.new("Int2DdsDataWriterQos **")
        check_ret(lib.int2dds_datawriter_get_qos(self._handle, qos_ptr))
        handle = qos_ptr[0]
        try:
            rel_kind = ffi.new("int32_t *")
            max_block = ffi.new("int64_t *")
            check_ret(lib.int2dds_datawriter_qos_get_reliability(handle, rel_kind, max_block))
            dur_kind = ffi.new("int32_t *")
            check_ret(lib.int2dds_datawriter_qos_get_durability(handle, dur_kind))
            hist_kind = ffi.new("int32_t *")
            depth = ffi.new("int32_t *")
            check_ret(lib.int2dds_datawriter_qos_get_history(handle, hist_kind, depth))
            frag = ffi.new("int32_t *")
            check_ret(lib.int2dds_datawriter_qos_get_data_frag(handle, frag))
            lifespan_ns = ffi.new("int64_t *")
            check_ret(lib.int2dds_datawriter_qos_get_lifespan(handle, lifespan_ns))
        finally:
            lib.int2dds_datawriter_qos_destroy(handle)

        return DataWriterQos(
            reliability=Reliability(
                kind=ReliabilityKind(rel_kind[0]).name,
                max_blocking_time=max_block[0] / 1_000_000_000,
            ),
            durability=Durability(kind=DurabilityKind(dur_kind[0]).name),
            history=History(kind=HistoryKind(hist_kind[0]).name, depth=depth[0]),
            data_frag=frag[0],
            lifespan=Lifespan(duration=lifespan_ns[0] / 1_000_000_000),
        )

    def set_qos(self, qos: "DataWriterQos") -> None:
        """Set this writer's QoS.

        The given policies are merged onto the writer's current QoS (fetched as the
        base), then applied. Changing an immutable policy to a different value is
        rejected by the core.
        """
        qos_ptr = ffi.new("Int2DdsDataWriterQos **")
        check_ret(lib.int2dds_datawriter_get_qos(self._handle, qos_ptr))
        handle = qos_ptr[0]
        try:
            _apply_datawriter_qos(handle, qos)
            check_ret(lib.int2dds_datawriter_set_qos(self._handle, handle))
        finally:
            lib.int2dds_datawriter_qos_destroy(handle)

    def write(self, sample: T) -> None:
        """
        Write a data sample.

        The sample is serialized using its _serialize_cdr() method
        and published to the topic.

        Args:
            sample: The data sample to write
        """
        # Serialize the sample; the core derives the canonical key/handle from it.
        data = sample._serialize_cdr(self._xcdr2)

        # Write to the FFI
        data_ptr = ffi.from_buffer(data)
        key_ptr = ffi.NULL
        key_len = 0

        check_ret(
            lib.int2dds_write_serialized(self._handle, data_ptr, len(data), key_ptr, key_len)
        )
    def register_instance(self, sample: T) -> bytes:
        """
        Register an instance and return a 16-byte InstanceHandle.

        Args:
            sample: A data sample with key fields set.

        Returns:
            16-byte InstanceHandle for use in subsequent write/dispose/unregister.
        """
        key: bytes | None = None
        if getattr(sample, "_has_key", False):
            key = sample._serialize_cdr(self._xcdr2)

        if not key:
            return b'\x00' * 16

        key_ptr = ffi.from_buffer(key)
        handle_out = ffi.new("uint8_t[16]")

        check_ret(
            lib.int2dds_datawriter_register_instance(
                self._handle, key_ptr, len(key), handle_out
            )
        )

        return bytes(ffi.buffer(handle_out))

    def unregister_instance(self, sample: T, handle: bytes) -> None:
        """
        Unregister a previously registered instance.

        Informs the DDS service that this DataWriter will no longer modify
        the specified instance. Readers will see NOT_ALIVE_NO_WRITERS.

        Args:
            sample: A data sample with key fields set.
            handle: 16-byte InstanceHandle from register_instance().
        """
        key: bytes | None = None
        if getattr(sample, "_has_key", False):
            key = sample._serialize_cdr(self._xcdr2)

        if not key:
            return

        key_ptr = ffi.from_buffer(key)
        handle_ptr = ffi.from_buffer(handle)

        check_ret(
            lib.int2dds_datawriter_unregister_instance(
                self._handle, key_ptr, len(key), handle_ptr
            )
        )

    def dispose(self, sample: T, handle: bytes) -> None:
        """
        Dispose an instance, marking it as no longer valid.

        Readers will see the instance state change to NOT_ALIVE_DISPOSED.
        Unlike unregister, dispose indicates the data itself is invalid.

        Args:
            sample: A data sample with key fields set.
            handle: 16-byte InstanceHandle from register_instance().
        """
        key: bytes | None = None
        if getattr(sample, "_has_key", False):
            key = sample._serialize_cdr(self._xcdr2)

        if not key:
            return

        key_ptr = ffi.from_buffer(key)
        handle_ptr = ffi.from_buffer(handle)

        check_ret(
            lib.int2dds_datawriter_dispose(
                self._handle, key_ptr, len(key), handle_ptr
            )
        )

    def lookup_instance(self, sample: T) -> bytes:
        """
        Look up the handle of a previously registered instance.

        Returns the 16-byte InstanceHandle if the instance is known,
        or 16 zero bytes (HANDLE_NIL) if not found.
        Does NOT register the instance.

        Args:
            sample: A data sample with key fields set.

        Returns:
            16-byte InstanceHandle, or NIL (all zeros) if not found.
        """
        key: bytes | None = None
        if getattr(sample, "_has_key", False):
            key = sample._serialize_cdr(self._xcdr2)

        if not key:
            return b'\x00' * 16

        key_ptr = ffi.from_buffer(key)
        handle_out = ffi.new("uint8_t[16]")

        check_ret(
            lib.int2dds_datawriter_lookup_instance(
                self._handle, key_ptr, len(key), handle_out
            )
        )

        return bytes(ffi.buffer(handle_out))

    def get_publication_matched_status(self) -> tuple[int, int]:
        """
        Get the publication matched status.

        Returns:
            Tuple of (total_count, current_count) indicating matched readers
        """
        total_out = ffi.new("int32_t *")
        current_out = ffi.new("int32_t *")
        check_ret(lib.int2dds_get_publication_matched_status(self._handle, total_out, current_out))
        return total_out[0], current_out[0]

    def get_statuscondition(self) -> StatusCondition:
        """Get the StatusCondition associated with this DataWriter."""
        cond_ptr = ffi.new("Int2DdsStatusCondition **")
        check_ret(lib.int2dds_datawriter_get_statuscondition(self._handle, cond_ptr))
        return StatusCondition(cond_ptr[0], owner=self)

    def get_status_changes(self) -> int:
        """Get the current status change bitmask of this DataWriter."""
        mask_out = ffi.new("uint32_t *")
        check_ret(lib.int2dds_datawriter_get_status_changes(self._handle, mask_out))
        return mask_out[0]

    @property
    def matched_readers(self) -> int:
        """Get the current number of matched readers."""
        _, current = self.get_publication_matched_status()
        return current

    def get_liveliness_lost_status(self) -> dict:
        """Get liveliness lost status.

        Returns:
            dict with total_count, total_count_change
        """
        status = ffi.new("Int2DdsLivelinessLostStatus *")
        check_ret(lib.int2dds_datawriter_get_liveliness_lost_status(self._handle, status))
        return {
            "total_count": status.total_count,
            "total_count_change": status.total_count_change,
        }

    def get_offered_deadline_missed_status(self) -> dict:
        """Get offered deadline missed status.

        Returns:
            dict with total_count, total_count_change, last_instance_handle
        """
        status = ffi.new("Int2DdsOfferedDeadlineMissedStatus *")
        check_ret(lib.int2dds_datawriter_get_offered_deadline_missed_status(self._handle, status))
        return {
            "total_count": status.total_count,
            "total_count_change": status.total_count_change,
            "last_instance_handle": bytes(status.last_instance_handle),
        }

    def get_offered_incompatible_qos_status(self) -> dict:
        """Get offered incompatible QoS status.

        Returns:
            dict with total_count, total_count_change, last_policy_id, policies_count
        """
        status = ffi.new("Int2DdsOfferedIncompatibleQosStatus *")
        check_ret(lib.int2dds_datawriter_get_offered_incompatible_qos_status(self._handle, status))
        return {
            "total_count": status.total_count,
            "total_count_change": status.total_count_change,
            "last_policy_id": int(status.last_policy_id),
            "policies_count": status.policies_count,
        }

    def get_offered_incompatible_type_status(self) -> dict:
        """Get offered incompatible type status.

        Returns:
            dict with total_count, total_count_change
        """
        status = ffi.new("Int2DdsOfferedIncompatibleTypeStatus *")
        check_ret(lib.int2dds_datawriter_get_offered_incompatible_type_status(self._handle, status))
        return {
            "total_count": status.total_count,
            "total_count_change": status.total_count_change,
        }

    def get_guid(self) -> bytes:
        """Get this DataWriter's 16-byte GUID."""
        buf = ffi.new("uint8_t[16]")
        check_ret(lib.int2dds_datawriter_get_guid(self._handle, ffi.cast("uint8_t(*)[16]", buf)))
        return bytes(ffi.buffer(buf, 16))

    def write_serialized_staged(self, data: bytes, key: bytes = b"") -> None:
        """Write pre-serialized CDR bytes via the zero-copy staging path.

        Reserves a native buffer, copies ``data`` in, then commits (or aborts on
        error). ``key`` is the serialized key for keyed types.
        """
        data_out = ffi.new("uint8_t **")
        cap_out = ffi.new("size_t *")
        loan_out = ffi.new("Int2DdsSerializedWriteLoan **")
        check_ret(lib.int2dds_prepare_serialized_write(
            self._handle, len(data), data_out, cap_out, loan_out))
        loan = loan_out[0]
        try:
            ffi.memmove(data_out[0], data, len(data))
            key_ptr = ffi.from_buffer(key) if key else ffi.NULL
            check_ret(lib.int2dds_commit_serialized_write(
                self._handle, loan, len(data), key_ptr, len(key)))
        except BaseException:
            lib.int2dds_abort_serialized_write(loan)
            raise

    def get_statuscondition(self) -> StatusCondition:
        """Get the StatusCondition associated with this DataWriter."""
        cond_ptr = ffi.new("Int2DdsStatusCondition **")
        check_ret(lib.int2dds_datawriter_get_statuscondition(self._handle, cond_ptr))
        return StatusCondition(cond_ptr[0], owner=self)

    def close(self) -> None:
        """Delete the DataWriter."""
        if not self._closed and self._handle is not None:
            check_ret(lib.int2dds_delete_datawriter(self._handle))
            self._handle = None
            self._closed = True

    def __enter__(self) -> DataWriter[T]:
        return self

    def __exit__(self, exc_type: object, exc_val: object, exc_tb: object) -> None:
        self.close()

    def __del__(self) -> None:
        if not getattr(self, "_closed", True):
            try:
                self.close()
            except Exception:
                pass  # Suppress errors during cleanup
