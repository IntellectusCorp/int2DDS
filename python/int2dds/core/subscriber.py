"""
Subscriber and DataReader for receiving DDS data.
"""

from __future__ import annotations

from dataclasses import dataclass
from typing import TYPE_CHECKING, Generic, TypeVar

from int2dds._ffi import ffi, lib
from int2dds.core.conditions import StatusCondition
from int2dds.core.listeners import (
    DataReaderListener,
    _create_reader_listener_struct,
    _remove_listener,
    STATUS_MASK_ALL,
)
from int2dds.exceptions import INT2DDS_RET_NO_DATA, check_ret

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
    """

    data: T | None
    valid_data: bool


class Subscriber:
    """
    Subscriber - groups DataReaders for coherent subscription.

    Subscribers are created through DomainParticipant.create_subscriber().
    """

    __slots__ = ("_handle", "_participant", "_closed")

    def __init__(self, participant: DomainParticipant, qos: "SubscriberQos | None" = None) -> None:
        self._participant = participant
        self._closed = False

        subscriber_ptr = ffi.new("Int2DdsSubscriber **")
        if qos is not None and qos.partition is not None and qos.partition.names:
            qos_handle_ptr = ffi.new("Int2DdsSubscriberQos **")
            check_ret(lib.int2dds_subscriber_qos_create_default(qos_handle_ptr))
            qos_handle = qos_handle_ptr[0]
            c_strings = [ffi.new("char[]", n.encode()) for n in qos.partition.names]
            c_array = ffi.new("char*[]", c_strings)
            check_ret(lib.int2dds_subscriber_qos_set_partition(
                qos_handle, c_array, len(qos.partition.names)))
            check_ret(lib.int2dds_create_subscriber_with_qos(
                participant._handle, qos_handle, subscriber_ptr))
            lib.int2dds_subscriber_qos_destroy(qos_handle)
        else:
            check_ret(lib.int2dds_create_subscriber(participant._handle, subscriber_ptr))
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

    def create_datareader_dynamic(self, topic, support):
        """Create a DataReader for a runtime XML/dynamic-typed topic."""
        from int2dds.types.dynamic import create_datareader_dynamic

        return create_datareader_dynamic(self, topic, support)

    def delete_contained_entities(self) -> None:
        """Delete all DataReaders created by this subscriber."""
        check_ret(lib.int2dds_subscriber_delete_contained_entities(self._handle))

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
    ) -> None:
        self._subscriber = subscriber
        self._topic = topic
        self._closed = False
        self._qos_handle: ffi.CData | None = None
        self._listener_ctx_id: int | None = None
        self._buffer_size = self.DEFAULT_BUFFER_SIZE
        self._buffer = ffi.new(f"uint8_t[{self._buffer_size}]")

        # Create QoS if provided
        qos_ptr = ffi.NULL
        if qos is not None:
            qos_handle_ptr = ffi.new("Int2DdsDataReaderQos **")
            check_ret(lib.int2dds_datareader_qos_create_default(qos_handle_ptr))
            self._qos_handle = qos_handle_ptr[0]

            # Apply QoS settings
            check_ret(
                lib.int2dds_datareader_qos_set_reliability(
                    self._qos_handle, qos.reliability._kind_int
                )
            )
            check_ret(
                lib.int2dds_datareader_qos_set_durability(
                    self._qos_handle, qos.durability._kind_int
                )
            )
            check_ret(
                lib.int2dds_datareader_qos_set_history(
                    self._qos_handle, qos.history._kind_int, qos.history.depth
                )
            )
            if qos.ownership is not None:
                check_ret(lib.int2dds_datareader_qos_set_ownership(
                    self._qos_handle, qos.ownership._kind_int))
            if qos.resource_limits is not None:
                check_ret(lib.int2dds_datareader_qos_set_resource_limits(
                    self._qos_handle,
                    qos.resource_limits.max_samples,
                    qos.resource_limits.max_instances,
                    qos.resource_limits.max_samples_per_instance))
            if qos.destination_order is not None:
                check_ret(lib.int2dds_datareader_qos_set_destination_order(
                    self._qos_handle, qos.destination_order._kind_int))
            if qos.time_based_filter is not None:
                check_ret(lib.int2dds_datareader_qos_set_time_based_filter(
                    self._qos_handle, qos.time_based_filter._minimum_separation_ns))
            if qos.latency_budget is not None:
                check_ret(lib.int2dds_datareader_qos_set_latency_budget(
                    self._qos_handle, qos.latency_budget._duration_ns))
            if qos.user_data is not None and qos.user_data.data:
                data_ptr = ffi.from_buffer(qos.user_data.data)
                check_ret(lib.int2dds_datareader_qos_set_user_data(
                    self._qos_handle, data_ptr, len(qos.user_data.data)))
            if qos.reader_data_lifecycle is not None:
                check_ret(lib.int2dds_datareader_qos_set_reader_data_lifecycle(
                    self._qos_handle,
                    qos.reader_data_lifecycle._autopurge_nowriter_ns,
                    qos.reader_data_lifecycle._autopurge_disposed_ns))
            if qos.data_representation is not None:
                check_ret(lib.int2dds_datareader_qos_set_data_representation(
                    self._qos_handle, qos.data_representation._kind_int))
            if qos.deadline is not None:
                check_ret(lib.int2dds_datareader_qos_set_deadline(
                    self._qos_handle, qos.deadline._period_ns))
            if qos.liveliness is not None:
                check_ret(lib.int2dds_datareader_qos_set_liveliness(
                    self._qos_handle, qos.liveliness._kind_int, qos.liveliness._lease_duration_ns))
            qos_ptr = self._qos_handle

        reader_ptr = ffi.new("Int2DdsDataReader **")

        from int2dds.core.topic import ContentFilteredTopic
        is_cft = isinstance(topic, ContentFilteredTopic)

        if listener is not None:
            mask = status_mask if status_mask is not None else STATUS_MASK_ALL
            c_listener, ctx_id = _create_reader_listener_struct(listener, self)
            if is_cft:
                check_ret(
                    lib.int2dds_create_datareader_cft_with_listener(
                        subscriber._handle, topic._handle, qos_ptr,
                        c_listener, mask, reader_ptr
                    )
                )
            else:
                check_ret(
                    lib.int2dds_create_datareader_with_listener(
                        subscriber._handle, topic._handle, qos_ptr,
                        c_listener, mask, reader_ptr
                    )
                )
            self._listener_ctx_id = ctx_id
        else:
            if is_cft:
                check_ret(
                    lib.int2dds_create_datareader_cft(
                        subscriber._handle, topic._handle, qos_ptr, reader_ptr
                    )
                )
            else:
                check_ret(
                    lib.int2dds_create_datareader(
                        subscriber._handle, topic._handle, qos_ptr, reader_ptr
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

    def _take_one(self) -> Sample[T] | None:
        """Take a single sample from the reader."""
        actual_size = ffi.new("size_t *")
        valid_data = ffi.new("bool *")

        ret = lib.int2dds_take_serialized(
            self._handle, self._buffer, self._buffer_size, actual_size, valid_data
        )

        if ret == INT2DDS_RET_NO_DATA:
            return None
        check_ret(ret)

        if valid_data[0]:
            # Deserialize the data
            data_bytes = ffi.buffer(self._buffer, actual_size[0])[:]
            data = self._topic.type_class._deserialize_cdr(data_bytes)
            return Sample(data=data, valid_data=True)
        else:
            return Sample(data=None, valid_data=False)

    def _read_one(self) -> Sample[T] | None:
        """Read a single sample without removing it from the cache."""
        actual_size = ffi.new("size_t *")
        valid_data = ffi.new("bool *")

        ret = lib.int2dds_read_serialized(
            self._handle, self._buffer, self._buffer_size, actual_size, valid_data
        )

        if ret == INT2DDS_RET_NO_DATA:
            return None
        check_ret(ret)

        if valid_data[0]:
            data_bytes = ffi.buffer(self._buffer, actual_size[0])[:]
            data = self._topic.type_class._deserialize_cdr(data_bytes)
            return Sample(data=data, valid_data=True)
        else:
            return Sample(data=None, valid_data=False)

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

    def get_subscription_matched_status(self) -> tuple[int, int]:
        """
        Get the subscription matched status.

        Returns:
            Tuple of (total_count, current_count) indicating matched writers
        """
        total_out = ffi.new("int32_t *")
        current_out = ffi.new("int32_t *")
        check_ret(
            lib.int2dds_get_subscription_matched_status(self._handle, total_out, current_out)
        )
        return total_out[0], current_out[0]

    @property
    def matched_writers(self) -> int:
        """Get the current number of matched writers."""
        _, current = self.get_subscription_matched_status()
        return current
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

    def get_statuscondition(self) -> StatusCondition:
        """Get the StatusCondition associated with this DataReader."""
        cond_ptr = ffi.new("Int2DdsStatusCondition **")
        check_ret(lib.int2dds_datareader_get_statuscondition(self._handle, cond_ptr))
        return StatusCondition(cond_ptr[0], owner=self)

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
