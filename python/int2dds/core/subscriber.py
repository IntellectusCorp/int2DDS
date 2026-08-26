"""
Subscriber and DataReader for receiving DDS data.
"""

from __future__ import annotations

from dataclasses import dataclass
from typing import TYPE_CHECKING, Generic, TypeVar

from int2dds._ffi import CData, ffi, lib
from int2dds.core.conditions import (
    ANY_INSTANCE_STATE,
    ANY_SAMPLE_STATE,
    ANY_VIEW_STATE,
    QueryCondition,
    ReadCondition,
    StatusCondition,
)
from int2dds.core.listeners import (
    DataReaderListener,
    _create_reader_listener_struct,
    _remove_listener,
    STATUS_MASK_ALL,
)
from int2dds.exceptions import (
    INT2DDS_RET_BUFFER_TOO_SMALL,
    INT2DDS_RET_NO_DATA,
    INT2DDS_RET_OK,
    check_ret,
)

if TYPE_CHECKING:
    from int2dds.core.participant import DomainParticipant
    from int2dds.core.qos import DataReaderQos
    from int2dds.core.topic import ContentFilteredTopic, Topic
    from int2dds.types.base import DdsType

T = TypeVar("T", bound="DdsType")


@dataclass
class Sample(Generic[T]):
    """
    A data sample received from a DataReader.

    Attributes:
        data: The deserialized data (None if not valid_data)
        valid_data: True if this is a valid data sample (not dispose/unregister)
        instance_handle: The 16-byte instance handle, when available.
        instance_state: ALIVE / NOT_ALIVE_DISPOSED / NOT_ALIVE_NO_WRITERS
            (see ``int2dds.core.conditions``); tells a dispose apart from an
            unregister on a metadata-only sample.
    """

    data: T | None
    valid_data: bool
    instance_handle: bytes | None = None
    instance_state: int | None = None


def _apply_datareader_qos(handle: CData, qos: "DataReaderQos") -> None:
    """Apply DataReaderQos policies onto a native reader QoS handle.

    Shared by reader creation and set_qos. Every policy is optional; only the ones
    explicitly set on `qos` are applied, so unset policies retain the base handle's
    value (the native default at creation, or the current QoS on set_qos merge).
    """
    if qos.reliability is not None:
        check_ret(lib.int2dds_datareader_qos_set_reliability(
            handle, qos.reliability._kind_int, qos.reliability._max_blocking_time_ns))
    if qos.durability is not None:
        check_ret(lib.int2dds_datareader_qos_set_durability(handle, qos.durability._kind_int))
    if qos.history is not None:
        check_ret(lib.int2dds_datareader_qos_set_history(
            handle, qos.history._kind_int, qos.history.depth))
    if qos.ownership is not None:
        check_ret(lib.int2dds_datareader_qos_set_ownership(handle, qos.ownership._kind_int))
    if qos.resource_limits is not None:
        check_ret(lib.int2dds_datareader_qos_set_resource_limits(
            handle,
            qos.resource_limits.max_samples,
            qos.resource_limits.max_instances,
            qos.resource_limits.max_samples_per_instance))
    if qos.destination_order is not None:
        check_ret(lib.int2dds_datareader_qos_set_destination_order(
            handle, qos.destination_order._kind_int))
    if qos.lifespan_reference is not None:
        check_ret(lib.int2dds_datareader_qos_set_lifespan_reference(
            handle, qos.lifespan_reference._kind_int))
    if qos.time_based_filter is not None:
        check_ret(lib.int2dds_datareader_qos_set_time_based_filter(
            handle, qos.time_based_filter._minimum_separation_ns))
    if qos.latency_budget is not None:
        check_ret(lib.int2dds_datareader_qos_set_latency_budget(
            handle, qos.latency_budget._duration_ns))
    if qos.user_data is not None and qos.user_data.data:
        data_ptr = ffi.from_buffer(qos.user_data.data)
        check_ret(lib.int2dds_datareader_qos_set_user_data(
            handle, data_ptr, len(qos.user_data.data)))
    if qos.reader_data_lifecycle is not None:
        check_ret(lib.int2dds_datareader_qos_set_reader_data_lifecycle(
            handle,
            qos.reader_data_lifecycle._autopurge_nowriter_ns,
            qos.reader_data_lifecycle._autopurge_disposed_ns))
    if qos.data_representation is not None:
        check_ret(lib.int2dds_datareader_qos_set_data_representation(
            handle, qos.data_representation._kind_int))
    if qos.deadline is not None:
        check_ret(lib.int2dds_datareader_qos_set_deadline(handle, qos.deadline._period_ns))
    if qos.liveliness is not None:
        check_ret(lib.int2dds_datareader_qos_set_liveliness(
            handle, qos.liveliness._kind_int, qos.liveliness._lease_duration_ns))


class Subscriber:
    """
    Subscriber - groups DataReaders for coherent subscription.

    Subscribers are created through DomainParticipant.create_subscriber().
    """

    __slots__ = ("_handle", "_participant", "_closed")

    def __init__(self, participant: DomainParticipant, qos: "SubscriberQos | None" = None,
                 profile: str | None = None) -> None:
        self._participant = participant
        self._closed = False

        subscriber_ptr = ffi.new("Int2DdsSubscriber **")
        if profile is not None:
            check_ret(lib.int2dds_create_subscriber_with_profile(
                participant._handle, profile.encode(), subscriber_ptr))
        elif qos is not None and qos.partition is not None and qos.partition.names:
            qos_handle_ptr = ffi.new("Int2DdsSubscriberQos **")
            check_ret(lib.int2dds_subscriber_qos_create_default(qos_handle_ptr))
            qos_handle = qos_handle_ptr[0]
            c_strings = [ffi.new("char[]", n.encode()) for n in qos.partition.names]
            c_array = ffi.new("char*[]", c_strings)
            check_ret(lib.int2dds_subscriber_qos_set_partition(
                qos_handle, c_array, len(qos.partition.names)))
            check_ret(lib.int2dds_create_subscriber(
                participant._handle, qos_handle, subscriber_ptr))
            lib.int2dds_subscriber_qos_destroy(qos_handle)
        else:
            check_ret(lib.int2dds_create_subscriber(
                participant._handle, ffi.NULL, subscriber_ptr))
        self._handle = subscriber_ptr[0]

    def create_datareader(
        self,
        topic: Topic[T] | ContentFilteredTopic[T],
        qos: DataReaderQos | None = None,
        listener: DataReaderListener | None = None,
        status_mask: int | None = None,
    ) -> DataReader[T]:
        """
        Create a DataReader for the given topic or content-filtered topic.

        Args:
            topic: The topic or ContentFilteredTopic to read from
            qos: Optional QoS settings
            listener: Optional listener for event callbacks
            status_mask: Bitmask of statuses to listen for

        Returns:
            A new DataReader instance
        """
        return DataReader(self, topic, qos, listener, status_mask)

    def create_datareader_with_profile(
        self, topic: Topic[T] | ContentFilteredTopic[T], profile: str
    ) -> DataReader[T]:
        """Create a DataReader whose QoS comes from the named XML profile."""
        return DataReader(self, topic, profile=profile)

    def create_datareader_dynamic(self, topic, support):
        """Create a DataReader for a runtime XML/dynamic-typed topic."""
        from int2dds.types.dynamic import create_datareader_dynamic

        return create_datareader_dynamic(self, topic, support)

    def delete_contained_entities(self) -> None:
        """Delete all DataReaders created by this subscriber."""
        check_ret(lib.int2dds_subscriber_delete_contained_entities(self._handle))

    def get_instance_handle(self) -> bytes:
        """Get this subscriber's 16-byte instance handle."""
        buf = ffi.new("uint8_t[16]")
        check_ret(lib.int2dds_subscriber_get_instance_handle(
            self._handle, ffi.cast("uint8_t(*)[16]", buf)))
        return bytes(ffi.buffer(buf, 16))

    def get_statuscondition(self) -> StatusCondition:
        """Get the StatusCondition associated with this subscriber."""
        cond_ptr = ffi.new("Int2DdsStatusCondition **")
        check_ret(lib.int2dds_subscriber_get_statuscondition(self._handle, cond_ptr))
        return StatusCondition(cond_ptr[0], owner=self)

    def get_status_changes(self) -> int:
        """Get the current status change bitmask of this subscriber."""
        mask_out = ffi.new("uint32_t *")
        check_ret(lib.int2dds_subscriber_get_status_changes(self._handle, mask_out))
        return mask_out[0]

    def set_qos(self, qos: "SubscriberQos") -> None:
        """Set this subscriber's QoS.

        The current QoS is used as the merge base, then the ``partition`` policy
        from ``qos`` (the only mutable SubscriberQos policy) is applied on top.
        """
        qos_ptr = ffi.new("Int2DdsSubscriberQos **")
        check_ret(lib.int2dds_subscriber_get_qos(self._handle, qos_ptr))
        handle = qos_ptr[0]
        try:
            if qos is not None and qos.partition is not None and qos.partition.names:
                c_strings = [ffi.new("char[]", n.encode()) for n in qos.partition.names]
                c_array = ffi.new("char*[]", c_strings)
                check_ret(lib.int2dds_subscriber_qos_set_partition(
                    handle, c_array, len(qos.partition.names)))
            check_ret(lib.int2dds_subscriber_set_qos(self._handle, handle))
        finally:
            lib.int2dds_subscriber_qos_destroy(handle)

    def close(self) -> None:
        """Delete the subscriber."""
        if not self._closed and self._handle is not None:
            self.delete_contained_entities()
            check_ret(lib.int2dds_delete_subscriber(self._handle))
            self._handle = None
            self._closed = True

    def __enter__(self) -> Subscriber:
        return self

    def __exit__(self, exc_type: object, exc_val: object, exc_tb: object) -> None:
        self.close()

    def __del__(self) -> None:
        if not getattr(self, "_closed", True):
            try:
                self.close()
            except Exception:
                pass  # Suppress errors during cleanup


class DataReader(Generic[T]):
    """
    DataReader - receives data samples from a topic.

    DataReaders are created through Subscriber.create_datareader().

    Example:
        >>> reader = subscriber.create_datareader(topic)
        >>> for sample in reader.take():
        ...     if sample.valid_data:
        ...         print(sample.data)
    """

    __slots__ = ("_handle", "_subscriber", "_topic", "_qos_handle", "_buffer", "_buffer_size", "_closed","_listener_ctx_id")

    # Default buffer size for reading samples
    DEFAULT_BUFFER_SIZE = 65536

    def __init__(
        self,
        subscriber: Subscriber,
        topic: Topic[T] | ContentFilteredTopic[T],
        qos: DataReaderQos | None = None,
        listener: DataReaderListener | None = None,
        status_mask: int | None = None,
        profile: str | None = None,
    ) -> None:
        self._subscriber = subscriber
        self._topic = topic
        self._closed = False
        self._qos_handle: CData | None = None
        self._listener_ctx_id: int | None = None
        self._buffer_size = self.DEFAULT_BUFFER_SIZE
        self._buffer = ffi.new("uint8_t[]", self._buffer_size)

        # Create QoS if provided
        qos_ptr = ffi.NULL
        if qos is not None:
            qos_handle_ptr = ffi.new("Int2DdsDataReaderQos **")
            check_ret(lib.int2dds_datareader_qos_create_default(qos_handle_ptr))
            self._qos_handle = qos_handle_ptr[0]
            _apply_datareader_qos(self._qos_handle, qos)
            qos_ptr = self._qos_handle

        reader_ptr = ffi.new("Int2DdsDataReader **")

        from int2dds.core.topic import ContentFilteredTopic
        is_cft = isinstance(topic, ContentFilteredTopic)

        c_listener = ffi.NULL
        mask = 0
        if listener is not None:
            mask = status_mask if status_mask is not None else STATUS_MASK_ALL
            c_listener, ctx_id = _create_reader_listener_struct(listener, self)
            self._listener_ctx_id = ctx_id

        if profile is not None:
            check_ret(
                lib.int2dds_create_datareader_with_profile(
                    subscriber._handle, topic._handle, profile.encode(),
                    c_listener, mask, reader_ptr
                )
            )
        elif is_cft:
            check_ret(
                lib.int2dds_create_datareader_cft(
                    subscriber._handle, topic._handle, qos_ptr,
                    c_listener, mask, reader_ptr
                )
            )
        else:
            check_ret(
                lib.int2dds_create_datareader(
                    subscriber._handle, topic._handle, qos_ptr,
                    c_listener, mask, reader_ptr
                )
            )

        self._handle = reader_ptr[0]


        # Clean up QoS handle after use
        if self._qos_handle is not None:
            lib.int2dds_datareader_qos_destroy(self._qos_handle)
            self._qos_handle = None

    @property
    def topic(self) -> Topic[T]:
        """Get the topic this reader subscribes to."""
        return self._topic

    def _grow_buffer(self, required: int) -> None:
        self._buffer_size = required
        self._buffer = ffi.new("uint8_t[]", required)

    def _take_or_read_one(self, native_fn) -> Sample[T] | None:
        actual_size = ffi.new("uintptr_t *")
        info = ffi.new("Int2DdsSampleInfo *")

        while True:
            ret = native_fn(
                self._handle, self._buffer, self._buffer_size, actual_size, info
            )

            if ret == INT2DDS_RET_BUFFER_TOO_SMALL:
                self._grow_buffer(actual_size[0])
                continue
            if ret == INT2DDS_RET_NO_DATA:
                return None
            check_ret(ret)

            handle = bytes(ffi.buffer(info.instance_handle, 16))
            if info.valid_data:
                data_bytes = ffi.buffer(self._buffer, actual_size[0])[:]
                data = self._topic._decode_sample(data_bytes)
                return Sample(data=data, valid_data=True, instance_handle=handle,
                              instance_state=info.instance_state)
            return Sample(data=None, valid_data=False, instance_handle=handle,
                          instance_state=info.instance_state)

    def _take_one(self) -> Sample[T] | None:
        """Take a single sample from the reader."""
        return self._take_or_read_one(lib.int2dds_datareader_take_serialized_w_info)

    def _read_one(self) -> Sample[T] | None:
        """Read a single sample without removing it from the cache."""
        return self._take_or_read_one(lib.int2dds_datareader_read_serialized_w_info)

    def take(self) -> list[Sample[T]]:
        """
        Take all available samples, removing them from the cache.

        Returns:
            List of Sample objects
        """
        samples: list[Sample[T]] = []
        while True:
            sample = self._take_one()
            if sample is None:
                break
            samples.append(sample)
        return samples

    def read(self) -> list[Sample[T]]:
        """
        Read all available samples without removing them from the cache.

        Note: This currently only reads one sample due to FFI limitations.
        Use take() for multiple samples.

        Returns:
            List of Sample objects
        """
        samples: list[Sample[T]] = []
        sample = self._read_one()
        if sample is not None:
            samples.append(sample)
        return samples

    def take_one(self) -> Sample[T] | None:
        """
        Take a single sample.

        Returns:
            A Sample object or None if no data available
        """
        return self._take_one()

    def create_read_condition(
        self,
        sample_states: int = ANY_SAMPLE_STATE,
        view_states: int = ANY_VIEW_STATE,
        instance_states: int = ANY_INSTANCE_STATE,
    ) -> ReadCondition:
        """Create a ReadCondition filtering by sample/view/instance state masks.

        Attach the returned condition to a WaitSet, and pass it to
        take_w_condition()/read_w_condition() to read the matching samples.
        """
        cond_ptr = ffi.new("Int2DdsReadCondition **")
        check_ret(
            lib.int2dds_datareader_create_readcondition(
                self._handle, sample_states, view_states, instance_states, cond_ptr
            )
        )
        return ReadCondition(cond_ptr[0], owner=self)

    def create_query_condition(
        self,
        query_expression: str,
        query_parameters: list[str] | None = None,
        sample_states: int = ANY_SAMPLE_STATE,
        view_states: int = ANY_VIEW_STATE,
        instance_states: int = ANY_INSTANCE_STATE,
    ) -> QueryCondition:
        """Create a QueryCondition: state masks plus a SQL-92 content filter.

        Content filtering requires the topic to carry field descriptors (as with
        ContentFilteredTopic).
        """
        params = query_parameters or []
        count = len(params)
        encoded = [ffi.new("char[]", p.encode("utf-8")) for p in params]
        arr = ffi.new("char *const[]", encoded) if count else ffi.NULL
        expr = ffi.new("char[]", query_expression.encode("utf-8"))
        cond_ptr = ffi.new("Int2DdsReadCondition **")
        check_ret(
            lib.int2dds_datareader_create_querycondition(
                self._handle,
                sample_states,
                view_states,
                instance_states,
                expr,
                arr,
                count,
                cond_ptr,
            )
        )
        return QueryCondition(cond_ptr[0], owner=self)

    def take_w_condition(
        self, condition: ReadCondition, max_samples: int = -1
    ) -> list[Sample[T]]:
        """Take samples matching a Read/QueryCondition (removed from the cache)."""
        return self._read_or_take_w_condition(
            condition, max_samples, lib.int2dds_datareader_take_serialized_batch_w_readcondition
        )

    def read_w_condition(
        self, condition: ReadCondition, max_samples: int = -1
    ) -> list[Sample[T]]:
        """Read samples matching a Read/QueryCondition (left in the cache)."""
        return self._read_or_take_w_condition(
            condition, max_samples, lib.int2dds_datareader_read_serialized_batch_w_readcondition
        )

    def _read_or_take_w_condition(
        self, condition: ReadCondition, max_samples: int, native_fn
    ) -> list[Sample[T]]:
        seq_ptr = ffi.new("Int2DdsSampleSeq **")
        ret = native_fn(self._handle, condition._handle, max_samples, seq_ptr)
        if ret == INT2DDS_RET_NO_DATA:
            if seq_ptr[0] != ffi.NULL:
                lib.int2dds_sample_seq_delete(seq_ptr[0])
            return []
        check_ret(ret)

        seq = seq_ptr[0]
        try:
            count = lib.int2dds_sample_seq_length(seq)
            info = ffi.new("Int2DdsSampleInfo *")
            actual_size = ffi.new("uintptr_t *")
            samples: list[Sample[T]] = []
            for i in range(count):
                check_ret(lib.int2dds_sample_seq_get_info(seq, i, info))
                handle = bytes(ffi.buffer(info.instance_handle, 16))
                if not info.valid_data:
                    samples.append(Sample(data=None, valid_data=False, instance_handle=handle,
                                          instance_state=info.instance_state))
                    continue
                # Copy the serialized bytes, growing the shared buffer if needed.
                while True:
                    ret = lib.int2dds_sample_seq_get_data(
                        seq, i, self._buffer, self._buffer_size, actual_size
                    )
                    if ret == INT2DDS_RET_OK:
                        break
                    if actual_size[0] > self._buffer_size:
                        self._grow_buffer(actual_size[0])
                        continue
                    check_ret(ret)
                data_bytes = ffi.buffer(self._buffer, actual_size[0])[:]
                data = self._topic._decode_sample(data_bytes)
                samples.append(Sample(data=data, valid_data=True, instance_handle=handle,
                                      instance_state=info.instance_state))
            return samples
        finally:
            lib.int2dds_sample_seq_delete(seq)

    def take_instance_serialized(
        self,
        handle: bytes,
        max_samples: int = -1,
        sample_states: int = 0xFFFF,
        view_states: int = 0xFFFF,
        instance_states: int = 0xFFFF,
    ) -> list[Sample[T]]:
        """Take samples belonging to a single instance (raw serialized path).

        ``handle`` is a 16-byte instance handle (from :meth:`lookup_instance` or a
        sample's info). A nil handle raises; an unknown handle returns ``[]``.
        """
        return self._read_or_take_instance_serialized(
            handle, max_samples, sample_states, view_states, instance_states,
            lib.int2dds_datareader_take_instance_serialized_batch)

    def read_instance_serialized(
        self,
        handle: bytes,
        max_samples: int = -1,
        sample_states: int = 0xFFFF,
        view_states: int = 0xFFFF,
        instance_states: int = 0xFFFF,
    ) -> list[Sample[T]]:
        """Read samples belonging to a single instance (samples stay in cache)."""
        return self._read_or_take_instance_serialized(
            handle, max_samples, sample_states, view_states, instance_states,
            lib.int2dds_datareader_read_instance_serialized_batch)

    def _read_or_take_instance_serialized(
        self, handle: bytes, max_samples: int,
        sample_states: int, view_states: int, instance_states: int, native_fn,
    ) -> list[Sample[T]]:
        if len(handle) != 16:
            raise ValueError("instance handle must be 16 bytes")
        handle_c = ffi.new("uint8_t[16]", list(handle))
        handle_ref = ffi.cast("const uint8_t(*)[16]", handle_c)
        seq_ptr = ffi.new("Int2DdsSampleSeq **")
        ret = native_fn(
            self._handle, handle_ref, max_samples,
            sample_states, view_states, instance_states, seq_ptr)
        if ret == INT2DDS_RET_NO_DATA:
            if seq_ptr[0] != ffi.NULL:
                lib.int2dds_sample_seq_delete(seq_ptr[0])
            return []
        check_ret(ret)

        seq = seq_ptr[0]
        try:
            count = lib.int2dds_sample_seq_length(seq)
            info = ffi.new("Int2DdsSampleInfo *")
            actual_size = ffi.new("uintptr_t *")
            samples: list[Sample[T]] = []
            for i in range(count):
                check_ret(lib.int2dds_sample_seq_get_info(seq, i, info))
                handle = bytes(ffi.buffer(info.instance_handle, 16))
                if not info.valid_data:
                    samples.append(Sample(data=None, valid_data=False, instance_handle=handle,
                                          instance_state=info.instance_state))
                    continue
                while True:
                    ret = lib.int2dds_sample_seq_get_data(
                        seq, i, self._buffer, self._buffer_size, actual_size)
                    if ret == INT2DDS_RET_OK:
                        break
                    if actual_size[0] > self._buffer_size:
                        self._grow_buffer(actual_size[0])
                        continue
                    check_ret(ret)
                data_bytes = ffi.buffer(self._buffer, actual_size[0])[:]
                data = self._topic._decode_sample(data_bytes)
                samples.append(Sample(data=data, valid_data=True, instance_handle=handle,
                                      instance_state=info.instance_state))
            return samples
        finally:
            lib.int2dds_sample_seq_delete(seq)

    def take_serialized(self, max_samples: int = -1) -> list[Sample[T]]:
        """Take all available samples in one batch (raw serialized path)."""
        return self._read_or_take_serialized_batch(
            max_samples, lib.int2dds_datareader_take_serialized_batch)

    def read_serialized(self, max_samples: int = -1) -> list[Sample[T]]:
        """Read all available samples in one batch (samples stay in cache)."""
        return self._read_or_take_serialized_batch(
            max_samples, lib.int2dds_datareader_read_serialized_batch)

    def _read_or_take_serialized_batch(
        self, max_samples: int, native_fn
    ) -> list[Sample[T]]:
        seq_ptr = ffi.new("Int2DdsSampleSeq **")
        ret = native_fn(self._handle, max_samples, seq_ptr)
        if ret == INT2DDS_RET_NO_DATA:
            if seq_ptr[0] != ffi.NULL:
                lib.int2dds_sample_seq_delete(seq_ptr[0])
            return []
        check_ret(ret)

        seq = seq_ptr[0]
        try:
            count = lib.int2dds_sample_seq_length(seq)
            info = ffi.new("Int2DdsSampleInfo *")
            actual_size = ffi.new("uintptr_t *")
            samples: list[Sample[T]] = []
            for i in range(count):
                check_ret(lib.int2dds_sample_seq_get_info(seq, i, info))
                handle = bytes(ffi.buffer(info.instance_handle, 16))
                if not info.valid_data:
                    samples.append(Sample(data=None, valid_data=False, instance_handle=handle,
                                          instance_state=info.instance_state))
                    continue
                while True:
                    ret = lib.int2dds_sample_seq_get_data(
                        seq, i, self._buffer, self._buffer_size, actual_size)
                    if ret == INT2DDS_RET_OK:
                        break
                    if actual_size[0] > self._buffer_size:
                        self._grow_buffer(actual_size[0])
                        continue
                    check_ret(ret)
                data_bytes = ffi.buffer(self._buffer, actual_size[0])[:]
                data = self._topic._decode_sample(data_bytes)
                samples.append(Sample(data=data, valid_data=True, instance_handle=handle,
                                      instance_state=info.instance_state))
            return samples
        finally:
            lib.int2dds_sample_seq_delete(seq)

    def get_subscription_matched_status(self) -> tuple[int, int]:
        """
        Get the subscription matched status.

        Returns:
            Tuple of (total_count, current_count) indicating matched writers
        """
        status = ffi.new("Int2DdsSubscriptionMatchedStatus *")
        check_ret(
            lib.int2dds_datareader_get_subscription_matched_status(self._handle, status)
        )
        return status.total_count, status.current_count

    @property
    def matched_writers(self) -> int:
        """Get the current number of matched writers."""
        _, current = self.get_subscription_matched_status()
        return current

    def lookup_instance(self, key: bytes) -> bytes:
        """Look up the 16-byte InstanceHandle for a stored serialized key.

        The key must be in the serialized form the reader stored for the instance,
        i.e. the bytes returned by get_key_value() (or a sample's instance handle on
        the raw-serialized path) — not a freshly serialized key.

        Args:
            key: Serialized key bytes as stored by the reader.

        Returns:
            16-byte InstanceHandle, or NIL (16 zero bytes) if the instance is unknown.
        """
        if not key:
            return b"\x00" * 16

        key_ptr = ffi.from_buffer(key)
        handle_out = ffi.new("uint8_t[16]")
        check_ret(
            lib.int2dds_datareader_lookup_instance(
                self._handle, key_ptr, len(key), ffi.cast("uint8_t(*)[16]", handle_out)
            )
        )
        return bytes(ffi.buffer(handle_out))

    def get_key_value(self, handle: bytes) -> bytes:
        """Get the serialized key bytes stored for an instance handle.

        Round-trips with lookup_instance(). On the raw-serialized path the stored
        key is the 16-byte instance handle itself.

        Args:
            handle: 16-byte InstanceHandle.

        Returns:
            The serialized key bytes stored for the instance.
        """
        if len(handle) != 16:
            raise ValueError("handle must be exactly 16 bytes")

        handle_ptr = ffi.from_buffer(handle)
        capacity = 64
        while True:
            key_buf = ffi.new("uint8_t[]", capacity)
            size_out = ffi.new("uintptr_t *")
            ret = lib.int2dds_datareader_get_key_value(
                self._handle,
                ffi.cast("const uint8_t(*)[16]", handle_ptr),
                key_buf,
                capacity,
                size_out,
            )
            if ret == INT2DDS_RET_OK:
                return bytes(ffi.buffer(key_buf, size_out[0]))
            if ret == INT2DDS_RET_BUFFER_TOO_SMALL and size_out[0] > capacity:
                capacity = size_out[0]
                continue
            check_ret(ret)

    def get_qos(self) -> "DataReaderQos":
        """Return the effective QoS (reliability, durability, history) in force."""
        from int2dds.core.qos import (
            DataReaderQos, Reliability, Durability, History, LifespanReference, ResourceLimits,
            ReliabilityKind, DurabilityKind, HistoryKind, LifespanReferenceKind,
        )

        qos_ptr = ffi.new("Int2DdsDataReaderQos **")
        check_ret(lib.int2dds_datareader_get_qos(self._handle, qos_ptr))
        handle = qos_ptr[0]
        try:
            rel_kind = ffi.new("int32_t *")
            max_block = ffi.new("int64_t *")
            check_ret(lib.int2dds_datareader_qos_get_reliability(handle, rel_kind, max_block))
            dur_kind = ffi.new("int32_t *")
            check_ret(lib.int2dds_datareader_qos_get_durability(handle, dur_kind))
            hist_kind = ffi.new("int32_t *")
            depth = ffi.new("int32_t *")
            check_ret(lib.int2dds_datareader_qos_get_history(handle, hist_kind, depth))
            lifespan_ref_kind = ffi.new("int32_t *")
            check_ret(lib.int2dds_datareader_qos_get_lifespan_reference(handle, lifespan_ref_kind))
            max_samples = ffi.new("int32_t *")
            max_instances = ffi.new("int32_t *")
            max_per_instance = ffi.new("int32_t *")
            check_ret(lib.int2dds_datareader_qos_get_resource_limits(
                handle, max_samples, max_instances, max_per_instance))
        finally:
            lib.int2dds_datareader_qos_destroy(handle)

        return DataReaderQos(
            reliability=Reliability(
                kind=ReliabilityKind(rel_kind[0]).name,
                max_blocking_time=max_block[0] / 1_000_000_000,
            ),
            durability=Durability(kind=DurabilityKind(dur_kind[0]).name),
            history=History(kind=HistoryKind(hist_kind[0]).name, depth=depth[0]),
            lifespan_reference=LifespanReference(
                kind=LifespanReferenceKind(lifespan_ref_kind[0]).name),
            resource_limits=ResourceLimits(
                max_samples=max_samples[0],
                max_instances=max_instances[0],
                max_samples_per_instance=max_per_instance[0],
            ),
        )

    def set_qos(self, qos: "DataReaderQos") -> None:
        """Set this reader's QoS.

        The given policies are merged onto the reader's current QoS (fetched as the
        base), then applied. Changing an immutable policy to a different value is
        rejected by the core.
        """
        qos_ptr = ffi.new("Int2DdsDataReaderQos **")
        check_ret(lib.int2dds_datareader_get_qos(self._handle, qos_ptr))
        handle = qos_ptr[0]
        try:
            _apply_datareader_qos(handle, qos)
            check_ret(lib.int2dds_datareader_set_qos(self._handle, handle))
        finally:
            lib.int2dds_datareader_qos_destroy(handle)

    def get_liveliness_changed_status(self) -> dict:
        """Get liveliness changed status.

        Returns:
            dict with alive_count, not_alive_count, alive_count_change,
            not_alive_count_change, last_publication_handle
        """
        status = ffi.new("Int2DdsLivelinessChangedStatus *")
        check_ret(lib.int2dds_datareader_get_liveliness_changed_status(self._handle, status))
        return {
            "alive_count": status.alive_count,
            "not_alive_count": status.not_alive_count,
            "alive_count_change": status.alive_count_change,
            "not_alive_count_change": status.not_alive_count_change,
            "last_publication_handle": bytes(status.last_publication_handle),
        }

    def get_sample_rejected_status(self) -> dict:
        """Get sample rejected status.

        Returns:
            dict with total_count, total_count_change, last_reason, last_instance_handle
        """
        status = ffi.new("Int2DdsSampleRejectedStatus *")
        check_ret(lib.int2dds_datareader_get_sample_rejected_status(self._handle, status))
        return {
            "total_count": status.total_count,
            "total_count_change": status.total_count_change,
            "last_reason": int(status.last_reason),
            "last_instance_handle": bytes(status.last_instance_handle),
        }

    def get_sample_lost_status(self) -> dict:
        """Get sample lost status.

        Returns:
            dict with total_count, total_count_change
        """
        status = ffi.new("Int2DdsSampleLostStatus *")
        check_ret(lib.int2dds_datareader_get_sample_lost_status(self._handle, status))
        return {
            "total_count": status.total_count,
            "total_count_change": status.total_count_change,
        }

    def get_requested_deadline_missed_status(self) -> dict:
        """Get requested deadline missed status.

        Returns:
            dict with total_count, total_count_change, last_instance_handle
        """
        status = ffi.new("Int2DdsRequestedDeadlineMissedStatus *")
        check_ret(lib.int2dds_datareader_get_requested_deadline_missed_status(self._handle, status))
        return {
            "total_count": status.total_count,
            "total_count_change": status.total_count_change,
            "last_instance_handle": bytes(status.last_instance_handle),
        }

    def get_requested_incompatible_qos_status(self) -> dict:
        """Get requested incompatible QoS status.

        Returns:
            dict with total_count, total_count_change, last_policy_id, policies_count
        """
        status = ffi.new("Int2DdsRequestedIncompatibleQosStatus *")
        check_ret(lib.int2dds_datareader_get_requested_incompatible_qos_status(self._handle, status))
        return {
            "total_count": status.total_count,
            "total_count_change": status.total_count_change,
            "last_policy_id": int(status.last_policy_id),
            "policies_count": status.policies_count,
        }

    def get_requested_incompatible_type_status(self) -> dict:
        """Get requested incompatible type status.

        Returns:
            dict with total_count, total_count_change
        """
        status = ffi.new("Int2DdsRequestedIncompatibleTypeStatus *")
        check_ret(lib.int2dds_datareader_get_requested_incompatible_type_status(self._handle, status))
        return {
            "total_count": status.total_count,
            "total_count_change": status.total_count_change,
        }

    def get_guid(self) -> bytes:
        """Get this DataReader's 16-byte GUID."""
        buf = ffi.new("uint8_t[16]")
        check_ret(lib.int2dds_datareader_get_guid(self._handle, ffi.cast("uint8_t(*)[16]", buf)))
        return bytes(ffi.buffer(buf, 16))

    def has_data(self) -> bool:
        """Return whether unread samples are available in the cache."""
        out = ffi.new("bool *")
        check_ret(lib.int2dds_datareader_has_data(self._handle, out))
        return out[0]

    def take_next_serialized_loaned(self) -> bytes | None:
        """Take one sample as raw CDR bytes via the zero-copy loan path.

        Returns the CDR bytes (copied out before the loan is returned), or None
        if no data is available.
        """
        data_out = ffi.new("const uint8_t **")
        size_out = ffi.new("uintptr_t *")
        valid_out = ffi.new("bool *")
        loan_out = ffi.new("Int2DdsSerializedLoan **")
        ret = lib.int2dds_datareader_take_serialized_loaned(
            self._handle, data_out, size_out, valid_out, loan_out)
        if ret == INT2DDS_RET_NO_DATA:
            return None
        check_ret(ret)
        try:
            if not valid_out[0] or loan_out[0] == ffi.NULL:
                return b""
            return bytes(ffi.buffer(data_out[0], size_out[0]))
        finally:
            if loan_out[0] != ffi.NULL:
                lib.int2dds_datareader_return_serialized_loan(loan_out[0])

    def get_statuscondition(self) -> StatusCondition:
        """Get the StatusCondition associated with this DataReader."""
        cond_ptr = ffi.new("Int2DdsStatusCondition **")
        check_ret(lib.int2dds_datareader_get_statuscondition(self._handle, cond_ptr))
        return StatusCondition(cond_ptr[0], owner=self)

    def get_status_changes(self) -> int:
        """Get the current status change bitmask of this DataReader."""
        mask_out = ffi.new("uint32_t *")
        check_ret(lib.int2dds_datareader_get_status_changes(self._handle, mask_out))
        return mask_out[0]

    def set_listener(
        self,
        listener: DataReaderListener | None,
        status_mask: int | None = None,
    ) -> None:
        """
        Set or replace the listener for this DataReader.

        Args:
            listener: The listener to set, or None to remove.
            status_mask: Bitmask of statuses to listen for.
                         Defaults to STATUS_MASK_ALL.
        """
        if self._listener_ctx_id is not None:
            _remove_listener(self._listener_ctx_id)
            self._listener_ctx_id = None

        mask = status_mask if status_mask is not None else STATUS_MASK_ALL

        if listener is not None:
            c_listener, ctx_id = _create_reader_listener_struct(listener, self)
            check_ret(
                lib.int2dds_datareader_set_listener(self._handle, c_listener, mask)
            )
            self._listener_ctx_id = ctx_id
        else:
            check_ret(
                lib.int2dds_datareader_set_listener(self._handle, ffi.NULL, mask)
            )
    def close(self) -> None:
        """Delete the DataReader."""
        if not self._closed and self._handle is not None:
            if self._listener_ctx_id is not None:
                _remove_listener(self._listener_ctx_id)
                self._listener_ctx_id = None
            check_ret(lib.int2dds_delete_datareader(self._handle))
            self._handle = None
            self._closed = True

    def __enter__(self) -> DataReader[T]:
        return self

    def __exit__(self, exc_type: object, exc_val: object, exc_tb: object) -> None:
        self.close()

    def __del__(self) -> None:
        if not getattr(self, "_closed", True):
            try:
                self.close()
            except Exception:
                pass  # Suppress errors during cleanup
